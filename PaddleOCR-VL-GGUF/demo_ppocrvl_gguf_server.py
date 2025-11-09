# https://linux.do/t/topic/1054681
# PaddleOCR-VL with GGUF LLM Backend
# 视觉编码器部分使用 PyTorch，LLM 部分使用 llama.cpp/GGUF 加速

import base64
from io import BytesIO
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

# 使用 llama-cpp-python 直接加载 GGUF 模型
try:
    from llama_cpp import Llama
    LLAMA_CPP_AVAILABLE = True
except ImportError:
    print("警告: llama-cpp-python 未安装")
    print("请运行: pip install llama-cpp-python")
    LLAMA_CPP_AVAILABLE = False

LOCAL_PATH = "PaddlePaddle/PaddleOCR-VL"  # 视觉模型路径
GGUF_MODEL_PATH = "extracted_llm/llm_model_q4.gguf"  # GGUF 模型路径
N_GPU_LAYERS = 0  # GPU 层数，0 表示纯 CPU
N_CTX = 4096  # 上下文长度
N_THREADS = 8  # CPU 线程数

print(f"=== PaddleOCR-VL GGUF API 启动中 ===")
print(f"视觉模型路径: {LOCAL_PATH}")
print(f"LLM 后端: llama.cpp (直接)")
print(f"GGUF 模型路径: {GGUF_MODEL_PATH}")

try:
    # 只加载 processor 和视觉模型部分
    processor = AutoProcessor.from_pretrained(LOCAL_PATH, trust_remote_code=True, use_fast=True)
    
    # 加载完整模型用于提取视觉编码器
    from transformers import AutoModelForCausalLM
    print("正在加载完整模型以提取视觉编码器...")
    full_model = AutoModelForCausalLM.from_pretrained(
        LOCAL_PATH, 
        trust_remote_code=True, 
        torch_dtype=torch.float32,
        low_cpu_mem_usage=True
    ).to("cpu")
    
    # 提取视觉编码器和投影层
    visual_encoder = full_model.visual
    projector = full_model.mlp_AR
    
    # 清理完整模型，只保留需要的部分
    del full_model.model  # 删除 LLM 部分
    del full_model.lm_head
    del full_model
    gc.collect()
    
    visual_encoder.eval()
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

app = FastAPI()

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
        
        # 获取文本嵌入
        input_ids = inputs["input_ids"]
        inputs_embeds = visual_encoder.vision_model.embeddings(input_ids) if hasattr(visual_encoder, 'vision_model') else None
        
        # 处理图像（如果存在）
        if image and "pixel_values" in inputs:
            pixel_values = inputs["pixel_values"]
            
            # 从配置中获取图像网格信息
            # 简化版本：假设单张图像
            image_grid_thw = [(1, 14, 14)]  # t, h, w - 根据实际配置调整
            
            # 构建视觉输入参数
            siglip_position_ids = torch.arange(np.prod(image_grid_thw[0])).to(pixel_values.device)
            cu_seqlens = torch.tensor([0, np.prod(image_grid_thw[0])], dtype=torch.int32).to(pixel_values.device)
            sample_indices = torch.zeros(np.prod(image_grid_thw[0]), dtype=torch.int64).to(pixel_values.device)
            
            # 视觉编码
            with torch.no_grad():
                vision_outputs = visual_encoder(
                    pixel_values=pixel_values.unsqueeze(0),
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
                
                image_embeds = vision_outputs.last_hidden_state
                
                # 通过投影层
                image_embeds = projector(image_embeds, image_grid_thw)
                
                # 如果是列表，拼接
                if isinstance(image_embeds, list):
                    image_embeds = torch.cat(image_embeds, dim=0)
        
        # 返回处理后的嵌入和 prompt
        return {
            "prompt": prompt,
            "image_embeds": image_embeds.cpu().numpy() if image and "pixel_values" in inputs else None,
            "input_ids": input_ids.cpu().numpy()
        }
        
    except Exception as e:
        print(f"视觉编码失败: {e}")
        traceback.print_exc()
        raise

def call_llama_cpp_generate(prompt, max_tokens=131072, temperature=0.7, stream=False):
    """
    使用 llama.cpp 进行文本生成
    
    注意: 这里简化处理,实际应该将视觉嵌入注入到模型中
    由于 llama.cpp 的限制,当前版本使用文本提示作为输入
    """
    if llm_model is None:
        raise HTTPException(status_code=500, detail="GGUF 模型未加载")
    
    try:
        if stream:
            # 流式生成
            return llm_model(
                prompt,
                max_tokens=max_tokens,
                temperature=temperature,
                stream=True,
                stop=["</s>", "<|im_end|>", "<|end|>"]
            )
        else:
            # 非流式生成
            result = llm_model(
                prompt,
                max_tokens=max_tokens,
                temperature=temperature,
                stream=False,
                stop=["</s>", "<|im_end|>", "<|end|>"]
            )
            return result
            
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
        def generate_stream():
            try:
                full_text = ""
                for chunk in call_llama_cpp_generate(prompt, max_tokens, temperature, stream=True):
                    if 'choices' in chunk and len(chunk['choices']) > 0:
                        text = chunk['choices'][0].get('text', '')
                        if text:
                            full_text += text
                            stream_chunk = {
                                "id": completion_id,
                                "object": "chat.completion.chunk",
                                "created": created_time,
                                "model": "paddleocr-vl-gguf",
                                "choices": [{
                                    "index": 0,
                                    "delta": {
                                        "role": "assistant" if len(full_text) == len(text) else None,
                                        "content": text
                                    },
                                    "finish_reason": None
                                }]
                            }
                            if stream_chunk["choices"][0]["delta"]["role"] is None:
                                del stream_chunk["choices"][0]["delta"]["role"]
                            
                            yield f"data: {json.dumps(stream_chunk, ensure_ascii=False)}\n\n"
                
                # 最后一个 chunk
                final_chunk = {
                    "id": completion_id,
                    "object": "chat.completion.chunk",
                    "created": created_time,
                    "model": "paddleocr-vl-gguf",
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": "stop"
                    }]
                }
                yield f"data: {json.dumps(final_chunk, ensure_ascii=False)}\n\n"
                yield "data: [DONE]\n\n"
                
            except Exception as e:
                print(f"流式生成失败: {e}")
                traceback.print_exc()
        
        return StreamingResponse(generate_stream(), media_type="text/event-stream")
    
    else:
        # 非流式
        result = call_llama_cpp_generate(prompt, max_tokens, temperature, stream=False)
        generated = result['choices'][0]['text'] if 'choices' in result else ""
        
        print(f"生成内容: {generated[:200]}{'...' if len(generated) > 200 else ''}")
        
        response = {
            "id": completion_id,
            "object": "chat.completion",
            "created": created_time,
            "model": "paddleocr-vl-gguf",
            "choices": [{
                "index": 0, 
                "message": {
                    "role": "assistant", 
                    "content": generated
                }, 
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": result.get('usage', {}).get('prompt_tokens', 0),
                "completion_tokens": result.get('usage', {}).get('completion_tokens', 0),
                "total_tokens": result.get('usage', {}).get('total_tokens', 0)
            }
        }
        
        return response

if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=7778)  # 使用不同端口避免冲突
