FROM rust:1.80-slim-bullseye

# all installed CLI programs
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        python3 \
        python3-venv \
        python3-pip \
        libseccomp-dev \
        lsof \
        strace \
        curl \ 
        ca-certificates \ 
        procps && \
    rm -rf /var/lib/apt/lists/*

# ----------- INSTALL NODE.JS 20.x LTS ---------------------
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# ----------- INSTALL CLAUDE CODE CLI ----------------------
RUN npm install -g @anthropic-ai/claude-code

# ----------- INSTALL OPENAI CODEX CLI ---------------------
RUN npm install -g @openai/codex@latest

# ----------- COPY AGENTS and CAPSULE ----------------------
WORKDIR /capsule
COPY agents/  ./agents
COPY capsule/ ./capsule

# ----------- INSTALL CAPSULE GLOBALLY --------------------
# Build and install capsule globally
RUN cd capsule && \
    cargo install --path cli/ --force

CMD ["/bin/bash"]
