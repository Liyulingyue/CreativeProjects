"""The messages the user reads have to be in Chinese.

The page is in Chinese, the buttons are in Chinese, and an error that
arrives in English is a sentence the user cannot read. This is a cheap
check that keeps the stragglers from coming back: every message the
server sends as a reason has to contain Chinese.

F-strings are left alone -- their wording lives in the format, and the
check would only see the placeholder.
"""
import re
from pathlib import Path

APP = Path(__file__).resolve().parent.parent / "app"

# detail="..." and detail=f"..."
DETAIL = re.compile(r'detail=(f?)"([^"]*)"')
# raise ValueError("...") and the same with HTTPException
RAISED = re.compile(r'raise (?:ValueError|FileExistsError|FileNotFoundError)\("([^"]*)"\)')

# A message that is only a technical term is not a sentence the user
# reads; those are allowed to stay as they are.
ALLOWED = {
    "",
    "LLM_API_KEY",
}


def _has_chinese(text: str) -> bool:
    return any("一" <= ch <= "鿿" for ch in text)


def test_no_english_error_messages_in_the_backend():
    offenders = []
    for path in sorted(APP.rglob("*.py")):
        text = path.read_text(encoding="utf-8")
        for pattern in (DETAIL, RAISED):
            for match in pattern.finditer(text):
                groups = match.groups()
                if groups and groups[0] == "f":
                    # An f-string: the wording is in the format string
                    # and the check would only see the placeholder.
                    continue
                message = groups[-1]
                if message in ALLOWED or _has_chinese(message):
                    continue
                line = text[: match.start()].count("\n") + 1
                offenders.append(f"{path.name}:{line}  {message!r}")
    assert not offenders, "这些报错还是英文：\n" + "\n".join(offenders)
