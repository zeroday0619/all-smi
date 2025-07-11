# all-smi

[![Crates.io version](https://img.shields.io/crates/v/all-smi.svg?style=flat-square)](https://crates.io/crates/all-smi)
[![Crates.io downloads](https://img.shields.io/crates/d/all-smi.svg?style=flat-square)](https://crates.io/crates/all-smi)
![CI](https://github.com/inureyes/all-smi/workflows/CI/badge.svg)
[![dependency status](https://deps.rs/repo/github/inureyes/all-smi/status.svg)](https://deps.rs/repo/github/inureyes/all-smi)


`all-smi` is a command-line utility for monitoring GPU hardware across multiple systems. It provides a real-time view of GPU utilization, memory usage, temperature, power consumption, and other metrics. The tool is designed to be a cross-platform alternative to `nvidia-smi`, with support for NVIDIA GPUs, Apple Silicon GPUs, and NVIDIA Jetson platforms.

The application presents a terminal-based user interface with cluster overview, interactive sorting, and both local and remote monitoring capabilities. It also provides an API mode for Prometheus metrics integration.

![screenshot](screenshots/all-smi-all-tab.png)

<p align="center">All-node view (remote mode)</p>

![screenshot](screenshots/all-smi-node-tab.png)

<p align="center">Node view (remote mode)</p>

## Installation

### Option 1: Install via Homebrew (macOS/Linux)

The easiest way to install all-smi on macOS and Linux is through Homebrew:

```bash
brew tap lablup/tap
brew install all-smi
```

### Option 2: Install from Cargo

Install all-smi through Cargo:

```bash
cargo install all-smi
```

After installation, the binary will be available in your `$PATH` as `all-smi`.

### Option 3: Download Pre-built Binary

Download the latest release from the [GitHub releases page](https://github.com/inureyes/all-smi/releases):

1. Go to https://github.com/inureyes/all-smi/releases
2. Download the appropriate binary for your platform
3. Extract the archive and place the binary in your `$PATH`

### Option 4: Build from Source

See [Building from Source](DEVELOPERS.md#building-from-source) in the developer documentation.

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
  - Supports up to 128 concurrent connections
  - Automatic retry with exponential backoff
  - TCP keepalive for persistent connections
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

## Development

For development documentation including building from source, testing with mock servers, architecture details, and technology stack information, see [DEVELOPERS.md](DEVELOPERS.md).

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
- **v0.4.3 (2025/07/14):** Fix P-CPU/E-CPU gauges for all Apple Silicon variants including M1 Pro hybrid format
- **v0.4.2 (2025/07/12):** Eliminate PowerMetrics temp file growth with in-memory buffer, Homebrew installation support
- **v0.4.1 (2025/07/10):** Mock server improvements, efficient Apple Silicon and NVidia GPU support
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