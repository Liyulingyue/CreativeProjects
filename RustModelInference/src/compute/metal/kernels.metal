#include <metal_stdlib>
using namespace metal;

struct Params {
    uint batch;
    uint n_in;
    uint local_rows;
    uint global_row_start;
    uint global_output_stride;
    uint mode;
    uint weight_byte_bias;
    uint output_row_start;
};

struct LayerParams {
    uint batch;
    uint width;
    uint groups;
    uint aux0;
    uint aux1;
    uint aux2;
    uint aux3;
    uint aux4;
};

static uint load_u32(device const uchar *bytes, uint offset) {
    return uint(bytes[offset])
        | (uint(bytes[offset + 1]) << 8)
        | (uint(bytes[offset + 2]) << 16)
        | (uint(bytes[offset + 3]) << 24);
}

static ushort load_u16(device const uchar *bytes, uint offset) {
    return ushort(bytes[offset]) | (ushort(bytes[offset + 1]) << 8);
}

static float load_weight(device const uchar *bytes, uint index, bool f16) {
    return f16 ? float(as_type<half>(load_u16(bytes, index * 2)))
               : as_type<float>(load_u32(bytes, index * 4));
}

static float q8_value(device const uchar *weights, constant Params &params,
                      uint row, uint column) {
    uint block = column >> 5;
    uint lane = column & 31;
    uint byte_index = params.weight_byte_bias
        + (row * (params.n_in >> 5) + block) * 34;
    ushort scale_bits = ushort(weights[byte_index])
        | (ushort(weights[byte_index + 1]) << 8);
    char q = char(weights[byte_index + 2 + lane]);
    return float(as_type<half>(scale_bits)) * float(q);
}

kernel void quantize_q8(device const float *input [[buffer(0)]],
                        device char *values [[buffer(1)]],
                        device float *scales [[buffer(2)]],
                        constant Params &params [[buffer(3)]],
                        uint block [[thread_position_in_grid]]) {
    uint block_count = params.batch * (params.n_in >> 5);
    if (block >= block_count) return;
    uint base = block * 32;
    float amax = 0.0f;
    for (uint lane = 0; lane < 32; ++lane) {
        amax = max(amax, abs(input[base + lane]));
    }
    float scale = amax == 0.0f ? 0.0f : amax / 127.0f;
    float inverse = scale == 0.0f ? 0.0f : 1.0f / scale;
    scales[block] = float(half(scale));
    for (uint lane = 0; lane < 32; ++lane) {
        values[base + lane] = char(clamp(rint(input[base + lane] * inverse), -128.0f, 127.0f));
    }
}

kernel void q8_rows(device const uchar *weights [[buffer(0)]],
                    device const uchar *input [[buffer(1)]],
                    device float *output [[buffer(2)]],
                    device const char *input_q8 [[buffer(3)]],
                    device const float *input_scales [[buffer(4)]],
                    constant Params &params [[buffer(5)]],
                    uint index [[thread_position_in_grid]]) {
    if (params.mode == 0) {
        uint count = params.batch * params.local_rows;
        if (index >= count) {
            return;
        }
        uint batch = index / params.local_rows;
        uint local_row = index % params.local_rows;
        uint global_row = params.global_row_start + local_row;
        (void)global_row;
        float sum = 0.0f;
        uint blocks = params.n_in >> 5;
        for (uint block = 0; block < blocks; ++block) {
            uint weight_index = params.weight_byte_bias
                + (local_row * blocks + block) * 34;
            ushort scale_bits = ushort(weights[weight_index])
                | (ushort(weights[weight_index + 1]) << 8);
            int dot = 0;
            uint input_index = batch * params.n_in + block * 32;
            for (uint lane = 0; lane < 32; ++lane) {
                dot += int(char(weights[weight_index + 2 + lane]))
                    * int(input_q8[input_index + lane]);
            }
            float scale = float(as_type<half>(scale_bits))
                * input_scales[batch * blocks + block];
            sum += float(dot) * scale;
        }
        output[batch * params.global_output_stride
            + params.output_row_start + local_row] = sum;
        return;
    }

    uint count = params.batch * params.n_in;
    if (index >= count) {
        return;
    }
    uint batch = index / params.n_in;
    uint column = index % params.n_in;
    uint global_row = load_u32(input, batch * 4);
    if (global_row < params.global_row_start
        || global_row >= params.global_row_start + params.local_rows) {
        return;
    }
    output[batch * params.global_output_stride + column] =
        q8_value(weights, params, global_row - params.global_row_start, column);
}

kernel void rms_norm(device const uchar *weights [[buffer(0)]],
                     device const float *input [[buffer(1)]],
                     device float *output [[buffer(2)]],
                     constant LayerParams &params [[buffer(3)]],
                     uint group [[thread_position_in_grid]]) {
    if (group >= params.batch * params.groups) return;
    uint base = group * params.width;
    float sum = 0.0f;
    for (uint i = 0; i < params.width; ++i) {
        float value = input[base + i];
        sum += value * value;
    }
    float scale = rsqrt(sum / float(params.width) + as_type<float>(params.aux0));
    for (uint i = 0; i < params.width; ++i) {
        output[base + i] = input[base + i] * scale
            * load_weight(weights, i, params.aux1 != 0);
    }
}

kernel void rope(device float *q [[buffer(0)]],
                 device float *k [[buffer(1)]],
                 constant LayerParams &params [[buffer(2)]],
                 uint index [[thread_position_in_grid]]) {
    uint half_width = params.aux0 / 2;
    uint q_pairs = params.width / 2;
    uint k_pairs = params.groups / 2;
    uint pairs = q_pairs + k_pairs;
    if (index >= params.batch * pairs) return;
    uint item = index / pairs;
    uint pair = index % pairs;
    device float *values = pair < q_pairs ? q : k;
    uint width = pair < q_pairs ? params.width : params.groups;
    pair = pair < q_pairs ? pair : pair - q_pairs;
    uint head = pair / half_width;
    uint lane = pair % half_width;
    uint base = item * width + head * params.aux0;
    float theta = float(params.aux1 + item);
    float theta_scale = pow(as_type<float>(params.aux2), -2.0f / float(params.aux0));
    for (uint i = 0; i < lane; ++i) theta *= theta_scale;
    float c = cos(theta);
    float s = sin(theta);
    float x0 = values[base + lane];
    float x1 = values[base + lane + half_width];
    values[base + lane] = fma(x0, c, -x1 * s);
    values[base + lane + half_width] = fma(x0, s, x1 * c);
}

struct MRopeParams { uint batch, q_width, k_width, head_dim, freq_base, positions[4], sections[4]; };

kernel void mrope(device float *q [[buffer(0)]], device float *k [[buffer(1)]],
                  constant MRopeParams &params [[buffer(2)]], uint index [[thread_position_in_grid]]) {
    uint half_width = params.head_dim / 2;
    uint q_pairs = params.q_width / 2, k_pairs = params.k_width / 2;
    if (index >= q_pairs + k_pairs) return;
    device float *values = index < q_pairs ? q : k;
    uint width = index < q_pairs ? params.q_width : params.k_width;
    uint pair = index < q_pairs ? index : index - q_pairs;
    uint lane = pair % half_width, head = pair / half_width;
    uint sector = lane % (params.sections[0] + params.sections[1] + params.sections[2] + params.sections[3]);
    uint axis = sector < params.sections[0] ? 0 : sector < params.sections[0] + params.sections[1] ? 1 : sector < params.sections[0] + params.sections[1] + params.sections[2] ? 2 : 3;
    float theta = float(params.positions[axis]);
    float theta_scale = pow(as_type<float>(params.freq_base), -2.0f / float(params.head_dim));
    for (uint i = 0; i < lane; ++i) theta *= theta_scale;
    uint base = head * params.head_dim;
    float x0 = values[base + lane], x1 = values[base + lane + half_width];
    float c = cos(theta), s = sin(theta);
    values[base + lane] = fma(x0, c, -x1 * s);
    values[base + lane + half_width] = fma(x0, s, x1 * c);
}

kernel void slice(device const float *input [[buffer(0)]], device float *output [[buffer(1)]],
                  constant LayerParams &params [[buffer(2)]], uint index [[thread_position_in_grid]]) {
    if (index < params.batch * params.width) output[index] = input[(index / params.width) * params.groups + params.aux0 + index % params.width];
}

kernel void kv_append(device const float *k [[buffer(0)]],
                      device const float *v [[buffer(1)]],
                      device half *keys [[buffer(2)]],
                      device half *values [[buffer(3)]],
                      constant LayerParams &params [[buffer(4)]],
                      uint index [[thread_position_in_grid]]) {
    uint item_width = params.width + params.groups;
    if (index >= params.batch * item_width) return;
    uint item = index / item_width;
    uint lane = index % item_width;
    uint position = params.aux0 + item;
    if (lane < params.width) {
        keys[position * params.width + lane] = half(k[item * params.width + lane]);
    } else {
        lane -= params.width;
        values[position * params.groups + lane] = half(v[item * params.groups + lane]);
    }
}

// ponytail: each output lane recomputes softmax; share scores only after profiling proves it matters.
kernel void attention(device const float *q [[buffer(0)]],
                      device const half *keys [[buffer(1)]],
                      device const half *values [[buffer(2)]],
                      device float *output [[buffer(3)]],
                      constant LayerParams &params [[buffer(4)]],
                      uint index [[thread_position_in_grid]]) {
    uint value_dim = params.aux2;
    uint count = params.batch * params.width * value_dim;
    if (index >= count) return;
    uint lane = index % value_dim;
    uint head_item = index / value_dim;
    uint head = head_item % params.width;
    uint item = head_item / params.width;
    uint kv_head = head / (params.width / params.groups);
    uint position = params.aux3 + item;
    uint cached = position + 1;
    uint key_dim = params.aux1;
    uint key_width = params.groups * key_dim;
    uint value_width = params.groups * value_dim;
    uint q_base = (item * params.width + head) * key_dim;
    float scale = rsqrt(float(key_dim));
    float maximum = -INFINITY;
    for (uint prior = 0; prior < cached; ++prior) {
        float score = 0.0f;
        uint key_base = prior * key_width + kv_head * key_dim;
        for (uint i = 0; i < key_dim; ++i) score += q[q_base + i] * float(keys[key_base + i]);
        maximum = max(maximum, score * scale);
    }
    float denominator = 0.0f;
    float result = 0.0f;
    for (uint prior = 0; prior < cached; ++prior) {
        float score = 0.0f;
        uint key_base = prior * key_width + kv_head * key_dim;
        for (uint i = 0; i < key_dim; ++i) score += q[q_base + i] * float(keys[key_base + i]);
        float probability = exp(score * scale - maximum);
        denominator += probability;
        result += probability * float(values[prior * value_width + kv_head * value_dim + lane]);
    }
    output[(item * params.width + head) * value_dim + lane] = result / denominator;
}

kernel void silu_mul(device const float *gate [[buffer(0)]],
                     device float *up [[buffer(1)]],
                     constant LayerParams &params [[buffer(2)]],
                     uint index [[thread_position_in_grid]]) {
    if (index >= params.batch * params.width) return;
    float value = gate[index];
    up[index] *= value / (1.0f + exp(-value));
}

kernel void sigmoid_mul(device const float *gate [[buffer(0)]], device float *values [[buffer(1)]],
                        constant LayerParams &params [[buffer(2)]], uint index [[thread_position_in_grid]]) {
    if (index < params.batch * params.width) values[index] *= 1.0f / (1.0f + exp(-gate[index]));
}

kernel void sigmoid(device float *values [[buffer(0)]],
                    constant LayerParams &params [[buffer(1)]],
                    uint index [[thread_position_in_grid]]) {
    if (index < params.batch * params.width) values[index] = 1.0f / (1.0f + exp(-values[index]));
}

kernel void silu(device float *values [[buffer(0)]],
                 constant LayerParams &params [[buffer(1)]],
                 uint index [[thread_position_in_grid]]) {
    if (index < params.batch * params.width) values[index] *= 1.0f / (1.0f + exp(-values[index]));
}

kernel void depthwise_causal_conv(device const float *weights [[buffer(0)]],
                                  device const float *input [[buffer(1)]],
                                  device float *state [[buffer(2)]],
                                  device float *output [[buffer(3)]],
                                  constant LayerParams &params [[buffer(4)]],
                                  uint channel [[thread_position_in_grid]]) {
    if (channel >= params.width) return;
    for (uint item = 0; item < params.batch; ++item) {
        float current = input[item * params.width + channel];
        for (uint tap = 0; tap + 1 < params.groups; ++tap) {
            state[tap * params.width + channel] = state[(tap + 1) * params.width + channel];
        }
        state[(params.groups - 1) * params.width + channel] = current;
        float value = 0.0f;
        for (uint tap = 0; tap < params.groups; ++tap) {
            value += weights[channel * params.groups + tap] * state[tap * params.width + channel];
        }
        output[item * params.width + channel] = value;
    }
}

kernel void l2_norm(device float *values [[buffer(0)]],
                    constant LayerParams &params [[buffer(1)]],
                    uint group [[thread_position_in_grid]]) {
    if (group >= params.batch * params.groups) return;
    uint base = group * params.width;
    float magnitude = 0.0f;
    for (uint lane = 0; lane < params.width; ++lane) magnitude += values[base + lane] * values[base + lane];
    float scale = 1.0f / max(sqrt(magnitude), as_type<float>(params.aux0));
    for (uint lane = 0; lane < params.width; ++lane) values[base + lane] *= scale;
}

kernel void softplus_affine(device const float *bias [[buffer(0)]],
                            device const float *scale [[buffer(1)]],
                            device float *values [[buffer(2)]],
                            constant LayerParams &params [[buffer(3)]],
                            uint index [[thread_position_in_grid]]) {
    if (index >= params.batch * params.width) return;
    uint lane = index % params.width;
    float value = values[index] + bias[lane];
    values[index] = (value > 20.0f ? value : log(1.0f + exp(value))) * scale[lane];
}

struct SsmParams { uint batch, state_size, group_count, dt_rank, inner_size; };

kernel void ssm_update(device const float *q [[buffer(0)]],
                       device const float *k [[buffer(1)]],
                       device const float *v [[buffer(2)]],
                       device const float *alpha [[buffer(3)]],
                       device const float *beta [[buffer(4)]],
                       device float *state [[buffer(5)]],
                       device float *output [[buffer(6)]],
                       constant SsmParams &params [[buffer(7)]],
                       uint head [[thread_position_in_grid]]) {
    if (head >= params.dt_rank) return;
    uint head_width = params.inner_size / params.dt_rank;
    uint q_width = params.state_size * params.group_count;
    uint key_head = head % params.group_count;
    uint state_base = head * head_width * head_width;
    for (uint item = 0; item < params.batch; ++item) {
        uint q_base = item * q_width + key_head * params.state_size;
        uint v_base = item * params.inner_size + head * head_width;
        float decay = exp(alpha[item * params.dt_rank + head]);
        for (uint index = 0; index < head_width * head_width; ++index) state[state_base + index] *= decay;
        for (uint row = 0; row < head_width; ++row) {
            float prior = 0.0f;
            uint row_base = state_base + row * head_width;
            for (uint column = 0; column < head_width; ++column) prior += state[row_base + column] * k[q_base + column];
            output[v_base + row] = prior;
        }
        for (uint row = 0; row < head_width; ++row) {
            float delta = (v[v_base + row] - output[v_base + row]) * beta[item * params.dt_rank + head];
            uint row_base = state_base + row * head_width;
            for (uint column = 0; column < head_width; ++column) state[row_base + column] += k[q_base + column] * delta;
        }
        float norm = rsqrt(float(params.state_size));
        for (uint row = 0; row < head_width; ++row) {
            float value = 0.0f;
            uint row_base = state_base + row * head_width;
            for (uint column = 0; column < head_width; ++column) value += state[row_base + column] * q[q_base + column];
            output[v_base + row] = value * norm;
        }
    }
}

kernel void add(device const float *left [[buffer(0)]],
                device const float *right [[buffer(1)]],
                device float *output [[buffer(2)]],
                constant LayerParams &params [[buffer(3)]],
                uint index [[thread_position_in_grid]]) {
    if (index < params.batch * params.width) output[index] = left[index] + right[index];
}
