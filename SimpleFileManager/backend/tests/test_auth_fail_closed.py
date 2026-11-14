"""A password file that cannot be read must not open the service."""

import json

import pytest

import importlib

# app.routers.auth is the module; app.routers.auth.auth is the router.
auth_mod = importlib.import_module("app.routers.auth")


def test_a_missing_password_file_is_the_normal_case(tmp_path, monkeypatch):
    """A fresh install has no password file, and that is fine."""
    path = tmp_path / "auth.json"
    monkeypatch.setattr(auth_mod, "AUTH_FILE", path)
    monkeypatch.setattr(auth_mod, "_auth_cache", {"data": None, "mtime": None})

    data = auth_mod._load_auth()
    assert data["password_hash"] is None


def test_a_broken_password_file_stops_the_service(tmp_path, monkeypatch):
    """Reading it as "no password" would leave the service open."""
    path = tmp_path / "auth.json"
    path.write_text('{"password_hash": tr')
    monkeypatch.setattr(auth_mod, "AUTH_FILE", path)
    monkeypatch.setattr(auth_mod, "_auth_cache", {"data": None, "mtime": None})

    with pytest.raises(Exception):
        auth_mod._load_auth()


def test_a_password_file_without_a_password_is_broken(tmp_path, monkeypatch):
    path = tmp_path / "auth.json"
    path.write_text(json.dumps({"tokens": {}}))
    monkeypatch.setattr(auth_mod, "AUTH_FILE", path)
    monkeypatch.setattr(auth_mod, "_auth_cache", {"data": None, "mtime": None})

    with pytest.raises(Exception):
        auth_mod._load_auth()
