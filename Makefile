.PHONY: install check-lima install-lima help

help:
	@echo "Capsule Installation Makefile"
	@echo ""
	@echo "Targets:"
	@echo "  install       - Install capsule (checks/installs Lima first on macOS)"
	@echo "  check-lima    - Check if Lima is installed"
	@echo "  install-lima  - Install Lima via Homebrew (macOS only)"
	@echo "  help          - Show this help message"

check-lima:
	@if [ "$$(uname)" = "Darwin" ]; then \
		if ! command -v limactl >/dev/null 2>&1; then \
			echo "Lima is not installed on this system."; \
			exit 1; \
		else \
			echo "Lima is already installed."; \
		fi \
	else \
		echo "Not running on macOS, skipping Lima check."; \
	fi

install-lima:
	@if [ "$$(uname)" != "Darwin" ]; then \
		echo "Error: Lima installation via this Makefile is only supported on macOS."; \
		exit 1; \
	fi
	@if ! command -v brew >/dev/null 2>&1; then \
		echo "Error: Homebrew is not installed. Please install Homebrew first:"; \
		echo "  /bin/bash -c \"\$$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""; \
		exit 1; \
	fi
	@echo "Installing Lima via Homebrew..."
	@brew install lima

install:
	@if [ "$$(uname)" = "Darwin" ]; then \
		if ! command -v limactl >/dev/null 2>&1; then \
			echo "Lima not found. Installing..."; \
			$(MAKE) install-lima; \
		else \
			echo "Lima is already installed."; \
		fi \
	fi
	@echo "Running capsule installation script..."
	@cd capsule && bash scripts/install.sh
