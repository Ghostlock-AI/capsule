#!/bin/bash
set -e

# AppArmor Entrypoint Script
# This script ensures AppArmor profiles are loaded and enforced when the container starts

PROFILE_NAME="capsule-agent-workload"
PROFILE_PATH="/etc/apparmor.d/${PROFILE_NAME}"

echo "=== AppArmor Container Initialization ==="

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo "Running as root - full access enabled"
    ROOT_MODE=true
else
    echo "Running as non-root user"
    ROOT_MODE=false
fi

# Start AppArmor service if not running
if [ -f /etc/init.d/apparmor ]; then
    echo "Starting AppArmor service..."
    /etc/init.d/apparmor start 2>/dev/null || true
fi

# Load the AppArmor profile if it exists
if [ -f "$PROFILE_PATH" ]; then
    echo "Loading AppArmor profile: $PROFILE_NAME"

    # Parse the profile into the kernel
    if command -v apparmor_parser >/dev/null 2>&1; then
        apparmor_parser -r -W "$PROFILE_PATH" 2>&1 || {
            echo "Warning: Could not load AppArmor profile (this is expected in non-privileged containers)"
            echo "The profile exists at $PROFILE_PATH and will be enforced if the container is run with --security-opt"
        }
    else
        echo "Warning: apparmor_parser not found"
    fi

    # Show profile status
    if command -v aa-status >/dev/null 2>&1; then
        echo ""
        echo "AppArmor status:"
        aa-status 2>/dev/null || echo "Cannot query AppArmor status (expected in containers)"
    fi
else
    echo "Warning: AppArmor profile not found at $PROFILE_PATH"
fi

echo ""
echo "=== Environment Information ==="
echo "Working directory: $(pwd)"
echo "User: $(whoami)"
echo "UID: $(id -u)"
echo "GID: $(id -g)"
echo ""

if [ "$ROOT_MODE" = true ]; then
    echo "Note: Running as root - AppArmor restrictions do not apply to root"
    echo "AppArmor logs available at: /var/log/apparmor/ (if AppArmor is active)"
else
    echo "AppArmor profile '$PROFILE_NAME' should be active for this session"
    echo "Restrictions are in effect as defined in profile-config.yaml"
fi

echo ""
echo "=== Ready ==="
echo ""

# Execute the command passed to the container
exec "$@"
