from __future__ import annotations

import os
import shlex
import subprocess
from dataclasses import dataclass
from typing import List, Tuple


SHELL_DISALLOWED_CHARS = set(";|&><`$\n\r")


def _is_truthy(val: str | None) -> bool:
    if not val:
        return False
    return val.strip().lower() in {"1", "true", "yes", "on"}


def _get_allowlist() -> List[str]:
    env_val = os.getenv("SHELL_TOOL_ALLOWLIST")
    if env_val:
        toks = [t.strip() for t in env_val.split(",")]
        return [t for t in toks if t]
    # conservative defaults
    return [
        "echo",
        "ls",
        "pwd",
        "cat",
        "head",
        "tail",
        "wc",
        "grep",
        "rg",
    ]


@dataclass
class ShellResult:
    command: str
    allowed: bool
    returncode: int
    stdout: str
    stderr: str
    truncated: bool

    def format_text(self) -> str:
        if not self.allowed:
            return (
                "Shell command rejected. Allowed commands: "
                + ", ".join(_get_allowlist())
            )
        parts = [f"Command: {self.command}", f"Exit code: {self.returncode}"]
        if self.stdout:
            parts.append("Stdout:\n" + self.stdout)
        if self.stderr:
            parts.append("Stderr:\n" + self.stderr)
        if self.truncated:
            parts.append("(output truncated)")
        return "\n".join(parts)


def run_shell(command: str, *, timeout_s: float | None = None, max_output: int | None = None) -> ShellResult:
    """Run a shell command with a strict allowlist and safety guards.

    - Validates the executable against an allowlist (env SHELL_TOOL_ALLOWLIST or defaults).
    - Rejects dangerous shell metacharacters and multi-line input.
    - Enforces timeout (env SHELL_TOOL_TIMEOUT, default 5s) and output truncation (env SHELL_TOOL_MAX_OUTPUT, default 8000 chars).
    - Returns a structured result with stdout/stderr/return code, formatted via `format_text()`.
    """
    allowlist = _get_allowlist()
    timeout = float(os.getenv("SHELL_TOOL_TIMEOUT") or (timeout_s if timeout_s else 5))
    max_len = int(os.getenv("SHELL_TOOL_MAX_OUTPUT") or (max_output if max_output else 8000))

    # Hard reject disallowed characters early
    if any(ch in SHELL_DISALLOWED_CHARS for ch in command):
        return ShellResult(command=command, allowed=False, returncode=127, stdout="", stderr="", truncated=False)

    try:
        parts = shlex.split(command)
    except Exception:
        return ShellResult(command=command, allowed=False, returncode=127, stdout="", stderr="", truncated=False)

    if not parts:
        return ShellResult(command=command, allowed=False, returncode=127, stdout="", stderr="", truncated=False)

    exe = parts[0]
    if exe not in allowlist:
        return ShellResult(command=command, allowed=False, returncode=127, stdout="", stderr="", truncated=False)

    try:
        proc = subprocess.run(parts, capture_output=True, text=True, timeout=timeout, check=False)
        stdout = proc.stdout or ""
        stderr = proc.stderr or ""
    except subprocess.TimeoutExpired:
        return ShellResult(command=command, allowed=True, returncode=124, stdout="", stderr="Timed out", truncated=False)
    except Exception as e:
        return ShellResult(command=command, allowed=True, returncode=1, stdout="", stderr=str(e), truncated=False)

    truncated = False
    if len(stdout) > max_len:
        stdout = stdout[:max_len]
        truncated = True
    if len(stderr) > max_len:
        stderr = stderr[:max_len]
        truncated = True

    return ShellResult(
        command=command,
        allowed=True,
        returncode=proc.returncode,
        stdout=stdout,
        stderr=stderr,
        truncated=truncated,
    )

