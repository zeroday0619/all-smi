#!/bin/bash
# Script to start multiple mock server processes for large port ranges

# Default values (matching mock server defaults)
PORT_RANGE=""
GPU_NAME="NVIDIA H200 141GB HBM3"
PLATFORM="nvidia"
OUTPUT="hosts.csv"
FAILURE_NODES=0
PORTS_PER_PROCESS=50

# Function to show help
show_help() {
    cat << EOF
Start or stop multiple mock server processes for large port ranges

USAGE:
    $(basename "$0") [OPTIONS]
    $(basename "$0") stop

COMMANDS:
    stop                        Stop all running mock servers

OPTIONS:
    --port-range <range>        Port range, e.g., 10001-10010 or 10001
    --gpu-name <name>           GPU name (default: $GPU_NAME)
    --platform <type>           Platform type: nvidia, apple, jetson, intel, amd (default: $PLATFORM)
    -o, --output <file>         Output CSV file name (default: $OUTPUT)
    --failure-nodes <count>     Number of nodes to simulate random failures (default: $FAILURE_NODES)
    --ports-per-process <num>   Maximum ports per process (default: $PORTS_PER_PROCESS, limited by system file descriptors)
    -h, --help                  Show this help message

EXAMPLES:
    # Start 125 mock servers on ports 10001-10125
    $(basename "$0") --port-range 10001-10125
    
    # Start with GPU name and failure simulation
    $(basename "$0") --port-range 10001-10200 --gpu-name "NVIDIA A100" --failure-nodes 5
    
    # Stop all mock servers
    $(basename "$0") stop

EOF
}

# Check for stop command
if [[ "$1" == "stop" ]]; then
    echo "Stopping all mock servers..."
    pkill -f all-smi-mock-server
    KILLED=$?
    if [ $KILLED -eq 0 ]; then
        echo "Mock servers stopped successfully"
    else
        echo "No mock servers were running"
    fi
    exit $KILLED
fi

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --port-range)
            PORT_RANGE="$2"
            shift 2
            ;;
        --gpu-name)
            GPU_NAME="$2"
            shift 2
            ;;
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT="$2"
            shift 2
            ;;
        --failure-nodes)
            FAILURE_NODES="$2"
            shift 2
            ;;
        --ports-per-process)
            PORTS_PER_PROCESS="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Check if port range is provided
if [ -z "$PORT_RANGE" ]; then
    echo "Error: --port-range is required"
    show_help
    exit 1
fi

# Parse port range
if [[ "$PORT_RANGE" =~ ^([0-9]+)-([0-9]+)$ ]]; then
    START_PORT="${BASH_REMATCH[1]}"
    END_PORT="${BASH_REMATCH[2]}"
    TOTAL_PORTS=$((END_PORT - START_PORT + 1))
elif [[ "$PORT_RANGE" =~ ^[0-9]+$ ]]; then
    START_PORT="$PORT_RANGE"
    END_PORT="$PORT_RANGE"
    TOTAL_PORTS=1
else
    echo "Error: Invalid port range format. Use 'start-end' or single port number"
    exit 1
fi

echo "Starting mock cluster with $TOTAL_PORTS ports ($START_PORT-$END_PORT)"
echo "Configuration:"
echo "  GPU Name: $GPU_NAME"
echo "  Platform: $PLATFORM"
echo "  Failure Nodes: $FAILURE_NODES"
echo "  Ports per Process: $PORTS_PER_PROCESS"

# Calculate number of processes needed
NUM_PROCESSES=$(( (TOTAL_PORTS + PORTS_PER_PROCESS - 1) / PORTS_PER_PROCESS ))
echo "  Processes: $NUM_PROCESSES"

# Check file descriptor limit
SOFT_LIMIT=$(ulimit -Sn)
if [ $SOFT_LIMIT -lt 1024 ] && [ $PORTS_PER_PROCESS -gt 20 ]; then
    echo ""
    echo "WARNING: Low file descriptor limit detected: $SOFT_LIMIT"
    echo "  Each process can only handle ~20 ports with this limit."
    echo "  To increase: ulimit -n 1024"
fi

# Clean up old temporary files
rm -f hosts_*.csv

# Start each process
PIDS=()
for i in $(seq 0 $((NUM_PROCESSES - 1))); do
    PROCESS_START=$((START_PORT + i * PORTS_PER_PROCESS))
    PROCESS_END=$((PROCESS_START + PORTS_PER_PROCESS - 1))
    
    # Don't exceed total ports
    MAX_PORT=$((START_PORT + TOTAL_PORTS - 1))
    if [ $PROCESS_END -gt $MAX_PORT ]; then
        PROCESS_END=$MAX_PORT
    fi
    
    if [ $PROCESS_START -le $MAX_PORT ]; then
        echo "Starting process $((i + 1)): ports $PROCESS_START-$PROCESS_END"
        
        # Build command with all arguments
        CMD="./target/release/all-smi-mock-server"
        CMD="$CMD --port-range $PROCESS_START-$PROCESS_END"
        CMD="$CMD --gpu-name \"$GPU_NAME\""
        CMD="$CMD --platform $PLATFORM"
        CMD="$CMD -o hosts_$i.csv"
        
        # Only add failure-nodes to first process to avoid conflicts
        if [ $i -eq 0 ] && [ $FAILURE_NODES -gt 0 ]; then
            CMD="$CMD --failure-nodes $FAILURE_NODES"
        fi
        
        # Execute in background with explicit ulimit and save PID
        (ulimit -n 1048575 && eval "$CMD") &
        PIDS+=($!)
    fi
done

# Wait a moment for all processes to start
echo "Waiting for processes to start..."
sleep 2

# Check if processes are running
RUNNING=0
for pid in "${PIDS[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
        ((RUNNING++))
    fi
done

if [ $RUNNING -eq 0 ]; then
    echo "Error: No mock server processes started successfully"
    exit 1
fi

echo "$RUNNING/$NUM_PROCESSES processes started successfully"

# Combine all CSV files
echo "Combining host files..."
if ls hosts_*.csv 1> /dev/null 2>&1; then
    cat hosts_*.csv > "$OUTPUT"
    rm -f hosts_*.csv
    echo "Created $OUTPUT with $TOTAL_PORTS hosts"
else
    echo "Warning: No host CSV files found"
fi

echo ""
echo "Mock cluster started with PIDs: ${PIDS[*]}"
echo "To stop all servers, run: $(basename "$0") stop"
echo "To view logs: ps aux | grep all-smi-mock-server"