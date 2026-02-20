#!/usr/bin/env python
"""minicoder/cli/main.py — CLI interface logic."""
import sys
from ..agents.agent import MiniCoderAgent

def interactive_shell():  
    print("\033[34m" + "="*50 + "\033[0m")
    print("\033[1;34m" + " 🚀 MiniCoder Plus - Agent CLI " + "\033[0m")
    print("\033[34m" + "="*50 + "\033[0m")
    print("Type 'exit' or 'q' to quit. Ask me to write, fix, or search code.")
    
    agent = MiniCoderAgent()
    history = []
    
    while True:
        try:
            query = input("\033[1;36m>> \033[0m").strip()
            if not query:
                continue
            if query.lower() in ("exit", "q", "quit"):
                print("👋 Happy coding!")
                break
            
            response = agent.run(query, history)
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
