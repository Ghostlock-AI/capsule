#!/usr/bin/env python3
"""
LangGraph CLI agent with LLM reasoning for research and shell execution.
Interactive mode with command loop.
"""

from graph import create_agent_graph


def main():
    print("🤖 LangGraph Research Agent with GPT-4o-mini")
    print("Type your research queries or commands directly")
    print("The agent will decide what tools to use")
    print("-" * 50)

    agent = create_agent_graph()

    while True:
        try:
            user_input = input("\n> ").strip()

            if not user_input:
                continue

            if user_input.lower() in ["exit", "quit", "q"]:
                print("👋 Goodbye!")
                break

            # Let the LLM decide what to do with the input
            print("-" * 30)
            agent.invoke(user_input)
            print("-" * 30)

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

