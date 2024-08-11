use std::process::Command;
use crate::gpu::{GpuInfo, GpuReader};
use std::str::FromStr;
use chrono::Local;

pub struct NvidiaGpuReader;

impl GpuReader for NvidiaGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // Execute the nvidia-smi command to get GPU information
        let output = Command::new("nvidia-smi")
            .arg("--format=csv,noheader,nounits")
            .arg("--query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total,clocks.current.graphics,power.draw")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines = output_str.lines();

                // Read each line and extract GPU information
                for line in lines {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() == 7 {
                        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let name = parts[0].trim().to_string();
                        let utilization = f64::from_str(parts[1].trim()).unwrap_or(0.0);
                        let temperature = u32::from_str(parts[2].trim()).unwrap_or(0);
                        let used_memory = u64::from_str(parts[3].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let total_memory = u64::from_str(parts[4].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let frequency = u32::from_str(parts[5].trim()).unwrap_or(0); // Frequency in MHz
                        let power_consumption = f64::from_str(parts[6].trim()).unwrap_or(0.0); // Power consumption in W

                        gpu_info.push(GpuInfo {
                            time,
                            name,
                            utilization,
                            temperature,
                            used_memory,
                            total_memory,
                            frequency,
                            power_consumption,
                        });
                    }
                }
            } else {
                eprintln!(
                    "nvidia-smi command failed with status: {}",
                    output.status
                );
            }
        } else {
            eprintln!("Failed to execute nvidia-smi command");
        }

        gpu_info
    }
}