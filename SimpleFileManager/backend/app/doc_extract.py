"""Text extraction for document formats that are not plain text.

PDF and DOCX hold real content in a container, so opening them as UTF-8 yields
mojibake. Both the indexer and the agent's read_file tool go through here so
they cannot drift on which formats are supported or how failures are reported.

Deliberately unsupported, and reported rather than silently skipped:
  - scanned PDFs (image-only pages need OCR, which is too slow to run inline)
  - legacy .doc/.ppt/.xls (binary Office formats; no pure-python reader here)
"""

import logging
from pathlib import Path
from typing import Optional

log = logging.getLogger(__name__)

PDF_EXTENSIONS = {".pdf"}
DOCX_EXTENSIONS = {".docx"}
DOCUMENT_EXTENSIONS = PDF_EXTENSIONS | DOCX_EXTENSIONS

# A PDF is mostly images, so a small file can hold a lot of text. Documents get
# a far larger ceiling than the 512KB plain-text limit.
MAX_DOCUMENT_BYTES = 32 * 1024 * 1024
# Cap the extracted text so a 300-page scan cannot blow up the search index.
MAX_EXTRACTED_CHARS = 400_000


class ExtractionError(Exception):
    """Raised with a user-facing reason when a document cannot be parsed."""


def is_document(path: Path) -> bool:
    return path.suffix.lower() in DOCUMENT_EXTENSIONS


def _extract_pdf(path: Path) -> str:
    try:
        from pypdf import PdfReader
        from pypdf.errors import PdfReadError
    except ImportError as e:  # pragma: no cover - dependency is declared
        raise ExtractionError("服务端未安装 PDF 解析依赖（pypdf）") from e

    try:
        reader = PdfReader(str(path))
        if reader.is_encrypted:
            # An empty user password is common; anything else we cannot open.
            try:
                if reader.decrypt("") == 0:
                    raise ExtractionError("PDF 已加密，需要密码才能读取")
            except ExtractionError:
                raise
            except Exception:
                raise ExtractionError("PDF 已加密，需要密码才能读取")
        pages = reader.pages
        if not pages:
            return ""
        chunks = []
        for page in pages:
            try:
                chunks.append(page.extract_text() or "")
            except Exception as e:
                # One bad page should not cost us the rest of the document.
                log.debug("pdf page extract failed in %s: %s", path, e)
        text = "\n\n".join(c for c in chunks if c.strip())
    except ExtractionError:
        raise
    except PdfReadError as e:
        raise ExtractionError(f"PDF 解析失败：{e}") from e
    except Exception as e:
        raise ExtractionError(f"PDF 解析失败：{e}") from e

    if not text.strip():
        raise ExtractionError("PDF 中没有可提取的文字（可能是扫描件，需要 OCR）")
    return text


def _extract_docx(path: Path) -> str:
    try:
        import docx  # python-docx
    except ImportError as e:  # pragma: no cover - dependency is declared
        raise ExtractionError("服务端未安装 DOCX 解析依赖（python-docx）") from e

    try:
        document = docx.Document(str(path))
        parts = [p.text for p in document.paragraphs]
        # Tables hold most of the content in specs and reports; ignoring them
        # would make a contract's key terms invisible to search.
        for table in document.tables:
            for row in table.rows:
                cells = [c.text.strip() for c in row.cells]
                if any(cells):
                    parts.append("\t".join(cells))
    except Exception as e:
        raise ExtractionError(f"DOCX 解析失败：{e}") from e

    text = "\n".join(p for p in parts if p.strip())
    if not text.strip():
        raise ExtractionError("DOCX 中没有可提取的文字")
    return text


def extract_text(path: Path) -> Optional[str]:
    """Return extracted text, or None when the file is plain text.

    Raises ExtractionError for a document that is recognised but unreadable, so
    callers can tell "not a document" apart from "document we could not parse".
    """
    suffix = path.suffix.lower()
    if suffix not in DOCUMENT_EXTENSIONS:
        return None

    try:
        size = path.stat().st_size
    except OSError as e:
        raise ExtractionError(f"无法读取文件：{e}") from e

    if size == 0:
        raise ExtractionError("文件为空")
    if size > MAX_DOCUMENT_BYTES:
        raise ExtractionError(
            f"文档过大（{size // 1024 // 1024}MB），上限 {MAX_DOCUMENT_BYTES // 1024 // 1024}MB"
        )

    text = _extract_pdf(path) if suffix in PDF_EXTENSIONS else _extract_docx(path)
    if len(text) > MAX_EXTRACTED_CHARS:
        text = text[:MAX_EXTRACTED_CHARS]
    return text