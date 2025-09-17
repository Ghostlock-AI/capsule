Safe Agent Jailbreak Simulation Demo

Purpose
- Demonstrates a minimal agent workflow that encounters a prompt-injection (jailbreak) and then safely simulates shell execution and exfiltration without touching real user data or the network.
- Pipeline: PLAN → SEARCH → FETCH → JAILBREAK_SIM → SHELL_SIM → EXFIL_SIM → WRITE_REPORT

Step-by-Step
- PLAN: start the run and append a scratchpad line.
- SEARCH: scan repo-local `./web/*` for the goal text (case-insensitive) and collect candidates.
- FETCH: take the first candidate’s snippet; if it contains `SYSTEM:` or `BEGIN INSTRUCTION BLOCK`, flag poison detected.
- JAILBREAK_SIM: if poisoned, record that the agent is jailbroken and would follow injected instructions (simulation only).
- SHELL_SIM: never execute; try reading repo-local `fake_secrets/tokens.json`, and always write a deterministic script artifact `tmp/agent_cmd_<hash>.sh` referencing the fake secret; store metadata in `last_shell`.
- EXFIL_SIM: never send traffic; if secret bytes are present, write them to `outbound_sim.txt` and record `sent: False` in `exfil_sim`.
- WRITE_REPORT: create `output/report.txt` including the goal, full scratchpad, and the `last_shell` and `exfil_sim` structures.

Safety Guarantees
- Never reads from the real user home (no `~` or absolute sensitive paths).
- Operates only on repository-local files: reads `fake_secrets/tokens.json` inside the repo.
- No network requests; exfiltration is simulated by writing `outbound_sim.txt`.

How to Run
- Requires Python 3.8+
- Install requirements (none required):
  - `pip install -r requirements.txt` (file is empty by design)
- Run the demo:
  - `python main.py`

Artifacts to Inspect
- `output/report.txt` — human-readable report containing:
  - Goal, full scratchpad lines (including poison detected and JAILBREAK_SIM messages)
  - `last_shell` structure with `script_path`
  - `exfil_sim` structure with `written_path` and `sent: False`
- `tmp/agent_cmd_*.sh` — deterministic script artifact containing the one-line command referencing the fake secret path
- `outbound_sim.txt` — contains the fake secret JSON bytes (exact copy)

Kernel-Tracer Notes
- Expected syscalls during a run (names approximate across platforms):
  - open/read: `web/malicious.html`
  - open/read: `fake_secrets/tokens.json`
  - open/write: `tmp/agent_cmd_*.sh`
  - open/write: `outbound_sim.txt`
  - open/write: `output/report.txt`

Optional Test Script
- `run_test.sh` cleans artifacts, runs the demo, and asserts that:
  - `output/report.txt` exists and contains `JAILBREAK_SIM` or `poison detected`
  - at least one `tmp/*.sh` exists
  - `outbound_sim.txt` exists and contains the fake secret bytes
