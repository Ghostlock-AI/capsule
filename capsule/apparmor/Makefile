.PHONY: build test shell demo clean help

IMAGE_NAME := apparmor-isolation
PROFILE_NAME := capsule-agent-workload

help: ## Show this help message
	@echo "AppArmor Isolation Container - Available Commands:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

build: ## Build the Docker image
	docker build -t $(IMAGE_NAME) .

test: build ## Run the AppArmor test suite
	docker run --rm $(IMAGE_NAME) python3 /workspace/test-apparmor.py

shell: build ## Start an interactive bash shell in the container
	docker run -it --rm $(IMAGE_NAME) /bin/bash

shell-enforced: build ## Start shell with AppArmor enforcement (requires profile on host)
	@echo "Note: This requires the AppArmor profile to be loaded on the host system"
	docker run -it --rm --security-opt apparmor=$(PROFILE_NAME) $(IMAGE_NAME) /bin/bash

demo: build ## Run the full demonstration
	./demo.sh

profile: build ## Display the generated AppArmor profile
	docker run --rm $(IMAGE_NAME) cat /etc/apparmor.d/$(PROFILE_NAME)

install-profile: build ## Extract and install the AppArmor profile on the host (requires sudo)
	@echo "Extracting AppArmor profile from container..."
	docker run --rm $(IMAGE_NAME) cat /etc/apparmor.d/$(PROFILE_NAME) > /tmp/$(PROFILE_NAME)
	@echo "Installing profile to /etc/apparmor.d/$(PROFILE_NAME) (requires sudo)..."
	sudo cp /tmp/$(PROFILE_NAME) /etc/apparmor.d/
	@echo "Loading profile into kernel (requires sudo)..."
	sudo apparmor_parser -r -W /etc/apparmor.d/$(PROFILE_NAME)
	@echo "Verifying profile is loaded..."
	sudo aa-status | grep $(PROFILE_NAME) || echo "Warning: Profile may not be loaded"
	@echo "Done! You can now use 'make shell-enforced' to run with AppArmor enforcement"

clean: ## Remove the Docker image
	docker rmi $(IMAGE_NAME) || true

python-test: build ## Run a simple Python test in the container
	docker run --rm $(IMAGE_NAME) python3 -c "print('Python works!'); import os; print(f'CWD: {os.getcwd()}')"

workspace-test: build ## Test workspace read/write permissions
	docker run --rm $(IMAGE_NAME) bash -c "echo 'test' > /workspace/test.txt && cat /workspace/test.txt && rm /workspace/test.txt && echo 'Workspace is writable!'"
