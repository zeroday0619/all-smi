# all-smi

## Description

`all-smi` is a command-line utility for monitoring GPU hardware. It provides a real-time view of GPU utilization, memory usage, temperature, power consumption, and other metrics. The tool is designed to be a cross-platform alternative to `nvidia-smi`, with support for both NVIDIA and Apple Silicon GPUs.

The application presents a terminal-based user interface that displays GPU information in a clear and organized manner. It also lists processes currently utilizing the GPU, along with their memory consumption.

## Technology Stack

- **Language:** Rust
- **Key Crates:**
  - `tokio`: For asynchronous runtime.
  - `crossterm`: For terminal manipulation and UI.
  - `chrono`: For timestamping.
  - `metal` & `objc`: For Apple Silicon GPU interaction on macOS.

## Functionality

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

## How to Run

1.  **Prerequisites:**
    - Rust and Cargo must be installed.
    - On Linux with an NVIDIA GPU, the `nvidia-smi` command must be available.
    - On macOS, the tool requires `sudo` privileges to run `powermetrics`.
2.  **Build the project:**
    ```bash
    cargo build --release
    ```
3.  **Run the application:**
    ```bash
    sudo ./target/release/all-smi
    ```

## Code Structure

The project is organized into several modules:

- `main.rs`: The entry point of the application. It handles the main loop, UI rendering, and user input.
- `gpu/mod.rs`: Defines the `GpuReader` trait, which provides a common interface for different GPU types. It also includes logic to detect the operating system and select the appropriate `GpuReader` implementation.
- `gpu/nvidia.rs`: Implements the `GpuReader` trait for NVIDIA GPUs. It works by parsing the output of the `nvidia-smi` command.
- `gpu/apple_silicon.rs`: Implements the `GpuReader` trait for Apple Silicon GPUs. It gathers information by executing various system commands available on macOS (e.g., `powermetrics`, `system_profiler`, `vm_stat`).
