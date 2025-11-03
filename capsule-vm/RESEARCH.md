# Capsule VM & Tracee Research Notes

## Capsule VM CLI & Lifecycle
- CLI entrypoint defines lifecycle subcommands (`Create`, `Ps`, `Start`, `Stop`, `Delete`, `Shell`) via Clap for `capsule-vm` (src/main.rs:36).
- `cmd_create` assembles `VmConfig`, injects default `cloud-init.yaml`, and waits for readiness so the VM is usable before returning (src/main.rs:145).
- Other subcommands delegate to the backend through the unified trait so Lima-specific logic stays encapsulated (src/main.rs:135; src/main.rs:245).
- User-facing defaults and banner are set in the same file, so any CLI copy edits happen centrally (src/main.rs:21).

## VM Backend Abstraction
- `VmConfig` builder tracks CPUs, memory, disk, and optional cloud-init path when provisioning (src/vm_backend.rs:13).
- `VmBackend` trait outlines the operations each backend must expose (`create`, `start`, `stop`, `delete`, `list`, `exec`, `shell`, `wait_for_ready`, `verify_state`) (src/vm_backend.rs:55).
- Helper factories select Lima automatically or error out with guidance if no backend exists (src/vm_backend.rs:104; src/vm_backend.rs:118).

## Lima Backend Internals
- `LimaBackend::new` discovers `limactl`, installing it through platform-specific helpers when missing (src/backends/lima/mod.rs:18; src/backends/lima/install.rs:7).
- Every Lima invocation goes through `run_command_checked`, which surfaces stderr and exit codes as structured errors (src/backends/lima/mod.rs:42).
- Instance creation renders a temporary template, calls `limactl start` with resource overrides, retries on failure, and verifies the VM reaches `Running` (src/backends/lima/mod.rs:132).
- Health checks reuse `wait_for_ready`, which probes SSH connectivity with exponential backoff before running the shared validators (src/backends/lima/mod.rs:238; src/validation.rs:57).

## Provisioning & Templates
- `render_template_with_cloudinit` copies `templates/lima-base.yaml` and inlines the chosen cloud-init so Lima receives a single file (src/backends/lima/template.rs:8).
- Cloud-init installs Tracee v0.23.2 into `/usr/local/bin`, copies bundled signatures, and clears temporary artifacts (cloud-init.yaml:8; cloud-init.yaml:15).
- The simplified Lima template ensures an `ubuntu` user, writable mounts, and a one-time script that just prepares `/home/ubuntu/workspace` (lima-template.yaml:33; lima-template.yaml:41).
- The base template shipped with the binary is identical except it leaves the cloud-init placeholder so the renderer can inject commands (templates/lima-base.yaml:1; templates/lima-base.yaml:33).

## Support Scripts & Tooling
- `scripts/install.sh` builds a release binary and installs it system-wide or per-user depending on flag (scripts/install.sh:33).
- `scripts/check.sh` runs the opinionated QA stack (`fmt`, `clippy`, `test`) in one shot (scripts/check.sh:4).
- README lists the pared-down workflow (`capsule-vm create`, `shell`, lifecycle ops) so operator docs stay aligned (README.md:18).

## Tracee Repository Orientation
- `tracee/cmd/tracee` houses the Go CLI that the VM executes, while sibling commands (`tracee-rules`, `traceectl`) sit alongside it (tracee/cmd:1).
- Rich documentation lives under `tracee/docs/docs`, with flags, output formats, and policy guides split into dedicated manuals (tracee/docs/docs:1).
- Reusable configs and policy samples are stored in `tracee/examples`, including a baseline `global_config.yaml` for inspiration (tracee/examples/config/global_config.yaml:1).

## Tracee CLI Defaults & Config Hooks
- The CLI defaults to table-mode output on stdout unless an explicit `--output` is provided (tracee/cmd/tracee/cmd/root.go:120).
- Logging defaults to `info` level; you can raise or lower verbosity or redirect to a file with additional `--log` arguments (tracee/cmd/tracee/cmd/root.go:282; tracee/docs/docs/flags/log.1.md:24).
- Tracee stores auxiliary assets under `/tmp/tracee` by default, controllable through `--install-path` (tracee/cmd/tracee/cmd/root.go:271).
- The sample config demonstrates how to emit JSON or table outputs and toggle options like argument parsing (tracee/examples/config/global_config.yaml:114).

## Targeted Tracing Filters
- Scope filters accept numeric comparisons on pids/uids and namespace IDs, letting you drop noisy system daemons (`=`/`!=`/`>` etc.) (tracee/docs/docs/flags/scope.1.md:28).
- String-based scope filters apply to command names and executables, so excluding `systemd` is as easy as `--scope comm!=systemd` (tracee/docs/docs/flags/scope.1.md:47).
- To focus only on your Python workload, pair a scope include with process-tree tracking, e.g. `--scope comm=python3 --scope follow` (tracee/docs/docs/flags/scope.1.md:214).
- Event filters can also constrain data fields, such as `--events openat.data.pathname='/tmp*'` to restrict file activity reporting (tracee/docs/docs/flags/events.1.md:106).

## Human-Readable Output & Storage
- Use `--output option:parse-arguments` to decode raw syscall parameters into friendlier strings (tracee/docs/docs/flags/output.1.md:42).
- Table, JSON, Go-template, webhook, and forwarders are all addressable from the same flag, with optional file destinations like `--output json:/var/log/tracee.json` (tracee/docs/docs/flags/output.1.md:23).
- Configuring the same via YAML is supported under `output.options.parse-arguments: true` and `output.table.files: - stdout` (tracee/docs/docs/outputs/output-options.md:23; tracee/docs/docs/outputs/output-formats.md:87).
- Diagnostic logger output can be redirected to a persistent file with `--log file:/var/log/tracee.log`, keeping stdout free for event tables (tracee/docs/docs/outputs/logging.md:18).

## Where Tracee Logs Land on the VM
- Without extra flags, events stream as a live table to the interactive terminal (tracee/cmd/tracee/cmd/root.go:123).
- Persist events by naming files in the output flag, e.g. `--output table:/home/ubuntu/workspace/tracee.log` or by editing the config example (tracee/docs/docs/flags/output.1.md:64).
- Operational logs respect the `--log file:` target, ideal for tailing in another shell while the main session stays focused on syscalls (tracee/docs/docs/flags/log.1.md:26).

## Following Descendants & Daemons
- `--scope follow` keeps tracing newly forked children that originate from your filtered process tree (tracee/docs/docs/flags/scope.1.md:70).
- `--scope tree=<pid>` watches an existing process and all descendants, which is handy for long-running daemons that re-parent themselves (tracee/docs/docs/flags/scope.1.md:143).
- Combine both with a command name filter to watch an agent end-to-end: `--scope comm=my-agent --scope follow --scope tree=<bootstrap-pid>` (tracee/docs/docs/flags/scope.1.md:155).

## Security Signatures & Risk Alerts
- Policies bundle scope + rule filters so you can load curated detections alongside raw syscalls (`tracee --policy ./policy.yaml`) (tracee/docs/docs/policies/usage/cli.md:7).
- The reference policy example enables the built-in security signatures that raise warnings for risky behaviors like dropped executables or sudoers edits (tracee/docs/docs/policies/usage/cli.md:108).
- Tracee ships a catalog of default signatures covering privilege escalation, code injection, and persistence; review the list to decide which alerts matter for your agents (tracee/docs/docs/events/builtin/security-events.md:44).
- Custom signature directories can be layered with `--signatures-dir` if you want bespoke warnings beyond the bundled rules (tracee/docs/docs/events/custom/overview.md:7).

## Practical VM Workflow
- After `capsule-vm create myvm --cpus 2 --memory 1G`, shell in and confirm the Tracee binary from `/usr/local/bin` (README.md:90; cloud-init.yaml:15).
- Tracee’s release tarball leaves a `signatures` directory beside the binary, copied during provisioning for policy support (cloud-init.yaml:16).
- Run your Python workload, then invoke Tracee with tailored filters such as `tracee --scope comm=python3 --scope follow --output table --log file:/var/log/tracee.log` so you can tail the structured log separately.
