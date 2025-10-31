# Implementation Plan

We will layer the requested features in order of difficulty, starting with the smallest change so each step can be validated independently.

## 1. Route Tracee Output to a Log File (Easiest)
- Update the Tracee launch command/config to include an explicit output target such as `--output table:/var/log/tracee/events.log`.
- Ensure the directory exists in cloud-init (e.g., `mkdir -p /var/log/tracee && chown ubuntu:ubuntu`).
- Document how to tail or rotate the log; defer rotation to a follow-up if necessary.

## 2. Limit Tracing to Relevant Syscall Families
- Extend the Tracee invocation with `--events` filters (process, network, file I/O, credential, signal sets). Use Tracee’s built-in event groups where possible for maintainability.
- Optionally combine with `--scope` filters to exclude noisy system daemons if they interfere with signal quality.
- Capture the final flag set in both documentation and (if we add one) a config file so operators can tweak it later.

## 3. Start Tracee Automatically After Installation (Most Involved)
- Decide on mechanism: simplest is a `cloud-init` `runcmd` that backgrounds Tracee; more robust is a dedicated systemd service unit.
- If we pick systemd, drop a unit file during provisioning (e.g., `/etc/systemd/system/tracee.service`) and enable it so Tracee restarts if the VM reboots.
- Integrate the logging path and event-filter flags defined above into the service/command so the VM boots with the correct behavior.

Each phase will be committed separately after verification on a fresh VM to keep the changes reviewable.
