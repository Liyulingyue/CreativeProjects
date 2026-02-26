import os
import base64
import requests
from dotenv import load_dotenv

load_dotenv()

OCR_V5_URL = "https://fanbt5bfa2mddd7c.aistudio-app.com/ocr"
OCR_VL_URL = "https://a2r0ua22x5k5f2lb.aistudio-app.com/layout-parsing"
STRUCTURE_URL = "https://q9a2r4uekfhdb0s9.aistudio-app.com/layout-parsing"
TOKEN = os.getenv("BAIDU_AISTUDIO_KEY", "")

def call_ocr_api(url: str, file_data: str, optional_payload: dict = None) -> dict:
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
    
    payload = {**required_payload, **(optional_payload or {})}
    
    response = requests.post(url, json=payload, headers=headers)
    
    if response.status_code == 200:
        return response.json()
    else:
        return {"error": f"API failed: {response.status_code}", "detail": response.text}

def ocr_v5_image(file_data: str) -> dict:
    result = call_ocr_api(OCR_V5_URL, file_data, {
        "useDocOrientationClassify": False,
        "useDocUnwarping": False,
        "useTextlineOrientation": False,
    })
    
    if "error" in result:
        return result
    
    ocr_results = []
    for res in result.get("result", {}).get("ocrResults", []):
        pruned = res.get("prunedResult", {})
        rec_texts = pruned.get("rec_texts", [])
        ocr_results.append({
            "texts": rec_texts,
            "image_url": res.get("ocrImage", "")
        })
    return {"results": ocr_results}

def ocr_vl_image(file_data: str) -> dict:
    result = call_ocr_api(OCR_VL_URL, file_data, {
        "useDocOrientationClassify": False,
        "useDocUnwarping": False,
        "useChartRecognition": False,
    })
    
    if "error" in result:
        return result
    
    parsing_results = []
    for res in result.get("result", {}).get("layoutParsingResults", []):
        parsing_results.append({
            "markdown": res.get("markdown", {}).get("text", ""),
            "type": res.get("type", ""),
        })
    return {"results": parsing_results}

def structure_image(file_data: str) -> dict:
    result = call_ocr_api(STRUCTURE_URL, file_data, {
        "useDocOrientationClassify": False,
        "useDocUnwarping": False,
        "useTextlineOrientation": False,
        "useChartRecognition": False,
    })
    
    if "error" in result:
        return result
    
    parsing_results = []
    for res in result.get("result", {}).get("layoutParsingResults", []):
        parsing_results.append({
            "markdown": res.get("markdown", {}).get("text", ""),
            "type": res.get("type", ""),
        })
    return {"results": parsing_results}
