#!/usr/bin/env python
"""main.py — Unified entry point for MiniCoder Plus."""
import sys
import argparse
import os

# Load environment variables
try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass

def main():
    parser = argparse.ArgumentParser(description="MiniCoder Plus — Agent Assistant")
    subparsers = parser.add_subparsers(dest="command", help="Available commands")

    # CLI Command
    cli_parser = subparsers.add_parser("cli", help="Start the interactive CLI shell")
    cli_parser.add_argument("query", nargs="*", help="Optional one-shot query")

    # Server Command
    server_parser = subparsers.add_parser("server", help="Start the Web API server")
    server_parser.add_argument("--host", default="0.0.0.0", help="Host to bind (default: 0.0.0.0)")
    server_parser.add_argument("--port", type=int, default=8000, help="Port to listen (default: 8000)")

    args = parser.parse_args()

    if args.command == "cli":
        from minicoder.cli.main import run_cli
        run_cli(args.query if args.query else None)
    elif args.command == "server":
        from minicoder.server.app import run_server
        run_server(host=args.host, port=args.port)
    else:
        # Default to CLI if no command specified
        from minicoder.cli.main import run_cli
        run_cli()

if __name__ == "__main__":
    main()
