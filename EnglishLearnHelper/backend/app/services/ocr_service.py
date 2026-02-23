import os
import base64
import requests
from dotenv import load_dotenv

load_dotenv()

API_URL = "https://fanbt5bfa2mddd7c.aistudio-app.com/ocr"
TOKEN = os.getenv("BAIDU_AISTUDIO_KEY", "")

def ocr_image(file_data: str) -> dict:
    if not TOKEN:
        return {"error": "BAIDU_AISTUDIO_KEY not configured"}
    
    headers = {
        "Authorization": f"token {TOKEN}",
        "Content-Type": "application/json"
    }
    
    required_payload = {
        "file": file_data,
        "fileType": 1
    }
    
    optional_payload = {
        "useDocOrientationClassify": False,
        "useDocUnwarping": False,
        "useTextlineOrientation": False,
    }
    
    payload = {**required_payload, **optional_payload}
    
    print(f"[OCR] Sending request to {API_URL}")
    response = requests.post(API_URL, json=payload, headers=headers)
    print(f"[OCR] Response status: {response.status_code}")
    
    if response.status_code == 200:
        result = response.json()
        
        ocr_results = []
        for res in result.get("result", {}).get("ocrResults", []):
            pruned = res.get("prunedResult", {})
            rec_texts = pruned.get("rec_texts", [])
            ocr_results.append({
                "texts": rec_texts,
                "image_url": res.get("ocrImage", "")
            })
        return {"results": ocr_results}
    else:
        return {"error": f"OCR failed: {response.status_code}", "detail": response.text}
