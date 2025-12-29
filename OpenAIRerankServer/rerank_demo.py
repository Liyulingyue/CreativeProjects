from sentence_transformers import CrossEncoder
import pprint

# Load the model, here we use our base sized model
model = CrossEncoder("mixedbread-ai/mxbai-rerank-base-v1")


# Example query and documents
query = "Who wrote 'To Kill a Mockingbird'?"
documents = [
    "'To Kill a Mockingbird' is a novel by Harper Lee published in 1960. It was immediately successful, winning the Pulitzer Prize, and has become a classic of modern American literature.",
    "The novel 'Moby-Dick' was written by Herman Melville and first published in 1851. It is considered a masterpiece of American literature and deals with complex themes of obsession, revenge, and the conflict between good and evil.",
    "Harper Lee, an American novelist widely known for her novel 'To Kill a Mockingbird', was born in 1926 in Monroeville, Alabama. She received the Pulitzer Prize for Fiction in 1961.",
    "Jane Austen was an English novelist known primarily for her six major novels, which interpret, critique and comment upon the British landed gentry at the end of the 18th century.",
    "The 'Harry Potter' series, which consists of seven fantasy novels written by British author J.K. Rowling, is among the most popular and critically acclaimed books of the modern era.",
    "'The Great Gatsby', a novel written by American author F. Scott Fitzgerald, was published in 1925. The story is set in the Jazz Age and follows the life of millionaire Jay Gatsby and his pursuit of Daisy Buchanan."
]

# Lets get the scores
results = model.rank(query, documents, return_documents=True, top_k=3)

# Print the structure of results
print("Type of results:", type(results))
print("Results content:")
pprint.pprint(results)

"""
[{'corpus_id': 0,
  'score': np.float32(0.9968497),
  'text': "'To Kill a Mockingbird' is a novel by Harper Lee published in 1960. "
          'It was immediately successful, winning the Pulitzer Prize, and has '
          'become a classic of modern American literature.'},
 {'corpus_id': 2,
  'score': np.float32(0.99251455),
  'text': "Harper Lee, an American novelist widely known for her novel 'To "
          "Kill a Mockingbird', was born in 1926 in Monroeville, Alabama. She "
          'received the Pulitzer Prize for Fiction in 1961.'},
 {'corpus_id': 1,
  'score': np.float32(0.25285733),
  'text': "The novel 'Moby-Dick' was written by Herman Melville and first "
          'published in 1851. It is considered a masterpiece of American '
          'literature and deals with complex themes of obsession, revenge, and '
          'the conflict between good and evil.'}]
"""

# Print the results
for item in results:
    score = item['score']
    doc = item['text']
    print(f"Score: {score:.4f}\nDocument: {doc}\n")