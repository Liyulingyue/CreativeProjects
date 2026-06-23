import sys
import json
import csv
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from src.exporter import convert_jsonl_to_csv

if __name__ == "__main__":
    jsonl_path = "analysis.jsonl"
    output_path = convert_jsonl_to_csv(jsonl_path)
    print(f"已转换为: {output_path}")
