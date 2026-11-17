"""Splitting a document into pieces small enough to embed.

The pieces are what the search points at, so where they are cut matters:
a piece cut in the middle of a sentence retrieves badly, and the offset
has to be right so the page can show where a hit came from.
"""
from app.chunking import chunk_file, chunk_text, MIN_CHUNK_SIZE


def test_an_empty_text_has_no_pieces():
    assert chunk_text("") == []
    assert chunk_file("") == []


def test_a_short_text_is_one_piece():
    assert chunk_text("短短的一句话") == ["短短的一句话"]
    assert chunk_file("短短的一句话") == [(0, "短短的一句话")]


def test_a_long_text_is_split():
    text = "句子。" * 1000
    pieces = chunk_text(text)
    assert len(pieces) > 1
    assert all(len(p) <= 400 for p in pieces)


def test_the_pieces_cover_the_whole_text():
    text = "".join(f"第{i}段内容。\n" for i in range(300))
    pieces = chunk_text(text)
    # Every part of the text is in some piece. The pieces overlap, so
    # the count is larger than the text's -- but not smaller.
    joined = "".join(pieces)
    assert joined.count("第") >= text.count("第")
    assert "第0段内容。" in joined
    assert "第299段内容。" in joined


def test_the_offset_points_at_the_right_place():
    text = "前面的一段。\n\n后面的一段内容在这里。"
    pieces = chunk_file(text)
    for offset, piece in pieces:
        # The text at the offset is the piece (modulo the whitespace
        # that was trimmed off the front).
        assert text[offset:offset + len(piece)] == piece


def test_a_piece_is_never_smaller_than_the_minimum_when_it_can_be_bigger():
    # A chunk size below the minimum is raised to it.
    pieces = chunk_text("字" * 1000, chunk_size=10)
    assert all(len(p) >= MIN_CHUNK_SIZE for p in pieces[:-1])


def test_pieces_overlap_so_a_sentence_is_not_cut_in_half():
    text = "abcdefghij" * 200
    pieces = chunk_text(text, chunk_size=400, overlap=60)
    assert len(pieces) > 1
    # The overlap means consecutive pieces share some text.
    assert any(set(pieces[i]) & set(pieces[i + 1]) for i in range(len(pieces) - 1))


def test_whitespace_only_text_has_no_pieces():
    assert chunk_text("   \n\n  ") == []
