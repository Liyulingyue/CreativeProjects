from fastapi import APIRouter, UploadFile, File, Body
import base64

router = APIRouter(prefix="/ocr", tags=["ocr"])

@router.post("/image")
async def ocr_image(file: UploadFile = File(...)):
    from app.services.ocr_service import ocr_image
    
    file_data = await file.read()
    encoded = base64.b64encode(file_data).decode("ascii")
    
    return ocr_image(encoded)

@router.post("/convert")
async def convert_ocr(texts: list[str] = Body(...)):
    from app.services.ai_service import convert_to_vocabulary
    
    return convert_to_vocabulary(texts)
