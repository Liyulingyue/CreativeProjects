import requests
import json

# Test the FastAPI rerank endpoint
def test_rerank_api():
    url = "http://localhost:10053/v1/rerank"

    # Test data
    data = {
        "model": "mixedbread-ai/mxbai-rerank-base-v1",
        "query": "Who wrote 'To Kill a Mockingbird'?",
        "documents": [
            "'To Kill a Mockingbird' is a novel by Harper Lee published in 1960. It was immediately successful, winning the Pulitzer Prize, and has become a classic of modern American literature.",
            "The novel 'Moby-Dick' was written by Herman Melville and first published in 1851. It is considered a masterpiece of American literature and deals with complex themes of obsession, revenge, and the conflict between good and evil.",
            "Harper Lee, an American novelist widely known for her novel 'To Kill a Mockingbird', was born in 1926 in Monroeville, Alabama. She received the Pulitzer Prize for Fiction in 1961.",
            "Jane Austen was an English novelist known primarily for her six major novels, which interpret, critique and comment upon the British landed gentry at the end of the 18th century.",
            "The 'Harry Potter' series, which consists of seven fantasy novels written by British author J.K. Rowling, is among the most popular and critically acclaimed books of the modern era.",
            "'The Great Gatsby', a novel written by American author F. Scott Fitzgerald, was published in 1925. The story is set in the Jazz Age and follows the life of millionaire Jay Gatsby and his pursuit of Daisy Buchanan."
        ],
        "parameters": {
            "return_documents": True,
            "top_k": 3
        }
    }

    try:
        response = requests.post(url, json=data)
        if response.status_code == 200:
            result = response.json()
            print("API Response:")
            print(json.dumps(result, indent=2))
        else:
            print(f"Error: {response.status_code}")
            print(response.text)
    except requests.exceptions.ConnectionError:
        print("Connection error: Make sure the FastAPI server is running on localhost:10053")

if __name__ == "__main__":
    test_rerank_api()