# PaddleOCR-VL-CPU
PaddleOCR-VL 是一个基于PaddlePaddle的视觉语言模型，当前专为GPU环境优化，适用于图像文本理解和生成任务。该模型结合了OCR技术和大规模语言模型，能够高效地处理图像中的文本信息并生成相关描述。

为了将PaddleOCR-VL模型适配到CPU环境，我们需要进行一些优化和调整。

https://linux.do/t/topic/1054681 提供了在CPU上运行的原始代码，在此的基础上，进行了一些优化：
- 修改了 image_urls 的处理逻辑，使其能够正确的在一些边缘设备上运行（在RDK X5上经过验证）
- 提供了完整的 requirements.txt 文件
- 包含了完整的调用示例

特别感谢 [@funkpopo](https://github.com/funkpopo) 的原始贡献！

## 文件说明
- `demo_ppocrvl_server.py`: 启动PaddleOCR-VL CPU API服务器
- `demo_ppocrvl_client.py`: PaddleOCR-VL API客户端调用示例(基于 requests)
- `demo_ppocrvl_client_openai.py`: 使用OpenAI库调用PaddleOCR-VL API示例
- `demo_camera_ocr.py`: 使用摄像头进行实时OCR演示
- `requirements.txt`: 所需Python依赖列表
- `test.png`: 测试图像文件，256x256像素

## 安装依赖
```
pip install -r requirements.txt
```

## 下载模型
```bash
pip install modelscope
modelscope download --model PaddlePaddle/PaddleOCR-VL --local_dir ./PaddlePaddle/PaddleOCR-VL
```
## 环境要求
运行PaddleOCR-VL CPU大约需要4GB内存，如果内存不足，建议开启Swap。

开启Swap的命令如下(Ubuntu)：
```bash 
free -h
sudo fallocate -l 1G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
free -h
```

## 启动服务器

```bash
python demo_ppocrvl_server.py
```

## 模型调用

### 基本文本对话

```bash
python demo_ppocrvl_client.py --text "你好，请介绍一下你自己"
```

### 图像分析

```bash
python demo_ppocrvl_client.py --text "描述这张图片的内容" --image path/to/image.jpg
```

### 流式响应

```bash
python demo_ppocrvl_client.py --text "请详细描述这张图片" --image path/to/image.jpg --stream
```

### 自定义参数

```bash
python demo_ppocrvl_client.py --text "分析这张文档" --image document.jpg --max-tokens 1000 --temperature 0.5
```

### 查看可用模型

```bash
python demo_ppocrvl_client.py --list-models
```

### OpenAI格式调用
OpenAI兼容客户端调用示例：
```bash
python demo_ppocrvl_client_openai.py
```

## 摄像头OCR演示
demo_camera_ocr.py 提供了一个使用摄像头进行实时OCR的示例，运行后会打开摄像头，拍照后进行OCR识别并显示结果。
```bash
python demo_camera_ocr.py
```

## 运行耗时测试
|设备 | 图片尺寸 | 耗时(秒) |
|----|---------|---------|
| RDK X5(8x A55@1.5GHz, 4G内存版本) | 640x480 | 435 |
| Intel 13th Gen Intel(R) Core(TM) i7-13700K | 256×256 | 7.3 |
| Intel 13th Gen Intel(R) Core(TM) i7-13700K | 640x480 | 13.25 |
