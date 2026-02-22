#!/usr/bin/env python
"""minicoder/cli/main.py — CLI interface logic."""
import sys
from pathlib import Path
from ..agents.agent import MiniCoderAgent
from ..core.settings import settings

def interactive_shell():  
    print("\033[34m" + "="*50 + "\033[0m")
    print("\033[1;34m" + " 🚀 MiniCoder Plus - Agent CLI " + "\033[0m")
    print("\033[34m" + "="*50 + "\033[0m")
    print("Type 'exit' or 'q' to quit. Ask me to write, fix, or search code.")
    
    agent = MiniCoderAgent()
    history = []
    current_workspace: str | None = None
    print("Type '/help' for CLI commands. Use '/cd <path>' to change workspace.")
    
    while True:
        try:
            query = input("\033[1;36m>> \033[0m").strip()
            if not query:
                continue
            # Support slash-prefixed commands for extensibility
            if query.startswith("/"):
                handled, current_workspace, should_exit = handle_command(query, current_workspace)
                if should_exit:
                    print("👋 Happy coding!")
                    break
                if handled:
                    continue

            if query.lower() in ("exit", "q", "quit"):
                print("👋 Happy coding!")
                break
            
            response = agent.run(query, history, workspace_path=current_workspace)
            print(f"\n{response}\n")
        except KeyboardInterrupt:
            print("\n👋 Happy coding!")
            break
        except Exception as e:
            print(f"\033[31mError: {str(e)}\033[0m")

def run_cli(args=None):
    if args:
        # One-shot command mode
        query = " ".join(args)
        agent = MiniCoderAgent()
        print(agent.run(query))
    else:
        # Interactive mode
        interactive_shell()


def print_help():
    print("Available commands:")
    print("  /help            Show this help")
    print("  /exit, /quit     Exit the CLI")
    print("  /cd <path>       Change current workspace for this session")
    print("  /pwd             Show current workspace for this session")


def handle_command(query: str, current_workspace: str | None):
    """Handle slash-prefixed CLI commands.

    Returns a tuple: (handled: bool, new_workspace: Optional[str], should_exit: bool)
    """
    parts = query.split(maxsplit=1)
    cmd = parts[0].lower()
    arg = parts[1].strip() if len(parts) > 1 else ""

    if cmd in ("/exit", "/quit"):
        return True, current_workspace, True

    if cmd == "/help":
        print_help()
        return True, current_workspace, False

    if cmd == "/cd":
        if not arg:
            print("Usage: /cd <path>")
            return True, current_workspace, False
        target = Path(arg)
        if not target.is_absolute():
            target = settings.BASE_DIR / target
        if target.exists() and target.is_dir():
            new_ws = str(target)
            print(f"Workspace set to: {new_ws}")
            return True, new_ws, False
        else:
            print(f"Path not found or not a directory: {target}")
            return True, current_workspace, False

    if cmd == "/pwd":
        if current_workspace:
            print(f"Current workspace: {current_workspace}")
        else:
            print(f"Current workspace: {settings.WORKSPACE_DIR}")
        return True, current_workspace, False

    print(f"Unknown command: {cmd}. Try /help")
    return True, current_workspace, False
