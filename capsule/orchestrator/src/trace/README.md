# Tracee Configuration Generation Module

This module provides dynamic Tracee configuration generation for the Capsule VM orchestrator. It allows flexible tracing profiles based on event categories, scope filters, and security requirements.

## Overview

The trace module consists of three main components:

1. **events.rs** - Event category to Tracee event name mapping
2. **config_gen.rs** - Tracee YAML configuration generator
3. **mod.rs** - Module entry point and public API

## Event Categories

The module supports five event categories:

### Process Events
- `sched_process_exec` - Process execution (scheduler event)
- `execve` - Process execution (syscall - backup)
- `exit_group` - Process termination (clean exit)
- `exit` - Process exit (fallback)

### File Events
- `openat` - File open/create with flags
- `close` - File/socket close
- `security_inode_rename` - File rename (LSM hook)
- `security_inode_unlink` - File deletion (LSM hook)

### Network Events
- `net_tcp_connect` - TCP connections with dst IP:port
- `connect` - Raw connect syscall (backup)
- `security_socket_bind` - Socket bind (LSM hook)
- `bind` - Raw bind syscall (backup)
- `net_packet_dns_request` - DNS query
- `net_packet_dns_response` - DNS response

### Credentials Events
- `security_bprm_check` - Binary execution permission check
- `commit_creds` - Credential commit
- `setuid` - Set user ID
- `setgid` - Set group ID

### Signal Events
- `signal_deliver` - Signal delivery
- `kill` - Kill syscall

## Tracing Profiles

The module provides three pre-configured tracing profiles:

### Minimal Profile
- **Events**: Process only
- **Use Case**: Basic process monitoring with minimal overhead
- **Example**: Tracking exec/exit events for audit logs

### Developer Profile (Default)
- **Events**: Process + File + Network
- **Use Case**: Development and debugging environments
- **Example**: Monitoring AI agent file access and network calls

### Full Profile
- **Events**: All categories (Process + File + Network + Credentials + Signal)
- **Use Case**: Maximum observability for security research
- **Example**: Complete system call tracing for threat detection

## Scope Filtering

Tracee configurations support three scope filters:

1. **User Filter**: `uid=$(username)` - Trace events from specific user
2. **New Processes**: `pid=new` - Only trace new processes spawned after Tracee starts
3. **Follow**: `follow` - Follow child processes

## Usage

### Basic Configuration Generation

```rust
use trace::config_gen::{TracingConfig, EventCategories, TraceScope, generate_tracee_config};

let config = TracingConfig {
    enabled: true,
    events: EventCategories {
        process: true,
        file: true,
        network: true,
        credentials: false,
        signal: false,
    },
    scope: TraceScope {
        user: "agent".to_string(),
        new_processes: true,
        follow: true,
    },
};

let yaml = generate_tracee_config(&config)?;
println!("{}", yaml);
```

### Using Pre-configured Profiles

```rust
use trace::config_gen::TraceeConfig;

// Minimal profile
let minimal = TraceeConfig::minimal();
let yaml = generate_tracee_config(&minimal.tracing)?;

// Developer profile
let developer = TraceeConfig::developer();
let yaml = generate_tracee_config(&developer.tracing)?;

// Full profile
let full = TraceeConfig::full();
let yaml = generate_tracee_config(&full.tracing)?;
```

### Event Category Mapping

```rust
use trace::events::{EventCategory, get_events_for_category};

// Get all process events
let process_events = get_events_for_category(EventCategory::Process);
// Returns: ["sched_process_exec", "execve", "exit_group", "exit"]

// Get all network events
let network_events = get_events_for_category(EventCategory::Network);
// Returns: ["net_tcp_connect", "connect", "security_socket_bind", ...]
```

## Example Configurations

Example Tracee configurations are generated in `examples/tracee-profiles/`:

- `minimal.yaml` - Process events only
- `developer.yaml` - Process + File + Network events
- `full.yaml` - All event categories

To generate these examples:

```bash
cargo run --example generate_tracee_profiles
```

## Output Format

Generated configurations follow the Tracee YAML schema:

```yaml
dnscache:
  enable: true
containers:
  enrich: false
proctree:
  source: both
  cache:
    process: 8192
    thread: 8192
output:
  options:
    parse-arguments: true
    parse-arguments-fds: true
    exec-hash: digest-inode
  json:
    files:
      - /var/log/tracee/events.jsonl
events:
    - sched_process_exec
    - execve
    - exit_group
    - exit
scope:
  - uid=$(agent)
  - pid=new
  - follow
```

## Integration with Capsule Config

When the `config` module is implemented, the trace module will integrate with the main `CapsuleConfig` schema:

```rust
// Future integration (when config module is complete)
use config::CapsuleConfig;
use trace::config_gen::generate_tracee_config;

let capsule_config = config::load_config("capsule.yaml")?;
let tracee_yaml = generate_tracee_config(&capsule_config.tracing)?;

// Write to /etc/tracee/config.yaml during VM provisioning
```

## Testing

Run tests for the trace module:

```bash
cargo test trace::
```

All 15 tests should pass:
- 5 tests for event category mappings
- 10 tests for configuration generation

## Future Enhancements

1. **Custom Event Selection**: Allow individual event selection beyond categories
2. **Performance Profiles**: Add low/medium/high overhead presets
3. **Filter Expressions**: Support complex Tracee filter expressions
4. **Output Formats**: Support multiple output formats (JSON, CSV, Syslog)
5. **Rate Limiting**: Configure event rate limiting to prevent log flooding
6. **Event Enrichment**: Configure container/process tree enrichment options

## References

- [Tracee Documentation](https://aquasecurity.github.io/tracee/)
- [Tracee Configuration Reference](https://aquasecurity.github.io/tracee/latest/docs/config/options/)
- [Capsule PLAN.md](../../PLAN.md) - Phase 4: Tracee Configuration Generation
