#!/usr/bin/env python3
import os
import sys
import tempfile
import argparse
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
import json

try:
    from faster_whisper import WhisperModel
except ImportError:
    print("ERROR: faster-whisper not installed.")
    print("Install with: pip install faster-whisper")
    sys.exit(1)


class TranscribeHandler(BaseHTTPRequestHandler):
    model = None
    model_name = None

    def log_message(self, format, *args):
        sys.stdout.write(f"[WhisperServer] {args[0]}\n")
        sys.stdout.flush()

    def do_GET(self):
        if self.path == "/health":
            status = "ok" if TranscribeHandler.model is not None else "loading"
            self.send_json({"status": status})
        elif self.path == "/":
            self.send_json({
                "status": "ok",
                "model": TranscribeHandler.model_name,
            })
        else:
            self.send_error(404)

    def do_POST(self):
        parsed = urlparse(self.path)
        if parsed.path != "/transcribe":
            self.send_error(404)
            return

        if TranscribeHandler.model is None:
            self.send_error(503, "Model not loaded")
            return

        content_type = self.headers.get("Content-Type", "")
        if "audio" not in content_type and "application/octet" not in content_type:
            self.send_error(400, "Expected audio data")
            return

        length = int(self.headers.get("Content-Length", 0))
        if length == 0:
            self.send_error(400, "No audio data")
            return

        language = None
        if "X-Language" in self.headers:
            lang = self.headers["X-Language"].strip()
            if lang and lang.lower() != "auto":
                language = lang

        params = parse_qs(parsed.query)
        beam_size = int(params.get("beam_size", ["5"])[0])

        audio_data = self.rfile.read(length)

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
            f.write(audio_data)
            temp_path = f.name

        try:
            segments, info = TranscribeHandler.model.transcribe(
                temp_path,
                language=language,
                beam_size=beam_size,
                vad_filter=True,
            )
            text = "".join(seg.text for seg in segments)
            self.send_json({
                "text": text.strip(),
                "language": info.language if info else language or "auto",
            })
        except Exception as e:
            self.send_error(500, str(e))
        finally:
            os.unlink(temp_path)

    def send_json(self, data):
        body = json.dumps(data).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", len(body))
        self.end_headers()
        self.wfile.write(body)


def load_model(model_name: str):
    size_map = {
        "tiny": "tiny",
        "base": "base",
        "small": "small",
        "medium": "medium",
        "large": "large-v3",
    }
    model_id = size_map.get(model_name, model_name)
    print(f"Loading whisper model: {model_id} ...")
    TranscribeHandler.model = WhisperModel(
        model_id,
        device="cpu",
        compute_type="int8",
    )
    TranscribeHandler.model_name = model_id
    print(f"Model '{model_id}' loaded successfully.")


def main():
    parser = argparse.ArgumentParser(description="Whisper HTTP Server")
    parser.add_argument("--host", default="127.0.0.1", help="Listen host")
    parser.add_argument("--port", type=int, default=18789, help="Listen port")
    parser.add_argument("--model", default="base", help="Model size: tiny, base, small, medium, large")
    parser.add_argument("--models-dir", default=None, help="Models directory")
    args = parser.parse_args()

    if args.models_dir:
        os.environ["HF_HUB_CACHE"] = args.models_dir
    else:
        default_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models")
        os.environ["HF_HUB_CACHE"] = default_dir
        os.makedirs(default_dir, exist_ok=True)
        print(f"Models will be saved to: {default_dir}")

    load_model(args.model)

    server = HTTPServer((args.host, args.port), TranscribeHandler)
    print(f"WhisperServer listening on http://{args.host}:{args.port}")
    print(f"  POST /transcribe  -- transcribe audio (wav)")
    print(f"  GET  /health      -- health check")
    print(f"  GET  /            -- server info")
    sys.stdout.flush()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
