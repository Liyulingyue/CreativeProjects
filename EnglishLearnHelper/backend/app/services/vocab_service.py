import re
from pathlib import Path
from typing import List, Optional
from app.config import DATA_DIR
from app.models.vocabulary import Vocabulary

_vocab_cache: List[Vocabulary] = []

def parse_vocab_file() -> List[Vocabulary]:
    global _vocab_cache
    if _vocab_cache:
        return _vocab_cache
    
    vocab_list = []
    vocab_file = DATA_DIR / "IELTS Word List.txt"
    
    if not vocab_file.exists():
        return []
    
    current_unit = ""
    
    with open(vocab_file, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            
            if line.startswith("Word List"):
                current_unit = line
                continue
            
            if line.startswith("Word List") or line.startswith("README") or line.startswith("雅思"):
                continue
            
            match = re.match(r'^(\S+?)\s+(/[^/]+/|\[[^\]]+\]|\{[^\}]+\})\s*([a-z]+\.?)\s*(.+)$', line)
            if match:
                word = match.group(1).replace("*", "")
                phonetic = match.group(2)
                pos = match.group(3)
                definition = match.group(4)
                
                vocab_list.append(Vocabulary(
                    word=word,
                    phonetic=phonetic,
                    part_of_speech=pos,
                    definition=definition,
                    unit=current_unit
                ))
            else:
                parts = line.split(None, 1)
                if len(parts) == 2:
                    word = parts[0].replace("*", "")
                    rest = parts[1]
                    phonetic_match = re.match(r'(/[^/]+/|\[[^\]]+\]|\{[^\}]+\})', rest)
                    if phonetic_match:
                        phonetic = phonetic_match.group(1)
                        rest = rest[phonetic_match.end():].strip()
                        pos_match = re.match(r'^([a-z]+\.?)\s*', rest)
                        if pos_match:
                            pos = pos_match.group(1)
                            definition = rest[pos_match.end():].strip()
                        else:
                            pos = None
                            definition = rest
                    else:
                        phonetic = None
                        pos = None
                        definition = rest
                    
                    if word and definition:
                        vocab_list.append(Vocabulary(
                            word=word,
                            phonetic=phonetic,
                            part_of_speech=pos,
                            definition=definition,
                            unit=current_unit
                        ))
    
    _vocab_cache = vocab_list
    return vocab_list

def get_all_vocab() -> List[Vocabulary]:
    return parse_vocab_file()

def search_vocab(keyword: str) -> List[Vocabulary]:
    vocab_list = parse_vocab_file()
    keyword = keyword.lower()
    return [v for v in vocab_list if keyword in v.word.lower() or keyword in v.definition.lower()]

def get_vocab_by_page(page: int = 1, page_size: int = 50) -> dict:
    vocab_list = parse_vocab_file()
    start = (page - 1) * page_size
    end = start + page_size
    total = len(vocab_list)
    return {
        "items": vocab_list[start:end],
        "total": total,
        "page": page,
        "page_size": page_size,
        "total_pages": (total + page_size - 1) // page_size
    }
