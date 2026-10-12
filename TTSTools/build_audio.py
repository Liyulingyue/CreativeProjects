"""Convert the Rust Book (markdown source) into listenable MP3s.

Pipeline per chapter:
  md file -> strip markdown noise -> split into TTS-friendly chunks
          -> code blocks run through a "code reader" that translates
             Rust syntax / operators into pronounceable words
          -> edge-tts (en-US-AriaNeural) renders each chunk to MP3
          -> chunks are concatenated into one MP3 per chapter.

Usage:
  python build_audio.py                # all chapters
  python build_audio.py ch01-02        # only files whose stem starts with prefix
  python build_audio.py --dry-run      # show what would be produced, no audio
"""

from __future__ import annotations

import argparse
import asyncio
import json
import re
import sys
import time
from dataclasses import dataclass
from pathlib import Path

import edge_tts

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
ROOT = Path(__file__).resolve().parent
SRC_DIR = ROOT / "references" / "book" / "src"
OUT_DIR = ROOT / "output" / "audio"
META_DIR = ROOT / "output" / "meta"

VOICE = "en-US-AriaNeural"      # female, clear, great for English practice
RATE = "+0%"                    # default speaking rate
PITCH = "+0Hz"
MAX_CHARS = 4500                # safe chunk size for edge-tts
RETRY = 3
RETRY_DELAY = 4

# ---------------------------------------------------------------------------
# Markdown cleaning
# ---------------------------------------------------------------------------
_FENCE = re.compile(r"```.*?```", re.DOTALL)
_HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
_HTML_TAG = re.compile(r"<[^>]+>")
_LISTING_TAG = re.compile(
    r'<Listing\b[^>]*\bcaption="([^"]+)"[^>]*>', re.IGNORECASE
)
_LISTING_CLOSE = re.compile(r"</Listing>", re.IGNORECASE)
_MDBOOK_INCLUDE = re.compile(r"\{\{#[^}]*\}\}")
_LATEX = re.compile(r"\$[^$]+\$")
_IMAGE = re.compile(r"!\[[^\]]*\]\([^)]+\)")
_LINK = re.compile(r"\[([^\]]+)\]\([^)]+\)")
_REF_DEF = re.compile(r"^\[[^\]]+\]:\s*\S+.*$", re.MULTILINE)
_LIST_BULLET = re.compile(r"^\s*[-*+]\s+", re.MULTILINE)
_LIST_NUM = re.compile(r"^\s*\d+\.\s+", re.MULTILINE)
_HEADER_HASH = re.compile(r"^#{1,6}\s*", re.MULTILINE)
_BOLD_ITAL = re.compile(r"(\*\*|__|\*|_)(.*?)\1")
_HR = re.compile(r"^\s*[-*_]{3,}\s*$", re.MULTILINE)
_ESCAPE = re.compile(r"\\([\\`*_{}\[\]()#+\-.!>])")
_MULTI_NL = re.compile(r"\n{3,}")

# Map common languages in fenced code blocks to human-friendly intros.
LANG_INTRO = {
    "rust": "Here is a Rust code example.",
    "rust,ignore": "Here is a code example that is not meant to compile.",
    "rust,editable": "Here is an editable Rust code example.",
    "rust,no_run": "Here is a Rust code example that should not be run automatically.",
    "rust,should_panic": "Here is a Rust code example that is expected to panic.",
    "console": "Here is a terminal session.",
    "shell": "Here is a shell session.",
    "bash": "Here is a shell session.",
    "cmd": "Here is a Windows command prompt session.",
    "powershell": "Here is a PowerShell session.",
    "toml": "Here is a configuration file.",
    "text": "Here is some text.",
    "": "Here is a code example.",
}


# ---------------------------------------------------------------------------
# Code reader
# ---------------------------------------------------------------------------
# Order matters: replace multi-char tokens before single chars.
_CODE_OPS: list[tuple[str, str]] = [
    ("..=", " inclusive to "),
    ("->", " arrow "),
    ("=>", " fat arrow "),
    ("::", " path "),
    ("==", " equals "),
    ("!=", " not equals "),
    ("<=", " less or equal "),
    (">=", " greater or equal "),
    ("&&", " and "),
    ("||", " or "),
    ("<<=", " left shift equals "),
    (">>=", " right shift equals "),
    ("<<", " left shift "),
    (">>", " right shift "),
    ("+=", " plus equals "),
    ("-=", " minus equals "),
    ("*=", " times equals "),
    ("/=", " divide equals "),
    ("%=", " modulo equals "),
    ("&=", " and equals "),
    ("|=", " or equals "),
    ("^=", " caret equals "),
    ("..", " to "),
    ("<", " less than "),
    (">", " greater than "),
    ("=", " equals "),
    ("+", " plus "),
    ("-", " minus "),
    ("*", " star "),
    ("/", " slash "),
    ("%", " modulo "),
    ("&", " and "),
    ("|", " or "),
    ("^", " caret "),
    ("!", " bang "),
    ("?", " question "),
    ("{", " open brace "),
    ("}", " close brace "),
    ("[", " open bracket "),
    ("]", " close bracket "),
    (",", " comma "),
    (";", " semicolon "),
    (":", " colon "),
    (".", " dot "),
    ("#", " hash "),
    ("@", " at "),
    ("$", " dollar "),
    ("\\", " backslash "),
]


def _strip_comments(code: str) -> str:
    """Remove // and /* */ comments, but not URL parts like https://."""
    # Block comments
    code = re.sub(r"/\*.*?\*/", " ", code, flags=re.DOTALL)
    # Line comments: only when // is not preceded by : (avoids URLs).
    code = re.sub(r"(?<!:)//[^\n]*", " ", code)
    return code


def _read_string_literal(m: re.Match[str]) -> str:
    body = m.group(1)
    body = body.replace("\\n", " new line ").replace("\\t", " tab ")
    return f' string "{body}" '


def read_code(code: str, lang: str) -> str:
    """Translate Rust-ish source into something TTS can read aloud."""
    code = _strip_comments(code)

    # String literals -- handled before generic char replacements.
    code = re.sub(r'"((?:\\.|[^"\\])*)"', _read_string_literal, code)
    # Char literals like 'a', '\n'.
    code = re.sub(r"'(?:\\.|[^'\\])'", " character ", code)

    # Operators / punctuation.
    for old, new in _CODE_OPS:
        code = code.replace(old, new)

    # Parens become silent (we already inserted words for braces/brackets).
    code = code.replace("(", " ").replace(")", " ")

    # Identifiers: split snake_case / camelCase / PascalCase so edge-tts can
    # speak them. We do this by inserting spaces at case / underscore /
    # digit boundaries inside word runs.
    code = re.sub(
        r"([A-Z]+)([A-Z][a-z])", r"\1 \2", code
    )  # HTTPRequest -> HTTP Request
    code = re.sub(
        r"([a-z\d])([A-Z])", r"\1 \2", code
    )  # fooBar -> foo Bar
    code = re.sub(r"([a-zA-Z])(\d)", r"\1 \2", code)
    code = re.sub(r"(\d)([a-zA-Z])", r"\1 \2", code)
    code = code.replace("_", " ")

    # Collapse whitespace.
    code = re.sub(r"\s+", " ", code).strip()
    return code


# ---------------------------------------------------------------------------
# Markdown -> listenable text
# ---------------------------------------------------------------------------
@dataclass
class Segment:
    kind: str   # "prose" or "code"
    lang: str   # for code: language tag from fence
    text: str   # already-processed speakable text


def _split_segments(md: str) -> list[Segment]:
    """Walk markdown, replacing fenced code blocks with read_code() output
    and cleaning prose in between. Returns ordered Segments."""
    out: list[Segment] = []

    pos = 0
    fence_iter = _FENCE.finditer(md)
    for m in fence_iter:
        # Look back at most 400 chars for a Listing caption that introduces
        # this code block. We do NOT re-process prose from before `pos`.
        lookback_start = max(pos, m.start() - 400)
        pre = md[lookback_start : m.start()]

        cap_match = _LISTING_TAG.search(pre)
        if cap_match:
            # Convert relative offset to absolute.
            cap_abs_start = lookback_start + cap_match.start()
            cap_abs_end = lookback_start + cap_match.end()
            caption = cap_match.group(1).strip()
            caption_spoken = _speak_inline_code(
                re.sub(r"`([^`]+)`", r"\1", caption)
            )
            leading = md[pos:cap_abs_start]
            trailing = md[cap_abs_end : m.start()]
            if leading.strip():
                out.append(Segment("prose", "", _clean_prose(leading)))
            if caption_spoken:
                out.append(Segment("prose", "", caption_spoken))
            if trailing.strip():
                out.append(Segment("prose", "", _clean_prose(trailing)))
        else:
            prose = md[pos : m.start()]
            if prose.strip():
                out.append(Segment("prose", "", _clean_prose(prose)))

        fence = m.group(0)
        first_nl = fence.find("\n")
        info = fence[3:first_nl].strip().lower()
        body = fence[first_nl + 1 : fence.rfind("```")]
        # Strip mdbook include directives -- they're build-time placeholders.
        body = _MDBOOK_INCLUDE.sub("", body)
        body = body.strip()
        if not body:
            spoken_text = "(code follows.)"
        else:
            spoken = read_code(body, info)
            intro = LANG_INTRO.get(info, LANG_INTRO[""])
            # If the code block is tiny, just say "code example" without
            # reading every symbol -- prevents stuttering on one-liners.
            if len(body) <= 2:
                spoken_text = intro
            else:
                spoken_text = f"{intro} {spoken}" if spoken else intro
        out.append(Segment("code", info, spoken_text))
        pos = m.end()

    tail = md[pos:]
    if tail.strip():
        out.append(Segment("prose", "", _clean_prose(tail)))

    return out


_INLINE_CODE = re.compile(r"`([^`]+)`")


def _speak_inline_code(text: str) -> str:
    """Translate just the operator-y bits of an inline code snippet so the
    TTS doesn't try to speak `<T>` as silence or `&str` as a single token.
    Identifiers are kept intact (no snake/camel splitting) so that words like
    `println!` stay pronounceable."""
    ops: list[tuple[str, str]] = [
        ("->", " arrow "), ("=>", " fat arrow "), ("::", " path "),
        ("==", " equals "), ("!=", " not equals "),
        ("<=", " less or equal "), (">=", " greater or equal "),
        ("&&", " and "), ("||", " or "),
        ("..", " to "), ("...", " and so on "),
        ("<<=", " left shift equals "), (">>=", " right shift equals "),
        ("<<", " left shift "), (">>", " right shift "),
        ("+=", " plus equals "), ("-=", " minus equals "),
        ("*=", " times equals "), ("/=", " divide equals "),
        ("%=", " modulo equals "),
        ("&=", " and equals "), ("|=", " or equals "), ("^=", " caret equals "),
        ("<", " less than "), (">", " greater than "),
        ("=", " equals "),
        ("+", " plus "), ("-", " minus "),
        ("*", " star "), ("/", " slash "),
        ("%", " modulo "),
        ("&", " and "), ("|", " or "), ("^", " caret "),
        ("!", " bang "), ("?", " question "),
        ("{", " open brace "), ("}", " close brace "),
        ("[", " open bracket "), ("]", " close bracket "),
        ("@", " at "), ("#", " hash "), ("$", " dollar "),
        ("\\", " backslash "),
        (":", " colon "), (";", " semicolon "), (".", " dot "),
    ]
    out = text
    for old, new in ops:
        out = out.replace(old, new)
    # Strings inside inline code (rare but happens).
    out = re.sub(r'"([^"]*)"', r' string "\1" ', out)
    out = re.sub(r"\s+", " ", out).strip()
    return out


def _clean_prose(text: str) -> str:
    # Strip mdbook include directives before anything else.
    text = _MDBOOK_INCLUDE.sub("", text)
    # Inline code MUST be processed before HTML tag stripping, otherwise
    # the <T> / <i32, E> inside `Vec<T>` gets eaten by `<[^>]+>`.
    text = _INLINE_CODE.sub(lambda m: _speak_inline_code(m.group(1)), text)
    text = _HTML_COMMENT.sub(" ", text)
    text = _LATEX.sub(" ", text)
    text = _IMAGE.sub(" ", text)
    text = _LINK.sub(r"\1", text)
    text = _REF_DEF.sub(" ", text)
    text = _HEADER_HASH.sub("", text)
    text = _LIST_BULLET.sub("", text)
    text = _LIST_NUM.sub("", text)
    text = _BOLD_ITAL.sub(r"\2", text)
    text = _HR.sub(" ", text)
    text = _HTML_TAG.sub(" ", text)
    text = _ESCAPE.sub(r"\1", text)
    # Drop blockquote markers but keep text.
    text = re.sub(r"^\s*>\s?", "", text, flags=re.MULTILINE)
    # Collapse blank lines / whitespace.
    text = _MULTI_NL.sub("\n\n", text)
    text = re.sub(r"[ \t]+", " ", text)
    return text.strip()


def md_to_text(md: str, intro: str = "") -> str:
    segs = _split_segments(md)
    parts: list[str] = [intro] if intro else []
    for seg in segs:
        if seg.kind == "prose":
            parts.append(seg.text)
        else:
            parts.append(seg.text)
    return "\n\n".join(p for p in parts if p).strip()


# ---------------------------------------------------------------------------
# Chunking
# ---------------------------------------------------------------------------
def chunk_text(text: str, limit: int = MAX_CHARS) -> list[str]:
    """Split into TTS-friendly chunks at paragraph / sentence boundaries."""
    paragraphs = [p.strip() for p in text.split("\n\n") if p.strip()]
    chunks: list[str] = []
    buf = ""
    for p in paragraphs:
        candidate = (buf + "\n\n" + p) if buf else p
        if len(candidate) <= limit:
            buf = candidate
            continue
        if buf:
            chunks.append(buf)
            buf = ""
        # Paragraph itself too big -- split by sentence.
        if len(p) > limit:
            sentences = re.split(r"(?<=[.!?])\s+", p)
            for s in sentences:
                cand = (buf + " " + s) if buf else s
                if len(cand) <= limit:
                    buf = cand
                else:
                    if buf:
                        chunks.append(buf)
                    buf = s
        else:
            buf = p
    if buf:
        chunks.append(buf)
    return chunks


# ---------------------------------------------------------------------------
# TTS
# ---------------------------------------------------------------------------
async def synthesize_one(text: str, voice: str, out_path: Path) -> None:
    """Synthesize a single chunk to out_path with retries."""
    last_err: Exception | None = None
    for attempt in range(1, RETRY + 1):
        try:
            comm = edge_tts.Communicate(
                text, voice, rate=RATE, pitch=PITCH
            )
            await comm.save(str(out_path))
            return
        except Exception as e:  # noqa: BLE001
            last_err = e
            if attempt < RETRY:
                await asyncio.sleep(RETRY_DELAY * attempt)
    raise RuntimeError(f"edge-tts failed after {RETRY} tries: {last_err}")


async def synth_chapter(
    chunks: list[str], voice: str, out_path: Path
) -> float:
    """Synthesize all chunks into one MP3, return seconds of audio."""
    tmp_dir = out_path.parent / "_tmp"
    tmp_dir.mkdir(exist_ok=True)
    tmp_files: list[Path] = []
    for i, chunk in enumerate(chunks):
        tmp_files.append(tmp_dir / f"{out_path.stem}_{i:03d}.mp3")
    try:
        for i, (chunk, tf) in enumerate(zip(chunks, tmp_files)):
            await synthesize_one(chunk, voice, tf)
        # Concat mp3 files byte-wise is safe (mp3 frames are independent).
        with open(out_path, "wb") as out:
            for tf in tmp_files:
                out.write(tf.read_bytes())
    finally:
        for tf in tmp_files:
            try:
                tf.unlink()
            except FileNotFoundError:
                pass
        try:
            tmp_dir.rmdir()
        except OSError:
            pass
    return out_path.stat().st_size / 16_000  # rough seconds at 128 kbps


# ---------------------------------------------------------------------------
# Driver
# ---------------------------------------------------------------------------
ORDER_HINT = ["foreword", "title-page", "SUMMARY", "ch00", "ch01", "ch02",
              "ch03", "ch04", "ch05", "ch06", "ch07", "ch08", "ch09",
              "ch10", "ch11", "ch12", "ch13", "ch14", "ch15", "ch16",
              "ch17", "ch18", "ch19", "ch20", "ch21", "appendix"]


def sort_key(p: Path) -> tuple[int, str]:
    name = p.stem
    for idx, prefix in enumerate(ORDER_HINT):
        if name.startswith(prefix):
            return (idx, name)
    return (len(ORDER_HINT), name)


def title_from_md(path: Path) -> str:
    md = path.read_text(encoding="utf-8")
    for line in md.splitlines():
        line = line.strip()
        if line.startswith("#"):
            return re.sub(r"\s+", " ", line.lstrip("# ").strip())
    return path.stem


async def process_one(
    md_path: Path, out_dir: Path, voice: str, dry: bool
) -> dict | None:
    md = md_path.read_text(encoding="utf-8")
    title = title_from_md(md_path)
    intro = f"From The Rust Programming Language. {title}."
    text = md_to_text(md, intro=intro)
    if not text.strip():
        return None
    chunks = chunk_text(text)
    out_path = out_dir / (md_path.stem + ".mp3")
    if dry:
        return {
            "file": md_path.name,
            "title": title,
            "out": out_path.name,
            "chars": len(text),
            "chunks": len(chunks),
            "first_chunk": chunks[0][:120] if chunks else "",
        }
    t0 = time.time()
    size = await synth_chapter(chunks, voice, out_path)
    dt = time.time() - t0
    return {
        "file": md_path.name,
        "title": title_from_md(md_path),
        "out": out_path.name,
        "chars": len(text),
        "chunks": len(chunks),
        "wall_seconds": round(dt, 1),
        "mp3_seconds_est": round(size, 1),
        "mp3_bytes": out_path.stat().st_size,
    }


async def main_async(args: argparse.Namespace) -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    META_DIR.mkdir(parents=True, exist_ok=True)

    files = sorted(SRC_DIR.glob("*.md"), key=sort_key)
    if args.prefix:
        files = [p for p in files if p.stem.startswith(args.prefix)]
    if not files:
        print("No matching files.", file=sys.stderr)
        return

    if args.dry_run:
        for f in files:
            r = await process_one(f, OUT_DIR, VOICE, dry=True)
            if r:
                print(f"{r['file']:<48} {r['chars']:>6} chars  {r['chunks']:>3} chunks")
                print(f"   {r['title']}")
        return

    manifest: list[dict] = []
    total = len(files)
    for i, f in enumerate(files, 1):
        t0 = time.time()
        r = await process_one(f, OUT_DIR, VOICE, dry=False)
        if r is None:
            print(f"[{i}/{total}] {f.name}: empty, skipped")
            continue
        meta_path = META_DIR / (f.stem + ".json")
        meta_path.write_text(
            json.dumps(r, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        manifest.append(r)
        print(
            f"[{i}/{total}] {f.name:<40} "
            f"{r['chars']:>6}c / {r['chunks']:>3}ch / "
            f"~{r['mp3_seconds_est']:>5.0f}s  "
            f"({r['wall_seconds']:.1f}s wall)"
        )
        # gentle pacing to avoid hammering edge-tts
        await asyncio.sleep(0.2)

    (META_DIR / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    # Build a playlist in book order.
    playlist = OUT_DIR / "rust_book.m3u"
    lines = ["#EXTM3U"]
    for r in manifest:
        lines.append(f"#EXTINF:-1,{r['title']}")
        lines.append(r["out"])
    playlist.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"\nWrote {playlist}")
    print(f"Wrote manifest with {len(manifest)} entries to {META_DIR}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "prefix", nargs="?",
        help="Only process files whose stem starts with this (e.g. ch01)",
    )
    ap.add_argument(
        "--dry-run", action="store_true",
        help="Show stats without making any network calls",
    )
    args = ap.parse_args()
    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()
