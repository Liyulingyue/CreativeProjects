import requests
import json
import sys

def test_inference(url="http://127.0.0.1:5000/inference", prompt="今天天气真好，", max_tokens=20):
    print(f"[v=2026-04-29T19:40:00] Testing inference: prompt={prompt!r}, max_tokens={max_tokens}")
    resp = requests.post(url, json={"prompt": prompt, "max_tokens": max_tokens}, timeout=120)
    data = resp.json()
    if resp.status_code != 200:
        print(f"  FAIL: {data}")
        return False
    print(f"  OK: {data}")
    return True

if __name__ == "__main__":
    ok = test_inference()
    sys.exit(0 if ok else 1)
