# 🚀 MiniCoder 快速启动指南

## 🎉 项目已就绪！

MiniCoder第二阶段开发已完成，包含完整的后端服务和前端界面。

## 📋 前置要求

### 后端
- Python 3.8+
- OpenAI API密钥 (可选，用于真实API调用)

### 前端
- Node.js 14+
- npm 或 yarn

## 🔧 安装步骤

### 1. 后端安装
```bash
cd ~/Codes/CreativeProjects/AgentLearn/MiniCoder

# 安装依赖
pip3 install -r requirements.txt

# 配置API密钥 (可选)
cp .env.example .env
# 编辑 .env 文件，添加:
# OPENAI_API_KEY=your_api_key_here
```

### 2. 前端安装
```bash
cd ~/Codes/CreativeProjects/AgentLearn/mini-coder-web

# 安装依赖
npm install

# 启动开发服务器
npm start
# 访问 http://localhost:3000
```

## 🎮 使用方法

### 后端 (命令行)
```bash
cd ~/Codes/CreativeProjects/AgentLearn/MiniCoder
python3 mini_coder.py
```

**交互菜单**:
1. 生成代码 - 根据描述生成代码
2. 解释代码 - 详细解释代码逻辑
3. 修复bug - 分析错误并提供修复方案
4. 优化代码 - 改进代码性能
5. 退出 - 退出程序

### 前端 (浏览器)
访问 http://localhost:3000

**功能切换**:
- ✨ 生成代码 - 输入描述和语言，点击执行
- 📚 解释代码 - 粘贴代码，点击执行
- 🔧 修复bug - 输入错误和代码，点击执行
- ⚡ 优化代码 - 粘贴代码，点击执行

## 🧪 运行测试

```bash
cd ~/Codes/CreativeProjects/AgentLearn/MiniCoder
python3 test_mini_coder.py
```

预期输出:
```
✅ test_basic_functionality
✅ test_code_generation
✅ test_code_explanation
✅ test_bug_fix
✅ test_code_optimization
✅ test_tools
✅ test_api_integration
```

## 💡 使用示例

### 示例1: 生成代码
```
选择功能: 1 (生成代码)
提示: 创建一个快速排序算法
语言: python

输出:
# 快速排序算法实现
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    left = [x for x in arr if x < pivot]
    middle = [x for x in arr if x == pivot]
    right = [x for x in arr if x > pivot]
    return quicksort(left) + middle + quicksort(right)
```

### 示例2: 解释代码
```
选择功能: 2 (解释代码)
输入代码: def hello(): print("Hello World")

输出:
这段代码定义了一个名为hello的函数:
- def hello(): 定义函数
- print("Hello World") 打印消息
调用hello()将输出: Hello World
```

### 示例3: 修复bug
```
选择功能: 3 (修复bug)
错误信息: IndexError: list index out of range
代码: arr = [1, 2, 3]; print(arr[5])

输出:
错误分析: 尝试访问索引5，但列表只有3个元素(索引0-2)
修复方案:
# 方法1: 检查索引范围
if index < len(arr):
    print(arr[index])

# 方法2: 使用try-except
try:
    print(arr[index])
except IndexError:
    print("索引超出范围")
```

### 示例4: 优化代码
```
选择功能: 4 (优化代码)
代码: for i in range(len(arr)): print(arr[i])

输出:
优化建议:
# 使用enumerate更Pythonic
for i, item in enumerate(arr):
    print(item)

# 或者直接迭代
for item in arr:
    print(item)
```

## 🔌 API配置

如需使用真实OpenAI API，请:

1. 获取API密钥: https://platform.openai.com/api-keys
2. 创建 `.env` 文件:
```env
OPENAI_API_KEY=sk-your-api-key-here
OPENAI_MODEL=gpt-4
```

## 📁 项目结构

```
MiniCoder/
├── mini_coder.py       # 主程序
├── tools.py            # 工具函数
├── test_mini_coder.py  # 测试
├── README.md           # 文档
├── requirements.txt    # 依赖
└── .env.example        # 环境变量模板

mini-coder-web/
├── src/
│   ├── components/     # React组件
│   ├── services/       # API服务
│   ├── types/          # TypeScript类型
│   ├── App.tsx         # 主应用
│   └── index.tsx       # 入口
└── package.json        # 前端配置
```

## 🆘 故障排除

### 后端问题
- **ModuleNotFoundError**: 运行 `pip3 install -r requirements.txt`
- **API错误**: 检查 `.env` 中的API密钥

### 前端问题
- **端口占用**: 杀掉占用3000端口的进程
- **依赖问题**: 删除node_modules并重新 `npm install`

## 📞 获取帮助

- 查看文档: `README.md`
- 运行测试: `python3 test_mini_coder.py`
- 查看进度: `PROGRESS.md`
- 完整总结: `MINICODER_SUMMARY.md`

## 🎉 开始使用

现在就开始使用MiniCoder吧！

```bash
# 启动后端
cd ~/Codes/CreativeProjects/AgentLearn/MiniCoder
python3 mini_coder.py

# 启动前端
cd ~/Codes/CreativeProjects/AgentLearn/mini-coder-web
npm start
```

享受智能编码助手带来的便利！🚀
