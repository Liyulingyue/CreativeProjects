# PaddleOCR-VL with GGUF LLM Backend
# 视觉编码器部分使用 PyTorch，LLM 部分使用 llama.cpp/GGUF 加速

import base64
from io import BytesIO
import ctypes
import os
import torch
from PIL import Image
from transformers import AutoProcessor
from fastapi import FastAPI, HTTPException
from fastapi.responses import StreamingResponse
import uvicorn
import requests
import time
import json
import gc
import traceback
import numpy as np
from vision_model_loader import load_vision_model

# 使用 llama-cpp-python 直接加载 GGUF 模型
try:
    from llama_cpp import Llama
    from llama_cpp import llama_cpp as llama_cpp_lib
    LLAMA_CPP_AVAILABLE = True
except ImportError:
    print("警告: llama-cpp-python 未安装")
    print("请运行: pip install llama-cpp-python")
    LLAMA_CPP_AVAILABLE = False
    llama_cpp_lib = None

LOCAL_PATH = "PaddlePaddle/PaddleOCR-VL"  # 原始完整模型路径（仅供参考）
VISION_MODEL_PATH = "vision_model"  # 微缩版视觉模型路径
GGUF_MODEL_PATH = "gguf_model/llm_model_q4.gguf"  # GGUF 模型路径
N_GPU_LAYERS = 0  # GPU 层数，0 表示纯 CPU
N_CTX = 4096  # 上下文长度
N_THREADS = 8  # CPU 线程数

print(f"=== PaddleOCR-VL GGUF API 启动中 ===")
print(f"视觉模型路径: {VISION_MODEL_PATH}")
print(f"LLM 后端: llama.cpp (直接)")
print(f"GGUF 模型路径: {GGUF_MODEL_PATH}")

try:
    # 加载微缩版视觉模型
    print("正在加载微缩版视觉模型...")
    vision_model, processor = load_vision_model(VISION_MODEL_PATH, device="cpu")
    visual_encoder = vision_model.visual
    projector = vision_model.mlp_AR
    print("✅ 微缩版视觉模型加载成功")
    projector.eval()

    print("视觉编码器加载成功")
    
    # 加载 GGUF LLM 模型
    llm_model = None
    if LLAMA_CPP_AVAILABLE:
        if os.path.exists(GGUF_MODEL_PATH):
            print(f"正在加载 GGUF 模型: {GGUF_MODEL_PATH}")
            llm_model = Llama(
                model_path=GGUF_MODEL_PATH,
                n_gpu_layers=N_GPU_LAYERS,
                n_ctx=N_CTX,
                n_threads=N_THREADS,
                verbose=False
            )
            print("GGUF 模型加载成功")
            print(f"  - GPU 层数: {N_GPU_LAYERS}")
            print(f"  - 上下文长度: {N_CTX}")
            print(f"  - CPU 线程数: {N_THREADS}")
        else:
            print(f"警告: GGUF 模型文件不存在: {GGUF_MODEL_PATH}")
            print("LLM 推理将无法工作，请先运行 convert_to_gguf.py 并完成 GGUF 转换")
    else:
        print("警告: llama-cpp-python 未安装，LLM 推理将无法工作")
    
except Exception as e:
    print(f"模型加载失败: {e}")
    traceback.print_exc()
    raise

IMAGE_PLACEHOLDER_TOKEN = "<|IMAGE_PLACEHOLDER|>"
try:
    IMAGE_PLACEHOLDER_ID = processor.tokenizer.convert_tokens_to_ids(IMAGE_PLACEHOLDER_TOKEN)
except Exception:
    IMAGE_PLACEHOLDER_ID = None

DEFAULT_STOP_STRINGS = ["</s>", "<|im_end|>", "<|end|>"]
STOP_TOKEN_IDS_FROM_TOKENIZER = set()
try:
    for _stop_token in DEFAULT_STOP_STRINGS:
        token_id = processor.tokenizer.convert_tokens_to_ids(_stop_token)
        if token_id is not None and token_id != getattr(processor.tokenizer, "unk_token_id", None):
            STOP_TOKEN_IDS_FROM_TOKENIZER.add(int(token_id))
except Exception:
    STOP_TOKEN_IDS_FROM_TOKENIZER = set()

app = FastAPI()


def _apply_stop_sequences(text: str, stop_strings):
    """Trim text at the earliest occurrence of any stop string."""
    earliest_idx = None
    for stop in stop_strings or []:
        if not stop:
            continue
        idx = text.find(stop)
        if idx != -1 and (earliest_idx is None or idx < earliest_idx):
            earliest_idx = idx
    if earliest_idx is not None:
        return text[:earliest_idx], True
    return text, False


def _longest_partial_stop_suffix(text: str, stop_strings):
    """Return length of the longest suffix that is a prefix of any stop string."""
    max_len = 0
    for stop in stop_strings or []:
        if not stop:
            continue
        max_check = min(len(stop) - 1, len(text))
        for length in range(max_check, 0, -1):
            if text.endswith(stop[:length]) and length < len(stop):
                if length > max_len:
                    max_len = length
                break
    return max_len


def _build_stop_token_ids(llm):
    stop_ids = set(STOP_TOKEN_IDS_FROM_TOKENIZER)
    try:
        eos_id = llm.token_eos()
        if eos_id is not None and eos_id >= 0:
            stop_ids.add(int(eos_id))
    except Exception:
        pass
    try:
        sep_id = llm._model.token_sep()
        if sep_id is not None and sep_id >= 0:
            stop_ids.add(int(sep_id))
    except Exception:
        pass
    return {sid for sid in stop_ids if sid is not None and sid >= 0}


def _inject_embedding_token(llm, embedding_vector, placeholder_token_id):
    if not LLAMA_CPP_AVAILABLE:
        raise RuntimeError("llama.cpp 后端不可用")
    embedding_view = np.asarray(embedding_vector, dtype=np.float32)
    if embedding_view.ndim != 1:
        embedding_view = embedding_view.reshape(-1)
    n_embd = llm.n_embd()
    if embedding_view.shape[0] != n_embd:
        raise ValueError(
            f"图像嵌入维度 {embedding_view.shape[0]} 与模型隐藏维度 {n_embd} 不匹配"
        )
    batch = llama_cpp_lib.llama_batch_init(1, n_embd, 1)
    try:
        float_ptr = ctypes.cast(batch.embd, ctypes.POINTER(ctypes.c_float))
        np_view = np.ctypeslib.as_array(float_ptr, shape=(n_embd,))
        np_view[:] = embedding_view

        batch.n_tokens = 1
        pos = llm.n_tokens
        batch.pos[0] = pos
        batch.n_seq_id[0] = 1
        batch.seq_id[0][0] = 0
        batch.logits[0] = 1
        token_ptr = getattr(batch, "token", None)
        if token_ptr:
            token_array = ctypes.cast(token_ptr, ctypes.POINTER(ctypes.c_int32))
            if placeholder_token_id is not None and placeholder_token_id >= 0:
                token_array[0] = int(placeholder_token_id)
            else:
                try:
                    token_array[0] = llama_cpp_lib.llama_token_null()
                except AttributeError:
                    token_array[0] = -1

        rc = llama_cpp_lib.llama_decode(llm._ctx.ctx, batch)
        if rc != 0:
            raise RuntimeError(f"llama_decode 返回错误码 {rc}")

        token_to_store = placeholder_token_id if placeholder_token_id is not None else -1
        llm.input_ids[pos] = token_to_store
        if getattr(llm, "_logits_all", False):
            cols = llm._n_vocab
            logits = np.ctypeslib.as_array(llm._ctx.get_logits(), shape=(cols,))
            llm.scores[pos : pos + 1, :].reshape(-1)[:] = logits
        llm.n_tokens += 1
    finally:
        llama_cpp_lib.llama_batch_free(batch)


def _apply_prompt_tokens(llm, prompt_tokens, image_embeds):
    if image_embeds is None or len(image_embeds) == 0:
        llm.eval([int(t) for t in prompt_tokens])
        return

    if IMAGE_PLACEHOLDER_ID is None:
        print("警告: 未找到图像占位符 token, 按纯文本提示处理", flush=True)
        llm.eval([int(t) for t in prompt_tokens])
        return

    placeholder_id = int(IMAGE_PLACEHOLDER_ID)
    token_buffer = []
    embed_idx = 0
    placeholder_hits = 0
    embed_count = len(image_embeds)

    for token in prompt_tokens:
        token_int = int(token)
        if token_int == placeholder_id:
            placeholder_hits += 1
            if token_buffer:
                llm.eval(token_buffer)
                token_buffer = []
            if embed_idx >= embed_count:
                # 没有对应嵌入, 回退为普通 token
                llm.eval([token_int])
            else:
                _inject_embedding_token(llm, image_embeds[embed_idx], placeholder_id)
                embed_idx += 1
        else:
            token_buffer.append(token_int)

    if token_buffer:
        llm.eval(token_buffer)

    if placeholder_hits != embed_count:
        print(
            f"警告: 图像占位符数量 ({placeholder_hits}) 与嵌入数量 ({embed_count}) 不一致",
            flush=True,
        )


def _prepare_context(llm, prompt_tokens, image_embeds):
    llm.reset()
    llm._sampler = None
    _apply_prompt_tokens(llm, prompt_tokens, image_embeds)


def _generate_non_stream_completion(llm, prompt_tokens, max_tokens, temperature, stop_strings):
    stop_token_ids = _build_stop_token_ids(llm)
    generator = llm.generate(
        [],
        top_p=0.95,
        temp=temperature,
        repeat_penalty=1.0,
        reset=False,
    )
    prompt_bytes = llm.detokenize(prompt_tokens)
    prompt_text = prompt_bytes.decode("utf-8", errors="ignore")

    generated_tokens = []
    full_output = ""
    stop_hit = False
    length_hit = False

    try:
        for token in generator:
            token_id = int(token)
            if token_id in stop_token_ids or llama_cpp_lib.llama_token_is_eog(llm._model.vocab, token_id):
                stop_hit = True
                break

            generated_tokens.append(token_id)
            all_text = llm.detokenize(prompt_tokens + generated_tokens).decode(
                "utf-8", errors="ignore"
            )
            candidate_output = all_text[len(prompt_text) :]
            candidate_output, triggered = _apply_stop_sequences(
                candidate_output, stop_strings
            )
            full_output = candidate_output

            if triggered:
                stop_hit = True
                break
            if len(generated_tokens) >= max_tokens:
                length_hit = True
                break
    finally:
        generator.close()
        llama_cpp_lib.llama_kv_self_clear(llm._ctx.ctx)
        llm.reset()
        llm._sampler = None

    finish_reason = "stop" if stop_hit else ("length" if length_hit else "stop")
    return full_output, generated_tokens, finish_reason


def _stream_completion(llm, prompt_tokens, max_tokens, temperature, stop_strings, completion_id, created_time, model_name):
    stop_token_ids = _build_stop_token_ids(llm)
    generator = llm.generate(
        [],
        top_p=0.95,
        temp=temperature,
        repeat_penalty=1.0,
        reset=False,
    )
    prompt_bytes = llm.detokenize(prompt_tokens)
    prompt_text = prompt_bytes.decode("utf-8", errors="ignore")

    full_output = ""
    buffered_suffix = ""
    generated_tokens = []
    first_chunk = True
    stop_hit = False
    length_hit = False

    def event_iterator():
        nonlocal full_output, buffered_suffix, first_chunk, stop_hit, length_hit
        try:
            for token in generator:
                token_id = int(token)
                if token_id in stop_token_ids or llama_cpp_lib.llama_token_is_eog(llm._model.vocab, token_id):
                    stop_hit = True
                    break

                generated_tokens.append(token_id)
                all_text = llm.detokenize(prompt_tokens + generated_tokens).decode(
                    "utf-8", errors="ignore"
                )
                candidate_output = all_text[len(prompt_text) :]
                candidate_output, triggered = _apply_stop_sequences(
                    candidate_output, stop_strings
                )
                if triggered:
                    stop_hit = True

                delta_full = candidate_output[len(full_output) :]
                full_output = candidate_output
                if delta_full:
                    delta_full = buffered_suffix + delta_full
                    buffered_suffix = ""

                pending = _longest_partial_stop_suffix(full_output, stop_strings)
                if pending:
                    if len(delta_full) >= pending:
                        buffered_suffix = delta_full[-pending:]
                        delta_to_emit = delta_full[:-pending]
                    else:
                        buffered_suffix = delta_full
                        delta_to_emit = ""
                else:
                    delta_to_emit = delta_full

                if delta_to_emit:
                    delta_payload = {"content": delta_to_emit}
                    if first_chunk:
                        delta_payload["role"] = "assistant"
                    chunk = {
                        "id": completion_id,
                        "object": "chat.completion.chunk",
                        "created": created_time,
                        "model": model_name,
                        "choices": [
                            {
                                "index": 0,
                                "delta": delta_payload,
                                "finish_reason": None,
                            }
                        ],
                    }
                    yield f"data: {json.dumps(chunk, ensure_ascii=False)}\n\n"
                    first_chunk = False

                if stop_hit or len(generated_tokens) >= max_tokens:
                    if len(generated_tokens) >= max_tokens and not stop_hit:
                        length_hit = True
                    break

            finish_reason = "stop" if stop_hit else ("length" if length_hit else "stop")
            final_chunk = {
                "id": completion_id,
                "object": "chat.completion.chunk",
                "created": created_time,
                "model": model_name,
                "choices": [
                    {
                        "index": 0,
                        "delta": {},
                        "finish_reason": finish_reason,
                    }
                ],
            }
            yield f"data: {json.dumps(final_chunk, ensure_ascii=False)}\n\n"
            yield "data: [DONE]\n\n"
        finally:
            generator.close()
            llama_cpp_lib.llama_kv_self_clear(llm._ctx.ctx)
            llm.reset()
            llm._sampler = None

    return event_iterator

@app.get("/v1/models")
async def list_models():
    return {
        "object": "list",
        "data": [
            {
                "id": "paddleocr-vl-gguf",
                "object": "model",
                "created": int(time.time()),
                "owned_by": "custom",
                "permission": [],
                "root": "paddleocr-vl-gguf",
                "parent": None,
                "capabilities": {
                    "vision": True,
                    "function_calling": False,
                    "fine_tuning": False,
                    "backend": "llama.cpp-gguf"
                }
            }
        ]
    }

async def encode_vision(image, text_prompt):
    """
    使用 PyTorch 视觉编码器处理图像
    返回视觉特征嵌入
    """
    try:
        # 构建消息格式
        content = []
        if image:
            content.append({"type": "image"})
        content.append({"type": "text", "text": text_prompt})
        
        chat_messages = [{"role": "user", "content": content}]
        
        # 应用 chat template
        try:
            prompt = processor.tokenizer.apply_chat_template(
                chat_messages, 
                tokenize=False, 
                add_generation_prompt=True
            )
        except Exception as e:
            print(f"Chat template 失败，使用备用格式: {e}")
            cls_token = "<|begin_of_sentence|>"
            if image:
                prompt = f"{cls_token}User: <|IMAGE_START|><|IMAGE_PLACEHOLDER|><|IMAGE_END|>{text_prompt}\nAssistant: "
            else:
                prompt = f"{cls_token}User: {text_prompt}\nAssistant: "
        
        # 处理输入
        inputs = processor(text=prompt, images=image, return_tensors="pt").to("cpu")
        
        input_ids = inputs["input_ids"]
        image_embeds = None

        if image and "pixel_values" in inputs:
            pixel_values = inputs["pixel_values"].unsqueeze(0)
            device = pixel_values.device
            if "image_grid_thw" in inputs:
                grid_values = inputs["image_grid_thw"][0].tolist()
                image_grid_thw = [tuple(int(x) for x in grid_values)]
                token_count = int(np.prod(image_grid_thw[0]))
                siglip_position_ids = torch.arange(token_count, dtype=torch.long, device=device)
                cu_seqlens = torch.tensor([0, token_count], dtype=torch.int32, device=device)
                sample_indices = torch.zeros(token_count, dtype=torch.long, device=device)

                with torch.no_grad():
                    vision_outputs = visual_encoder(
                        pixel_values=pixel_values,
                        image_grid_thw=image_grid_thw,
                        position_ids=siglip_position_ids,
                        vision_return_embed_list=True,
                        interpolate_pos_encoding=True,
                        sample_indices=sample_indices,
                        cu_seqlens=cu_seqlens,
                        return_pooler_output=False,
                        use_rope=True,
                        window_size=-1,
                    )

                    hidden_states = vision_outputs.last_hidden_state
                    if isinstance(hidden_states, list):
                        hidden_states_for_projector = [hs.cpu() for hs in hidden_states]
                    else:
                        hidden_states_for_projector = hidden_states.cpu()

                    projected = projector(hidden_states_for_projector, image_grid_thw)
                    if isinstance(projected, list):
                        projected = torch.cat(projected, dim=0)
                    image_embeds = projected.cpu().numpy()

        return {
            "prompt": prompt,
            "image_embeds": image_embeds,
            "input_ids": input_ids.cpu().numpy()
        }
        
    except Exception as e:
        print(f"视觉编码失败: {e}")
        traceback.print_exc()
        raise

def call_llama_cpp_generate(
    prompt_tokens,
    image_embeds=None,
    *,
    max_tokens=131072,
    temperature=0.7,
    stream=False,
    completion_id=None,
    created_time=None,
    model_name="paddleocr-vl-gguf",
):
    if llm_model is None or not LLAMA_CPP_AVAILABLE:
        raise HTTPException(status_code=500, detail="GGUF 模型未加载或 llama.cpp 不可用")

    if prompt_tokens is None:
        raise HTTPException(status_code=500, detail="缺少 token 化后的提示信息")

    prompt_tokens_list = [int(t) for t in prompt_tokens]
    embeds_array = None
    if image_embeds is not None:
        embeds_array = np.asarray(image_embeds, dtype=np.float32)
        if embeds_array.ndim == 1:
            embeds_array = embeds_array.reshape(1, -1)

    if stream:
        _prepare_context(llm_model, prompt_tokens_list, embeds_array)
        return _stream_completion(
            llm_model,
            prompt_tokens_list,
            max_tokens,
            temperature,
            DEFAULT_STOP_STRINGS,
            completion_id or f"chatcmpl-{int(time.time())}",
            created_time or int(time.time()),
            model_name,
        )

    _prepare_context(llm_model, prompt_tokens_list, embeds_array)

    try:
        text, generated_tokens, finish_reason = _generate_non_stream_completion(
            llm_model,
            prompt_tokens_list,
            max_tokens,
            temperature,
            DEFAULT_STOP_STRINGS,
        )
        completion_tokens = len(generated_tokens)
        usage = {
            "prompt_tokens": len(prompt_tokens_list),
            "completion_tokens": completion_tokens,
            "total_tokens": len(prompt_tokens_list) + completion_tokens,
        }
        return {
            "text": text,
            "generated_tokens": generated_tokens,
            "finish_reason": finish_reason,
            "usage": usage,
        }
    except Exception as e:
        print(f"llama.cpp 生成失败: {e}")
        traceback.print_exc()
        raise

@app.post("/v1/chat/completions")
async def chat_completions(body: dict):
    try:
        messages = body.get("messages", [])
        if not messages:
            raise HTTPException(status_code=400, detail="请求体中缺少messages字段")

        content = messages[-1].get("content", [])
        if isinstance(content, str):
            text_prompt = content
            image_urls = []
        else:
            # 提取图像和文本
            image_urls = [c["image_url"]["url"] for c in content if c["type"] == "image_url"]
            text_parts = [c["text"] for c in content if c["type"] == "text"]
            text_prompt = " ".join(text_parts) or "Parse the document."
    except KeyError as e:
        print(f"请求格式错误: {e}")
        traceback.print_exc()
        raise HTTPException(status_code=400, detail=f"请求格式错误: {e}")

    print(f"接收到请求: 文本='{text_prompt}', 图像数量={len(image_urls)}")

    # 加载图像
    images = []
    for idx, url in enumerate(image_urls):
        img = None
        try:
            if url.startswith("data:"):
                if "," in url:
                    _, b64_data = url.split(",", 1)
                else:
                    b64_data = url.replace("data:image/", "").split(";")[0]
                
                img_bytes = base64.b64decode(b64_data)
                img = Image.open(BytesIO(img_bytes))
            else:
                response = requests.get(url, timeout=10)
                response.raise_for_status()
                img = Image.open(BytesIO(response.content))
        except Exception as e:
            print(f"图片处理失败: {e}")
            traceback.print_exc()
            raise HTTPException(status_code=400, detail=f"图片处理失败: {e}")

        if img:
            if img.mode != 'RGB':
                img = img.convert('RGB')
            images.append(img)

    image = images[0] if images else None

    # 步骤1: 使用视觉编码器处理
    print("步骤1: 视觉编码...")
    vision_result = await encode_vision(image, text_prompt)
    prompt = vision_result["prompt"]
    image_embeds = vision_result["image_embeds"]
    input_ids_array = vision_result.get("input_ids")
    prompt_tokens = None
    if input_ids_array is not None:
        ids_np = np.asarray(input_ids_array)
        if ids_np.ndim == 2:
            prompt_tokens = [int(x) for x in ids_np[0].tolist()]
        else:
            prompt_tokens = [int(x) for x in ids_np.tolist()]
    if prompt_tokens is None:
        raise HTTPException(status_code=500, detail="未能获取 prompt 的 token 序列")
    
    # 关闭图片释放内存
    for img in images:
        try:
            img.close()
        except:
            pass
    
    # 步骤2: 调用 llama.cpp 生成
    print("步骤2: LLM 生成...")
    max_tokens = body.get("max_tokens", 1024)  # 降低默认值
    temperature = body.get("temperature", 0.7)
    stream = body.get("stream", False)
    
    completion_id = f"chatcmpl-{int(time.time())}"
    created_time = int(time.time())
    
    if stream:
        try:
            event_stream = call_llama_cpp_generate(
                prompt_tokens,
                image_embeds,
                max_tokens=max_tokens,
                temperature=temperature,
                stream=True,
                completion_id=completion_id,
                created_time=created_time,
                model_name="paddleocr-vl-gguf",
            )
        except Exception as e:
            print(f"流式生成失败: {e}")
            traceback.print_exc()
            raise
        return StreamingResponse(event_stream, media_type="text/event-stream")

    # 非流式
    result = call_llama_cpp_generate(
        prompt_tokens,
        image_embeds,
        max_tokens=max_tokens,
        temperature=temperature,
        stream=False,
        completion_id=completion_id,
        created_time=created_time,
    )
    generated = result.get("text", "")
    finish_reason = result.get("finish_reason", "stop")
    usage_stats = result.get("usage", {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0})

    print(f"生成内容: {generated[:200]}{'...' if len(generated) > 200 else ''}")

    response = {
        "id": completion_id,
        "object": "chat.completion",
        "created": created_time,
        "model": "paddleocr-vl-gguf",
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": generated,
                },
                "finish_reason": finish_reason,
            }
        ],
        "usage": usage_stats,
    }

    return response

if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=7778)  # 使用不同端口避免冲突
