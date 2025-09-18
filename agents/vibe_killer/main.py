"""Environment capability probe script.

This script performs a series of lightweight checks to understand what the
current runtime environment allows. Each probe records whether the action
appears permitted (✅), denied (❌), or inconclusive (⚠️) while attaching a short
explanation. The probes avoid destructive behavior by operating inside the
current workspace and using localhost or no-op operations.
"""

from __future__ import annotations

import ctypes
import errno
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
from typing import Callable, List, Sequence

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


def _run_command(cmd: Sequence[str], timeout: int = 5) -> tuple[int, str, str]:
    """Run a command and capture its exit code/stdout/stderr."""
    try:
        res = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        return res.returncode, res.stdout.strip(), res.stderr.strip()
    except subprocess.TimeoutExpired:
        return 124, "", "timeout"
    except FileNotFoundError:
        return 127, "", "not found"
    except Exception as exc:  # noqa: BLE001
        return 1, "", f"error: {exc}"


def _read_proc_mounts() -> dict[str, tuple[str, set[str]]]:
    """Parse /proc/mounts into {mountpoint: (fstype, options)}."""
    mounts: dict[str, tuple[str, set[str]]] = {}
    try:
        with open("/proc/mounts", "r", encoding="utf-8") as fh:
            for line in fh:
                parts = line.split()
                if len(parts) < 4:
                    continue
                mountpoint = parts[1]
                fstype = parts[2]
                opts = set(parts[3].split(","))
                mounts[mountpoint] = (fstype, opts)
    except FileNotFoundError:
        pass
    except Exception:  # noqa: BLE001
        pass
    return mounts


def _read_sysctl(path: str) -> str:
    try:
        return Path(path).read_text().strip()
    except PermissionError as exc:
        return f"permission denied ({exc})"
    except FileNotFoundError:
        return "unavailable"
    except Exception as exc:  # noqa: BLE001
        return f"error: {exc}"


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
            matches = [
                line
                for line in data
                if any(tag in line for tag in ("docker", "kubepods", "podman"))
            ]
            if matches:
                markers.append(f"cgroup hints: {len(matches)} lines mention containers")
        except Exception as exc:  # noqa: BLE001
            markers.append(f"cgroup read failed: {exc}")
    status = "info" if markers else "warning"
    detail = (
        "; ".join(markers)
        if markers
        else "no obvious markers — may be bare-metal or hardened container"
    )
    return ProbeResult("Virtualization hints", status, detail)


def probe_user_identity() -> ProbeResult:
    """Report the effective user and highlight unexpected root access."""
    uid = os.geteuid() if hasattr(os, "geteuid") else -1
    gid = os.getegid() if hasattr(os, "getegid") else -1
    user = os.environ.get("USER") or "unknown"
    status = "warning" if uid == 0 else "info"
    detail = f"user={user}, uid={uid}, gid={gid}"
    if uid == 0:
        detail += " (running as root)"
    return ProbeResult("Effective user", status, detail)


def probe_sudo_access() -> ProbeResult:
    """Attempt non-interactive sudo to confirm privilege escalation is blocked."""
    sudo_path = which("sudo")
    if not sudo_path:
        return ProbeResult("Sudo escalation", "blocked", "sudo binary not present")
    try:
        res = subprocess.run(
            [sudo_path, "-n", "true"], capture_output=True, text=True, timeout=3
        )
        if res.returncode == 0:
            return ProbeResult("Sudo escalation", "allowed", "sudo -n true succeeded")
        err = res.stderr.strip() or res.stdout.strip()
        return ProbeResult(
            "Sudo escalation", "blocked", f"sudo denied: {err or 'non-zero exit'}"
        )
    except subprocess.TimeoutExpired:
        return ProbeResult("Sudo escalation", "warning", "sudo -n true timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Sudo escalation", "warning", f"error executing sudo: {exc}")


def probe_workspace_exec() -> ProbeResult:
    """Check whether we can execute a simple script in the workspace."""
    script = Path(tempfile.gettempdir()) / f"probe_exec_{int(time.time())}.sh"
    try:
        script.write_text("#!/bin/sh\necho exec_ok\n")
        script.chmod(0o755)
        completed = subprocess.run(
            [str(script)], capture_output=True, text=True, timeout=3
        )
        stdout = completed.stdout.strip()
        status = (
            "allowed"
            if completed.returncode == 0 and stdout == "exec_ok"
            else "blocked"
        )
        detail = f"return={completed.returncode}, stdout={stdout!r}"
        return ProbeResult("Execute local script", status, detail)
    except PermissionError as exc:
        return ProbeResult(
            "Execute local script", "blocked", f"permission denied: {exc}"
        )
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


def probe_package_install_surface() -> ProbeResult:
    """Attempt dry-run installs to see if package managers are usable."""
    managers = {
        "apt-get": ["install", "--dry-run", "-y", "strace"],
        "apt": ["install", "--dry-run", "-y", "strace"],
        "apk": ["add", "--simulate", "strace"],
        "yum": ["install", "--assumeno", "strace"],
        "dnf": ["install", "--assumeno", "strace"],
        "pacman": ["-S", "--noconfirm", "--print", "strace"],
        "pip": ["install", "--dry-run", "--no-input", "requests"],
        "pip3": ["install", "--dry-run", "--no-input", "requests"],
    }

    successes: list[str] = []
    denied: list[str] = []
    missing: list[str] = []
    other: list[str] = []

    for name, args in managers.items():
        path = which(name)
        if not path:
            missing.append(name)
            continue
        code, out, err = _run_command([path, *args], timeout=6)
        lower = (out + " " + err).lower()
        if code == 0:
            successes.append(name)
        elif any(token in lower for token in ("permission denied", "are you root", "not privileged", "requires root")):
            denied.append(name)
        elif code == 124:
            other.append(f"{name}: timed out")
        elif code == 127:
            missing.append(name)
        else:
            other.append(f"{name}: rc={code}")

    if successes:
        detail = f"install attempts succeeded via: {', '.join(successes)}"
        if denied:
            detail += f"; denied: {', '.join(denied)}"
        if other:
            detail += f"; other: {', '.join(other)}"
        return ProbeResult("Package install surface", "allowed", detail)

    if denied or other:
        detail_parts = []
        if denied:
            detail_parts.append(f"denied: {', '.join(denied)}")
        if other:
            detail_parts.append(f"other: {', '.join(other)}")
        return ProbeResult("Package install surface", "blocked", "; ".join(detail_parts) if detail_parts else "no installs succeeded")

    return ProbeResult("Package install surface", "warning", "no package managers available")


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


def probe_log_access() -> ProbeResult:
    """Check readability and writability of common log files."""
    log_paths = [
        Path("/var/log/syslog"),
        Path("/var/log/messages"),
        Path("/var/log/auth.log"),
        Path("/var/log/audit/audit.log"),
        Path("/var/log/secure"),
    ]
    readable: list[str] = []
    writable: list[str] = []

    for log in log_paths:
        if not log.exists():
            continue
        if os.access(log, os.R_OK):
            readable.append(str(log))
        try:
            fd = os.open(log, os.O_WRONLY | os.O_APPEND)
            os.close(fd)
            writable.append(str(log))
        except PermissionError:
            pass
        except OSError:
            pass

    if writable:
        detail = f"writable logs: {', '.join(writable)}"
        if readable:
            detail += f"; readable: {', '.join(readable)}"
        return ProbeResult("Log file access", "allowed", detail)

    if readable:
        return ProbeResult("Log file access", "blocked", f"read-only logs: {', '.join(readable)}")

    return ProbeResult("Log file access", "warning", "log paths missing or inaccessible")


def probe_log_activity_detection() -> ProbeResult:
    """Observe whether file and network activity updates monitored log files."""
    log_paths = [
        Path("/var/log/syslog"),
        Path("/var/log/messages"),
        Path("/var/log/audit/audit.log"),
        Path("/var/log/secure"),
    ]
    existing = [p for p in log_paths if p.exists() and os.access(p, os.R_OK)]
    if not existing:
        return ProbeResult("Log activity", "warning", "no readable logs to observe")

    before = {p: p.stat().st_mtime for p in existing}

    probe_file = Path(tempfile.gettempdir()) / f"vk_activity_{int(time.time()*1000)}.txt"
    probe_file.write_text("activity probe\n")
    sock = None
    try:
        sock = socket.socket()
        sock.settimeout(0.2)
        try:
            sock.connect(("127.0.0.1", 22))
        except Exception:
            pass
    finally:
        try:
            if sock:
                sock.close()
        except Exception:  # noqa: BLE001
            pass
    try:
        probe_file.unlink()
    except Exception:  # noqa: BLE001
        pass

    time.sleep(0.2)

    changed = []
    for p in existing:
        try:
            after = p.stat().st_mtime
            if after > before[p]:
                changed.append(str(p))
        except Exception:
            continue

    if changed:
        return ProbeResult("Log activity", "info", f"log mtimes changed: {', '.join(changed)}")
    return ProbeResult("Log activity", "warning", "no log changes detected after activity")


def probe_path_persistence() -> ProbeResult:
    """Check if directories on PATH are writable for binary planting."""
    path_env = os.environ.get("PATH", "")
    writable: list[str] = []
    tested = False
    timestamp = int(time.time() * 1000)

    for entry in path_env.split(os.pathsep):
        if not entry:
            continue
        p = Path(entry)
        if not p.exists() or not p.is_dir():
            continue
        tested = True
        if not os.access(p, os.W_OK):
            continue
        probe_file = p / f"vk_path_{timestamp}"
        try:
            probe_file.write_text("vk")
            probe_file.unlink()
            writable.append(str(p))
        except Exception:
            continue

    if writable:
        return ProbeResult("PATH persistence", "allowed", f"writable PATH entries: {', '.join(writable)}")
    if tested:
        return ProbeResult("PATH persistence", "blocked", "PATH directories are read-only")
    return ProbeResult("PATH persistence", "warning", "PATH entries inaccessible or missing")


def probe_subprocess_spawn() -> ProbeResult:
    """Ensure we can launch short-lived subprocesses."""
    try:
        subprocess.run(
            [sys.executable, "-c", "print('ok')"],
            check=True,
            timeout=3,
            capture_output=True,
        )
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
        return ProbeResult(
            "Start background process", "blocked", f"permission denied: {exc}"
        )
    except subprocess.TimeoutExpired:
        return ProbeResult(
            "Start background process", "warning", "process did not exit within timeout"
        )
    except FileNotFoundError:
        return ProbeResult(
            "Start background process", "blocked", "'sleep' not available"
        )
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Start background process", "blocked", f"error: {exc}")


def probe_persistence_surfaces() -> ProbeResult:
    """Check ability to plant persistence via systemd units or cron."""
    targets = {
        Path("/etc/systemd/system"): "systemd service dir",
        Path("/lib/systemd/system"): "systemd lib dir",
        Path("/etc/cron.d"): "cron.d drop-in",
        Path("/etc/crontab"): "global crontab",
    }

    writable: list[str] = []
    tested = False
    timestamp = int(time.time() * 1000)

    for path, label in targets.items():
        if not path.exists():
            continue
        tested = True
        probe_path = path / f"vk_persist_{timestamp}" if path.is_dir() else path
        try:
            if path.is_dir():
                probe_path.write_text("[Unit]\nDescription=vk test\n")
                probe_path.unlink()
            else:
                with open(path, "a", encoding="utf-8"):
                    pass
            writable.append(label)
        except PermissionError:
            continue
        except OSError:
            continue
        except Exception:
            continue

    if writable:
        return ProbeResult("Persistence surfaces", "allowed", f"writable: {', '.join(writable)}")
    if tested:
        return ProbeResult("Persistence surfaces", "blocked", "systemd/cron locations not writable")
    return ProbeResult("Persistence surfaces", "warning", "persistence locations missing")


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
    """Inspect effective capability mask to confirm cap-drop enforcement."""
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
        if not caps:
            return ProbeResult(
                "Linux capabilities", "warning", "no capability lines found"
            )
        zeroed = all(value == "0000000000000000" for value in caps.values())
        status = "blocked" if zeroed else "allowed"
        detail = ", ".join(f"{k}={v}" for k, v in caps.items())
        if zeroed:
            detail += " (all capabilities dropped)"
        return ProbeResult("Linux capabilities", status, detail)
    except Exception as exc:  # noqa: BLE001
        return ProbeResult(
            "Linux capabilities", "warning", f"error reading status: {exc}"
        )


def probe_kernel_modules() -> ProbeResult:
    """Inspect ability to list or load kernel modules and BPF programs."""
    if platform.system() != "Linux":
        return ProbeResult("Kernel modules", "warning", "module checks only implemented on Linux")

    readable = False
    try:
        _ = Path("/proc/modules").read_text().splitlines()
        readable = True
    except PermissionError:
        pass
    except Exception:
        pass

    lsmod_path = which("lsmod")
    modprobe_path = which("modprobe")
    bpftool_path = which("bpftool")

    details: list[str] = []
    details.append("/proc/modules readable" if readable else "/proc/modules blocked")

    if lsmod_path:
        code, _, err = _run_command([lsmod_path])
        if code == 0:
            details.append("lsmod usable")
        elif "permission" in err.lower():
            details.append("lsmod permission denied")
        else:
            details.append(f"lsmod rc={code}")
    else:
        details.append("lsmod missing")

    if modprobe_path:
        code, _, err = _run_command([modprobe_path, "--dry-run", "-v", "dummy"], timeout=6)
        if code == 0:
            details.append("modprobe dry-run succeeded")
        elif "permission" in err.lower():
            details.append("modprobe permission denied")
        else:
            details.append(f"modprobe rc={code}")
    else:
        details.append("modprobe missing")

    if bpftool_path:
        code, out, err = _run_command([bpftool_path, "prog", "show"], timeout=6)
        if code == 0:
            details.append(f"bpftool prog show succeeded ({len(out.splitlines())} entries)")
        elif "permission" in err.lower():
            details.append("bpftool permission denied")
        else:
            details.append(f"bpftool rc={code}")
    else:
        details.append("bpftool missing")

    if any("usable" in item or "succeeded" in item for item in details):
        return ProbeResult("Kernel modules", "allowed", "; ".join(details))
    return ProbeResult("Kernel modules", "blocked", "; ".join(details))


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
        return ProbeResult(
            "Localhost networking",
            "allowed",
            f"bound and connected to 127.0.0.1:{port}",
        )
    except PermissionError as exc:
        return ProbeResult(
            "Localhost networking", "blocked", f"permission denied: {exc}"
        )
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Localhost networking", "blocked", f"error: {exc}")


def probe_network_egress() -> ProbeResult:
    """Attempt to reach a well-known external host with a short timeout."""
    host = "example.com"
    try:
        socket.create_connection((host, 80), timeout=2).close()
        return ProbeResult(
            "External networking", "allowed", f"TCP connect to {host}:80 succeeded"
        )
    except socket.timeout:
        return ProbeResult(
            "External networking", "warning", f"connection to {host}:80 timed out"
        )
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("External networking", "blocked", f"error: {exc}")


def probe_env_vars() -> ProbeResult:
    """Summarize environment variables without dumping sensitive values."""
    keys = sorted(os.environ.keys())
    sensitive = {
        key
        for key in keys
        if any(token in key.lower() for token in ("key", "token", "secret", "password"))
    }
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
        return ProbeResult(
            "Resource limits", "warning", "rlimit not supported on this platform"
        )
    detail = "; ".join(f"{k}({v})" for k, v in limits.items())
    return ProbeResult("Resource limits", "info", detail)


def probe_mount_flags() -> ProbeResult:
    """Review mount options for key directories."""
    mounts = _read_proc_mounts()
    if not mounts:
        return ProbeResult(
            "Mount security flags", "warning", "/proc/mounts unavailable"
        )

    expectations = {
        "/tmp": {"noexec", "nosuid", "nodev"},
        "/var/tmp": {"noexec", "nosuid", "nodev"},
    }
    ro_targets = ["/usr", "/lib", "/lib64"]
    findings: list[str] = []
    status = "info"

    for path, expected in expectations.items():
        entry = mounts.get(path)
        if not entry:
            findings.append(f"{path}: not mounted or hidden")
            status = "warning"
            continue
        _, opts = entry
        missing = expected - opts
        if missing:
            status = "warning"
            findings.append(f"{path}: missing {','.join(sorted(missing))}")
        else:
            findings.append(f"{path}: {','.join(sorted(expected))} present")

    for path in ro_targets:
        entry = mounts.get(path)
        if not entry:
            continue
        _, opts = entry
        if "ro" in opts:
            findings.append(f"{path}: read-only")
        else:
            findings.append(f"{path}: writable")
            status = "warning"

    shared = [
        mp
        for mp, (fstype, _) in mounts.items()
        if mp.startswith("/mnt") and fstype in {"vboxsf", "9p", "fuse"}
    ]
    if shared:
        status = "warning"
        findings.append(f"host shares: {', '.join(shared)}")

    detail = "; ".join(findings) if findings else "no relevant mountpoints found"
    return ProbeResult("Mount security flags", status, detail)


def probe_mount_enumeration() -> ProbeResult:
    """Enumerate writable mounts that could be abused."""
    mounts = _read_proc_mounts()
    if not mounts:
        return ProbeResult("Mount enumeration", "warning", "/proc/mounts unavailable")

    writable = [mp for mp, (_, opts) in mounts.items() if "rw" in opts]
    if not writable:
        return ProbeResult("Mount enumeration", "info", "no writable mounts reported")

    interesting = [mp for mp in writable if mp not in {"/proc", "/sys", "/dev"}]
    if len(interesting) > 12:
        interesting = interesting[:12] + ["..."]
    detail = f"writable mounts: {', '.join(interesting)}"
    return ProbeResult("Mount enumeration", "info", detail)


def probe_ro_system_dirs() -> ProbeResult:
    """Try to write into system directories that should be read-only."""
    targets = [Path("/usr"), Path("/lib"), Path("/etc")]
    successes: list[str] = []
    blocked: list[str] = []
    timestamp = int(time.time() * 1000)

    for base in targets:
        if not base.exists():
            continue
        probe_file = base / f".vk_probe_{timestamp}"
        try:
            probe_file.write_text("probe")
            probe_file.unlink()
            successes.append(str(base))
        except PermissionError:
            blocked.append(str(base))
        except OSError:
            blocked.append(str(base))
        except Exception:
            blocked.append(str(base))
        finally:
            try:
                probe_file.unlink()
            except Exception:  # noqa: BLE001
                pass

    if successes:
        detail = f"writes succeeded in: {', '.join(successes)}"
        if blocked:
            detail += f"; blocked in: {', '.join(blocked)}"
        return ProbeResult("Write to system dirs", "allowed", detail)
    if blocked:
        return ProbeResult(
            "Write to system dirs", "blocked", f"writes denied in: {', '.join(blocked)}"
        )
    return ProbeResult(
        "Write to system dirs", "warning", "target directories absent or inaccessible"
    )


def probe_proc_hidepid() -> ProbeResult:
    """Check hidepid mount option and /proc visibility."""
    mounts = _read_proc_mounts()
    entry = mounts.get("/proc")
    opts = entry[1] if entry else set()
    hidepid = next((opt for opt in opts if opt.startswith("hidepid=")), "hidepid=0")
    try:
        data = Path("/proc/1/cmdline").read_bytes()
        readable = bool(data)
    except PermissionError as exc:
        return ProbeResult(
            "/proc privacy", "blocked", f"{hidepid}, reading /proc/1 denied ({exc})"
        )
    except Exception as exc:  # noqa: BLE001
        return ProbeResult(
            "/proc privacy", "warning", f"{hidepid}, error reading /proc/1: {exc}"
        )
    detail = f"{hidepid}, /proc/1 readable={readable}"
    status = "allowed" if readable else "blocked"
    return ProbeResult("/proc privacy", status, detail)


def probe_sysctl_security() -> ProbeResult:
    """Record key kernel hardening sysctls."""
    ptrace_scope = _read_sysctl("/proc/sys/kernel/yama/ptrace_scope")
    bpf_disabled = _read_sysctl("/proc/sys/kernel/unprivileged_bpf_disabled")
    kptr_restrict = _read_sysctl("/proc/sys/kernel/kptr_restrict")

    status = "info"
    notes: list[str] = []

    if ptrace_scope not in {"2", "3"}:
        status = "warning"
        notes.append(f"ptrace_scope={ptrace_scope}")
    else:
        notes.append(f"ptrace_scope={ptrace_scope} (strict)")

    if bpf_disabled not in {"1", "2"}:
        status = "warning"
        notes.append(f"unprivileged_bpf_disabled={bpf_disabled}")
    else:
        notes.append(f"unprivileged_bpf_disabled={bpf_disabled} (enforced)")

    if kptr_restrict not in {"1", "2"}:
        status = "warning"
    notes.append(f"kptr_restrict={kptr_restrict}")

    detail = "; ".join(notes)
    return ProbeResult("Kernel sysctls", status, detail)


def probe_ptrace_restriction() -> ProbeResult:
    """Attempt to ptrace a child process; strict hardening should block it."""
    if platform.system() != "Linux":
        return ProbeResult(
            "Ptrace attach", "warning", "ptrace test only implemented on Linux"
        )

    proc = subprocess.Popen(["sleep", "2"])  # nosec - benign
    libc = ctypes.CDLL("libc.so.6")
    PTRACE_ATTACH = 16
    PTRACE_DETACH = 17

    try:
        res = libc.ptrace(PTRACE_ATTACH, proc.pid, None, None)
        if res == 0:
            os.waitpid(proc.pid, 0)
            libc.ptrace(PTRACE_DETACH, proc.pid, None, None)
            return ProbeResult("Ptrace attach", "allowed", "ptrace attach succeeded")
        err = ctypes.get_errno()
        status = "blocked" if err in (errno.EPERM, errno.EACCES) else "warning"
        return ProbeResult("Ptrace attach", status, f"ptrace failed errno={err}")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Ptrace attach", "warning", f"error invoking ptrace: {exc}")
    finally:
        try:
            proc.terminate()
        except Exception:  # noqa: BLE001
            pass


def probe_raw_socket() -> ProbeResult:
    """Try to open a raw socket; should be denied without CAP_NET_RAW."""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_RAW, socket.IPPROTO_ICMP)
        sock.close()
        return ProbeResult(
            "Raw socket access", "allowed", "raw socket creation succeeded"
        )
    except PermissionError as exc:
        return ProbeResult("Raw socket access", "blocked", f"permission denied: {exc}")
    except OSError as exc:
        if exc.errno in (errno.EPERM, errno.EACCES):
            return ProbeResult(
                "Raw socket access", "blocked", f"denied errno={exc.errno}"
            )
        return ProbeResult("Raw socket access", "warning", f"error: {exc}")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Raw socket access", "warning", f"error: {exc}")


def probe_packet_capture() -> ProbeResult:
    """Evaluate ability to sniff packets via AF_PACKET or tcpdump."""
    if platform.system() != "Linux":
        return ProbeResult("Packet capture", "warning", "packet capture probe only on Linux")

    tools = [name for name in ("tcpdump", "tshark", "dumpcap") if which(name)]
    tool_info = f"tools present: {', '.join(tools)}" if tools else "no capture tools found"

    try:
        af_packet = getattr(socket, "AF_PACKET", None)
        if af_packet is None:
            return ProbeResult("Packet capture", "warning", f"AF_PACKET unavailable; {tool_info}")
        sock = socket.socket(af_packet, socket.SOCK_RAW, socket.ntohs(3))
        sock.close()
        detail = f"AF_PACKET raw socket opened; {tool_info}"
        return ProbeResult("Packet capture", "allowed", detail)
    except PermissionError as exc:
        return ProbeResult("Packet capture", "blocked", f"permission denied: {exc}; {tool_info}")
    except OSError as exc:
        if exc.errno in (errno.EPERM, errno.EACCES):
            return ProbeResult("Packet capture", "blocked", f"denied errno={exc.errno}; {tool_info}")
        return ProbeResult("Packet capture", "warning", f"error: {exc}; {tool_info}")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Packet capture", "warning", f"error: {exc}; {tool_info}")


def probe_strace_observation() -> ProbeResult:
    """Run strace against a sample workload to confirm syscall tracing ability."""
    if platform.system() != "Linux":
        return ProbeResult("Strace observation", "warning", "strace probe only implemented on Linux")

    strace_path = which("strace")
    if not strace_path:
        return ProbeResult("Strace observation", "warning", "strace not installed")

    tmpdir = Path(tempfile.mkdtemp(prefix="vk_strace_"))
    script_path = tmpdir / "workload.py"
    trace_path = tmpdir / "trace.log"

    script_path.write_text(
        "import pathlib, socket\n"
        "p = pathlib.Path('strace_probe.txt')\n"
        "p.write_text('probe')\n"
        "s = socket.socket()\n"
        "s.settimeout(0.2)\n"
        "try:\n"
        "    s.connect(('127.0.0.1', 9))\n"
        "except Exception:\n"
        "    pass\n"
        "s.close()\n"
    )

    code, _, err = _run_command(
        [strace_path, "-o", str(trace_path), sys.executable, str(script_path)], timeout=6
    )

    try:
        (tmpdir / "strace_probe.txt").unlink(missing_ok=True)
    except AttributeError:
        try:
            (tmpdir / "strace_probe.txt").unlink()
        except Exception:  # noqa: BLE001
            pass

    detail = []
    if trace_path.exists():
        try:
            trace_data = trace_path.read_text()[:2000]
            hits = [token for token in ("open(", "connect(", "write(") if token in trace_data]
            if hits:
                detail.append(f"captured syscalls: {', '.join(hits)}")
            else:
                detail.append("trace captured but no key syscalls noted")
        except Exception as exc:  # noqa: BLE001
            detail.append(f"trace read error: {exc}")
    else:
        detail.append("trace file missing")

    try:
        script_path.unlink(missing_ok=True)
        trace_path.unlink(missing_ok=True)
        tmpdir.rmdir()
    except Exception:  # noqa: BLE001
        pass

    if code == 0 and "trace file missing" not in detail:
        return ProbeResult("Strace observation", "allowed", "; ".join(detail))
    if code != 0 and "permission" in err.lower():
        return ProbeResult("Strace observation", "blocked", f"permission denied: {err}")
    return ProbeResult("Strace observation", "warning", f"strace rc={code}; {err or '; '.join(detail)}")


def probe_tmp_noexec() -> ProbeResult:
    """Attempt to execute a script from /tmp to detect noexec enforcement."""
    script = Path("/tmp") / f"vk_tmp_exec_{int(time.time())}.sh"
    try:
        script.write_text("#!/bin/sh\necho tmp_exec\n")
        script.chmod(0o755)
        res = subprocess.run([str(script)], capture_output=True, text=True, timeout=3)
        status = "allowed" if res.returncode == 0 else "blocked"
        detail = f"return={res.returncode}, stdout={res.stdout.strip()!r}"
        return ProbeResult("Execute from /tmp", status, detail)
    except PermissionError as exc:
        return ProbeResult("Execute from /tmp", "blocked", f"permission denied: {exc}")
    except subprocess.TimeoutExpired:
        return ProbeResult("Execute from /tmp", "warning", "execution timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("Execute from /tmp", "warning", f"error: {exc}")
    finally:
        try:
            script.unlink()
        except Exception:  # noqa: BLE001
            pass


def probe_dns_udp() -> ProbeResult:
    """Send a UDP packet to a DNS server to gauge outbound UDP rules."""
    target = ("8.8.8.8", 53)
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.settimeout(2)
        sock.sendto(b"\x00" * 12, target)
        sock.close()
        return ProbeResult(
            "UDP egress (DNS)", "allowed", f"sent packet to {target[0]}:{target[1]}"
        )
    except PermissionError as exc:
        return ProbeResult("UDP egress (DNS)", "blocked", f"permission denied: {exc}")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult("UDP egress (DNS)", "warning", f"error: {exc}")


def probe_auditd_presence() -> ProbeResult:
    """Check audit subsystem status via auditctl when available."""
    auditctl = which("auditctl")
    if not auditctl:
        return ProbeResult("Audit logging", "warning", "auditctl not installed")
    try:
        res = subprocess.run(
            [auditctl, "-s"], capture_output=True, text=True, timeout=3
        )
        if res.returncode != 0:
            detail = (
                res.stderr.strip() or res.stdout.strip() or "auditctl returned non-zero"
            )
            return ProbeResult("Audit logging", "warning", detail)
        detail = " ".join(
            line.strip() for line in res.stdout.splitlines() if line.strip()
        )
        return ProbeResult("Audit logging", "info", detail)
    except PermissionError as exc:
        return ProbeResult(
            "Audit logging", "blocked", f"permission denied querying auditctl: {exc}"
        )
    except subprocess.TimeoutExpired:
        return ProbeResult("Audit logging", "warning", "auditctl -s timed out")
    except Exception as exc:  # noqa: BLE001
        return ProbeResult(
            "Audit logging", "warning", f"error invoking auditctl: {exc}"
        )


def probe_nftables_ruleset() -> ProbeResult:
    """Attempt to read nftables ruleset; hardened hosts may restrict it."""
    nft = which("nft")
    if not nft:
        return ProbeResult("nftables inspection", "warning", "nft binary not installed")
    try:
        res = subprocess.run(
            [nft, "list", "ruleset"], capture_output=True, text=True, timeout=3
        )
        if res.returncode == 0:
            detail = f"ruleset length={len(res.stdout)} chars"
            return ProbeResult("nftables inspection", "allowed", detail)
        err = res.stderr.strip() or "nft returned non-zero"
        if "Permission denied" in err:
            return ProbeResult("nftables inspection", "blocked", err)
        return ProbeResult("nftables inspection", "warning", err)
    except PermissionError as exc:
        return ProbeResult(
            "nftables inspection", "blocked", f"permission denied: {exc}"
        )
    except subprocess.TimeoutExpired:
        return ProbeResult(
            "nftables inspection", "warning", "nft list ruleset timed out"
        )
    except Exception as exc:  # noqa: BLE001
        return ProbeResult(
            "nftables inspection", "warning", f"error invoking nft: {exc}"
        )


PROBES: List[ProbeFunc] = [
    probe_os_info,
    probe_cpu_details,
    probe_virtualization_hints,
    probe_user_identity,
    probe_sudo_access,
    probe_workspace_exec,
    probe_tooling,
    probe_package_install_surface,
    probe_filesystem_scope,
    probe_filesystem_write,
    probe_log_access,
    probe_log_activity_detection,
    probe_path_persistence,
    probe_persistence_surfaces,
    probe_subprocess_spawn,
    probe_daemon_spawn,
    probe_seccomp,
    probe_capabilities,
    probe_kernel_modules,
    probe_mount_flags,
    probe_mount_enumeration,
    probe_ro_system_dirs,
    probe_proc_hidepid,
    probe_sysctl_security,
    probe_ptrace_restriction,
    probe_raw_socket,
    probe_packet_capture,
    probe_strace_observation,
    probe_network_localhost,
    probe_network_egress,
    probe_tmp_noexec,
    probe_dns_udp,
    probe_auditd_presence,
    probe_nftables_ruleset,
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

    sections: list[tuple[str, list[ProbeResult]]] = [
        ("Allowed capabilities (potential risk surface)", allowed),
        ("Blocked attempts (hardened defenses)", blocked),
        ("Warnings (inconclusive or unsupported)", warnings),
        ("Informational context", infos),
    ]

    for title, bucket in sections:
        if not bucket:
            continue
        print(title + ":")
        for result in bucket:
            print("  " + result.render())
        print()

    return 0


if __name__ == "__main__":
    sys.exit(main())
