use crate::device::powermetrics_manager::get_powermetrics_manager;
use crate::device::{get_system_process_info, GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub struct AppleSiliconGpuReader {
    name: String,
    driver_version: Option<String>,
}

impl AppleSiliconGpuReader {
    pub fn new() -> Self {
        let (name, driver_version) = get_gpu_name_and_version();
        AppleSiliconGpuReader {
            name,
            driver_version,
        }
    }

    #[allow(dead_code)]
    fn get_process_info_direct(&self) -> Vec<(String, u32, f64)> {
        let output = Command::new("sudo")
            .arg("powermetrics")
            .arg("-n")
            .arg("1")
            .arg("-i")
            .arg("1000")
            .arg("--samplers")
            .arg("tasks")
            .arg("--show-process-gpu")
            .output()
            .expect("Failed to execute powermetrics command");

        let mut processes = Vec::new();
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines = output_str.lines();

            for line in lines {
                if line.contains("pid") {
                    continue;
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let process_name = parts[0].to_string();
                    let pid_str = parts[1];
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        let gpu_usage_str = parts.get(2).unwrap_or(&"0.0");
                        if let Ok(gpu_usage) = gpu_usage_str.parse::<f64>() {
                            if gpu_usage > 0.0 {
                                processes.push((process_name, pid, gpu_usage));
                            }
                        }
                    }
                }
            }
        } else {
            #[cfg(debug_assertions)]
            eprintln!("powermetrics command failed with status: {}", output.status);
        }
        processes
    }
}

impl GpuReader for AppleSiliconGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::<GpuInfo>::new();

        let total_memory = get_total_memory();
        let used_memory = get_used_memory();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Get comprehensive metrics from PowerMetricsManager or enhanced parser
        let metrics = if let Some(manager) = get_powermetrics_manager() {
            match manager.get_latest_data_result() {
                Ok(data) => data,
                Err(_) => {
                    // Use old method without spawning new powermetrics processes
                    let gpu_metrics = get_gpu_metrics();
                    let mut detail = HashMap::new();
                    if let Some(version) = &self.driver_version {
                        detail.insert("driver_version".to_string(), version.clone());
                    }

                    gpu_info.push(GpuInfo {
                        uuid: "AppleSiliconGPU".to_string(),
                        time: current_time,
                        name: self.name.clone(),
                        hostname: get_hostname(),
                        instance: get_hostname(),
                        utilization: gpu_metrics.utilization.unwrap_or(0.0),
                        ane_utilization: gpu_metrics.ane_utilization.unwrap_or(0.0),
                        dla_utilization: None,
                        temperature: gpu_metrics.thermal_pressure.unwrap_or(0),
                        used_memory,
                        total_memory,
                        frequency: gpu_metrics.frequency.unwrap_or(0),
                        power_consumption: gpu_metrics.power_consumption.unwrap_or(0.0),
                        detail,
                    });
                    return gpu_info;
                }
            }
        } else {
            // Use old method without spawning new powermetrics processes
            let gpu_metrics = get_gpu_metrics();
            let mut detail = HashMap::new();
            if let Some(version) = &self.driver_version {
                detail.insert("driver_version".to_string(), version.clone());
            }

            gpu_info.push(GpuInfo {
                uuid: "AppleSiliconGPU".to_string(),
                time: current_time,
                name: self.name.clone(),
                hostname: get_hostname(),
                instance: get_hostname(),
                utilization: gpu_metrics.utilization.unwrap_or(0.0),
                ane_utilization: gpu_metrics.ane_utilization.unwrap_or(0.0),
                dla_utilization: None,
                temperature: gpu_metrics.thermal_pressure.unwrap_or(0),
                used_memory,
                total_memory,
                frequency: gpu_metrics.frequency.unwrap_or(0),
                power_consumption: gpu_metrics.power_consumption.unwrap_or(0.0),
                detail,
            });
            return gpu_info;
        };

        let mut detail = HashMap::new();

        // Add driver version to detail map
        if let Some(version) = &self.driver_version {
            detail.insert("driver_version".to_string(), version.clone());
        }

        // Add CPU metrics to detail for comprehensive monitoring
        detail.insert(
            "cpu_utilization".to_string(),
            format!("{:.1}%", metrics.cpu_utilization()),
        );
        detail.insert(
            "e_cluster_active".to_string(),
            format!("{:.1}%", metrics.e_cluster_active_residency),
        );
        detail.insert(
            "p_cluster_active".to_string(),
            format!("{:.1}%", metrics.p_cluster_active_residency),
        );
        detail.insert(
            "cpu_power".to_string(),
            format!("{:.1}W", metrics.cpu_power_mw / 1000.0),
        );

        gpu_info.push(GpuInfo {
            uuid: "AppleSiliconGPU".to_string(),
            time: current_time,
            name: self.name.clone(),
            hostname: get_hostname(),
            instance: get_hostname(),
            utilization: metrics.gpu_utilization(),
            ane_utilization: metrics.ane_power_mw / 1000.0, // Convert mW to W
            dla_utilization: None,
            temperature: metrics.thermal_pressure.unwrap_or(0),
            used_memory,
            total_memory,
            frequency: metrics.gpu_frequency,
            power_consumption: metrics.gpu_power_mw / 1000.0, // Convert mW to W
            detail,
        });

        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        let mut process_list = Vec::new();

        // Try to get process info from PowerMetricsManager
        let process_data = if let Some(manager) = get_powermetrics_manager() {
            manager.get_process_info()
        } else {
            // Return empty list if PowerMetricsManager is not available
            // This avoids spawning additional powermetrics processes
            vec![]
        };

        // Convert process data to ProcessInfo
        for (process_name, pid, gpu_usage) in process_data {
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
                device_id: 0,
                device_uuid: "AppleSiliconGPU".to_string(),
                pid,
                process_name,
                used_memory: gpu_usage as u64, // Using GPU ms/s as a proxy for memory
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

        process_list
    }
}

struct GpuMetrics {
    utilization: Option<f64>,
    ane_utilization: Option<f64>,
    frequency: Option<u32>,
    power_consumption: Option<f64>,
    thermal_pressure: Option<u32>,
}

fn get_gpu_metrics() -> GpuMetrics {
    let output = Command::new("sudo")
        .arg("powermetrics")
        .arg("-n")
        .arg("1")
        .arg("-i")
        .arg("1000")
        .arg("--samplers")
        .arg("gpu_power,ane_power,thermal")
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute powermetrics");

    let reader = BufReader::new(output.stdout.as_slice());

    let mut utilization: Option<f64> = None;
    let mut ane_utilization: Option<f64> = None;
    let mut frequency: Option<u32> = None;
    let mut power_consumption: Option<f64> = None;
    let mut thermal_pressure: Option<u32> = None;

    for line in reader.lines().map_while(Result::ok) {
        if line.contains("GPU HW active residency:") {
            if let Some(usage_str) = line.split(':').nth(1) {
                utilization = usage_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .trim_end_matches('%')
                    .parse::<f64>()
                    .ok();
            }
        } else if line.contains("ANE Power:") {
            if let Some(power_str) = line.split(':').nth(1) {
                ane_utilization = power_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .ok();
            }
        } else if line.contains("GPU HW active frequency:") {
            if let Some(freq_str) = line.split(':').nth(1) {
                frequency = freq_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<u32>()
                    .ok();
            }
        } else if line.contains("GPU Power:") && !line.contains("CPU + GPU") {
            if let Some(power_str) = line.split(':').nth(1) {
                power_consumption = power_str
                    .split_whitespace()
                    .next()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .ok();
                // Convert power from mW to W
                if let Some(p) = power_consumption {
                    power_consumption = Some(p / 1000.0);
                }
            }
        } else if line.contains("CPU Thermal pressure") {
            if let Some(pressure_str) = line.split(':').nth(1) {
                thermal_pressure = pressure_str.trim().parse::<u32>().ok();
            }
        }
    }

    GpuMetrics {
        utilization,
        ane_utilization,
        frequency,
        power_consumption,
        thermal_pressure,
    }
}

fn get_gpu_name_and_version() -> (String, Option<String>) {
    let output = Command::new("system_profiler")
        .arg("SPDisplaysDataType")
        .output()
        .expect("Failed to execute system_profiler command");

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut name = String::from("Apple Silicon GPU");
    let mut driver_version: Option<String> = None;

    for line in output_str.lines() {
        if line.contains("Chipset Model:") {
            if let Some(model_str) = line.split(':').nth(1) {
                name = model_str.trim().to_string();
            }
        } else if line.contains("Metal Support:") {
            if let Some(version_str) = line.split("Metal Support:").nth(1) {
                driver_version = Some(version_str.trim().to_string());
            }
        }
    }

    (name, driver_version)
}

fn get_total_memory() -> u64 {
    let output = Command::new("sysctl")
        .arg("hw.memsize")
        .output()
        .expect("Failed to execute sysctl command");

    let output_str = String::from_utf8_lossy(&output.stdout);
    output_str
        .split(':')
        .nth(1)
        .unwrap_or("0")
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
}

fn get_used_memory() -> u64 {
    let output = Command::new("vm_stat")
        .output()
        .expect("Failed to execute vm_stat command");

    let output_str = String::from_utf8_lossy(&output.stdout);
    let page_size = 4096;

    let _free_pages: u64 = output_str
        .lines()
        .find(|line| line.starts_with("Pages free:"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pages| pages.replace(".", "").parse::<u64>().ok())
        .unwrap_or(0);

    let active_pages: u64 = output_str
        .lines()
        .find(|line| line.starts_with("Pages active:"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pages| pages.replace(".", "").parse::<u64>().ok())
        .unwrap_or(0);

    let inactive_pages: u64 = output_str
        .lines()
        .find(|line| line.starts_with("Pages inactive:"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pages| pages.replace(".", "").parse::<u64>().ok())
        .unwrap_or(0);

    let wired_pages: u64 = output_str
        .lines()
        .find(|line| line.starts_with("Pages wired down:"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pages| pages.replace(".", "").parse::<u64>().ok())
        .unwrap_or(0);

    let compressed_pages: u64 = output_str
        .lines()
        .find(|line| line.starts_with("Pages occupied by compressor:"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pages| pages.replace(".", "").parse::<u64>().ok())
        .unwrap_or(0);

    let used_pages = active_pages + inactive_pages + wired_pages + compressed_pages;
    used_pages * page_size
}
