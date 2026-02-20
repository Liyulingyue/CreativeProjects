#!/usr/bin/env python
"""mini_coder.py — MiniCoder Agent REPL"""
import os
import sys

# load environment variables if user has python-dotenv
try:
    from dotenv import load_dotenv
    load_dotenv()
except Exception:
    pass

from agent import MiniCoderAgent

def interactive_shell():  
    print("\033[34m" + "="*50 + "\033[0m")
    print("\033[1;34m" + " 🚀 MiniCoder - Agent Coder Shell " + "\033[0m")
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

if __name__ == "__main__":
    if len(sys.argv) > 1:
        # One-shot command mode
        query = " ".join(sys.argv[1:])
        agent = MiniCoderAgent()
        print(agent.run(query))
    else:
        # Interactive mode
        interactive_shell()
