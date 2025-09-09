FROM rust:1.80-slim-bullseye

# Included:
#           python3
#           python3-venv
#           lsof
#           libseccomp-dev
#           strace
#           procps (ps)
#           curl (for Node.js install)
#           ca-certificates (for HTTPS)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        python3 python3-venv python3-pip libseccomp-dev lsof strace \
        curl ca-certificates procps && \
    rm -rf /var/lib/apt/lists/*

# ----------- INSTALL NODE.JS 20.x LTS ---------------------
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# ----------- INSTALL CLAUDE CODE CLI ----------------------
RUN npm install -g @anthropic-ai/claude-code

# ----------- INSTALL OPENAI SDK/CLI -----------------------
# Note: OpenAI Codex models are retired and not downloadable.
# This installs the official OpenAI SDK/CLI so you can call
# current OpenAI models via API from within the container.
RUN npm install -g openai

WORKDIR /capsule
COPY agents/  ./agents
COPY capsule/ ./capsule
# ----------- INSTALL CAPSULE GLOBALLY --------------------
# Build and install capsule globally
RUN cd capsule && \
    cargo install --path cli/ --force

# ----------- SET UP AGENT ENVS ----------------------------
# Set up a Python virtual environment in capsule-agents base
RUN cd agents/catalog/base && \
    python3 -m venv .venv && \
    .venv/bin/pip install --upgrade pip && \
    .venv/bin/pip install openai


CMD ["/bin/bash"]
