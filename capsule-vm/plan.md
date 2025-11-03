# Capsule Shell Session Tracing Plan

## Components Overview
- **shell**: interactive shell binary that replaces the `agent` user's login shell and intercepts every command line.
- **launcher**: setuid-root helper that creates per-session Tracee runs, executes the agent's command as `agent`, and manages session metadata/log storage.
- **capsule-vm CLI**: remains the host-side management tool; will gain helper commands later if needed (out of scope for this pass).

## Task Breakdown
1. **Cargo / Project Layout**
   - Provide dedicated crates `shell/` and `launcher/`, sharing utilities via common modules as needed.
   - Ensure `cargo build` produces all three binaries and add them to the install artifacts (scripts or cloud-init copy logic).

2. **Implement `shell`**
   - Use a line-editing crate (e.g., `rustyline`) to provide a familiar prompt and history.
   - Loop forever: read a command, ignore blanks, block `exit`/`logout`, handle EOF (`Ctrl+D`) by resetting the prompt.
   - Generate a unique `session_id` (timestamp + random suffix) per command and call `launcher --session <id> --cwd <pwd> --cmd <command string>`.
   - Stream subprocess STDOUT/STDERR directly to the terminal and propagate its exit code.
   - On fatal errors, print a friendly message and continue instead of dropping the user to a real shell.

3. **Implement `launcher`**
   - Validate it is running as effective UID 0 (fail fast otherwise) and accept CLI flags for session ID, cwd, and command.
   - Create `/var/log/tracee/sessions/<session_id>` with `0700` root ownership; write an initial `metadata.json` capturing command, cwd, env snapshot, start time.
   - `fork()` and in the child drop privileges to UID/GID of `agent` (lookup via `libc::getpwnam`), `chdir` to requested cwd, and `execve` `/bin/bash -lc <command>`.
   - In the parent, launch Tracee: `/usr/local/bin/tracee --config /etc/tracee/session.yaml --scope pid=<child_pid> --scope follow --output json:/var/log/tracee/sessions/<session_id>/events.jsonl`.
   - Relay the child's exit status back to `shell`; when the child finishes, send SIGTERM to Tracee, wait for it to exit, finalize `metadata.json` (end time, exit code), and clean up any lock marker.

4. **Tracee Session Configuration**
   - Add `/etc/tracee/session.yaml` derived from the current config but scoped for per-session runs (same event sets, no global logging).
   - Ensure the helper references that config and that `tracee` binary is present/exec.

5. **Provisioning Changes (cloud-init)**
   - Remove the always-on Tracee systemd service and its unit file.
   - Copy the new binaries into `/usr/local/bin`, mark `launcher` as setuid-root (mode `04750`, owner `root:tracee` or `root:root`).
   - Set `agent` user's shell to `/usr/local/bin/shell` (using `chsh` or direct `/etc/passwd` edit) and place a symlink for compatibility if needed.
   - Create `/var/log/tracee/sessions` with strict permissions.
   - Install `rustyline` history file location under `/home/agent/.capsule_history` with ownership `agent:agent`.

6. **Documentation Updates**
   - Explain the capsule shell behaviour, session logs location, and how to access root (`capsule-vm shell <vm> --root`) for debugging.
   - Note the removal of the background Tracee service and clarify that every command automatically generates a session archive.

7. **Verification Steps**
   - `cargo build` to produce new binaries.
   - Launch a fresh VM; confirm login drops into capsule shell (prompt shows, `exit` is ignored).
   - Run a simple command (`python3 -c 'print("hi")'`), verify matching session directory with `events.jsonl` and `metadata.json`, and confirm Tracee process dies afterward.
   - Run multiple commands back-to-back to ensure session IDs are unique and directories accumulate.
   - Intentionally run a failing command to confirm exit codes propagate and metadata records failure.

8. **Follow-up / Future Work (for tracking)**
   - Add tooling to list sessions from the host CLI.
   - Implement log rotation/retention policies.
   - Harden the setuid helper (input validation, logging, sandboxing) and consider dropping capabilities instead of full root.
