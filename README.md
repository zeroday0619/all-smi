# all-smi

`all-smi` is a command-line utility for monitoring GPU hardware. It provides a real-time view of GPU utilization, memory usage, temperature, power consumption, and other metrics. The tool is designed to be a cross-platform alternative to `nvidia-smi`, with support for both NVIDIA and Apple Silicon GPUs.

The application presents a terminal-based user interface that displays GPU information in a clear and organized manner. It also lists processes currently utilizing the GPU, along with their memory consumption.

## Features

- **GPU Monitoring:** Displays real-time metrics for each detected GPU, including:
  - GPU Name and Driver Version
  - Utilization Percentage
  - Memory Usage (Used and Total)
  - Temperature
  - Clock Frequency
  - Power Consumption
- **Process Information:** Lists processes running on the GPU, showing:
  - Process ID (PID)
  - Process Name
  - GPU Memory Usage
- **Cross-Platform Support:**
  - **Linux:** Supports NVIDIA GPUs via the `nvidia-smi` command.
  - **macOS:** Supports Apple Silicon GPUs by interfacing with system commands like `powermetrics` and `system_profiler`.
- **Interactive UI:**
  - A terminal-based UI that refreshes periodically.
  - Allows sorting of the process list by PID or memory usage.
  - Provides clear visual bars for utilization and memory.

## Technology Stack

- **Language:** Rust
- **Key Crates:**
  - `tokio`: For asynchronous runtime.
  - `crossterm`: For terminal manipulation and UI.
  - `chrono`: For timestamping.
  - `metal` & `objc`: For Apple Silicon GPU interaction on macOS.

## Installation

### Prerequisites

- Rust and Cargo must be installed.
- On Linux with an NVIDIA GPU, the `nvidia-smi` command must be available.
- On macOS, the tool requires `sudo` privileges to run `powermetrics`.

### Building from source

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/inureyes/all-smi.git
    cd all-smi
    ```
2.  **Build the project:**
    ```bash
    # Build the main application
    cargo build --release
    
    # Build specific binary (e.g., mock-server)
    cargo build --release --bin mock-server
    ```

## Usage

`all-smi` can be run in two modes: `view` and `api`. You can see the help message for each mode by running:

```bash
./target/release/all-smi --help
./target/release/all-smi view --help
./target/release/all-smi api --help
```

### View Mode

The `view` mode displays a terminal-based user interface. This is the default mode if no command is provided.

```bash
sudo ./target/release/all-smi view
```

You can also monitor remote machines running `all-smi` in API mode.

#### Remote Monitoring

To monitor remote machines, you can use the `--hosts` or `--hostfile` argument.

- **Using `--hosts`:**

  Pass a list of host addresses to the `--hosts` argument.

  ```bash
  ./target/release/all-smi view --hosts http://remote1:9090 http://remote2:9090
  ```

- **Using `--hostfile`:**

  Create a file with a list of host addresses (one per line) and pass the file path to the `--hostfile` argument.

  ```bash
  # hosts.txt
  http://remote1:9090
  http://remote2:9090
  ```

  ```bash
  ./target/release/all-smi view --hostfile hosts.txt
  ```

### API Mode

The `api` mode exposes the GPU metrics in Prometheus format.

```bash
./target/release/all-smi api --port 9090
```

The metrics will be available at `http://localhost:9090/metrics`.

## Testing with Mock Server

For testing and development purposes, a mock server is provided that simulates multiple GPU nodes with realistic metrics.

### Building the Mock Server

```bash
cargo build --release --bin mock-server
```

### Running the Mock Server

```bash
# Start a single mock server on port 9090
./target/release/mock-server --port-range 9090

# Start multiple mock servers on a port range
./target/release/mock-server --port-range 10001-10010

# Specify custom GPU name and output file
./target/release/mock-server --port-range 10001-10005 --gpu-name "NVIDIA RTX 4090" -o test-hosts.csv
```

The mock server will:
- Generate realistic GPU metrics with random variations
- Create a CSV file with the host addresses for easy testing
- Simulate multiple GPUs per node
- Provide disk usage information

### Testing with Mock Data

After starting the mock servers, you can test the view mode with the generated hosts file:

```bash
# View mock data from multiple hosts
./target/release/all-smi view --hostfile hosts.csv
```

## Contributing

Contributions are welcome! Please feel free to submit a pull request or open an issue.

## License

This project is licensed under the MIT License.
