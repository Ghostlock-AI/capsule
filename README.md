<h1 align="center">Capsule</h1>

<p align="center">
  <img src="capsule.gif" width="600">
</p>

---

## TL;DR

Capsule watches agent behavior from the kernel (eBPF/LSM), enriches events into human-readable timelines, and lays the groundwork for dynamic, policy-driven security backed by small ML models. It’s **pre-alpha**, **Linux aarch64 only** right now, written in **Rust**.

## License

This project is licensed under the MIT License.

---

## Why we are building Capsule

Capsule is a permanently open-source kernel-level tracing and sandboxing project. It will never be closed, dual-licensed, or converted into a proprietary core.

Here at Ghostlock, we are building Capsule because application-level observability and enforcement no longer hold up in a world of autonomous agents. User-mode hooks are easy to bypass, and existing tools cannot reliably explain what code actually did once it runs.

As software shifts toward autonomous agents—systems that write code, spawn processes, and make decisions with minimal supervision—the human role moves from approving outputs to understanding and constraining behavior.

Kernel-level tracing provides a stronger foundation. By observing execution below the application layer, it becomes possible to produce durable, verifiable records of behavior that software cannot evade. Today, however, these tools remain fragmented, hard to use, and accessible only to specialists.

Capsule exists to make kernel-level tracing practical:
	
	•	simple enough to experiment with,

	•	explicit enough to reason about, and
	
	•	open enough to trust long-term.

If you work on kernels, runtimes, security, or systems tooling—and care about making autonomous software observable and accountable—we welcome your contributions!

---

## What Capsule observes

| Area                 | In plain terms                                              |
| -------------------- | ----------------------------------------------------------- |
| Process execution    | When programs start, fork, or become backgroud processes    |
| Network              | All network communication—who talks to whom.                |
| File I/O             | Read/write/create/delete/move files and folders.            |
| Credentials          | Changes to identity (UID/GID/capabilities).                 |
| Memory / code        | Risky mappings (e.g., W+X), code loading.                   |
| IPC orchestration    | Local process-to-process comms (pipes, UNIX sockets, etc.). |
| Device access        | Access to `/dev/*` (KVM, tun/tap, GPU, disks, USB/TTY).     |
| System configuration | Mounts, chroot/pivot_root, persistence paths.               |
| Containers & cgroups | Enter/leave namespaces; resource limits and cgroup changes. |
| Signals              | Software interrupts (SIGKILL, SIGTERM, etc.).               |

---

## Architecture

- **Kernel Probes**: eBPF kprobes/tracepoints/LSM hooks (Linux) capture syscall-level and semantic events.
- **Userspace Daemon**: stream ingestion, async enrichment of syscalls for better readability.
- **Policy/ML Layer**: deterministic rules + sequence/graph model that categorizes prompt, syscall sequence, and resource utilization combinations as risky or harmless.
