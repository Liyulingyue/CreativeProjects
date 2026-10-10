"""Split document text into embedding-sized chunks.

A 1.7MB novel is roughly 300k tokens: too large for the embedding model in one
call, and semantically useless as a single vector — every query would match the
whole book diluted equally. Chunking fixes both, and gives retrieval something
to point at.
"""

# 1500 characters keeps a chunk well inside the embedding model's context while
# holding enough surrounding text for a passage to be self-describing.
DEFAULT_CHUNK_SIZE = 1500
DEFAULT_OVERLAP = 200

MIN_CHUNK_SIZE = 200


def chunk_text(
    text: str,
    chunk_size: int = DEFAULT_CHUNK_SIZE,
    overlap: int = DEFAULT_OVERLAP,
) -> list[str]:
    """Split `text` into overlapping chunks, preferring paragraph breaks.

    Splitting on a fixed character count would cut mid-sentence, which costs
    retrieval accuracy, so boundaries are nudged back to the nearest paragraph
    or line break when one is close by.
    """
    if not text:
        return []

    chunk_size = max(MIN_CHUNK_SIZE, chunk_size)
    overlap = max(0, min(overlap, chunk_size - MIN_CHUNK_SIZE))

    chunks: list[str] = []
    start = 0
    length = len(text)

    while start < length:
        end = min(start + chunk_size, length)
        if end < length:
            window = text[start:end]
            # Look back over the last fifth of the chunk for a natural break.
            floor = start + int(chunk_size * 0.8)
            for sep in ("\n\n", "\n", "。", "！", "？", ". ", " "):
                idx = window.rfind(sep, floor - start)
                if idx != -1:
                    end = start + idx + len(sep)
                    break

        piece = text[start:end].strip()
        if piece:
            chunks.append(piece)

        if end >= length:
            break
        # Always advance, otherwise a long run of whitespace could stall us.
        start = max(end - overlap, start + 1)

    return chunks


def chunk_file(
    text: str,
    chunk_size: int = DEFAULT_CHUNK_SIZE,
    overlap: int = DEFAULT_OVERLAP,
) -> list[tuple[int, str]]:
    """Chunk with the character's offset in the original text.

    The offset lets the UI point at where in the file a hit came from instead
    of only naming the file.
    """
    if not text:
        return []

    chunks: list[tuple[int, str]] = []
    start = 0
    length = len(text)

    while start < length:
        end = min(start + chunk_size, length)
        if end < length:
            window = text[start:end]
            floor = start + int(chunk_size * 0.8)
            for sep in ("\n\n", "\n", "。", "！", "？", ". ", " "):
                idx = window.rfind(sep, floor - start)
                if idx != -1:
                    end = start + idx + len(sep)
                    break

        piece = text[start:end]
        stripped = piece.strip()
        if stripped:
            chunks.append((start + (len(piece) - len(piece.lstrip())), stripped))

        if end >= length:
            break
        start = max(end - overlap, start + 1)

    return chunks
