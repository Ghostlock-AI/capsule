#!/bin/bash
set -e

echo "==================================="
echo "Starting Tracee eBPF Tracer"
echo "==================================="
echo "Configuration: /etc/tracee/config.yaml"
echo "Event log: /var/log/tracee/events.jsonl"
echo "Tracee log: /var/log/tracee/tracee.log"
echo "==================================="

# Start tracee in the background with agent UID scope (UID 1001)
/usr/local/bin/tracee \
    --config /etc/tracee/config.yaml \
    --scope uid=1001 \
    --scope pid=new \
    --scope follow \
    --log file:/var/log/tracee/tracee.log &

TRACEE_PID=$!
echo "Tracee started (PID: $TRACEE_PID)"
echo "Waiting for tracee to initialize..."
sleep 3

# Check if tracee is running
if ! kill -0 $TRACEE_PID 2>/dev/null; then
    echo "ERROR: Tracee failed to start!"
    echo "--- Tracee log ---"
    cat /var/log/tracee/tracee.log 2>/dev/null || echo "No log file"
    exit 1
fi

echo "Tracee is running!"
echo ""
echo "==================================="
echo "You can now:"
echo "  1. Run commands as agent user:"
echo "     docker exec -it -u agent <container> bash"
echo "  2. View raw tracee events:"
echo "     docker exec -it <container> tail -f /var/log/tracee/events.jsonl | jq ."
echo "  3. View tracee logs:"
echo "     docker exec -it <container> tail -f /var/log/tracee/tracee.log"
echo "==================================="

# Keep container running and monitor tracee
while kill -0 $TRACEE_PID 2>/dev/null; do
    sleep 5
done

echo "ERROR: Tracee process died!"
echo "--- Tracee log ---"
tail -100 /var/log/tracee/tracee.log
exit 1
