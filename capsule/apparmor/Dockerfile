FROM ubuntu:20.04

# Prevent interactive prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install AppArmor, Python3, and other utilities
RUN apt-get update && apt-get install -y \
    apparmor \
    apparmor-utils \
    python3 \
    python3-pip \
    python3-yaml \
    vim \
    sudo \
    && rm -rf /var/lib/apt/lists/*

# Create workspace directory
RUN mkdir -p /workspace /tmp /var/log/capsule

# Create the AppArmor profile directory
RUN mkdir -p /etc/apparmor.d

# Copy configuration files
COPY profile-config.yaml /etc/apparmor/profile-config.yaml
COPY generate-profile.py /usr/local/bin/generate-profile.py
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY test-apparmor.py /workspace/test-apparmor.py

# Make scripts executable
RUN chmod +x /usr/local/bin/generate-profile.py \
    /usr/local/bin/entrypoint.sh

# Generate the AppArmor profile from YAML config
RUN python3 /usr/local/bin/generate-profile.py

# Set default working directory
WORKDIR /workspace

# Use entrypoint to ensure AppArmor is enforced
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["/bin/bash"]
