# ── Dockerfile ────────────────────────────────────────────────────────────────
# 0. Base OS                                                             
FROM ubuntu:22.04                                                      

# 1. System packages – Python 3.11 + curl (needed by httpx for HTTP/2)  
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        python3.11 python3.11-venv python3.11-dev \
        build-essential curl && \
    rm -rf /var/lib/apt/lists/*

# If we create a non-root user we basically
# give the agent more security out of the box
# than many agent alphas launch with so 
# leave it as root for now 😂
# RUN useradd -m capsule
# WORKDIR /app
# USER capsule

# Virtual-env + requirements
RUN python3.11 -m venv /home/capsule/venv
ENV PATH="/home/capsule/venv/bin:${PATH}"

COPY requirements.txt .
RUN pip install -U pip && pip install -r requirements.txt

# Copy source and set default command
COPY src ./src
EXPOSE 8000
CMD ["uvicorn", "src.server:app", "--host", "0.0.0.0", "--port", "8000", "--workers", "1"]
