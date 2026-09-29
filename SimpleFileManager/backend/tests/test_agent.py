"""Tests for agent tools (list_directory, read_file, find_files, search_content)."""
from tests.conftest import create_test_file, get_storage_root
from app.routers.agent import (
    tool_list_directory,
    tool_read_file,
    tool_find_files,
    tool_search_content,
)


class TestListDirectory:
    def test_list_root(self):
        create_test_file("a.txt", "aaa")
        create_test_file("b.txt", "bbb")
        result = tool_list_directory(".")
        assert result["success"] is True
        names = [i["name"] for i in result["items"]]
        assert "a.txt" in names
        assert "b.txt" in names

    def test_list_subdirectory(self):
        create_test_file("sub/c.txt", "ccc")
        result = tool_list_directory("sub")
        assert result["success"] is True
        assert any(i["name"] == "c.txt" for i in result["items"])

    def test_list_nonexistent(self):
        result = tool_list_directory("no_such_dir")
        assert result["success"] is False

    def test_hidden_files_excluded(self):
        create_test_file(".secret", "hidden")
        result = tool_list_directory(".")
        assert not any(i["name"] == ".secret" for i in result["items"])


class TestReadFile:
    def test_read_text_file(self):
        create_test_file("read.txt", "hello world")
        result = tool_read_file("read.txt")
        assert result["success"] is True
        assert "hello world" in result["content"]

    def test_read_nonexistent(self):
        result = tool_read_file("nope.txt")
        assert result["success"] is False

    def test_read_truncated(self):
        create_test_file("big.txt", "x" * 3000)
        result = tool_read_file("big.txt", limit=1000)
        assert result["success"] is True
        assert result["truncated"] is True
        assert len(result["content"]) <= 1000


class TestFindFiles:
    def test_find_by_name(self):
        create_test_file("screenshot1.png", "")
        create_test_file("screenshot2.png", "")
        create_test_file("doc.txt", "")
        result = tool_find_files("screenshot.*png")
        assert result["success"] is True
        assert result["total"] >= 2

    def test_find_no_matches(self):
        result = tool_find_files("zzzznotexist")
        assert result["success"] is True
        assert result["total"] == 0

    def test_invalid_regex(self):
        result = tool_find_files("[invalid(")
        assert result["success"] is False


class TestSearchContent:
    def test_search_content(self):
        create_test_file("doc1.txt", "The quick brown fox")
        create_test_file("doc2.txt", "Nothing interesting here")
        result = tool_search_content("quick.*fox")
        assert result["success"] is True
        assert result["total"] >= 1
        assert any("doc1.txt" in m["path"] for m in result["matches"])

    def test_search_no_matches(self):
        create_test_file("empty.txt", "nothing here")
        result = tool_search_content("zzzznotfound")
        assert result["success"] is True
        assert result["total"] == 0


class TestPathSafety:
    def test_list_directory_path_escape(self):
        result = tool_list_directory("../../../etc")
        assert result["success"] is False

    def test_read_file_path_escape(self):
        result = tool_read_file("../../../etc/passwd")
        assert result["success"] is False
