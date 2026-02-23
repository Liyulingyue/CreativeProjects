from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from app.routers import vocabulary

app = FastAPI(title="English Learning Helper API")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(vocabulary.router)

@app.get("/")
def root():
    return {"message": "English Learning Helper API", "status": "running"}
