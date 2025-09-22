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

# ----------- COPY AGENTS and CAPSULE ----------------------
WORKDIR /capsule
COPY agents/  ./agents
COPY capsule/ ./capsule

# ----------- INSTALL CAPSULE GLOBALLY --------------------
# Build and install capsule globally
RUN cd capsule && \
    cargo install --path cli/ --force

# ----------- SETUP PYTHON ENVIRONMENT FOR AGENTS ----------
# Create virtual environment for Python agents
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Upgrade pip and install wheel for better package support
RUN pip install --upgrade pip wheel

# Pre-install common Python packages to cache them
RUN pip install --only-binary=all \
        langchain-core \
        langchain-community \
        langchain-openai \
        langgraph \
        python-dotenv \
        openai \
        pydantic

# Install Tavily search (pure Python, no Rust compilation needed)
RUN pip install langchain-tavily

CMD ["/bin/bash"]