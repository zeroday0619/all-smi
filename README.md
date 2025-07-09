# all-smi

[![Crates.io version](https://img.shields.io/crates/v/all-smi.svg?style=flat-square)](https://crates.io/crates/all-smi)
[![Crates.io downloads](https://img.shields.io/crates/d/all-smi.svg?style=flat-square)](https://crates.io/crates/all-smi)
![CI](https://github.com/inureyes/all-smi/workflows/CI/badge.svg)
[![dependency status](https://deps.rs/repo/github/inureyes/all-smi/status.svg)](https://deps.rs/repo/github/inureyes/all-smi)


`all-smi` is a command-line utility for monitoring GPU hardware across multiple systems. It provides a real-time view of GPU utilization, memory usage, temperature, power consumption, and other metrics. The tool is designed to be a cross-platform alternative to `nvidia-smi`, with support for NVIDIA GPUs, Apple Silicon GPUs, and NVIDIA Jetson platforms.

The application presents a terminal-based user interface with cluster overview, interactive sorting, and both local and remote monitoring capabilities. It also provides an API mode for Prometheus metrics integration.

![screenshot](screenshots/all-smi-all-tab.png)
All-node view (remote mode)

![screenshot](screenshots/all-smi-node-tab.png)
Node view (remote mode)

## Features

### GPU Monitoring
- **Real-time Metrics:** Displays comprehensive GPU information including:
  - GPU Name and Driver Version
  - Utilization Percentage with color-coded status
  - Memory Usage (Used/Total in GB)
  - Temperature in Celsius
  - Clock Frequency in MHz
  - Power Consumption in Watts
- **Multi-GPU Support:** Handles multiple GPUs per system with individual monitoring
- **Interactive Sorting:** Sort GPUs by utilization, memory usage, or default (hostname+index) order

### Cluster Management
- **Cluster Overview Dashboard:** Real-time statistics showing:
  - Total nodes and GPUs across the cluster
  - Average utilization and memory usage
  - Temperature statistics with standard deviation
  - Total and average power consumption
- **Live Statistics History:** Visual graphs showing utilization, memory, and temperature trends
- **Tabbed Interface:** Switch between "All" view and individual host tabs

### Process Information
- **GPU Process Monitoring:** Lists processes running on GPUs with:
  - Process ID (PID) and Parent PID
  - Process Name and Command Line
  - GPU Memory Usage
  - User and State Information
- **Interactive Sorting:** Sort processes by PID or memory usage
- **System Integration:** Full process details from system information

### Cross-Platform Support
- **Linux:** Supports NVIDIA GPUs via `NVML` and `nvidia-smi`(fallback) command
- **macOS:** Supports Apple Silicon GPUs via `powermetrics` and Metal framework
- **NVIDIA Jetson:** Special support for Tegra-based systems with DLA (Deep Learning Accelerator)

### Remote Monitoring
- **Multi-Host Support:** Monitor up to 256+ remote systems simultaneously
- **Connection Management:** Optimized networking with connection pooling and retry logic
- **Storage Monitoring:** Disk usage information for remote hosts
- **High Availability:** Resilient to connection failures with automatic retry

### Interactive UI
- **Keyboard Controls:**
  - Navigation: Arrow keys, Page Up/Down for scrolling
  - Sorting: 'd' (default), 'u' (utilization), 'g' (GPU memory), 'p' (PID), 'm' (memory)
  - Interface: '1' or 'h' (help), 'q' (quit), Tab switching
- **Color-Coded Status:** Green (≤60%), Yellow (60-80%), Red (>80%) for resource usage
- **Responsive Design:** Adapts to terminal size with optimized space allocation
- **Help System:** Comprehensive built-in help with context-sensitive shortcuts

## Technology Stack

- **Language:** Rust 2021 Edition
- **Async Runtime:** Tokio for high-performance networking
- **Key Dependencies:**
  - `crossterm`: Terminal manipulation and UI
  - `axum`: Web framework for API mode
  - `reqwest`: HTTP client for remote monitoring
  - `chrono`: Date/time handling
  - `clap`: Command-line argument parsing
  - `serde`: Serialization for data exchange
  - `metal`/`objc`: Apple Silicon GPU integration on macOS
  - `sysinfo`: System information gathering

## Installation

### Option 1: Install from Cargo (Recommended)

The easiest way to install all-smi is through Cargo:

```bash
cargo install all-smi
```

After installation, the binary will be available in your `$PATH` as `all-smi`.

### Option 2: Download Pre-built Binary

Download the latest release from the [GitHub releases page](https://github.com/inureyes/all-smi/releases):

1. Go to https://github.com/inureyes/all-smi/releases
2. Download the appropriate binary for your platform
3. Extract the archive and place the binary in your `$PATH`

### Option 3: Build from Source

### Prerequisites

- **Rust:** Version 1.75 or later with Cargo
- **Linux (NVIDIA):** `CUDA`, `nvidia-smi` command must be available
- **macOS:** Requires `sudo` privileges for `powermetrics` access
- **Network:** For remote monitoring functionality

### Building from Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/inureyes/all-smi.git
   cd all-smi
   ```

2. **Build the project:**
   ```bash
   # Build the main application
   cargo build --release
   
   # Build mock server for testing
   cargo build --release --bin all-smi-mock-server --features mock
   ```

3. **Run tests:**
   ```bash
   cargo test
   cargo clippy
   cargo fmt --check
   ```

## Usage

### Command Overview

```bash
# Show help
all-smi --help

# Local monitoring (requires sudo on macOS)
sudo all-smi view

# Remote monitoring
all-smi view --hosts http://node1:9090 http://node2:9090
all-smi view --hostfile hosts.csv

# API mode
all-smi api --port 9090
```

### Quick Start with Make Commands

For development and testing, you can use the provided Makefile:

```bash
# Run local monitoring
make local

# Run remote monitoring with hosts file
make remote

# Start mock server for testing
make mock

# Build release version
make release

# Run tests
make test
```

### View Mode (Interactive Monitoring)

The `view` mode provides a terminal-based interface with real-time updates.

#### Local Mode
```bash
# Monitor local GPUs (requires sudo on macOS)
sudo all-smi view

# With custom refresh interval
sudo all-smi view --interval 5
```

#### Remote Monitoring

Monitor multiple remote systems running in API mode:

```bash
# Direct host specification
all-smi view --hosts http://gpu-node1:9090 http://gpu-node2:9090

# Using host file
all-smi view --hostfile hosts.csv --interval 2
```

Host file format (CSV):
```
http://gpu-node1:9090
http://gpu-node2:9090
http://gpu-node3:9090
```

#### Keyboard Controls

- **Navigation:** ←→ (switch tabs), ↑↓ (scroll), PgUp/PgDn (page navigation)
- **Sorting:** 'd' (default), 'u' (utilization), 'g' (GPU memory), 'p' (PID), 'm' (memory)
- **Interface:** '1'/'h' (help), 'q' (quit), ESC (close help)

### API Mode (Prometheus Metrics)

Expose GPU metrics in Prometheus format for integration with monitoring systems:

```bash
# Start API server
all-smi api --port 9090

# Custom bind address
all-smi api --port 8080 --bind 0.0.0.0
```

Metrics available at `http://localhost:9090/metrics` include:

**GPU Metrics:**
- `all_smi_gpu_utilization`
- `all_smi_gpu_memory_used_bytes`
- `all_smi_gpu_memory_total_bytes`
- `all_smi_gpu_temperature_celsius`
- `all_smi_gpu_power_consumption_watts`
- `all_smi_gpu_frequency_mhz`

**CPU Metrics:**
- `all_smi_cpu_utilization`
- `all_smi_cpu_socket_count`
- `all_smi_cpu_core_count`
- `all_smi_cpu_thread_count`
- `all_smi_cpu_frequency_mhz`
- `all_smi_cpu_temperature_celsius`
- `all_smi_cpu_power_consumption_watts`
- `all_smi_cpu_socket_utilization` (per-socket for multi-socket systems)

**Apple Silicon Specific:**
- `all_smi_cpu_p_core_count`
- `all_smi_cpu_e_core_count`
- `all_smi_cpu_gpu_core_count`
- `all_smi_cpu_p_core_utilization`
- `all_smi_cpu_e_core_utilization`

**Storage Metrics:**
- `all_smi_disk_total_bytes`
- `all_smi_disk_available_bytes`

## Development and Testing

### Mock Server for Testing

The included mock server simulates realistic GPU and CPU clusters for development and testing:

```bash
# Build mock server (requires mock feature)
cargo build --release --bin all-smi-mock-server --features mock

# Start single mock instance
./target/release/all-smi-mock-server --port-range 9090

# Start multiple instances
./target/release/all-smi-mock-server --port-range 10001-10010 -o hosts.csv

# Custom GPU configuration
./target/release/all-smi-mock-server --port-range 10001-10005 \
  --gpu-name "NVIDIA H100 80GB HBM3" -o test-hosts.csv
```

#### Platform-Specific Testing

Test different hardware platforms with realistic CPU and GPU metrics:

```bash
# NVIDIA GPU servers (default - Intel/AMD CPUs with NVIDIA GPUs)
./target/release/all-smi-mock-server --platform nvidia \
  --port-range 10001-10005 -o nvidia-hosts.csv

# Apple Silicon systems (M1/M2/M3 with P/E cores)
./target/release/all-smi-mock-server --platform apple \
  --gpu-name "Apple M2" --port-range 11001-11005 -o apple-hosts.csv

# Intel CPU servers
./target/release/all-smi-mock-server --platform intel \
  --gpu-name "NVIDIA RTX 4090" --port-range 12001-12005 -o intel-hosts.csv

# AMD CPU servers
./target/release/all-smi-mock-server --platform amd \
  --gpu-name "NVIDIA A100 80GB PCIe" --port-range 13001-13005 -o amd-hosts.csv

# NVIDIA Jetson platforms
./target/release/all-smi-mock-server --platform jetson \
  --gpu-name "NVIDIA Jetson AGX Orin" --port-range 14001-14005 -o jetson-hosts.csv
```

#### Platform-Specific Features

- **NVIDIA Platform**: Multi-socket Intel/AMD CPUs with NVIDIA GPUs
- **Apple Silicon**: P-core/E-core CPU monitoring with integrated GPU metrics
- **Intel Platform**: Intel Xeon processors with hyperthreading
- **AMD Platform**: AMD EPYC/Ryzen processors with SMT
- **Jetson Platform**: ARM-based Tegra processors with integrated GPUs

Mock server features:
- **8 GPUs per node** with realistic metrics
- **Platform-specific CPU metrics** (socket count, core types, utilization)
- **Randomized values** that change over time
- **Storage simulation** with various disk sizes (1TB/4TB/12TB)
- **Template-based responses** for performance
- **Instance naming** with node-XXXX format

### Testing High-Scale Scenarios

```bash
# Start 128 mock servers
./target/release/all-smi-mock-server --port-range 10001-10128 -o large-cluster.csv &

# Monitor large cluster
all-smi view --hostfile large-cluster.csv --interval 1

# Test mixed platform environments
./target/release/all-smi-mock-server --platform nvidia --port-range 10001-10064 -o nvidia.csv &
./target/release/all-smi-mock-server --platform apple --port-range 11001-11032 -o apple.csv &
./target/release/all-smi-mock-server --platform amd --port-range 12001-12032 -o amd.csv &

# Combine host files and monitor mixed environment
cat nvidia.csv apple.csv amd.csv > mixed-cluster.csv
all-smi view --hostfile mixed-cluster.csv --interval 2
```

## Architecture

### Core Components

- **GPU Abstraction Layer:** Platform-specific readers implementing the `GpuReader` trait
- **Async Networking:** Concurrent remote data collection with connection pooling
- **Terminal UI:** Double-buffered rendering with responsive layout
- **Data Processing:** Real-time metrics aggregation and historical tracking

### Platform Support

- **NVIDIA GPUs:** Via `NVML` direct query (default) and `nvidia-smi` (fallback) command parsing
- **Apple Silicon:** Via `powermetrics` and Metal framework integration
- **NVIDIA Jetson:** Specialized Tegra platform support with DLA monitoring

### Performance Optimizations

- **Connection Management:** 64 concurrent connections with retry logic
- **Adaptive Intervals:** 2-6 second refresh based on cluster size
- **Memory Efficiency:** Stream processing and connection pooling
- **Rendering:** Double buffering to prevent display flickering

## Contributing

Contributions are welcome! Areas for contribution include:

- **Platform Support:** Additional GPU vendors or operating systems
- **Features:** New metrics, visualization improvements, or monitoring capabilities
- **Performance:** Optimization for larger clusters or resource usage
- **Documentation:** Examples, tutorials, or API documentation

Please submit pull requests or open issues for bugs, feature requests, or questions.

## License

This project is licensed under the MIT License. See the LICENSE file for details.

## Changelog

### Recent Updates
- **v0.4.0 (2025/07/08):** Architectural refactoring, Smart sudo detection and comprehensive unit testing
- **v0.3.3 (2025/07/07):** CPU, Memory, and ANE support, and UI fixes
- **v0.3.2 (2025/07/06):** Cargo.toml for publishing and release process
- **v0.3.1 (2025/07/06):** GitHub actions and Dockerfile, and UI fixes
- **v0.3.0 (2025/07/06):** Multi-architecture support, optimized space allocation, enhanced UI
- **v0.2.2 (2025/07/06):** GPU sorting functionality with hotkeys
- **v0.2.1 (2025/07/05):** Help system improvements and code refactoring
- **v0.2.0 (2025/07/05):** Remote monitoring and cluster management features
- **v0.1.1 (2025/07/04):** ANE (Apple Neural Engine) support, page navigation keys, and scrolling fixes
- **v0.1.0 (2024/08/11):** Initial release with local GPU monitoring
