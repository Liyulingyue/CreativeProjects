import json
import re
from typing import Any, List, Optional, Tuple, Union, Dict

TOOL_MARKER_START = "```json"
TOOL_MARKER_END = "```"

class OpenAIJsonWrapper:
    """
    OpenAI 客户端封装，用于强制模型输出 JSON 并自动解析。
    用户只需提供数据结构定义（dict/list），框架会自动拼接 System Prompt 并解析返回。
    """

    def __init__(
        self, 
        client: Any, 
        model: str = "gpt-3.5-turbo", 
        target_structure: Optional[Any] = None,
        requirements: Optional[Union[str, List[str]]] = None,
        background: Optional[str] = None
    ):
        """
        :param client: OpenAI 风格的客户端对象 (需具备 chat.completions.create 方法)
        :param model: 默认使用的模型名称
        :param target_structure: 默认的输出结构定义
        :param requirements: 默认的特定需求说明
        :param background: 默认的背景信息
        """
        self.client = client
        self.model = model
        self.target_structure = target_structure
        self.requirements = requirements
        self.background = background

    def _build_system_prompt(self, target_structure: Any, requirements: Optional[Union[str, List[str]]] = None, background: Optional[str] = None) -> str:
        structure_str = json.dumps(target_structure, indent=2, ensure_ascii=False)
        prompt = (
            "You are a helpful assistant that MUST output your response in a specific JSON format.\n"
        )
        
        if background:
            prompt += f"\nBackground Information:\n{background}\n"
            
        prompt += f"\nThe required JSON structure is:\n{structure_str}\n\n"
        
        if requirements:
            if isinstance(requirements, list):
                req_text = "\n".join([f"- {r}" for r in requirements])
            else:
                req_text = requirements
            prompt += f"Specific Requirements:\n{req_text}\n\n"
            
        prompt += (
            "Rules:\n"
            f"1. Your final JSON data MUST be wrapped between '{TOOL_MARKER_START}' and '{TOOL_MARKER_END}' markdown blocks.\n"
            "2. Everything before the code block is considered your reasoning or conversational text.\n"
            "3. Ensure the JSON inside the block is valid and matches the requested structure strictly.\n"
        )
        return prompt

    def _parse_content(self, text: str) -> Tuple[str, Any, Optional[str]]:
        """
        解析模型文本中内嵌的 JSON 块。
        返回 (reasoning_content, parsed_data, error_message)
        """
        if not text:
            return "", None, "Empty content"

        # 处理思考链：如果存在 </think>，则只保留其之后的内容
        think_end_marker = "</think>"
        if think_end_marker in text:
            # 找到最后一个 </think> 的位置并截取其后的内容
            parts = text.split(think_end_marker)
            text = parts[-1].strip()

        # 1) 查找特定的 Markdown JSON 块
        # 使用正则匹配 ```json ... ```，尽量抓取内部内容
        md_pattern = r"```json\s*([\s\S]*?)\s*```"
        match = re.search(md_pattern, text)
        if match:
            inner = match.group(1).strip()
            reasoning = text[:match.start()].strip()
            try:
                parsed = json.loads(inner)
                return reasoning, parsed, None
            except Exception as e:
                # 如果标准加载失败，尝试简单的清理（比如去掉可能的尾随逗号）再解析
                try:
                    # 极简修复：去除非法字符
                    cleaned = re.sub(r',\s*([\]}])', r'\1', inner)
                    parsed = json.loads(cleaned)
                    return reasoning, parsed, None
                except:
                    return text, None, f"JSON parse error in markdown block: {e}"

        # 2) 备选：尝试从末尾提取最后一个 JSON 结构 (兼容没有标记的情况)
        for opener, closer in (("[", "]"), ("{", "}")):
            idx = text.rfind(opener)
            if idx != -1:
                cand = text[idx:].strip()
                # 寻找匹配的闭合符号位置
                last_closer = cand.rfind(closer)
                if last_closer != -1:
                    cand = cand[:last_closer+1]
                try:
                    parsed = json.loads(cand)
                    reasoning = text[:idx].strip()
                    return reasoning, parsed, None
                except Exception:
                    continue

        return text, None, "No JSON structure found"

    def chat(
        self, 
        messages: List[Dict[str, str]], 
        target_structure: Optional[Any] = None, 
        requirements: Optional[Union[str, List[str]]] = None,
        background: Optional[str] = None,
        model: Optional[str] = None,
        **kwargs
    ) -> Dict[str, Any]:
        """
        执行对话并自动解析结果。
        
        :param messages: 对话上下文
        :param target_structure: 希望获取的 JSON 模板/定义（若为 None 则使用实例初始化时的值）
        :param requirements: 特定需求说明 (str 或 list)
        :param background: 背景背景知识/上下文 (str)
        :param model: 模型名称（若为 None 则使用实例初始化时的值）
        :return: 包含 'reasoning', 'data', 'raw', 'error' 的字典
        """
        # 优先级：方法参数 > 初始化参数
        target = target_structure if target_structure is not None else self.target_structure
        reqs = requirements if requirements is not None else self.requirements
        bg = background if background is not None else self.background
        selected_model = model if model is not None else self.model

        if target is None:
            raise ValueError("target_structure must be provided either in __init__ or in chat()")

        # 复制消息列表并注入系统提示
        prompt = self._build_system_prompt(target, requirements=reqs, background=bg)
        
        new_messages = []
        has_system = False
        for m in messages:
            if m.get("role") == "system":
                new_messages.append({"role": "system", "content": f"{prompt}\n\n{m['content']}"})
                has_system = True
            else:
                new_messages.append(m)
        
        if not has_system:
            new_messages.insert(0, {"role": "system", "content": prompt})

        # 调用模型
        response = self.client.chat.completions.create(
            model=selected_model,
            messages=new_messages,
            **kwargs
        )

        # 获取 content
        choice = response.choices[0]
        content = choice.message.content or ""
        
        reasoning, data, error = self._parse_content(content)
        
        return {
            "reasoning": reasoning,
            "data": data,
            "error": error,
            "raw_content": content,
            "response_id": getattr(response, "id", None)
        }
