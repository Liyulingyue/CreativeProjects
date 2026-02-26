from fastapi import APIRouter, UploadFile, File, Body
import base64

router = APIRouter(prefix="/ocr", tags=["ocr"])

@router.post("/v5")
async def ocr_v5(file: UploadFile = File(...)):
    from app.services.ocr_service import ocr_v5_image
    
    file_data = await file.read()
    encoded = base64.b64encode(file_data).decode("ascii")
    
    return ocr_v5_image(encoded)

@router.post("/vl")
async def ocr_vl(file: UploadFile = File(...)):
    from app.services.ocr_service import ocr_vl_image
    
    file_data = await file.read()
    encoded = base64.b64encode(file_data).decode("ascii")
    
    return ocr_vl_image(encoded)

@router.post("/structure")
async def structure_image(file: UploadFile = File(...)):
    from app.services.ocr_service import structure_image
    
    file_data = await file.read()
    encoded = base64.b64encode(file_data).decode("ascii")
    
    return structure_image(encoded)

@router.post("/convert")
async def convert_ocr(texts: list[str] = Body(...)):
    from app.services.ai_service import convert_to_vocabulary
    
    return convert_to_vocabulary(texts)
