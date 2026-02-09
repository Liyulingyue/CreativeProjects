import os
from sentence_transformers import SentenceTransformer

MODEL_NAME = 'all-MiniLM-L6-v2'
MODEL_PATH = os.path.join('models', MODEL_NAME)

def download_model():
    """Download and save the SentenceTransformer model locally."""
    if not os.path.exists(MODEL_PATH):
        print(f"🔄 Downloading model '{MODEL_NAME}' to '{MODEL_PATH}'...")
        # This will download the model and then we save it to the specific path
        model = SentenceTransformer(MODEL_NAME)
        model.save(MODEL_PATH)
        print(f"✅ Model saved locally to {MODEL_PATH}")
    else:
        print(f"✨ Model already exists at {MODEL_PATH}")

if __name__ == "__main__":
    download_model()
