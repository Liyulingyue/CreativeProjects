from fastapi import FastAPI, APIRouter
from fastapi.middleware.cors import CORSMiddleware
from app.routers import vocabulary
from app.routers import article
from app.routers import ocr

app = FastAPI(title="English Learning Helper API")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

api_router = APIRouter(prefix="/api/v1")
api_router.include_router(vocabulary.router)
api_router.include_router(article.router)
api_router.include_router(ocr.router)

app.include_router(api_router)

@app.get("/")
def root():
    return {"message": "English Learning Helper API", "status": "running"}
