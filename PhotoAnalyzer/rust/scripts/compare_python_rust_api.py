#!/usr/bin/env python3
"""Minimal API contract comparison helper for PhotoAnalyzer Python vs Rust backends.

Usage:
  python rust/scripts/compare_python_rust_api.py --py http://localhost:8001 --rs http://localhost:3000
"""

from __future__ import annotations

import argparse
import json
import urllib.request
import urllib.error


def fetch(url: str):
    req = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = resp.read().decode("utf-8")
            ct = resp.headers.get("content-type", "")
            if "application/json" in ct:
                return resp.status, json.loads(data)
            try:
                return resp.status, json.loads(data)
            except Exception:
                return resp.status, data
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", errors="ignore")
        return e.code, {"error": body}
    except Exception as e:
        return None, {"error": str(e)}


def shape(value):
    if isinstance(value, dict):
        return {k: shape(v) for k, v in sorted(value.items())}
    if isinstance(value, list):
        if not value:
            return []
        return [shape(value[0])]
    return type(value).__name__


def compare_endpoint(py_base: str, rs_base: str, path: str):
    py_status, py_data = fetch(py_base.rstrip("/") + path)
    rs_status, rs_data = fetch(rs_base.rstrip("/") + path)

    print(f"\\n=== {path} ===")
    print(f"python status: {py_status}")
    print(f"rust   status: {rs_status}")

    py_shape = shape(py_data)
    rs_shape = shape(rs_data)

    if py_shape == rs_shape and py_status == rs_status:
        print("shape/status: OK")
    else:
        print("shape/status: DIFF")
        print("python shape:", json.dumps(py_shape, ensure_ascii=False, indent=2))
        print("rust shape:", json.dumps(rs_shape, ensure_ascii=False, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--py", default="http://localhost:8001", help="Python backend base URL")
    parser.add_argument("--rs", default="http://localhost:3000", help="Rust backend base URL")
    args = parser.parse_args()

    endpoints = [
        "/api/settings",
        "/api/stats",
        "/api/dirs",
        "/api/results",
        "/api/dedup/cache/stats",
        "/api/dedup/cache/entries",
    ]

    for ep in endpoints:
        compare_endpoint(args.py, args.rs, ep)


if __name__ == "__main__":
    main()
