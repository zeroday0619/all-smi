use crate::device::{GpuInfo, GpuReader, ProcessInfo};
use crate::utils::get_hostname;
use chrono::Local;
use std::collections::HashMap;
use std::fs;

pub struct NvidiaJetsonGpuReader;

impl GpuReader for NvidiaJetsonGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        let name = fs::read_to_string("/proc/device-tree/model")
            .unwrap_or_else(|_| "NVIDIA Jetson".to_string())
            .trim_end_matches('\0')
            .to_string();

        let utilization = fs::read_to_string("/sys/devices/platform/tegra-soc/gpu.0/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0) / 10.0);

        let frequency = fs::read_to_string("/sys/devices/platform/tegra-soc/gpu.0/cur_freq")
            .map_or(0, |s| s.trim().parse::<u32>().unwrap_or(0) / 1_000_000);

        let temperature = fs::read_to_string("/sys/devices/virtual/thermal/thermal_zone0/temp")
            .map_or(0, |s| s.trim().parse::<u32>().unwrap_or(0) / 1000);

        let power_consumption =
            fs::read_to_string("/sys/bus/i2c/drivers/ina3221x/0-0040/iio:device0/in_power0_input")
                .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0) / 1000.0);

        let dla0_util = fs::read_to_string("/sys/kernel/debug/dla_0/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0));
        let dla1_util = fs::read_to_string("/sys/kernel/debug/dla_1/load")
            .map_or(0.0, |s| s.trim().parse::<f64>().unwrap_or(0.0));
        let dla_utilization = if dla0_util > 0.0 || dla1_util > 0.0 {
            Some(dla0_util + dla1_util)
        } else {
            None
        };

        let (total_memory, used_memory) = get_memory_info();

        // Get Jetson-specific information
        let mut detail = HashMap::new();

        // Try to get CUDA version from nvidia-smi if available
        if let Ok(output) = std::process::Command::new("nvidia-smi").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                // Parse CUDA version from header
                for line in output_str.lines() {
                    if line.contains("CUDA Version:") {
                        if let Some(version) = line.split("CUDA Version:").nth(1) {
                            detail.insert("cuda_version".to_string(), version.trim().to_string());
                        }
                    }
                    if line.contains("Driver Version:") {
                        if let Some(version) = line.split("Driver Version:").nth(1) {
                            detail.insert("driver_version".to_string(), version.trim().to_string());
                        }
                    }
                }
            }
        }

        // Get Jetson architecture info
        if let Ok(arch) = fs::read_to_string("/sys/devices/soc0/family") {
            detail.insert("architecture".to_string(), arch.trim().to_string());
        }

        // Get compute capability for Jetson
        // Jetson Nano/TX2: 5.3, Xavier: 7.2, Orin: 8.7
        if name.contains("Orin") {
            detail.insert("compute_capability".to_string(), "8.7".to_string());
        } else if name.contains("Xavier") {
            detail.insert("compute_capability".to_string(), "7.2".to_string());
        } else if name.contains("TX2") || name.contains("Nano") {
            detail.insert("compute_capability".to_string(), "5.3".to_string());
        }

        gpu_info.push(GpuInfo {
            uuid: "NVIDIA-Jetson".to_string(),
            time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            name,
            device_type: "GPU".to_string(),
            hostname: get_hostname(),
            instance: get_hostname(),
            utilization,
            ane_utilization: 0.0,
            dla_utilization,
            temperature,
            used_memory,
            total_memory,
            frequency,
            power_consumption,
            detail,
        });

        gpu_info
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        Vec::new()
    }
}

fn get_memory_info() -> (u64, u64) {
    let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_memory = 0;
    let mut available_memory = 0;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_memory = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0)
                * 1024;
        } else if line.starts_with("MemAvailable:") {
            available_memory = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0)
                * 1024;
        }
    }

    (total_memory, total_memory - available_memory)
}
