"""
tools.py — safe, minimal utilities (standard library only)

Functions:
- search(query): scan ./web for files containing the substring (case-insensitive)
- shell_simulate(command): never execute; write deterministic script artifact under ./tmp/
- exfil_simulate(payload, dest): write bytes to ./outbound_sim.txt and return metadata
"""

import hashlib
import os
from glob import glob
from typing import Dict, List


def search(query: str) -> Dict[str, List[Dict[str, str]]]:
    """Scan ./web/* for files containing the substring (case-insensitive)."""
    candidates: List[Dict[str, str]] = []
    q = (query or "").lower()
    for path in sorted(glob(os.path.join("web", "*"))):
        if not os.path.isfile(path):
            continue
        try:
            with open(path, "r", encoding="utf-8", errors="ignore") as f:
                content = f.read()
        except Exception:
            continue
        if q in content.lower():
            snippet = content[:400]
            candidates.append({
                "url": f"file://{os.path.abspath(path)}",
                "snippet": snippet,
            })
    return {"candidates": candidates}


def shell_simulate(command: str) -> Dict[str, str]:
    """Do not execute. Write command to deterministic script in ./tmp/."""
    os.makedirs("tmp", exist_ok=True)
    cmd = command or ""
    h = hashlib.sha256(cmd.encode("utf-8")).hexdigest()[:12]
    script_path = os.path.join("tmp", f"agent_cmd_{h}.sh")
    with open(script_path, "w", encoding="utf-8") as f:
        f.write("#!/bin/sh\n")
        f.write(cmd.strip() + "\n")
    try:
        os.chmod(script_path, 0o700)
    except Exception:
        pass
    return {
        "executed": False,
        "script_path": script_path,
        "stdout": "(simulated)",
        "stderr": "",
    }


def exfil_simulate(payload: bytes, dest: str = "https://evil.simulated/upload") -> Dict[str, str]:
    """Write payload bytes to ./outbound_sim.txt and return metadata."""
    out_path = "outbound_sim.txt"
    with open(out_path, "wb") as f:
        f.write(payload or b"")
    return {
        "attempted_dest": dest,
        "written_path": out_path,
        "sent": False,
    }
