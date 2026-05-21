import json
from typing import Any, Dict, List, Optional
from openaijsonwrapper import OpenAIJsonWrapper

class QAGenerator:
    """
    自适应 QA 生成器 (QA Generator)。
    专注于生成逻辑，可以通过外部提供动态需求。
    """
    
    def __init__(
        self,
        client: Any,
        model: str = "gpt-4o",
        background: Optional[str] = None,
        base_requirements: Optional[List[str]] = None,
        output_path: Optional[str] = None,
        target_structure: Optional[List[Dict[str, Any]]] = None
    ):
        self.background = background or (
            "你是一个专业的知识库问答生成专家。你必须【严格基于提供的参考资料】生成问答对。\n"
            "严禁脱离原文瞎编或引入无关的外部常识。你的目标是作为'原文复述与深度解析者'，"
            "将长文本中隐藏的细节、数据、逻辑因果链条挖掘出来并转化为结构化 QA。"
        )
        self.base_requirements = base_requirements or [
            "1. 【严格准则】：所有生成的 Q 和 A 必须能从【参考资料】中找到原文支撑或逻辑推导，禁止编造事实。",
            "2. 【逻辑扩展】：针对中/长问答，要求答案必须包含：[原文事实描述] + [逻辑原理拆解] + [影响/结论总结]。",
            "3. 【细节密度】：长篇 QA 必须包含原文中至少 3 个以上的具体数值、专有名词 or 特定场景描述。",
            "4. 【字数与任务标准】：",
            "   - [短篇/简练型]: 10-50字。目标：定义、事实。格式：单刀直入，一句话回答。",
            "   - [中篇/精炼型]: 110-200字。目标：原理解析、步骤说明。格式：逻辑连贯，4-6句话。",
            "   - [长篇/详尽型]: 300-600字。目标：综合综述、长篇总结。格式：必须分段，展现完整的知识脉络，字数不足将视为不合格。",
            "5. 务必根据【动态要求】给出的风格建议进行生成。"
        ]
        
        self.output_path = output_path
        # 允许外部配置目标结构，默认仅 Q/A
        self.target_structure = target_structure or [{"Q": "问题描述", "A": "最终给出的答案"}]
        self.wrapper = OpenAIJsonWrapper(
            client, 
            model=model, 
            target_structure=self.target_structure,
            requirements=self.base_requirements,
            background=self.background
        )
        self.all_qas = []

    def generate_single(self, text: str, dynamic_requirements: Optional[List[str]] = None, **kwargs) -> List[Dict[str, str]]:
        """
        针对单段文本生成 QA。
        """
        result = self.wrapper.chat(
            messages=[{"role": "user", "content": f"【参考资料如下】：\n---\n{text}\n---\n基于上述参考资料进行任务处理。"}] ,
            extra_requirements=dynamic_requirements,
            **kwargs
        )
        
        qas = result.get("data") or []
        if isinstance(qas, dict):
            qas = [qas]
        
        if qas:
            self.all_qas.extend(qas)
            # 如果指定了输出路径，则立即追加写入文件
            if self.output_path:
                self._append_to_file(qas)
                
        return qas

    def _append_to_file(self, qas: List[Dict[str, str]]):
        """流式追加写入文件"""
        with open(self.output_path, "a", encoding="utf-8") as f:
            for qa in qas:
                f.write(json.dumps(qa, ensure_ascii=False) + "\n")

    def save_to_file(self, path: str = None):
        """保存所有 QA。如果已经流式写入过，则此操作通常是可选的或用于另存为。"""
        target_path = path or self.output_path
        if not target_path:
            return
            
        with open(target_path, "w", encoding="utf-8") as f:
            for qa in self.all_qas:
                f.write(json.dumps(qa, ensure_ascii=False) + "\n")
