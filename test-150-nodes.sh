#!/bin/bash

echo "=== Testing 150 mock nodes ==="

# Start mock servers
echo "Starting 150 mock servers..."
./start-mock-cluster.sh --port-range 10001-10150 --ports-per-process 50

# Wait for servers to fully start
echo "Waiting for servers to initialize..."
sleep 5

# Count running processes
RUNNING=$(ps aux | grep -c "all-smi-mock-server.*--port-range" | grep -v grep || echo 0)
echo "Mock server processes running: $RUNNING"

# Check if all ports are listening
echo "Checking listening ports..."
LISTENING_10K=$(lsof -iTCP:10001-10150 -sTCP:LISTEN 2>/dev/null | grep -c "all-smi" || echo 0)
echo "Ports actually listening: $LISTENING_10K"

# Test direct connections
echo -e "\nTesting direct connections to sample ports..."
SUCCESS=0
FAIL=0
for port in 10001 10050 10100 10150; do
    if curl -s -m 2 http://localhost:$port/metrics | grep -q "all_smi_"; then
        echo "Port $port: OK"
        ((SUCCESS++))
    else
        echo "Port $port: FAILED"
        ((FAIL++))
    fi
done
echo "Direct test: $SUCCESS succeeded, $FAIL failed"

# Now test with all-smi
echo -e "\nTesting with all-smi view..."
timeout 10 ./target/release/all-smi view --hostfile hosts.csv --interval 2 > all-smi-output.log 2>&1 &
VIEWER_PID=$!

# Wait for connections
sleep 5

# Check how many nodes connected
if [ -f all-smi-output.log ]; then
    CONNECTED=$(grep -o "node-[0-9]*" all-smi-output.log 2>/dev/null | sort -u | wc -l || echo 0)
    echo "Nodes visible in all-smi: $CONNECTED"
    
    # Check for connection errors
    ERRORS=$(grep -c "Connection error\|failed\|timeout" all-smi-output.log 2>/dev/null || echo 0)
    echo "Connection errors in log: $ERRORS"
fi

# Check resource usage
echo -e "\nResource usage:"
echo "Total file descriptors in use: $(lsof 2>/dev/null | wc -l)"
echo "Mock server FDs: $(lsof -c all-smi-mock 2>/dev/null | wc -l || echo 0)"

# Kill viewer
kill $VIEWER_PID 2>/dev/null

# Sample some specific mock servers
echo -e "\nChecking individual mock server processes:"
for pid in $(pgrep -f "all-smi-mock-server" | head -3); do
    if [ -n "$pid" ]; then
        FDS=$(lsof -p $pid 2>/dev/null | wc -l || echo 0)
        PORTS=$(ps aux | grep -E "^\S+\s+$pid" | grep -oE "port-range [0-9]+-[0-9]+" || echo "unknown")
        echo "PID $pid: $FDS FDs, $PORTS"
    fi
done

# Cleanup
rm -f all-smi-output.log