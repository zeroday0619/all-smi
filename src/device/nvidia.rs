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

    // Set fallback warned flag but don't print here - will be handled by UI
    if !available {
        NVML_FALLBACK_WARNED.store(true, Ordering::Relaxed);
    }

    available
}

// Public function to get NVML status message for UI display
pub fn get_nvml_status_message() -> Option<String> {
    if NVML_AVAILABILITY_CHECKED.load(Ordering::Relaxed)
        && !NVML_IS_AVAILABLE.load(Ordering::Relaxed)
    {
        Some("NVML unavailable - using nvidia-smi fallback".to_string())
    } else {
        None
    }
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

            // Create detail map with driver version and CUDA info
            let mut detail = HashMap::new();
            detail.insert("driver_version".to_string(), driver_version.clone());

            // Get CUDA version
            if let Ok(cuda_version) = nvml.sys_cuda_driver_version() {
                let major = cuda_version / 1000;
                let minor = (cuda_version % 1000) / 10;
                detail.insert("cuda_version".to_string(), format!("{major}.{minor}"));
            }

            // Get additional device information
            if let Ok(arch) = device.architecture() {
                detail.insert("architecture".to_string(), format!("{arch:?}"));
            }

            if let Ok(brand) = device.brand() {
                detail.insert("brand".to_string(), format!("{brand:?}"));
            }

            if let Ok(compute_mode) = device.compute_mode() {
                detail.insert("compute_mode".to_string(), format!("{compute_mode:?}"));
            }

            // Note: persistence_mode is not directly available in nvml-wrapper
            // We'll get it from nvidia-smi fallback if needed

            // PCIe information
            if let Ok(pcie_gen) = device.current_pcie_link_gen() {
                detail.insert("pcie_gen_current".to_string(), pcie_gen.to_string());
            }
            if let Ok(pcie_gen_max) = device.max_pcie_link_gen() {
                detail.insert("pcie_gen_max".to_string(), pcie_gen_max.to_string());
            }
            if let Ok(pcie_width) = device.current_pcie_link_width() {
                detail.insert("pcie_width_current".to_string(), pcie_width.to_string());
            }
            if let Ok(pcie_width_max) = device.max_pcie_link_width() {
                detail.insert("pcie_width_max".to_string(), pcie_width_max.to_string());
            }

            // Performance state
            if let Ok(perf_state) = device.performance_state() {
                detail.insert("performance_state".to_string(), format!("{perf_state:?}"));
            }

            // Power limits
            if let Ok(power_limit) = device.power_management_limit() {
                detail.insert(
                    "power_limit_current".to_string(),
                    format!("{:.2}", power_limit as f64 / 1000.0),
                );
            }
            if let Ok(power_limit_default) = device.power_management_limit_default() {
                detail.insert(
                    "power_limit_default".to_string(),
                    format!("{:.2}", power_limit_default as f64 / 1000.0),
                );
            }
            if let Ok(constraints) = device.power_management_limit_constraints() {
                detail.insert(
                    "power_limit_min".to_string(),
                    format!("{:.2}", constraints.min_limit as f64 / 1000.0),
                );
                detail.insert(
                    "power_limit_max".to_string(),
                    format!("{:.2}", constraints.max_limit as f64 / 1000.0),
                );
            }

            // Max clocks
            if let Ok(max_graphics_clock) = device.max_customer_boost_clock(Clock::Graphics) {
                detail.insert(
                    "clock_graphics_max".to_string(),
                    max_graphics_clock.to_string(),
                );
            }
            if let Ok(max_memory_clock) = device.max_customer_boost_clock(Clock::Memory) {
                detail.insert("clock_memory_max".to_string(), max_memory_clock.to_string());
            }

            // ECC mode
            if let Ok(ecc_enabled) = device.is_ecc_enabled() {
                detail.insert(
                    "ecc_mode_current".to_string(),
                    if ecc_enabled.currently_enabled {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                    .to_string(),
                );
                if ecc_enabled.currently_enabled != ecc_enabled.pending_enabled {
                    detail.insert(
                        "ecc_mode_pending".to_string(),
                        if ecc_enabled.pending_enabled {
                            "Enabled"
                        } else {
                            "Disabled"
                        }
                        .to_string(),
                    );
                }
            }

            // MIG mode
            if let Ok(mig_mode) = device.mig_mode() {
                detail.insert(
                    "mig_mode_current".to_string(),
                    format!("{:?}", mig_mode.current),
                );
                if mig_mode.current != mig_mode.pending {
                    detail.insert(
                        "mig_mode_pending".to_string(),
                        format!("{:?}", mig_mode.pending),
                    );
                }
            }

            // VBIOS version
            if let Ok(vbios) = device.vbios_version() {
                detail.insert("vbios_version".to_string(), vbios);
            }

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

        // First, get CUDA version using nvidia-smi without query (appears in header)
        let mut cuda_version = String::new();
        if let Ok(output) = Command::new("nvidia-smi").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Parse CUDA version from header (e.g., "CUDA Version: 12.6")
                for line in output_str.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(version) = line.split("CUDA Version:").nth(1) {
                            cuda_version = version.trim().to_string();
                        }
                        break;
                    }
                }
            }
        }

        // Execute the nvidia-smi command to get GPU information, including driver version
        let output = Command::new("nvidia-smi")
            .arg("--format=csv,noheader,nounits")
            .arg("--query-gpu=uuid,driver_version,name,utilization.gpu,temperature.gpu,memory.used,memory.total,clocks.current.graphics,power.draw,compute_mode,persistence_mode,pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,clocks.max.graphics,clocks.max.memory,power.limit,power.default_limit,power.min_limit,power.max_limit,pstate,vbios_version")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines = output_str.lines();

                for line in lines {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() >= 9 {
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

                        // Add CUDA version if we found it
                        if !cuda_version.is_empty() {
                            detail.insert("cuda_version".to_string(), cuda_version.clone());
                        }

                        // Parse additional fields if available
                        if parts.len() >= 23 {
                            // Compute mode
                            if parts.len() > 9 && !parts[9].trim().is_empty() {
                                detail.insert(
                                    "compute_mode".to_string(),
                                    parts[9].trim().to_string(),
                                );
                            }

                            // Persistence mode
                            if parts.len() > 10 && !parts[10].trim().is_empty() {
                                detail.insert(
                                    "persistence_mode".to_string(),
                                    parts[10].trim().to_string(),
                                );
                            }

                            // PCIe information
                            if parts.len() > 11 && !parts[11].trim().is_empty() {
                                detail.insert(
                                    "pcie_gen_current".to_string(),
                                    parts[11].trim().to_string(),
                                );
                            }
                            if parts.len() > 12 && !parts[12].trim().is_empty() {
                                detail.insert(
                                    "pcie_gen_max".to_string(),
                                    parts[12].trim().to_string(),
                                );
                            }
                            if parts.len() > 13 && !parts[13].trim().is_empty() {
                                detail.insert(
                                    "pcie_width_current".to_string(),
                                    parts[13].trim().to_string(),
                                );
                            }
                            if parts.len() > 14 && !parts[14].trim().is_empty() {
                                detail.insert(
                                    "pcie_width_max".to_string(),
                                    parts[14].trim().to_string(),
                                );
                            }

                            // Max clocks
                            if parts.len() > 15 && !parts[15].trim().is_empty() {
                                detail.insert(
                                    "clock_graphics_max".to_string(),
                                    parts[15].trim().to_string(),
                                );
                            }
                            if parts.len() > 16 && !parts[16].trim().is_empty() {
                                detail.insert(
                                    "clock_memory_max".to_string(),
                                    parts[16].trim().to_string(),
                                );
                            }

                            // Power limits
                            if parts.len() > 17 && !parts[17].trim().is_empty() {
                                detail.insert(
                                    "power_limit_current".to_string(),
                                    parts[17].trim().to_string(),
                                );
                            }
                            if parts.len() > 18 && !parts[18].trim().is_empty() {
                                detail.insert(
                                    "power_limit_default".to_string(),
                                    parts[18].trim().to_string(),
                                );
                            }
                            if parts.len() > 19 && !parts[19].trim().is_empty() {
                                detail.insert(
                                    "power_limit_min".to_string(),
                                    parts[19].trim().to_string(),
                                );
                            }
                            if parts.len() > 20 && !parts[20].trim().is_empty() {
                                detail.insert(
                                    "power_limit_max".to_string(),
                                    parts[20].trim().to_string(),
                                );
                            }

                            // Performance state
                            if parts.len() > 21 && !parts[21].trim().is_empty() {
                                detail.insert(
                                    "performance_state".to_string(),
                                    parts[21].trim().to_string(),
                                );
                            }

                            // VBIOS version
                            if parts.len() > 22 && !parts[22].trim().is_empty() {
                                detail.insert(
                                    "vbios_version".to_string(),
                                    parts[22].trim().to_string(),
                                );
                            }
                        }

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
