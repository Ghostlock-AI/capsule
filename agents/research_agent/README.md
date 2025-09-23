# Simple Research Agent

This repository contains a minimal, research agent that accepts a
command-line query, performs a lightweight internet search, and stores the
results in a notes file using a shell command. It serves as the baseline system
before introducing prompt-injection or hijacking behavior.

Features

- shell tool for executing commands
- Tavily search, (you need a tavily search API key)
- Shell tool that creates `output/research_summary.txt` via a captured
  `bash` command, demonstrating basic tool usage.
- Timestamped stdout logs describing each step of the agent run.

Setup

- Requires Python 3.8+
- Install dependencies:
  ```bash
  pip install -r requirements.txt
  ```
  You need a `.env` in the `research_agent directory`.
  It should have this in it

```bash
OPENAI_API_KEY=
TAVILY_API_KEY=
```

to easily make the `.env` fill out `env-template` then

```bash
mv env-template .env
```

`.env` is `.gitignore`d so it will not be commit-able.

Usage

```bash
python3 src/main.py
```

You will see a prompt on command line.
