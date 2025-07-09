#!/bin/bash

echo "=== Testing all-smi cleanup ==="

# Check initial state
echo "Initial temp files:"
ls -la /tmp/all-smi_powermetrics_* 2>/dev/null || echo "No temp files found"

# Run all-smi for a few seconds
echo -e "\nStarting all-smi..."
timeout 5 sudo ./target/release/all-smi view 2>&1 &
PID=$!

# Wait a moment for it to start
sleep 2

# Check if temp file was created
echo -e "\nTemp files while running:"
ls -la /tmp/all-smi_powermetrics_* 2>/dev/null || echo "No temp files found"

# Wait for timeout to kill it
wait $PID 2>/dev/null

# Give cleanup a moment
sleep 1

# Check final state
echo -e "\nTemp files after exit:"
ls -la /tmp/all-smi_powermetrics_* 2>/dev/null || echo "No temp files found"

echo -e "\n=== Test complete ==="