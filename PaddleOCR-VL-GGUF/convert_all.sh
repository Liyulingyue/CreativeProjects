#!/bin/bash
# PaddleOCR-VL 全流程转换脚本
# 从原始模型到 GGUF 量化模型的完整流程

set -e  # 遇到错误立即退出

# 默认参数
INPUT_MODEL_PATH="PaddlePaddle/PaddleOCR-VL"
VISION_OUTPUT_PATH="vision_model"
LLM_OUTPUT_PATH="language_model"
GGUF_OUTPUT_PATH="gguf_model"
GGUF_MODEL_PATH="${GGUF_OUTPUT_PATH}/llm_model.gguf"
QUANTIZED_MODEL_PATH="${GGUF_OUTPUT_PATH}/llm_model_q4.gguf"
QUANTIZATION_TYPE="Q4_K_M"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 日志函数
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 检查依赖
check_dependencies() {
    log_info "检查系统依赖..."

    # 检查Python
    if ! command -v python &> /dev/null; then
        log_error "Python 未找到，请安装 Python 3.8+"
        exit 1
    fi

    # 检查pip
    if ! command -v pip &> /dev/null; then
        log_error "pip 未找到，请安装 pip"
        exit 1
    fi

    # 检查git
    if ! command -v git &> /dev/null; then
        log_error "git 未找到，请安装 git"
        exit 1
    fi

    # 检查cmake
    if ! command -v cmake &> /dev/null; then
        log_error "cmake 未找到，请安装 cmake"
        exit 1
    fi

    # 检查gcc/g++
    if ! command -v gcc &> /dev/null || ! command -v g++ &> /dev/null; then
        log_error "gcc/g++ 未找到，请安装 build-essential"
        exit 1
    fi

    log_success "系统依赖检查通过"
}

# 检查输入参数
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --input-path)
                INPUT_MODEL_PATH="$2"
                shift 2
                ;;
            --vision-output)
                VISION_OUTPUT_PATH="$2"
                shift 2
                ;;
            --llm-output)
                LLM_OUTPUT_PATH="$2"
                shift 2
                ;;
            --gguf-output)
                GGUF_OUTPUT_PATH="$2"
                shift 2
                ;;
            --quantization-type)
                QUANTIZATION_TYPE="$2"
                shift 2
                ;;
            --help)
                echo "PaddleOCR-VL 全流程转换脚本"
                echo ""
                echo "用法: $0 [选项]"
                echo ""
                echo "选项:"
                echo "  --input-path PATH        输入模型路径 (默认: PaddlePaddle/PaddleOCR-VL)"
                echo "  --vision-output PATH     视觉模型输出路径 (默认: vision_model)"
                echo "  --llm-output PATH        语言模型输出路径 (默认: language_model)"
                echo "  --gguf-output PATH       GGUF模型输出路径 (默认: gguf_model)"
                echo "  --quantization-type TYPE 量化类型 (默认: Q4_K_M)"
                echo "  --help                   显示此帮助信息"
                echo ""
                echo "示例:"
                echo "  $0"
                echo "  $0 --input-path /path/to/model --quantization-type Q8_0"
                exit 0
                ;;
            *)
                log_error "未知选项: $1"
                echo "使用 --help 查看帮助信息"
                exit 1
                ;;
        esac
    done

    # 更新路径相关的变量
    GGUF_MODEL_PATH="${GGUF_OUTPUT_PATH}/llm_model.gguf"
    QUANTIZED_MODEL_PATH="${GGUF_OUTPUT_PATH}/llm_model_q4.gguf"
}

# 检查输入模型是否存在
check_input_model() {
    if [ ! -d "$INPUT_MODEL_PATH" ]; then
        log_error "输入模型路径不存在: $INPUT_MODEL_PATH"
        log_error "请确保已下载 PaddleOCR-VL 模型到指定路径"
        exit 1
    fi

    log_success "输入模型路径存在: $INPUT_MODEL_PATH"
}

# 激活虚拟环境
activate_venv() {
    if [ -d ".venv" ]; then
        log_info "激活虚拟环境..."
        source .venv/bin/activate
    else
        log_warning "未找到虚拟环境 (.venv)，将使用系统 Python"
    fi
}

# 安装Python依赖
install_dependencies() {
    log_info "安装 Python 依赖..."

    if [ ! -f "requirements.txt" ]; then
        log_error "requirements.txt 文件不存在"
        exit 1
    fi

    pip install -r requirements.txt
    pip install llama-cpp-python

    log_success "Python 依赖安装完成"
}

# 导出视觉模型
export_vision_model() {
    log_info "步骤 1/5: 导出视觉模型..."
    log_info "输入: $INPUT_MODEL_PATH"
    log_info "输出: $VISION_OUTPUT_PATH"

    if [ -d "$VISION_OUTPUT_PATH" ]; then
        log_warning "视觉模型输出目录已存在，跳过导出: $VISION_OUTPUT_PATH"
        return
    fi

    python export_vision_model.py \
        --input-path "$INPUT_MODEL_PATH" \
        --output-path "$VISION_OUTPUT_PATH"

    log_success "视觉模型导出完成"
}

# 导出语言模型
export_language_model() {
    log_info "步骤 2/5: 导出语言模型..."
    log_info "输入: $INPUT_MODEL_PATH"
    log_info "输出: $LLM_OUTPUT_PATH"

    if [ -d "$LLM_OUTPUT_PATH" ]; then
        log_warning "语言模型输出目录已存在，跳过导出: $LLM_OUTPUT_PATH"
        return
    fi

    python export_language_model.py \
        --input-path "$INPUT_MODEL_PATH" \
        --output-path "$LLM_OUTPUT_PATH"

    log_success "语言模型导出完成"
}

# 编译llama.cpp
build_llama_cpp() {
    log_info "步骤 3/5: 编译 llama.cpp..."

    if [ -d "llama.cpp" ]; then
        log_info "llama.cpp 已存在，检查是否已编译..."
        if [ -f "llama.cpp/bin/llama-quantize" ]; then
            log_success "llama.cpp 已编译，跳过"
            return
        fi
    else
        log_info "克隆 llama.cpp..."
        git clone https://github.com/ggml-org/llama.cpp
    fi

    log_info "编译 llama.cpp..."
    cd llama.cpp
    cmake . -DCMAKE_BUILD_TYPE=Release
    cmake --build . -j$(nproc)
    cd ..

    if [ ! -f "llama.cpp/bin/llama-quantize" ]; then
        log_error "llama.cpp 编译失败"
        exit 1
    fi

    log_success "llama.cpp 编译完成"
}

# 转换为GGUF格式
convert_to_gguf() {
    log_info "步骤 4/5: 转换为 GGUF 格式..."
    log_info "输入: $LLM_OUTPUT_PATH/hf_model"
    log_info "输出: $GGUF_MODEL_PATH"

    if [ -f "$GGUF_MODEL_PATH" ]; then
        log_warning "GGUF 文件已存在，跳过转换: $GGUF_MODEL_PATH"
        return
    fi

    # 创建输出目录
    mkdir -p "$GGUF_OUTPUT_PATH"

    python llama.cpp/convert_hf_to_gguf.py \
        "$LLM_OUTPUT_PATH/hf_model" \
        --outfile "$GGUF_MODEL_PATH" \
        --outtype f16

    if [ ! -f "$GGUF_MODEL_PATH" ]; then
        log_error "GGUF 转换失败"
        exit 1
    fi

    log_success "GGUF 转换完成"
}

# 量化模型
quantize_model() {
    log_info "步骤 5/5: 量化模型..."
    log_info "输入: $GGUF_MODEL_PATH"
    log_info "输出: $QUANTIZED_MODEL_PATH"
    log_info "量化类型: $QUANTIZATION_TYPE"

    if [ -f "$QUANTIZED_MODEL_PATH" ]; then
        log_warning "量化文件已存在，跳过量化: $QUANTIZED_MODEL_PATH"
        return
    fi

    ./llama.cpp/bin/llama-quantize \
        "$GGUF_MODEL_PATH" \
        "$QUANTIZED_MODEL_PATH" \
        "$QUANTIZATION_TYPE"

    if [ ! -f "$QUANTIZED_MODEL_PATH" ]; then
        log_error "模型量化失败"
        exit 1
    fi

    log_success "模型量化完成"
}

# 显示结果
show_results() {
    echo ""
    echo "========================================"
    log_success "🎉 全流程转换完成！"
    echo ""
    echo "📁 生成的文件:"
    echo "   视觉模型: $VISION_OUTPUT_PATH/"
    echo "   语言模型: $LLM_OUTPUT_PATH/"
    echo "   GGUF 模型: $GGUF_MODEL_PATH"
    echo "   量化模型: $QUANTIZED_MODEL_PATH"
    echo ""
    echo "🚀 启动服务器:"
    echo "   python demo_ppocrvl_gguf_server.py"
    echo ""
    echo "🧪 测试客户端:"
    echo "   python demo_ppocrvl_gguf_client.py --image test.png"
    echo "========================================"
}

# 主函数
main() {
    echo "========================================"
    echo "🚀 PaddleOCR-VL 全流程转换脚本"
    echo "========================================"

    parse_args "$@"
    check_dependencies
    check_input_model
    activate_venv
    install_dependencies

    # 执行转换流程
    export_vision_model
    export_language_model
    build_llama_cpp
    convert_to_gguf
    quantize_model

    show_results

    log_success "所有步骤完成！"
}

# 运行主函数
main "$@"