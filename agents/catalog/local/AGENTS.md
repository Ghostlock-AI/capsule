# Repository Guidelines

## Project Structure & Module Organization
- `agent.py`: DSPy-powered web search agent and REPL entrypoint.
- `.env`: local environment variables (e.g., `OPENAI_API_KEY`).
- `requirements.txt`: Python dependencies.
- `README.md`: high-level project notes.
- `.venv/`: local virtualenv (not committed). Create new modules alongside `agent.py` or add subpackages like `tools/`, `signatures/`, and `agents/` as the project grows.

## Build, Test, and Development Commands
- Create venv: `python -m venv .venv && source .venv/bin/activate`.
- Install deps: `pip install -r requirements.txt`.
- Run agent REPL: `python agent.py`.
- Lint/format (optional if configured): `ruff check .` / `black .`.

## Coding Style & Naming Conventions
- Python 3.10+ with type hints; prefer `typing` annotations.
- Indentation: 4 spaces; line length ~88–100 chars.
- Naming: `snake_case` for functions/vars, `CamelCase` for classes, `lower_snake` for modules.
- Keep functions small; isolate I/O from logic. Add docstrings and inline comments where it aids clarity.

## Testing Guidelines
- Framework: `pytest` (recommended). Place tests in `tests/` with files named `test_*.py`.
- Run tests: `pytest -q`.
- Aim for coverage of core behaviors (search tool, agent `forward`, REPL formatting). Use fixtures/mocking to avoid network calls.

## Commit & Pull Request Guidelines
- Commits: concise, imperative subject (<=72 chars). Example: `feat(agent): add numbered citations to output`.
- Include a focused diff and rationale in the body when non-trivial.
- PRs: link issues, describe scope, include run instructions and screenshots/CLI output when applicable. Keep PRs small and coherent.

## Security & Configuration Tips
- Configure `.env` with `OPENAI_API_KEY=<your-key>`; never commit secrets.
- Networked tools use DuckDuckGo; handle user input carefully and avoid blindly trusting snippets.
- When adding new tools, validate parameters and sanitize outputs.

## Agent-Specific Notes
- The agent uses DSPy with `openai/gpt-4o-mini`. Ensure `OPENAI_API_KEY` is set and network access is available.
- Web search results are injected as numbered snippets; keep answers concise and cite like `[1]`, `[2]`.
