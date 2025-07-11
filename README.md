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

## Features

### GPU Monitoring
- **Real-time Metrics:** Displays comprehensive GPU information including:
  - GPU Name and Driver Version
  - Utilization Percentage with color-coded status
  - Memory Usage (Used/Total in GB)
  - Temperature in Celsius (or Thermal Pressure for Apple Silicon)
  - Clock Frequency in MHz
  - Power Consumption in Watts (2 decimal precision for Apple Silicon)
- **Multi-GPU Support:** Handles multiple GPUs per system with individual monitoring
- **Interactive Sorting:** Sort GPUs by utilization, memory usage, or default (hostname+index) order
- **Platform-Specific Features:**
  - NVIDIA: PCIe info, performance states, power limits
  - Apple Silicon: ANE power monitoring, thermal pressure levels
  - Jetson: DLA utilization monitoring

### CPU Monitoring
- **Comprehensive CPU Metrics:**
  - Real-time CPU utilization with per-socket breakdown
  - Core and thread counts
  - Frequency monitoring (P+E format for Apple Silicon)
  - Temperature and power consumption
- **Apple Silicon Enhanced:**
  - P-core and E-core utilization tracking
  - P-cluster and E-cluster frequency monitoring
  - Integrated GPU core count

### Memory Monitoring
- **System Memory Tracking:**
  - Total, used, available, and free memory
  - Memory utilization percentage
  - Swap space monitoring
  - Linux: Buffer and cache memory tracking
- **Visual Indicators:** Color-coded memory usage bars

### Process Monitoring
- **Enhanced GPU Process View:**
  - Process ID (PID) and Parent PID
  - Process Name and Command Line
  - GPU Memory Usage with per-column coloring
  - CPU usage percentage
  - User and State Information
- **Advanced Features:**
  - Mouse click sorting on column headers
  - Multi-criteria sorting (PID, memory, GPU memory, CPU usage)
  - Per-column color coding for better visibility
  - Full process tree integration

### Cluster Management
- **Cluster Overview Dashboard:** Real-time statistics showing:
  - Total nodes and GPUs across the cluster
  - Average utilization and memory usage
  - Temperature statistics with standard deviation
  - Total and average power consumption
- **Live Statistics History:** Visual graphs showing utilization, memory, and temperature trends
- **Tabbed Interface:** Switch between "All" view and individual host tabs
- **Adaptive Update Intervals:**
  - Local monitoring: 1 second (Apple Silicon) or 2 seconds (others)
  - 1-10 remote nodes: 3 seconds
  - 11-50 nodes: 4 seconds
  - 51-100 nodes: 5 seconds
  - 101+ nodes: 6 seconds

### Cross-Platform Support
- **Linux:** 
  - NVIDIA GPUs via NVML and nvidia-smi (fallback)
  - CPU monitoring via /proc filesystem
  - Memory monitoring with detailed statistics
- **macOS:** 
  - Apple Silicon GPUs via powermetrics and Metal framework
  - ANE (Apple Neural Engine) power tracking
  - Thermal pressure monitoring
  - P/E core architecture support
- **NVIDIA Jetson:** 
  - Special support for Tegra-based systems
  - DLA (Deep Learning Accelerator) monitoring

### Remote Monitoring
- **Multi-Host Support:** Monitor up to 256+ remote systems simultaneously
- **Connection Management:** Optimized networking with:
  - Connection pooling (200 idle connections per host)
  - Concurrent connection limiting (64 max)
  - Automatic retry with exponential backoff
  - TCP keepalive for persistent connections
  - Connection staggering to prevent overload
- **Storage Monitoring:** Disk usage information for all hosts
- **High Availability:** Resilient to connection failures with automatic recovery

### Interactive UI
- **Enhanced Controls:**
  - Keyboard: Arrow keys, Page Up/Down, Tab switching
  - Mouse: Click column headers to sort (process view)
  - Sorting: 'd' (default), 'u' (utilization), 'g' (GPU memory), 'p' (PID), 'm' (memory), 'c' (CPU)
  - Interface: '1'/'h' (help), 'q' (quit), ESC (close help)
- **Visual Design:**
  - Color-coded status: Green (≤60%), Yellow (60-80%), Red (>80%)
  - Per-column coloring in process view
  - Responsive layout adapting to terminal size
  - Double-buffered rendering for flicker-free display
- **Help System:** Context-sensitive help with all keyboard shortcuts

### Development & Testing
- **Mock Server:** Built-in mock server for testing and development
  - Simulates realistic GPU clusters with 8 GPUs per node
  - Configurable port ranges for multiple instances
  - Failure simulation for resilience testing
  - Platform-specific metric generation (NVIDIA, Apple Silicon, Jetson)
  - Background metric updates with realistic variations
- **Performance Optimized:**
  - Template-based response generation
  - Efficient memory management
  - Minimal CPU overhead

### API Mode (Prometheus Metrics)

Expose GPU metrics in Prometheus format for integration with monitoring systems:

```bash
# Start API server
all-smi api --port 9090

# Custom update interval (default: 3 seconds)
all-smi api --port 9090 --interval 5

# Include process information
all-smi api --port 9090 --processes
```

Metrics available at `http://localhost:9090/metrics` include:

**GPU Metrics:**
- `all_smi_gpu_utilization` - GPU utilization percentage
- `all_smi_gpu_memory_used_bytes` - GPU memory used in bytes
- `all_smi_gpu_memory_total_bytes` - GPU memory total in bytes
- `all_smi_gpu_temperature_celsius` - GPU temperature in celsius
- `all_smi_gpu_power_consumption_watts` - GPU power consumption in watts
- `all_smi_gpu_frequency_mhz` - GPU frequency in MHz
- `all_smi_gpu_info` - GPU device information (info metric with labels)

**NVIDIA Specific:**
- `all_smi_gpu_pcie_gen_current` - Current PCIe generation
- `all_smi_gpu_pcie_width_current` - Current PCIe link width
- `all_smi_gpu_performance_state` - GPU performance state (P0=0, P1=1, etc.)
- `all_smi_gpu_clock_graphics_max_mhz` - Maximum graphics clock in MHz
- `all_smi_gpu_clock_memory_max_mhz` - Maximum memory clock in MHz
- `all_smi_gpu_power_limit_current_watts` - Current power limit in watts
- `all_smi_gpu_power_limit_max_watts` - Maximum power limit in watts

**Jetson Specific:**
- `all_smi_dla_utilization` - DLA utilization percentage

**Apple Silicon GPU Specific:**
- `all_smi_ane_utilization` - ANE utilization in mW
- `all_smi_ane_power_watts` - ANE power consumption in watts
- `all_smi_thermal_pressure_info` - Thermal pressure level (info metric with level label)

**CPU Metrics:**
- `all_smi_cpu_utilization` - CPU utilization percentage
- `all_smi_cpu_socket_count` - Number of CPU sockets
- `all_smi_cpu_core_count` - Total number of CPU cores
- `all_smi_cpu_thread_count` - Total number of CPU threads
- `all_smi_cpu_frequency_mhz` - CPU frequency in MHz
- `all_smi_cpu_temperature_celsius` - CPU temperature in celsius
- `all_smi_cpu_power_consumption_watts` - CPU power consumption in watts
- `all_smi_cpu_socket_utilization` - Per-socket CPU utilization (multi-socket systems)

**Apple Silicon CPU Specific:**
- `all_smi_cpu_p_core_count` - Number of performance cores
- `all_smi_cpu_e_core_count` - Number of efficiency cores
- `all_smi_cpu_gpu_core_count` - Number of integrated GPU cores
- `all_smi_cpu_p_core_utilization` - P-core utilization percentage
- `all_smi_cpu_e_core_utilization` - E-core utilization percentage
- `all_smi_cpu_p_cluster_frequency_mhz` - P-cluster frequency in MHz
- `all_smi_cpu_e_cluster_frequency_mhz` - E-cluster frequency in MHz

**Memory Metrics:**
- `all_smi_memory_total_bytes` - Total system memory in bytes
- `all_smi_memory_used_bytes` - Used system memory in bytes
- `all_smi_memory_available_bytes` - Available system memory in bytes
- `all_smi_memory_free_bytes` - Free system memory in bytes
- `all_smi_memory_utilization` - Memory utilization percentage
- `all_smi_swap_total_bytes` - Total swap space in bytes
- `all_smi_swap_used_bytes` - Used swap space in bytes
- `all_smi_swap_free_bytes` - Free swap space in bytes
- `all_smi_memory_buffers_bytes` - Memory used for buffers (Linux)
- `all_smi_memory_cached_bytes` - Memory used for cache (Linux)

**Storage Metrics:**
- `all_smi_disk_total_bytes` - Total disk space in bytes
- `all_smi_disk_available_bytes` - Available disk space in bytes

**Process Metrics (when --processes flag is used):**
- `all_smi_gpu_process_memory_bytes` - GPU memory used by process
- `all_smi_gpu_process_sm_util` - Process GPU SM utilization
- `all_smi_gpu_process_mem_util` - Process GPU memory utilization
- `all_smi_gpu_process_enc_util` - Process GPU encoder utilization
- `all_smi_gpu_process_dec_util` - Process GPU decoder utilization

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
- **v0.5.0 (2025/07/16):** Enhanced Apple Silicon support with ANE power in watts, P+E frequency display, thermal pressure text, interactive process sorting, and configurable PowerMetrics intervals
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