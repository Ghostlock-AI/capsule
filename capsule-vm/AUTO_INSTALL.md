# Auto-Installation Feature

Capsule VM now automatically installs VM backends (Multipass or Lima) when they're not found on your system!

## How It Works

When you run a command with a backend that isn't installed:

```bash
capsule-vm --backend lima create myvm .
```

**Before:**
```
Error: limactl not found in PATH
```

**Now:**
```
📦 Lima not found. Installing lima...
🔧 Detecting platform and installing Lima...
🍺 Installing Lima via Homebrew...
✅ Lima installed successfully!
🔧 Using backend: lima
🚀 Creating VM 'myvm'...
```

## Platform Support

### macOS

**Multipass:**
- Installed via Homebrew (`brew install --cask multipass`)
- Requires: Homebrew installed

**Lima:**
- Installed via Homebrew (`brew install lima`)
- Requires: Homebrew installed

**If Homebrew is not installed:**
```
Homebrew not found. Please install it first:
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

### Linux

**Multipass:**
- Installed via Snap (`sudo snap install multipass`)
- Requires: snapd installed

**Lima:**
- **Ubuntu/Debian:** `sudo apt-get install lima` (falls back to GitHub if not in repos)
- **Fedora/RHEL/CentOS:** `sudo dnf install lima` or `sudo yum install lima`
- **Arch Linux:** `sudo pacman -S lima`
- **Other distros:** Downloads from GitHub releases

**If package manager fails:**
- Falls back to downloading from https://github.com/lima-vm/lima/releases

### Windows

**Multipass:**
- **winget:** `winget install Canonical.Multipass`
- **Chocolatey:** `choco install multipass -y`
- Falls back to manual installation instructions if neither is available

**Lima:**
- Not officially supported on Windows
- Displays manual installation instructions

## Usage Examples

### Automatic Backend Selection

```bash
# Uses multipass if available, otherwise tries lima
capsule-vm create myvm .

# If neither is installed, installs multipass (original default)
```

### Explicit Backend with Auto-Install

```bash
# Force Lima (installs if needed)
capsule-vm --backend lima create myvm .

# Force Multipass (installs if needed)
capsule-vm --backend multipass create myvm .
```

### First-Time Setup

```bash
# First run on a fresh system
capsule-vm create myvm .

# Output:
# 📦 Multipass not found. Installing multipass...
# 🔧 Detecting platform and installing Multipass...
# 🍺 Installing Multipass via Homebrew...
# ==> Downloading https://github.com/canonical/multipass/releases/...
# ==> Installing Cask multipass
# ✅ Multipass installed successfully!
# 🔧 Using backend: multipass
# 🚀 Creating VM 'myvm'...
```

## Installation Methods by Platform

| Platform | Backend | Method | Requires |
|----------|---------|--------|----------|
| macOS | Multipass | Homebrew cask | brew |
| macOS | Lima | Homebrew | brew |
| Linux | Multipass | Snap | snapd |
| Linux (Ubuntu/Debian) | Lima | apt → GitHub fallback | sudo |
| Linux (Fedora/RHEL) | Lima | dnf/yum → GitHub fallback | sudo |
| Linux (Arch) | Lima | pacman → GitHub fallback | sudo |
| Linux (other) | Lima | GitHub releases | curl, sudo |
| Windows | Multipass | winget or choco | winget/choco |

## Manual Installation Override

If you prefer to install manually:

### Multipass
```bash
# macOS
brew install --cask multipass

# Linux
sudo snap install multipass

# Windows
winget install Canonical.Multipass
```

### Lima
```bash
# macOS
brew install lima

# Linux (Ubuntu/Debian)
sudo apt-get update && sudo apt-get install -y lima

# Linux (generic - downloads from GitHub)
VERSION=$(curl -fsSL https://api.github.com/repos/lima-vm/lima/releases/latest | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')
curl -fsSL "https://github.com/lima-vm/lima/releases/download/v${VERSION}/lima-${VERSION}-Linux-x86_64.tar.gz" | sudo tar -C /usr/local -xzf -
```

## Troubleshooting

### Installation Fails

**Error:** `Failed to install Lima via Homebrew`

**Solution:**
```bash
# Update Homebrew
brew update

# Try manual installation
brew install lima

# If that fails, check Homebrew health
brew doctor
```

### Binary Not Found After Installation

**Error:** `installation completed but binary not found in PATH`

**Possible causes:**
1. Package manager succeeded but binary location not in PATH
2. Terminal session needs to be restarted

**Solution:**
```bash
# Restart terminal or reload shell
source ~/.bashrc  # or ~/.zshrc

# Check if binary is now available
which limactl
which multipass

# If still not found, check installation location
brew --prefix lima    # macOS
which -a limactl      # Linux
```

### Homebrew Not Installed (macOS)

**Error:** `Homebrew not found`

**Solution:**
```bash
# Install Homebrew first
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Then run capsule-vm again
capsule-vm create myvm .
```

### Snap Not Available (Linux)

**Error:** `Snap not found. Multipass on Linux requires snap.`

**Solution:**
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install snapd

# Fedora
sudo dnf install snapd
sudo systemctl enable --now snapd.socket

# Arch
sudo pacman -S snapd
sudo systemctl enable --now snapd.socket

# Then run capsule-vm again
capsule-vm create myvm .
```

### Permission Denied During Installation

**Error:** `Permission denied` or `sudo required`

**Explanation:** Installing system packages requires root privileges.

**Solution:**
- Commands will prompt for sudo password automatically
- Ensure your user has sudo access
- Alternatively, install manually without capsule-vm

## Disabling Auto-Install

If you want to prevent auto-installation and get an error instead:

Currently not supported - auto-install always attempts when backend is missing.

**Workaround:** Install backends manually before using capsule-vm.

## Security Considerations

Auto-installation:
- ✅ Uses official package managers (Homebrew, apt, snap, etc.)
- ✅ Downloads from official sources only
- ✅ Verifies package manager availability first
- ✅ Provides clear output of what's being installed
- ⚠️  Requires sudo/admin privileges for system-level installation
- ⚠️  Network access required to download packages

**Best practice:** Review installation scripts and only run on trusted systems.

## Testing Auto-Install

To test the auto-install feature without uninstalling backends:

```bash
# Temporarily hide the binary
sudo mv /usr/local/bin/limactl /usr/local/bin/limactl.bak

# Test auto-install
capsule-vm --backend lima ps

# Restore original
sudo mv /usr/local/bin/limactl.bak /usr/local/bin/limactl
```

## Implementation Details

Auto-installation is triggered in `backends/{multipass,lima}.rs`:

```rust
pub fn new() -> Result<Self> {
    let binary = match which::which("limactl") {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(_) => {
            eprintln!("📦 Lima not found. Installing lima...");
            Self::install_lima()?;  // ← Auto-install
            which::which("limactl")
                .context("installation completed but binary not found")?
                .to_string_lossy()
                .to_string()
        }
    };
    Ok(Self { binary })
}
```

**Flow:**
1. Check if binary exists in PATH
2. If not found, detect OS and architecture
3. Choose appropriate installation method
4. Execute installation via package manager or direct download
5. Verify binary is now available
6. Return backend instance or error

## Future Improvements

Planned enhancements:
- [ ] Option to disable auto-install via config file
- [ ] Installation progress bars for large downloads
- [ ] Parallel backend installation (try both multipass and lima)
- [ ] Cache downloaded binaries for faster reinstalls
- [ ] Support for custom backend installation paths
- [ ] Pre-flight check to estimate installation time
- [ ] Rollback capability if installation fails

## FAQ

**Q: Does auto-install work in CI/CD environments?**

A: Yes, but ensure the CI runner has:
- Sudo access (Linux) or admin privileges (Windows)
- Package manager installed (brew, apt, snap, etc.)
- Network access to download packages

**Q: Can I use a custom Lima/Multipass installation?**

A: Yes! If the binary is in PATH, auto-install won't trigger. Just ensure `limactl` or `multipass` is available.

**Q: What if I want to install to a custom location?**

A: Auto-install uses system package managers which install to standard locations. For custom installations, install manually then use capsule-vm.

**Q: Does this slow down startup?**

A: Installation only happens once (when binary is missing). Subsequent runs are instant since the binary is found in PATH.

**Q: Can I see what commands will be run before installation?**

A: Currently no, but the code is open source. Review `src/backends/{multipass,lima}.rs` for exact commands.

---

## Summary

Auto-installation makes Capsule VM truly zero-setup:

**Before:**
1. Install Homebrew
2. Run `brew install lima`
3. Run `capsule-vm create myvm .`

**Now:**
1. Run `capsule-vm create myvm .`
2. *(Automatically installs lima if needed)*

**That's it!** 🎉
