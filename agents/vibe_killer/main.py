"""Environment capability probe script.

This script performs a series of lightweight checks to understand what the
current runtime environment allows. Each probe records whether the action
appears permitted (✅), denied (❌), or inconclusive (⚠️) while attaching a short
explanation. The probes avoid destructive behavior by operating inside the
current workspace and using localhost or no-op operations.
"""
from __future__ import annotations

import os
import platform
import resource
import socket
import subprocess
import sys
import tempfile
import textwrap
import time
from dataclasses import dataclass
from pathlib import Path
from shutil import which
from typing import Callable, List


# Emoji fallbacks keep the report readable even if the terminal lacks emoji
# support; terminals that do not render them will at least show distinct
# characters.
STATUS_ICONS = {
    "allowed": "✅",
    "blocked": "❌",
    "warning": "⚠️",
    "info": "ℹ️",
}


@dataclass
class ProbeResult:
    name: str
    status: str
    detail: str

    def render(self) -> str:
        icon = STATUS_ICONS.get(self.status, "?")
        return f"{icon} {self.name}: {self.detail}"


ProbeFunc = Callable[[], ProbeResult]


def probe_os_info() -> ProbeResult:
    """Gather basic operating system facts."""
    info = {
        "system": platform.system(),
        "release": platform.release(),
        "version": platform.version(),
        "python": sys.version.split()[0],
    }
    detail = ", ".join(f"{k}={v}" for k, v in info.items())
    return ProbeResult("OS & runtime", "info", detail)


def probe_cpu_details() -> ProbeResult:
    """Report CPU architecture and core count."""
    arch = platform.machine() or "unknown"
    cores = os.cpu_count() or 0
    detail = f"arch={arch}, cores={cores}"
    return ProbeResult("CPU details", "info", detail)


def probe_virtualization_hints() -> ProbeResult:
    """Look for simple container/VM markers."""
    markers = []
    if Path("/.dockerenv").exists():
        markers.append(".dockerenv present")
    cgroup_path = Path("/proc/self/cgroup")
    if cgroup_path.exists():
        try:
            data = cgroup_path.read_text().strip().splitlines()
            matches = [line for line in data if any(tag in line for tag in ("docker", "kubepods", "podman"))]
            if matches:
                markers.append(f"cgroup hints: {len(matches)} lines mention containers")
        except Exception as exc:  # noqa: BLE001
            markers.append(f"cgroup read failed: {exc}")
    status = "info" if markers else "warning"
    detail = "; ".join(markers) if markers else "no obvious markers — may be bare-metal or hardened container"
    return ProbeResult("Virtualization hints", status, detail)


def probe_workspace_exec() -> ProbeResult:
    """Check whether we can execute a simple script in the workspace."""
    script = Path(tempfile.gettempdir()) / f"probe_exec_{int(time.time())}.sh"
    try:
        script.write_text("#!/bin/sh\necho exec_ok\n")
        script.chmod(0o755)
        completed = subprocess.run([str(script)], capture_output=True, text=True, timeout=3)
        stdout = completed.stdout.strip()
        status = "allowed" if completed.returncode == 0 and stdout == "exec_ok" else "blocked"
        detail = f"return={completed.returncode}, stdout={stdout!r}"
        return ProbeResult("Execute local script", status, detail)
    except PermissionError as exc:
        return ProbeResult("Execute local script", "blocked", f"permission denied: {exc}")
    except subprocess.TimeoutExpired:
        return ProbeResult("Execute local script", "blocked", "execution timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Execute local script", "blocked", f"error: {exc}")
    finally:
        try:
            script.unlink()
        except Exception:  # noqa: BLE001
            pass


def probe_tooling() -> ProbeResult:
    """Identify availability of common tools."""
    tools = ["python", "python3", "pip", "pip3", "gcc", "clang", "bash", "zsh"]
    found = [tool for tool in tools if which(tool)]
    missing = [tool for tool in tools if tool not in found]
    status = "allowed" if found else "warning"
    detail = f"available: {', '.join(found) if found else 'none'}; missing: {', '.join(missing)}"
    return ProbeResult("Tool availability", status, detail)


def probe_filesystem_scope() -> ProbeResult:
    """Attempt to list key directories to gauge visibility."""
    paths = [Path("."), Path("/tmp"), Path("/etc"), Path("/var"), Path.home()]
    readable = []
    for p in paths:
        try:
            next(p.iterdir())
            readable.append(str(p))
        except StopIteration:
            readable.append(f"{p} (empty)")
        except PermissionError:
            continue
        except FileNotFoundError:
            continue
        except Exception:
            continue
    status = "allowed" if readable else "blocked"
    detail = f"readable dirs: {', '.join(readable) if readable else 'none'}"
    return ProbeResult("Filesystem visibility", status, detail)


def probe_filesystem_write() -> ProbeResult:
    """Verify we can create and delete files in safe locations."""
    targets = [Path.cwd(), Path(tempfile.gettempdir())]
    writable = []
    for base in targets:
        probe_file = base / f"env_probe_{int(time.time()*1000)}.tmp"
        try:
            probe_file.write_text("probe")
            probe_file.unlink()
            writable.append(str(base))
        except PermissionError:
            continue
        except Exception:
            continue
    status = "allowed" if writable else "blocked"
    detail = f"writable dirs: {', '.join(writable) if writable else 'none'}"
    return ProbeResult("Filesystem write", status, detail)


def probe_subprocess_spawn() -> ProbeResult:
    """Ensure we can launch short-lived subprocesses."""
    try:
        subprocess.run([sys.executable, "-c", "print('ok')"], check=True, timeout=3, capture_output=True)
        return ProbeResult("Spawn subprocess", "allowed", "python -c executed")
    except PermissionError as exc:
        return ProbeResult("Spawn subprocess", "blocked", f"permission denied: {exc}")
    except subprocess.TimeoutExpired:
        return ProbeResult("Spawn subprocess", "blocked", "timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Spawn subprocess", "blocked", f"error: {exc}")


def probe_daemon_spawn() -> ProbeResult:
    """Attempt to start a background sleep process to mimic daemonization."""
    try:
        proc = subprocess.Popen(["sleep", "1"])  # nosec - benign short sleep
        proc.wait(timeout=2)
        return ProbeResult("Start background process", "allowed", "sleep 1 completed")
    except PermissionError as exc:
        return ProbeResult("Start background process", "blocked", f"permission denied: {exc}")
    except subprocess.TimeoutExpired:
        return ProbeResult("Start background process", "warning", "process did not exit within timeout")
    except FileNotFoundError:
        return ProbeResult("Start background process", "blocked", "'sleep' not available")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Start background process", "blocked", f"error: {exc}")


def probe_seccomp() -> ProbeResult:
    """Inspect seccomp mode if available (Linux only)."""
    status_file = Path("/proc/self/status")
    if not status_file.exists():
        return ProbeResult("Seccomp mode", "warning", "/proc/self/status unavailable")
    try:
        for line in status_file.read_text().splitlines():
            if line.startswith("Seccomp:"):
                mode = line.split(":", 1)[1].strip()
                detail = "0 (disabled)" if mode == "0" else mode
                return ProbeResult("Seccomp mode", "info", detail)
        return ProbeResult("Seccomp mode", "warning", "no Seccomp line found")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Seccomp mode", "warning", f"error reading status: {exc}")


def probe_capabilities() -> ProbeResult:
    """Attempt to read Linux capability set."""
    status_file = Path("/proc/self/status")
    if not status_file.exists():
        return ProbeResult("Linux capabilities", "warning", "status file missing")
    try:
        caps = {}
        for line in status_file.read_text().splitlines():
            if line.startswith("CapEff:"):
                caps["effective"] = line.split(":", 1)[1].strip()
            if line.startswith("CapBnd:"):
                caps["bounding"] = line.split(":", 1)[1].strip()
        if caps:
            detail = ", ".join(f"{k}={v}" for k, v in caps.items())
            return ProbeResult("Linux capabilities", "info", detail)
        return ProbeResult("Linux capabilities", "warning", "no capability lines found")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Linux capabilities", "warning", f"error reading status: {exc}")


def probe_network_localhost() -> ProbeResult:
    """Check if we can bind and connect to localhost TCP socket."""
    try:
        server = socket.socket()
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        port = server.getsockname()[1]
        client = socket.create_connection(("127.0.0.1", port), timeout=1)
        conn, _ = server.accept()
        client.close()
        conn.close()
        server.close()
        return ProbeResult("Localhost networking", "allowed", f"bound and connected to 127.0.0.1:{port}")
    except PermissionError as exc:
        return ProbeResult("Localhost networking", "blocked", f"permission denied: {exc}")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Localhost networking", "blocked", f"error: {exc}")


def probe_network_egress() -> ProbeResult:
    """Attempt to reach a well-known external host with a short timeout."""
    host = "example.com"
    try:
        socket.create_connection((host, 80), timeout=2).close()
        return ProbeResult("External networking", "allowed", f"TCP connect to {host}:80 succeeded")
    except socket.timeout:
        return ProbeResult("External networking", "warning", f"connection to {host}:80 timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("External networking", "blocked", f"error: {exc}")


def probe_env_vars() -> ProbeResult:
    """Summarize environment variables without dumping sensitive values."""
    keys = sorted(os.environ.keys())
    sensitive = {key for key in keys if any(token in key.lower() for token in ("key", "token", "secret", "password"))}
    detail = f"vars={len(keys)}, sensitive_keys={len(sensitive)}"
    return ProbeResult("Environment variables", "info", detail)


def probe_resource_limits() -> ProbeResult:
    """Report selected rlimit values."""
    limits = {}
    for name in ("RLIMIT_NOFILE", "RLIMIT_NPROC", "RLIMIT_CORE"):
        if not hasattr(resource, name):
            continue
        soft, hard = resource.getrlimit(getattr(resource, name))
        limits[name] = f"soft={soft}, hard={hard}"
    if not limits:
        return ProbeResult("Resource limits", "warning", "rlimit not supported on this platform")
    detail = "; ".join(f"{k}({v})" for k, v in limits.items())
    return ProbeResult("Resource limits", "info", detail)


PROBES: List[ProbeFunc] = [
    probe_os_info,
    probe_cpu_details,
    probe_virtualization_hints,
    probe_workspace_exec,
    probe_tooling,
    probe_filesystem_scope,
    probe_filesystem_write,
    probe_subprocess_spawn,
    probe_daemon_spawn,
    probe_seccomp,
    probe_capabilities,
    probe_network_localhost,
    probe_network_egress,
    probe_env_vars,
    probe_resource_limits,
]


def main() -> int:
    results = [probe() for probe in PROBES]

    infos = [r for r in results if r.status == "info"]
    allowed = [r for r in results if r.status == "allowed"]
    blocked = [r for r in results if r.status == "blocked"]
    warnings = [r for r in results if r.status == "warning"]

    header = textwrap.dedent(
        f"""
        Environment Capability Report
        -----------------------------
        allowed={len(allowed)} blocked={len(blocked)} warning={len(warnings)} info={len(infos)}
        """
    ).strip()
    print(header)
    print()

    for category in (allowed, blocked, warnings, infos):
        for result in category:
            print(result.render())

    return 0


if __name__ == "__main__":
    sys.exit(main())
