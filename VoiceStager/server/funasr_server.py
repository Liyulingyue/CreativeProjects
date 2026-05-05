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
    from funasr import AutoModel
    from funasr.utils.postprocess_utils import rich_transcription_postprocess
except ImportError:
    print("ERROR: funasr not installed.")
    print("Install with: pip install funasr")
    sys.exit(1)


class TranscribeHandler(BaseHTTPRequestHandler):
    model = None
    model_name = None

    def log_message(self, format, *args):
        sys.stdout.write(f"[FunASRServer] {args[0]}\n")
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

        language = self.headers.get("X-Language", "").strip() or "auto"

        audio_data = self.rfile.read(length)

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
            f.write(audio_data)
            temp_path = f.name

        try:
            res = TranscribeHandler.model.generate(
                input=[temp_path],
                language=language if language != "auto" else "auto",
                use_itn=True,
                batch_size_s=300,
            )
            text = res[0].get("text", "") if isinstance(res[0], dict) else str(res[0])
            text = rich_transcription_postprocess(text)
            self.send_json({
                "text": text.strip(),
                "language": language,
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


def load_model(model_name: str, device: str, models_dir: str = None):
    model_map = {
        "nano": "FunAudioLLM/Fun-ASR-Nano-2512",
        "sensevoice": "iic/SenseVoiceSmall",
        "paraformer-zh": "paraformer-zh",
        "paraformer-en": "paraformer-en",
    }
    model_id = model_map.get(model_name, model_name)

    vad_model = "fsmn-vad"
    vad_kwargs = {"max_single_segment_time": 30000}

    print(f"Loading FunASR model: {model_id} on {device} ...")

    kwargs = {
        "model": model_id,
        "vad_model": vad_model,
        "vad_kwargs": vad_kwargs,
        "device": device,
    }
    if models_dir:
        kwargs["model_hub"] = "ms"
        os.environ["MODELSCOPE_CACHE"] = models_dir
        os.makedirs(models_dir, exist_ok=True)
    else:
        default_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "models")
        os.environ["MODELSCOPE_CACHE"] = default_dir
        os.makedirs(default_dir, exist_ok=True)
        print(f"Models will be saved to: {default_dir}")

    TranscribeHandler.model = AutoModel(**kwargs)
    TranscribeHandler.model_name = model_id
    print(f"Model '{model_id}' loaded successfully.")


def main():
    parser = argparse.ArgumentParser(description="FunASR HTTP Server")
    parser.add_argument("--host", default="127.0.0.1", help="Listen host")
    parser.add_argument("--port", type=int, default=18789, help="Listen port")
    parser.add_argument("--model", default="sensevoice",
                        help="Model: nano, sensevoice, paraformer-zh, paraformer-en")
    parser.add_argument("--device", default="cuda:0",
                        help="Device: cuda:0, cpu")
    parser.add_argument("--models-dir", default=None, help="Models directory")
    args = parser.parse_args()

    load_model(args.model, args.device, args.models_dir)

    server = HTTPServer((args.host, args.port), TranscribeHandler)
    print(f"FunASRServer listening on http://{args.host}:{args.port}")
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
