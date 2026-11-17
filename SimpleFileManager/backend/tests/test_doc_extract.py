"""Turning documents into text.

A Word, Excel or PowerPoint file is a ZIP archive: a few megabytes on
disk can be a few hundred megabytes of XML inside, and the parsers
read all of it into memory. So the size is checked before the file is
opened.

The tests here use real documents -- python-docx writes one -- plus a
hand-made archive that claims to be much bigger than it is.
"""
import io
import zipfile

import pytest

from app.doc_extract import (
    ExtractionError,
    MAX_UNZIPPED_BYTES,
    extract_text,
)


def _make_docx(path, text="里面有一些字"):
    import docx

    document = docx.Document()
    document.add_paragraph(text)
    document.save(str(path))
    return path


def _make_lying_docx(path, declared_size):
    """A .docx whose contents claim to be much larger than they are.

    The number in the central directory is patched by hand, which is
    what an archive built to be malicious looks like.
    """
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("word/document.xml", "<w:document/>")
        zf.writestr("[Content_Types].xml", "<Types/>")
    data = bytearray(path.read_bytes())
    at = data.find(b"PK\x01\x02")
    assert at > 0
    data[at + 24:at + 28] = declared_size.to_bytes(4, "little")
    path.write_bytes(bytes(data))
    return path


class TestPlainText:
    def test_a_text_file_is_not_a_document(self, tmp_path):
        path = tmp_path / "笔记.txt"
        path.write_text("就是文字", encoding="utf-8")

        assert extract_text(path) is None


class TestDocx:
    def test_the_words_come_out(self, tmp_path):
        path = _make_docx(tmp_path / "文档.docx", "合同里的关键条款")

        text = extract_text(path)

        assert text is not None
        assert "关键条款" in text

    def test_an_empty_file_is_refused(self, tmp_path):
        path = tmp_path / "空的.docx"
        path.write_bytes(b"")

        with pytest.raises(ExtractionError):
            extract_text(path)

    def test_a_file_that_is_too_big_is_refused(self, tmp_path, monkeypatch):
        from app import doc_extract

        monkeypatch.setattr(doc_extract, "MAX_DOCUMENT_BYTES", 10)
        path = _make_docx(tmp_path / "大.docx", "内容")

        with pytest.raises(ExtractionError) as e:
            extract_text(path)

        assert "过大" in str(e.value)


class TestTheUnzippedSize:
    """What is inside the archive, not how big the archive is."""

    def test_a_document_that_unpacks_to_too_much_is_refused(self, tmp_path, monkeypatch):
        from app import doc_extract

        monkeypatch.setattr(doc_extract, "MAX_UNZIPPED_BYTES", 1024)
        path = _make_lying_docx(tmp_path / "炸弹.docx", 50 * 1024 * 1024)

        with pytest.raises(ExtractionError) as e:
            extract_text(path)

        assert "太大" in str(e.value) or "太多" in str(e.value)

    def test_a_document_with_many_pieces_is_refused(self, tmp_path, monkeypatch):
        from app import doc_extract

        monkeypatch.setattr(doc_extract, "MAX_UNZIPPED_FILES", 3)
        path = tmp_path / "碎.docx"
        with zipfile.ZipFile(path, "w") as zf:
            for i in range(10):
                zf.writestr(f"word/part{i}.xml", "<x/>")

        with pytest.raises(ExtractionError) as e:
            extract_text(path)

        assert "太多" in str(e.value)

    def test_a_broken_archive_says_so_in_chinese(self, tmp_path):
        path = tmp_path / "坏的.docx"
        path.write_bytes(b"this is not a zip file at all")

        with pytest.raises(ExtractionError) as e:
            extract_text(path)

        assert "压缩包" in str(e.value)
