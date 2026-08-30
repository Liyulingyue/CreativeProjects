"""核心领域模型：一份「测评」由维度、题目、计分规则和结果模板组成。

report_type 决定打分方式：
- type_matching: 维度是二极轴（如 E/I），各轴取极性字母，拼出类型码查表（MBTI / 猫格）
- dimension_profile: 维度按 0-100 分呈现画像（领导力等）
"""
from enum import Enum
from typing import Literal, Optional

from pydantic import BaseModel, Field


class Option(BaseModel):
    label: str  # "非常不同意" ... "非常同意"
    value: int  # 1-5，计分值（反向题在计分时取 6-value）


class Dimension(BaseModel):
    key: str                      # "EI", "SN" ... 或 "drive"
    name: str                     # 展示名，如 "能量取向"
    pole_high: str                # 高分极，如 "外向 E"
    pole_low: str = ""            # 低分极（type_matching 必填），如 "内向 I"
    description: str = ""


def standard_options() -> list[Option]:
    return [
        Option(label="非常不同意", value=1),
        Option(label="比较不同意", value=2),
        Option(label="一般", value=3),
        Option(label="比较同意", value=4),
        Option(label="非常同意", value=5),
    ]


class Question(BaseModel):
    id: str
    text: str
    dimension: str                # 指向 Dimension.key
    reverse: bool = False         # 反向计分
    options: list[Option] = Field(default_factory=standard_options)


class ResultType(BaseModel):
    """type_matching 报告中的一个类型，如 INFJ / 长老猫。"""
    code: str                     # "INFJ" 或 "E-" 组合占位
    name: str                     # "共情建筑师" / "哲学家猫"
    keywords: list[str] = Field(default_factory=list)
    description: str = ""
    advice: str = ""              # 饲养建议 / 发展建议


class Assessment(BaseModel):
    id: str
    name: str
    description: str = ""
    subject: Literal["human", "cat", "other"] = "human"
    report_type: Literal["type_matching", "dimension_profile"] = "type_matching"
    time_estimate_minutes: int = 10
    source: Literal["builtin", "ai_generated"] = "builtin"
    dimensions: list[Dimension]
    questions: list[Question]
    types: list[ResultType] = Field(default_factory=list)  # type_matching 用
    profile_levels: dict[str, list[str]] = Field(default_factory=dict)  # dimension_profile 用：维度->三档描述

    def dimension_map(self) -> dict[str, Dimension]:
        return {d.key: d for d in self.dimensions}

    def type_by_code(self, code: str) -> Optional[ResultType]:
        for t in self.types:
            if t.code == code:
                return t
        return None


class Answer(BaseModel):
    question_id: str
    value: int


class SessionState(BaseModel):
    session_id: str
    assessment_id: str
    status: Literal["ongoing", "finished"] = "ongoing"
    answers: list[Answer] = Field(default_factory=list)
    current_index: int = 0        # 下一道待答题的序号
    started_at: str = ""
    finished_at: Optional[str] = None


class DimensionScore(BaseModel):
    key: str
    name: str
    raw: float                    # 原始均分 1-5
    percent: int                  # 0-100
    pole: str                     # 命中的极 / 档位描述


class Report(BaseModel):
    session_id: str
    assessment_id: str
    assessment_name: str
    type_code: Optional[str] = None
    type_name: Optional[str] = None
    keywords: list[str] = Field(default_factory=list)
    type_description: str = ""
    advice: str = ""
    dimension_scores: list[DimensionScore] = Field(default_factory=list)
    narrative: str = ""           # LLM 生成的深度解读（无 LLM 时为模板文案）
    narrative_source: Literal["llm", "template"] = "template"
