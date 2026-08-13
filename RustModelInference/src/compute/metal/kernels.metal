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

static uint load_u32(device const uchar *bytes, uint offset) {
    return uint(bytes[offset])
        | (uint(bytes[offset + 1]) << 8)
        | (uint(bytes[offset + 2]) << 16)
        | (uint(bytes[offset + 3]) << 24);
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

kernel void q8_rows(device const uchar *weights [[buffer(0)]],
                    device const uchar *input [[buffer(1)]],
                    device float *output [[buffer(2)]],
                    constant Params &params [[buffer(3)]],
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
        for (uint column = 0; column < params.n_in; ++column) {
            float value = as_type<float>(load_u32(input, (batch * params.n_in + column) * 4));
            sum += q8_value(weights, params, local_row, column) * value;
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
