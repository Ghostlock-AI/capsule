# Trace Enablement & Telemetry Roadmap

## Vision
- Every capsule VM starts with Tracee running continuously; all agent activity is captured as structured JSON without manual intervention.
- Trace coverage spans process, file I/O, network, credential, and signal syscalls, with room to add higher-level security heuristics later.
- Tracing focuses on the user-invoked agent (and its descendants) while minimizing noise from unrelated background services.
- Recorded events flow to OpenTelemetry-compatible sinks for dashboards, alerting, and retrospective investigation.
- A future LangChain-based assistant can answer natural-language questions over stored trace data (e.g., "Which files were exfiltrated?", "What did the agent query before escalating permissions?").
- The codebase remains Lima-only, clean, modular, and easy to extend.

## Clean Code Commitments (Rust-centric)
Guiding principles distilled from clean-code best practices:
- **Clarity over cleverness:** Prefer descriptive naming, straightforward control flow, and explicit error handling (`anyhow::Context` for rich diagnostics).
- **Single responsibility:** Keep modules and functions focused; extract command handlers, provisioning logic, and telemetry plumbing into dedicated files.
- **Predictable style:** Adhere to `rustfmt`, keep functions short, and avoid deep nesting by early returns and helper functions.
- **Testability:** Provide unit/integration tests for trace orchestration, config parsing, and exporter logic; use dependency injection traits for shell/Tracee runners.
- **Safe abstractions:** Model shared behavior via enums/traits instead of ad hoc `String` flags; leverage the type system to prevent invalid states.
- **Fail loudly, fail fast:** Use `Result` everywhere, propagate actionable errors, and log context for operator troubleshooting.
- **Docs & examples:** Document public interfaces and add usage snippets for new CLI commands and configuration files.

## Current Snapshot
- Cloud-init installs Tracee and AppArmor, but tracing is manual via `scripts/run-tracee.sh` / `run-agent-with-trace.sh`.
- Event selection is limited (exec/open/connect). Credential & signal coverage, auto-start, filtering, and streaming are missing.
- Multipass backend has been removed; Lima templating is still complex and monolithic.
- No telemetry pipeline or storage layer is integrated; raw JSONL lives under `~/.capsule-vm/traces/` on the host.

## Milestones & To-Do List

### 1. Repository Hygiene & Preparation
- [x] Remove Multipass backend implementation, CLI references, and docs; ensure Lima remains the sole backend.
- [x] Archive/delete legacy files (`main_old.rs`, `installs_old.rs`) once functionality is ported or confirmed obsolete.
- [x] Split `src/backends/lima.rs` into smaller modules (template rendering, provisioning, tracing helpers) for clarity.
- [x] Establish baseline tests (unit coverage for config validation & installs); CI wiring deferred.
- [ ] **After trace milestones:** wire `scripts/check.sh` into CI.

### 2. Automatic Trace Lifecycle
- [ ] Introduce a trace supervisor module that launches Tracee during VM provisioning/start (systemd unit or managed process).
- [ ] Ensure Tracee starts with an allowlist of event sets covering process, file I/O, network, signal, and credential syscalls.
- [ ] Implement heartbeat/status checks so the CLI can detect Tracee health on each VM.

### 3. Agent-Focused Scoping
- [ ] Define how agents are launched (e.g., via `capsule-run` or new `capsule-vm exec`) to tag the controlling process.
- [ ] Add a lightweight in-VM watcher that observes agent PIDs and updates Tracee filters (`--scope follow`, `comm`, `pid`, `tree`).
- [ ] Provide CLI flags/config to select tracing mode: `global`, `agent-only`, or custom policies.
- [ ] Store per-session metadata (start time, filter strategy, command) alongside JSON logs.

### 4. Event Schema & Storage Pipeline
- [ ] Normalize Tracee JSON into a typed Rust struct; document the schema and version it.
- [ ] Write a host-side collector that streams events via Lima SSH socket, parses, enriches (VM name, session), and persists locally.
- [ ] Support log rotation and compression for `~/.capsule-vm/traces/<vm>/`.

### 5. OpenTelemetry Export
- [ ] Add an OTLP exporter (gRPC/HTTP) that emits spans or logs with the normalized schema.
- [ ] Make exporter configurable (enable/disable, endpoint, credentials) via config file/environment.
- [ ] Provide starter Grafana dashboards / queries referencing the exported attributes.

### 6. Security Heuristics (Phase 2)
- [ ] Evaluate Tracee signatures and custom policies for high-risk behaviors (credential reads, kernel module loads, outbound data spikes).
- [ ] Surface warnings/alerts in both local JSON and OTLP streams with severity metadata.

### 7. NL Query Foundations (Phase 3)
- [ ] Decide on storage backend for historical queries (e.g., Arrow/Parquet, Elasticsearch, Tempo).
- [ ] Expose a query service (REST/GraphQL) that translates structured requests into backend queries.
- [ ] Prototype a LangChain agent that converts natural language into those structured queries, leveraging schema documentation and prompt templates.

### 8. Developer Experience & Documentation
- [ ] Update CLI help, README, and new `TRACE_PLAN.md` references after each milestone.
- [ ] Add examples/tests demonstrating auto-trace on `capsule-vm create`, OTLP export, and agent-focused filtering.
- [ ] Document clean code practices in CONTRIBUTING guide; enforce with CI (fmt, clippy, tests).
- [ ] **Final catch-up:** completely rewrite the README once tracing/telemetry are stable.

## Notes
- Keep milestones small and independently verifiable; ship vertical slices (auto-start + JSON capture before OTLP, etc.).
- Treat tracing configuration as data: YAML/TOML files that the CLI validates and applies.
- Maintain backward compatibility for existing users, providing migration notes when Multipass support is removed.
- CI integration and README overhaul are sequenced after the tracing milestones to avoid churn while core functionality is still in flux.
