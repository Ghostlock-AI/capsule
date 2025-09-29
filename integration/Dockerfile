FROM rust:1.81-slim-bullseye

# Install system dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        python3 \
        python3-venv \
        python3-pip \
        python3-dev \
        build-essential \
        libseccomp-dev \
        lsof \
        strace \
        curl \
        ca-certificates \
        procps && \
    rm -rf /var/lib/apt/lists/*

# Update Rust to latest stable version to handle primp compilation
RUN rustup update stable && \
    rustup default stable

# Verify Rust version (should be 1.81+)
RUN rustc --version

# ----------- INSTALL NODE.JS 20.x LTS ---------------------
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# ----------- INSTALL CLAUDE CODE CLI ----------------------
RUN npm install -g @anthropic-ai/claude-code

# ----------- INSTALL OPENAI CODEX CLI ---------------------
RUN npm install -g @openai/codex@latest

# ----------- SETUP WORKING DIRECTORY ----------------------
WORKDIR /working

# Note: Code is shared via volume mounts in docker-compose.yml
# This allows live code changes to be reflected in the container

CMD ["/bin/bash"]
