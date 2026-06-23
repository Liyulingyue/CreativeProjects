import os
import sys
import time
import json
from pathlib import Path
from typing import Optional
from dataclasses import dataclass, field, asdict

import dotenv
dotenv.load_dotenv()

from openaijsonwrapper import OpenAIJsonWrapper
from openai import OpenAI

from .config import (
    API_KEY, BASE_URL, MODEL_NAME,
    DEFAULT_TARGET_STRUCTURE, DEFAULT_BACKGROUND, DEFAULT_REQUIREMENTS,
    get_image_files, is_image_file
)


@dataclass
class AnalysisResult:
    file_path: str
    file_name: str
    error: Optional[str] = None
    data: Optional[dict] = None
    reasoning: Optional[str] = None
    raw_content: Optional[str] = None
    success: bool = False

    def to_dict(self) -> dict:
        d = asdict(self)
        d["success"] = self.success
        return d


class PhotoAnalyzer:
    def __init__(
        self,
        api_key: Optional[str] = None,
        base_url: Optional[str] = None,
        model: Optional[str] = None,
        target_structure: Optional[dict] = None,
        background: Optional[str] = None,
        requirements: Optional[list] = None,
        delay_between_requests: float = 1.0,
        max_retries: int = 3,
        retry_delay: float = 5.0,
    ):
        self.client = OpenAI(
            api_key=api_key or API_KEY,
            base_url=base_url or BASE_URL,
        )
        self.model = model or MODEL_NAME
        self.target_structure = target_structure or DEFAULT_TARGET_STRUCTURE
        self.background = background or DEFAULT_BACKGROUND
        self.requirements = requirements or DEFAULT_REQUIREMENTS
        self.delay = delay_between_requests
        self.max_retries = max_retries
        self.retry_delay = retry_delay

        self.wrapper = OpenAIJsonWrapper(
            self.client,
            model=self.model,
            target_structure=self.target_structure,
            background=self.background,
            requirements=self.requirements,
        )

    def analyze_image(self, image_path: str | Path) -> AnalysisResult:
        image_path = Path(image_path)
        result = AnalysisResult(
            file_path=str(image_path.absolute()),
            file_name=image_path.name,
        )

        if not image_path.exists():
            result.error = f"文件不存在: {image_path}"
            return result

        if not is_image_file(image_path):
            result.error = f"不支持的图片格式: {image_path.suffix}"
            return result

        messages = [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "请仔细观察这张图片，按指定 JSON 结构输出。"},
                    {"type": "image_path", "image_path": str(image_path.absolute())},
                ],
            }
        ]

        for attempt in range(self.max_retries):
            try:
                response = self.wrapper.chat(messages=messages)

                if not response["error"]:
                    result.data = response["data"]
                    result.reasoning = response.get("reasoning")
                    result.success = True
                    return result
                else:
                    result.raw_content = response.get("raw_content")
                    if attempt < self.max_retries - 1:
                        time.sleep(self.retry_delay)
                        continue
                    result.error = response["error"]
                    return result

            except Exception as e:
                if attempt < self.max_retries - 1:
                    time.sleep(self.retry_delay)
                    continue
                result.error = str(e)
                return result

        return result

    def analyze_folder(
        self,
        folder_path: str | Path,
        recursive: bool = True,
        extensions: Optional[set] = None,
    ) -> list[AnalysisResult]:
        folder_path = Path(folder_path)
        if not folder_path.exists() or not folder_path.is_dir():
            return [AnalysisResult(
                file_path=str(folder_path),
                file_name=folder_path.name,
                error=f"文件夹不存在或不是有效目录: {folder_path}",
            )]

        if recursive:
            image_files = list(folder_path.rglob("*"))
            image_files = [f for f in image_files if f.is_file() and is_image_file(f)]
        else:
            image_files = [f for f in folder_path.iterdir() if f.is_file() and is_image_file(f)]

        results = []
        for img_path in image_files:
            result = self.analyze_image(img_path)
            results.append(result)
            if result.success:
                print(f"[OK] {img_path.name}")
            else:
                print(f"[FAIL] {img_path.name}: {result.error}")

            if self.delay > 0:
                time.sleep(self.delay)

        return results


class BatchPhotoAnalyzer:
    def __init__(self, analyzer: PhotoAnalyzer):
        self.analyzer = analyzer

    def analyze_paths(
        self,
        paths: list[str | Path],
    ) -> list[AnalysisResult]:
        results = []
        for p in paths:
            result = self.analyzer.analyze_image(p)
            results.append(result)
            if self.analyzer.delay > 0:
                time.sleep(self.analyzer.delay)
        return results
