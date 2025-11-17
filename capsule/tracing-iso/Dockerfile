FROM ubuntu:24.04

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    linux-headers-generic \
    bpfcc-tools \
    build-essential \
    git \
    jq \
    && rm -rf /var/lib/apt/lists/*

# Create agent user with UID 1000 (matching VM setup)
# Ubuntu 24.04 already has ubuntu user with GID/UID 1000, so we'll use different IDs
RUN groupadd -g 1001 agent && \
    useradd -u 1001 -g agent -s /bin/bash -m agent

# Create log directory
RUN mkdir -p /var/log/tracee && \
    chown -R root:root /var/log/tracee && \
    chmod 755 /var/log/tracee

# Install Tracee v0.23.2
RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "aarch64" ]; then \
        TAR="tracee-aarch64.v0.23.2.tar.gz"; \
    else \
        TAR="tracee-x86_64.v0.23.2.tar.gz"; \
    fi && \
    curl -fsSL "https://github.com/aquasecurity/tracee/releases/download/v0.23.2/$TAR" -o /tmp/tracee.tar.gz && \
    cd /tmp && tar -xzf tracee.tar.gz && \
    cp dist/tracee /usr/local/bin/tracee && \
    cp -r dist/signatures /usr/local/bin/signatures && \
    chmod +x /usr/local/bin/tracee && \
    rm -rf /tmp/tracee.tar.gz /tmp/dist /tmp/LICENSE

# Create tracee configuration (matching VM setup)
RUN mkdir -p /etc/tracee
COPY tracee-config.yaml /etc/tracee/config.yaml

# Create workspace for agent
RUN mkdir -p /home/agent/workspace && \
    chown -R agent:agent /home/agent/workspace

# Create startup script
COPY start-tracee.sh /usr/local/bin/start-tracee.sh
RUN chmod +x /usr/local/bin/start-tracee.sh

# Expose log volume
VOLUME ["/var/log/tracee"]

# Run tracee as root (eBPF requires privileges)
ENTRYPOINT ["/usr/local/bin/start-tracee.sh"]
