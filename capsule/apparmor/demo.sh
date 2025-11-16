#!/bin/bash
# Demo script for AppArmor isolation container

set -e

echo "================================================================"
echo "AppArmor Isolation Container Demo"
echo "================================================================"
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building the container image...${NC}"
docker build -t apparmor-isolation .
echo ""

echo -e "${BLUE}1. Running test suite to verify AppArmor profile${NC}"
echo "================================================================"
docker run --rm apparmor-isolation python3 /workspace/test-apparmor.py
echo ""

echo -e "${BLUE}2. Showing the generated AppArmor profile${NC}"
echo "================================================================"
docker run --rm apparmor-isolation cat /etc/apparmor.d/capsule-agent-workload
echo ""

echo -e "${BLUE}3. Testing Python execution (SHOULD WORK)${NC}"
echo "================================================================"
docker run --rm apparmor-isolation python3 -c "
print('✓ Python works!')
print('✓ Can import modules:', end=' ')
import os, sys, json
print('os, sys, json')
print('✓ Current directory:', os.getcwd())
"
echo ""

echo -e "${BLUE}4. Testing file operations${NC}"
echo "================================================================"
docker run --rm apparmor-isolation bash -c "
echo '✓ Creating file in /workspace...'
echo 'Hello World' > /workspace/test.txt
cat /workspace/test.txt
echo ''
echo '✓ Creating file in /tmp...'
echo 'Temp data' > /tmp/test.txt
cat /tmp/test.txt
echo ''
echo '✓ Reading system binaries...'
file /bin/bash
file /usr/bin/python3
"
echo ""

echo -e "${BLUE}5. Verifying root access to all directories${NC}"
echo "================================================================"
docker run --rm apparmor-isolation bash -c "
echo 'Root user can access:'
echo '  /etc:' && ls /etc | head -3 && echo '  ... (more files)'
echo ''
echo '  /root:' && ls -la /root 2>&1 | head -3
echo ''
echo '  /var/log:' && ls /var/log 2>&1 | head -3 && echo '  ... (more files)'
"
echo ""

echo -e "${GREEN}================================================================${NC}"
echo -e "${GREEN}Demo completed successfully!${NC}"
echo -e "${GREEN}================================================================${NC}"
echo ""
echo -e "${YELLOW}To start an interactive shell:${NC}"
echo "  docker run -it --rm apparmor-isolation"
echo ""
echo -e "${YELLOW}To run with AppArmor enforcement (requires profile on host):${NC}"
echo "  docker run -it --rm --security-opt apparmor=capsule-agent-workload apparmor-isolation"
echo ""
echo -e "${YELLOW}To customize the profile:${NC}"
echo "  1. Edit profile-config.yaml"
echo "  2. Rebuild: docker build -t apparmor-isolation ."
echo "  3. Test: docker run --rm apparmor-isolation python3 /workspace/test-apparmor.py"
echo ""
