#!/usr/bin/env python3
"""
Generate AppArmor profile from YAML configuration.
"""

import yaml
import sys
from pathlib import Path


def generate_apparmor_profile(config_path="/etc/apparmor/profile-config.yaml"):
    """Generate AppArmor profile from YAML configuration."""

    # Load configuration
    with open(config_path, 'r') as f:
        config = yaml.safe_load(f)

    profile_name = config.get('profile_name', 'capsule-agent-workload')
    output_path = f"/etc/apparmor.d/{profile_name}"

    # Start building the profile
    profile_lines = []

    # Header
    profile_lines.append(f"# AppArmor profile for {profile_name}")
    profile_lines.append(f"# Auto-generated from {config_path}")
    profile_lines.append("")

    # Include base abstractions if requested
    if config.get('settings', {}).get('include_base', True):
        profile_lines.append("#include <tunables/global>")
        profile_lines.append("")

    # Profile declaration
    mode = "flags=(complain)" if config.get('settings', {}).get('complain_mode', False) else ""
    profile_lines.append(f"/bin/bash {mode} {{")

    # Include base abstractions
    if config.get('settings', {}).get('include_base', True):
        profile_lines.append("  #include <abstractions/base>")
        profile_lines.append("")

    # Capabilities
    cap_config = config.get('capabilities', {})

    # Allowed capabilities
    allowed_caps = cap_config.get('allow', [])
    if allowed_caps:
        profile_lines.append("  # Allowed capabilities")
        for cap in allowed_caps:
            profile_lines.append(f"  capability {cap},")
        profile_lines.append("")

    # Denied capabilities
    denied_caps = cap_config.get('deny', [])
    if denied_caps:
        profile_lines.append("  # Explicitly denied capabilities")
        for cap in denied_caps:
            profile_lines.append(f"  deny capability {cap},")
        profile_lines.append("")

    # File rules
    file_config = config.get('file_rules', {})

    # Deny rules first (more specific)
    denied_paths = file_config.get('deny', [])
    if denied_paths:
        profile_lines.append("  # Explicitly denied file paths")
        for path in denied_paths:
            profile_lines.append(f"  deny {path} rwklx,")
        profile_lines.append("")

    # Read-only paths
    ro_paths = file_config.get('read_only', [])
    if ro_paths:
        profile_lines.append("  # Read-only access")
        for path in ro_paths:
            profile_lines.append(f"  {path} r,")
        profile_lines.append("")

    # Read-execute paths
    rx_paths = file_config.get('read_execute', [])
    if rx_paths:
        profile_lines.append("  # Read and execute access")
        for path in rx_paths:
            profile_lines.append(f"  {path} rix,")
        profile_lines.append("")

    # Read-write paths
    rw_paths = file_config.get('read_write', [])
    if rw_paths:
        profile_lines.append("  # Read-write access")
        for path in rw_paths:
            profile_lines.append(f"  {path} rw,")
        profile_lines.append("")

    # Network rules
    net_config = config.get('network', {})
    allowed_nets = net_config.get('allow', [])
    if allowed_nets:
        profile_lines.append("  # Network access")
        for net in allowed_nets:
            profile_lines.append(f"  network {net},")
        profile_lines.append("")

    # Signal rules
    signal_config = config.get('signals', {})
    if signal_config.get('allow', False):
        profile_lines.append("  # Signal access")
        profile_lines.append("  signal,")
        profile_lines.append("")

    # Allow execution of common shells and interpreters
    profile_lines.append("  # Allow execution of shells and interpreters")
    profile_lines.append("  /bin/bash ix,")
    profile_lines.append("  /bin/dash ix,")
    profile_lines.append("  /bin/sh ix,")
    profile_lines.append("  /usr/bin/python3* ix,")
    profile_lines.append("")

    # Allow basic proc and sys access
    profile_lines.append("  # Proc and sys access")
    profile_lines.append("  @{PROC}/ r,")
    profile_lines.append("  @{PROC}/@{pid}/** r,")
    profile_lines.append("  /sys/kernel/mm/transparent_hugepage/hpage_pmd_size r,")
    profile_lines.append("")

    # Close the profile
    profile_lines.append("}")

    # Write the profile
    profile_content = "\n".join(profile_lines)
    with open(output_path, 'w') as f:
        f.write(profile_content)

    print(f"Generated AppArmor profile: {output_path}")
    print(f"Profile name: {profile_name}")

    # Print the profile for debugging
    print("\n--- Profile Content ---")
    print(profile_content)
    print("--- End Profile ---\n")

    return output_path


if __name__ == "__main__":
    try:
        profile_path = generate_apparmor_profile()
        print(f"\nSuccess! Profile written to {profile_path}")
        sys.exit(0)
    except Exception as e:
        print(f"Error generating profile: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)
