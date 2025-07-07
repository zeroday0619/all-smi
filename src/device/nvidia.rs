use crate::device::{get_system_process_info, GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::Nvml;
use std::collections::HashMap;
use std::process::Command;
use std::str::FromStr;
use std::sync::{atomic::AtomicBool, atomic::Ordering, OnceLock};

pub struct NvidiaGpuReader;

// Singleton Nvml instance - initialized once and reused
static NVML_INSTANCE: OnceLock<Result<Nvml, NvmlError>> = OnceLock::new();

// Flag to track if NVML is available and if we've warned
static NVML_AVAILABILITY_CHECKED: AtomicBool = AtomicBool::new(false);
static NVML_IS_AVAILABLE: AtomicBool = AtomicBool::new(false);
static NVML_FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);

// Check NVML availability once and cache the result
fn is_nvml_available() -> bool {
    if NVML_AVAILABILITY_CHECKED.load(Ordering::Relaxed) {
        return NVML_IS_AVAILABLE.load(Ordering::Relaxed);
    }

    // Check if NVML can be initialized
    let available = get_nvml_instance().is_ok();

    NVML_IS_AVAILABLE.store(available, Ordering::Relaxed);
    NVML_AVAILABILITY_CHECKED.store(true, Ordering::Relaxed);

    // Warn only once if NVML is not available
    if !available && !NVML_FALLBACK_WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("NVML library not available, using nvidia-smi fallback");
    }

    available
}

// Initialize NVML instance only once
fn get_nvml_instance() -> Result<&'static Nvml, &'static NvmlError> {
    let nvml_result = NVML_INSTANCE.get_or_init(Nvml::init);
    nvml_result.as_ref()
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
        // Check NVML availability once and use appropriate method
        if is_nvml_available() {
            match self.get_gpu_info_nvml() {
                Ok(gpu_info) if !gpu_info.is_empty() => gpu_info,
                Ok(_) | Err(_) => self.get_gpu_info_nvidia_smi(),
            }
        } else {
            self.get_gpu_info_nvidia_smi()
        }
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        // Check NVML availability once and use appropriate method
        if is_nvml_available() {
            match self.get_process_info_nvml() {
                Ok(process_info) => process_info,
                Err(_) => self.get_process_info_nvidia_smi(),
            }
        } else {
            self.get_process_info_nvidia_smi()
        }
    }
}

impl NvidiaGpuReader {
    fn get_gpu_info_nvml(&self) -> Result<Vec<GpuInfo>, &'static str> {
        let mut gpu_info = Vec::new();

        // Initialize NVML and get device count
        let nvml = get_nvml_instance().map_err(|_| "Failed to initialize NVML")?;
        let device_count = nvml
            .device_count()
            .map_err(|_| "Failed to get device count")?;

        // Get driver version once (shared across all devices)
        let driver_version = nvml
            .sys_driver_version()
            .unwrap_or_else(|_| "unknown".to_string());

        // Iterate through all devices
        for device_index in 0..device_count {
            let device = nvml
                .device_by_index(device_index)
                .map_err(|_| "Failed to get device")?;

            // Collect GPU information using NVML API
            let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            let hostname = get_hostname();

            // UUID
            let uuid = device
                .uuid()
                .unwrap_or_else(|_| format!("unknown_{device_index}"));

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

        Ok(gpu_info)
    }

    fn get_process_info_nvml(&self) -> Result<Vec<ProcessInfo>, &'static str> {
        let mut process_list = Vec::new();

        // Initialize NVML and get device count
        let nvml = get_nvml_instance().map_err(|_| "Failed to initialize NVML")?;
        let device_count = nvml
            .device_count()
            .map_err(|_| "Failed to get device count")?;

        // Iterate through all devices
        for device_index in 0..device_count {
            let device = nvml
                .device_by_index(device_index)
                .map_err(|_| "Failed to get device")?;

            // Get device UUID
            let device_uuid = device
                .uuid()
                .unwrap_or_else(|_| format!("unknown_{device_index}"));

            // Get running compute processes
            let processes = device.running_compute_processes().unwrap_or_default();

            // Process each running process
            for process in processes {
                let pid = process.pid;
                let used_memory = match process.used_gpu_memory {
                    UsedGpuMemory::Used(bytes) => bytes,
                    _ => 0,
                };
                let process_name = format!("pid_{pid}"); // NVML doesn't provide process name directly

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

        Ok(process_list)
    }

    // Fallback implementation using nvidia-smi
    fn get_gpu_info_nvidia_smi(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // Execute the nvidia-smi command to get GPU information, including driver version
        let output = Command::new("nvidia-smi")
            .arg("--format=csv,noheader,nounits")
            .arg("--query-gpu=uuid,driver_version,name,utilization.gpu,temperature.gpu,memory.used,memory.total,clocks.current.graphics,power.draw")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines = output_str.lines();

                for line in lines {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() == 9 {
                        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let uuid = parts[0].trim().to_string();
                        let driver_version = parts[1].trim().to_string();
                        let name = parts[2].trim().to_string();
                        let utilization = f64::from_str(parts[3].trim()).unwrap_or(0.0);
                        let temperature = u32::from_str(parts[4].trim()).unwrap_or(0);
                        let used_memory = u64::from_str(parts[5].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let total_memory =
                            u64::from_str(parts[6].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let frequency = u32::from_str(parts[7].trim()).unwrap_or(0); // Frequency in MHz
                        let power_consumption = f64::from_str(parts[8].trim()).unwrap_or(0.0); // Power consumption in W

                        let mut detail = HashMap::new();
                        detail.insert("driver_version".to_string(), driver_version);

                        gpu_info.push(GpuInfo {
                            uuid,
                            time,
                            name,
                            hostname: get_hostname(),
                            instance: get_hostname(),
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
                }
            } else {
                eprintln!("nvidia-smi command failed with status: {}", output.status);
            }
        } else {
            eprintln!("Failed to execute nvidia-smi command");
        }

        gpu_info
    }

    fn get_process_info_nvidia_smi(&self) -> Vec<ProcessInfo> {
        let mut process_list = Vec::new();

        // Execute the nvidia-smi command to get the process information
        let output = Command::new("nvidia-smi")
            .arg("--format=csv,noheader,nounits")
            .arg("--query-compute-apps=gpu_uuid,pid,process_name,used_gpu_memory")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines = output_str.lines();

                for line in lines {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() == 4 {
                        let device_uuid = parts[0].trim().to_string();
                        let pid = u32::from_str(parts[1].trim()).unwrap_or(0);
                        let process_name = parts[2].trim().to_string();
                        let used_memory = u64::from_str(parts[3].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes

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
                            device_id: 0, // Actual GPU index would need additional logic
                            device_uuid,
                            pid,
                            process_name,
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
            } else {
                eprintln!(
                    "nvidia-smi process query failed with status: {}",
                    output.status
                );
            }
        } else {
            eprintln!("Failed to execute nvidia-smi process query");
        }

        process_list
    }
}
