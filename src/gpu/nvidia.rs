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
            .arg("--format=csv")
            .output();

        if let Ok(output) = output {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let mut lines = output_str.lines();

            // Skip the first line as it is the header
            lines.next();

            // Read each line and extract GPU information
            for line in lines {
                let parts: Vec<&str> = line.trim().split(',').collect();
                if parts.len() >= 6 {
                    let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                    let name = parts[1].to_string();
                    let utilization = f64::from_str(parts[2]).unwrap_or(0.0);
                    let temperature = u32::from_str(parts[3]).unwrap_or(0);
                    let used_memory = u64::from_str(parts[4]).unwrap_or(0);
                    let total_memory = u64::from_str(parts[5]).unwrap_or(0);

                    gpu_info.push(GpuInfo {
                        time,
                        name,
                        utilization,
                        temperature,
                        used_memory,
                        total_memory,
                    });
                }
            }
        } else {
            println!("Failed to execute nvidia-smi command");
        }

        gpu_info
    }
}