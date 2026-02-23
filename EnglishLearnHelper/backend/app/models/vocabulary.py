from pydantic import BaseModel
from typing import Optional

class Vocabulary(BaseModel):
    id: Optional[int] = None
    word: str
    phonetic: Optional[str] = None
    part_of_speech: Optional[str] = None
    definition: str
    unit: Optional[str] = None
