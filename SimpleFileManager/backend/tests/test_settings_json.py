"""The settings file must never keep the service from starting."""

import json

from app.deps import _load_json, _save_json


def test_a_broken_settings_file_is_kept_aside(tmp_path):
    path = tmp_path / "settings.json"
    path.write_text('{"storage_path": "./data", "image_index_enabled": tr')

    assert _load_json(path, {"default": True}) == {"default": True}
    # The broken file is kept, so it can be looked at afterwards.
    assert path.with_suffix(".json.broken").exists()


def test_settings_are_written_atomically(tmp_path):
    path = tmp_path / "settings.json"
    _save_json(path, {"a": 1})
    assert json.loads(path.read_text()) == {"a": 1}
    # No leftover temporary file.
    assert list(tmp_path.iterdir()) == [path]


def test_a_missing_settings_file_is_not_an_error(tmp_path):
    assert _load_json(tmp_path / "nope.json", {"default": True}) == {"default": True}
