use crate::device::{get_system_process_info, GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::Nvml;
use std::collections::HashMap;
use std::sync::OnceLock;

pub struct NvidiaGpuReader;

// Singleton Nvml instance - initialized once and reused
static NVML_INSTANCE: OnceLock<Result<Nvml, NvmlError>> = OnceLock::new();

// Initialize NVML instance only once
fn get_nvml_instance() -> Result<&'static Nvml, &'static NvmlError> {
    let nvml_result = NVML_INSTANCE.get_or_init(|| Nvml::init());
    nvml_result.as_ref()
}

// Get driver version using NVML
fn get_driver_version() -> Result<String, &'static str> {
    let nvml = get_nvml_instance().map_err(|_| "Failed to initialize NVML")?;
    nvml.sys_driver_version()
        .map_err(|_| "Failed to get driver version")
}

// Convert NVML utilization to percentage (same as nvidia-smi)
fn get_gpu_utilization(device: &nvml_wrapper::Device) -> f64 {
    match device.utilization_rates() {
        Ok(utilization) => utilization.gpu as f64,
        Err(_) => 0.0,
    }
}

impl GpuReader for NvidiaGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // Initialize NVML and get device count
        let nvml = match get_nvml_instance() {
            Ok(nvml) => nvml,
            Err(e) => {
                eprintln!("Failed to initialize NVML: {}", e);
                return gpu_info;
            }
        };

        let device_count = match nvml.device_count() {
            Ok(count) => count,
            Err(e) => {
                eprintln!("Failed to get device count: {}", e);
                return gpu_info;
            }
        };

        // Get driver version once (shared across all devices)
        let driver_version = get_driver_version().unwrap_or_else(|_| "unknown".to_string());

        // Iterate through all devices
        for device_index in 0..device_count {
            let device = match nvml.device_by_index(device_index) {
                Ok(device) => device,
                Err(e) => {
                    eprintln!("Failed to get device {}: {}", device_index, e);
                    continue;
                }
            };

            // Collect GPU information using NVML API
            let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let hostname = get_hostname();

            // UUID
            let uuid = device
                .uuid()
                .unwrap_or_else(|_| format!("unknown_{}", device_index));

            // Name
            let name = device.name().unwrap_or_else(|_| "Unknown GPU".to_string());

            // GPU utilization (same calculation as nvidia-smi)
            let utilization = get_gpu_utilization(&device);

            // Temperature
            let temperature = device.temperature(TemperatureSensor::Gpu).unwrap_or(0);

            // Memory information
            let (used_memory, total_memory) = match device.memory_info() {
                Ok(mem_info) => (mem_info.used, mem_info.total),
                Err(_) => (0, 0),
            };

            // Graphics clock frequency
            let frequency = device.clock_info(Clock::Graphics).unwrap_or(0);

            // Power consumption
            let power_consumption = device
                .power_usage()
                .map(|p| p as f64 / 1000.0) // Convert milliwatts to watts
                .unwrap_or(0.0);

            // Create detail map with driver version
            let mut detail = HashMap::new();
            detail.insert("driver_version".to_string(), driver_version.clone());

            gpu_info.push(GpuInfo {
                uuid,
                time,
                name,
                hostname: hostname.clone(),
                instance: hostname.clone(),
                utilization,
                ane_utilization: 0.0,
                dla_utilization: None,
                temperature,
                used_memory,
                total_memory,
                frequency,
                power_consumption,
                detail,
            });
        }

        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        let mut process_list = Vec::new();

        // Initialize NVML and get device count
        let nvml = match get_nvml_instance() {
            Ok(nvml) => nvml,
            Err(e) => {
                eprintln!("Failed to initialize NVML: {}", e);
                return process_list;
            }
        };

        let device_count = match nvml.device_count() {
            Ok(count) => count,
            Err(e) => {
                eprintln!("Failed to get device count: {}", e);
                return process_list;
            }
        };

        // Iterate through all devices
        for device_index in 0..device_count {
            let device = match nvml.device_by_index(device_index) {
                Ok(device) => device,
                Err(e) => {
                    eprintln!("Failed to get device {}: {}", device_index, e);
                    continue;
                }
            };

            // Get device UUID
            let device_uuid = device
                .uuid()
                .unwrap_or_else(|_| format!("unknown_{}", device_index));

            // Get running compute processes
            let processes = match device.running_compute_processes() {
                Ok(processes) => processes,
                Err(e) => {
                    eprintln!(
                        "Failed to get running processes for device {}: {}",
                        device_index, e
                    );
                    continue;
                }
            };

            // Process each running process
            for process in processes {
                let pid = process.pid;
                let used_memory = match process.used_gpu_memory {
                    UsedGpuMemory::Used(bytes) => bytes,
                    _ => 0,
                };
                let process_name = format!("pid_{}", pid); // NVML doesn't provide process name directly

                // Get additional system process information
                let (
                    cpu_percent,
                    memory_percent,
                    memory_rss,
                    memory_vms,
                    user,
                    state,
                    start_time,
                    cpu_time,
                    command,
                    ppid,
                    threads,
                ) = get_system_process_info(pid).unwrap_or((
                    0.0,                   // cpu_percent
                    0.0,                   // memory_percent
                    0,                     // memory_rss
                    0,                     // memory_vms
                    "unknown".to_string(), // user
                    "?".to_string(),       // state
                    "unknown".to_string(), // start_time
                    0,                     // cpu_time
                    process_name.clone(),  // command (fallback to process_name)
                    0,                     // ppid
                    1,                     // threads
                ));

                process_list.push(ProcessInfo {
                    device_id: device_index as usize,
                    device_uuid: device_uuid.clone(),
                    pid,
                    process_name: process_name.clone(),
                    used_memory,
                    cpu_percent,
                    memory_percent,
                    memory_rss,
                    memory_vms,
                    user,
                    state,
                    start_time,
                    cpu_time,
                    command,
                    ppid,
                    threads,
                });
            }
        }

        process_list
    }
}
