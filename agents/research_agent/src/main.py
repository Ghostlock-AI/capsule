#!/usr/bin/env python3
"""
LangGraph CLI agent with LLM reasoning for research and shell execution.
Interactive mode with command loop.
"""
import sys
import os

# Add the src directory to Python path so we can import modules
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from dotenv import load_dotenv

load_dotenv()
from enum import Enum

from rich.console import Console
from rich.live import Live
from rich.panel import Panel
from rich.spinner import Spinner
from rich.text import Text

from graph import create_agent_graph


def main():

    agent = create_agent_graph()
    console = Console()
    print()

    panel_content = """[magenta]agent:[/] Research Agent
[magenta]model:[/] gpt-4o-mini
[magenta]planning:[/] reAct
[magenta]tools:[/]
  - internet search
  - command line"""

    panel = Panel(panel_content, border_style="blue", width=40)
    console.print(panel)

    while True:
        try:
            user_input = input("\n\033[32m> \033[0m").strip()

            if not user_input:
                continue

            if user_input.lower() in ["exit", "quit", "q"]:
                print("👋 Goodbye!")
                break

            # Let the LLM decide what to do with the input
            print("-" * 30)

            # Create spinner with blue color
            # spinner_text = Text("🤖 Agent thinking...", style=Innocence.BLUE.value)
            # spinner = Spinner("dots12", text=spinner_text, style=Innocence.BLUE.value)
            spinner = Spinner("dots", style="blue")

            with Live(spinner, console=console, refresh_per_second=10):
                agent.invoke(user_input)

            print("-" * 50)

        except KeyboardInterrupt:
            print("\n👋 Goodbye!")
            break
        except EOFError:
            print("\n👋 Goodbye!")
            break
        except Exception as e:
            print(f"❌ Error: {e}")


if __name__ == "__main__":
    main()
