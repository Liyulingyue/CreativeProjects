from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse
from pydantic import BaseModel
from typing import List, Optional, Dict, Any
import uuid
from sentence_transformers import CrossEncoder
import logging

# Set up logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

app = FastAPI(title="Rerank API", description="OpenAI-compatible rerank API using local model")

# Load the model
model = CrossEncoder("mixedbread-ai/mxbai-rerank-base-v1")

class RerankRequest(BaseModel):
    model: str = "mixedbread-ai/mxbai-rerank-base-v1"
    query: str
    documents: List[str]
    parameters: Optional[Dict[str, Any]] = {"return_documents": True}

class RerankResult(BaseModel):
    index: int
    relevance_score: float
    document: Optional[Dict[str, str]] = None

class RerankResponse(BaseModel):
    id: str
    object: str = "list"
    model: str
    usage: Dict[str, Any] = {}
    results: List[RerankResult]

@app.post("/v1/rerank", response_model=RerankResponse)
async def rerank(request: Request, req_data: RerankRequest):
    """OpenAI-compatible rerank endpoint"""
    try:
        # Log request details
        logger.info(f"Request method: {request.method}")
        logger.info(f"Request URL: {request.url}")
        logger.info(f"Request headers: {dict(request.headers)}")
        logger.info(f"Request body: {await request.body()}")

        # Extract parameters
        query = req_data.query
        documents = req_data.documents
        return_documents = req_data.parameters.get("return_documents", True) if req_data.parameters else True
        top_k = req_data.parameters.get("top_k", len(documents)) if req_data.parameters else len(documents)

        logger.info(f"Query: {query}")
        logger.info(f"Number of documents: {len(documents)}")
        logger.info(f"Return documents: {return_documents}")
        logger.info(f"Top K: {top_k}")

        # Call the model
        results = model.rank(query, documents, return_documents=return_documents, top_k=top_k)

        logger.info(f"Model results: {results}")

        # Convert results to OpenAI format
        openai_results = []
        for i, item in enumerate(results):
            result = RerankResult(
                index=item['corpus_id'],
                relevance_score=float(item['score'])
            )
            if return_documents:
                result.document = {"text": item['text']}
            openai_results.append(result)

        # Create response
        response = RerankResponse(
            id=f"rerank-{uuid.uuid4().hex}",
            model=req_data.model,
            results=openai_results
        )

        logger.info(f"Response: {response.dict()}")

        return response

    except Exception as e:
        logger.error(f"Error: {str(e)}", exc_info=True)
        raise HTTPException(status_code=500, detail=str(e))

@app.get("/health")
async def health():
    """Health check endpoint"""
    return {"status": "ok"}

@app.api_route("/{path:path}", methods=["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"])
async def catch_all(request: Request, path: str):
    """Catch all unmatched routes to log them"""
    logger.warning(f"Unmatched request - Method: {request.method}, Path: /{path}, URL: {request.url}")
    logger.warning(f"Headers: {dict(request.headers)}")
    try:
        body = await request.body()
        if body:
            logger.warning(f"Body: {body.decode('utf-8', errors='ignore')}")
    except Exception as e:
        logger.warning(f"Could not read body: {e}")

    
    return JSONResponse(
        status_code=404,
        content={"error": {"message": f"Endpoint /{path} not found", "code": 404}}
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=10053)