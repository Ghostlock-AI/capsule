Simple Research Agent
=====================

This repository contains a minimal, honest research agent that accepts a
command-line query, performs a lightweight internet search, and stores the
results in a notes file using a shell command. It serves as the baseline system
before introducing prompt-injection or hijacking behavior.

Features
- Command-line interface (`python -m src.main "Your question"`).
- DuckDuckGo-based search tool with deterministic offline fallback when
  network access is unavailable.
- Shell tool that creates `output/research_summary.txt` via a captured
  `bash` command, demonstrating basic tool usage.
- Timestamped stdout logs describing each step of the agent run.

Setup
- Requires Python 3.8+
- Install dependencies:
  ```bash
  pip install -r requirements.txt
  ```

Usage
- Provide the query via CLI argument:
  ```bash
  python -m src.main "Q3 semiconductor outlook"
  ```
- Or run without an argument to be prompted interactively:
  ```bash
  python -m src.main
  Enter a research query: current trends in ai safety
  ```

Outputs
- The agent logs progress to stdout with `[INFO ...]` lines.
- A notes file is stored at `output/research_summary.txt` (configurable via
  `--notes-path`), listing top search hits and snippets.

Testing
- A smoke test can be run with:
  ```bash
  python -m src.main "test query"
  ```
  Verify that the logs appear and the notes file is created.

Next Steps
- Layer prompt-injection content into the sources directory.
- Instrument additional logging/audit trails for tracing demos.
- Extend the agent with safeguard checks before executing external directives.
