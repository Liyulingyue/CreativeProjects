"""search_utils.py - Utility functions for file searching in MiniRAG"""
import os
from pathlib import Path
import numpy as np
from sentence_transformers import SentenceTransformer
import faiss
import pickle

# Tool definitions
SEARCH_FILES_TOOL = {
    "type": "function",
    "function": {
        "name": "search_files",
        "description": "Search for keywords in file contents within the knowledge base",
        "parameters": {
            "type": "object",
            "properties": {
                "keyword": {"type": "string", "description": "The keyword to search for in file contents"}
            },
            "required": ["keyword"]
        }
    }
}

SEARCH_FILENAMES_TOOL = {
    "type": "function",
    "function": {
        "name": "search_filenames",
        "description": "Search for files by filename containing the keyword",
        "parameters": {
            "type": "object",
            "properties": {
                "keyword": {"type": "string", "description": "The keyword to search for in filenames"}
            },
            "required": ["keyword"]
        }
    }
}

SEMANTIC_SEARCH_TOOL = {
    "type": "function",
    "function": {
        "name": "semantic_search",
        "description": "Perform semantic search using vector similarity to find relevant content in knowledge base",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "The semantic query to search for"}
            },
            "required": ["query"]
        }
    }
}

# Global variables for semantic search
model = None
index = None
documents = []
file_paths = []
INDEX_FILE = "semantic_index.faiss"
DOCUMENTS_FILE = "documents.pkl"
def save_index():
    """Save the current index and documents to disk"""
    if index is not None and documents and file_paths:
        faiss.write_index(index, INDEX_FILE)
        with open(DOCUMENTS_FILE, 'wb') as f:
            pickle.dump(documents, f)
        with open(FILE_PATHS_FILE, 'wb') as f:
            pickle.dump(file_paths, f)
        print(f"✅ Index saved to {INDEX_FILE}, {DOCUMENTS_FILE}, {FILE_PATHS_FILE}")

def load_index():
    """Load index and documents from disk if available"""
    global index, documents, file_paths
    if os.path.exists(INDEX_FILE) and os.path.exists(DOCUMENTS_FILE) and os.path.exists(FILE_PATHS_FILE):
        try:
            index = faiss.read_index(INDEX_FILE)
            with open(DOCUMENTS_FILE, 'rb') as f:
                documents = pickle.load(f)
            with open(FILE_PATHS_FILE, 'rb') as f:
                file_paths = pickle.load(f)
            print(f"✅ Loaded existing index with {len(documents)} chunks")
            return True
        except Exception as e:
            print(f"⚠️ Failed to load index: {e}")
            return False
    return False

def initialize_semantic_search():
    """Initialize the semantic search model and index"""
    global model, index, documents, file_paths
    
    # Try to load existing index first
    if load_index():
        return
    
    # If no saved index, build from scratch
    if model is None:
        print("🔄 Loading semantic search model...")
        model = SentenceTransformer('all-MiniLM-L6-v2')
        print("✅ Model loaded")
    
    knowledge_path = Path("knowledge")
    if knowledge_path.exists() and not documents:
        print("🔄 Processing knowledge base documents...")
        file_count = 0
        for file_path in knowledge_path.rglob("*"):
            if file_path.is_file() and file_path.suffix.lower() in ['.txt', '.md', '.py', '.json']:
                try:
                    with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                        content = f.read()
                        if content.strip():
                            # Split into chunks (simple approach)
                            chunks = [content[i:i+500] for i in range(0, len(content), 400)]
                            for chunk in chunks:
                                if chunk.strip():
                                    documents.append(chunk)
                                    file_paths.append(str(file_path))
                except Exception as e:
                    continue
                file_count += 1
                if file_count % 10 == 0:  # Progress every 10 files
                    print(f"📄 Processed {file_count} files...")
        
        if documents:
            print(f"🔄 Encoding {len(documents)} text chunks...")
            embeddings = model.encode(documents)
            print("🔄 Building search index...")
            # Create FAISS index
            dimension = embeddings.shape[1]
            index = faiss.IndexFlatIP(dimension)  # Inner product for cosine similarity
            # Normalize embeddings for cosine similarity
            faiss.normalize_L2(embeddings)
            index.add(embeddings)
            print(f"✅ Semantic search ready! Indexed {len(documents)} chunks from {file_count} files")
            
            # Auto-save the index
            save_index()
        else:
            print("⚠️  No documents found in knowledge base")

def semantic_search(query, top_k=5):
    """
    Perform semantic search using vector similarity.
    
    Args:
        query (str): The semantic query
        top_k (int): Number of top results to return
        
    Returns:
        str: Search results with relevant content and file paths
    """
    initialize_semantic_search()
    if not documents or index is None:
        return "(no documents available for semantic search)"
    
    # Encode query
    query_embedding = model.encode([query])
    faiss.normalize_L2(query_embedding)
    
    # Search
    scores, indices = index.search(query_embedding, top_k)
    
    results = []
    for score, idx in zip(scores[0], indices[0]):
        if idx < len(documents) and score > 0.1:  # Threshold for relevance
            results.append(f"Score: {score:.3f} | {file_paths[idx]}: {documents[idx][:200]}...")
    
    return '\n'.join(results) if results else "(no relevant matches found)"

def search_files(keyword):
    """
    Search for keyword in file contents within knowledge folder.
    
    Args:
        keyword (str): The keyword to search for
        
    Returns:
        str: Search results with file paths, line numbers, and matching lines
    """
    results = []
    keyword_lower = keyword.lower()
    knowledge_path = Path("knowledge")
    if knowledge_path.exists():
        for file_path in knowledge_path.rglob("*"):
            if file_path.is_file():
                try:
                    with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                        content = f.read()
                        lines = content.split('\n')
                        for line_num, line in enumerate(lines, 1):
                            if keyword_lower in line.lower():
                                results.append(f"{file_path}:{line_num}:{line.strip()}")
                except Exception as e:
                    continue
    return '\n'.join(results) if results else "(no matches found)"

def search_filenames(keyword):
    """
    Search for files by filename containing the keyword.
    
    Args:
        keyword (str): The keyword to search for in filenames
        
    Returns:
        str: List of matching filenames
    """
    results = []
    keyword_lower = keyword.lower()
    knowledge_path = Path("knowledge")
    if knowledge_path.exists():
        for file_path in knowledge_path.rglob("*"):
            if file_path.is_file():
                if keyword_lower in file_path.name.lower():
                    results.append(f"Filename match: {file_path}")
    return '\n'.join(results) if results else "(no filename matches found)"

if __name__ == "__main__":
    print("🔄 Pre-building semantic search index...")
    initialize_semantic_search()
    print("✅ Pre-build complete! Index saved for fast loading.")