use std::process::Command;
use crate::gpu::{GpuInfo, GpuReader, ProcessInfo};
use std::str::FromStr;
use chrono::Local;
use std::collections::HashMap;

pub struct NvidiaGpuReader;

impl GpuReader for NvidiaGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::new();

        // Execute the nvidia-smi command to get GPU information, including driver version
        let output = Command::new("nvidia-smi")
            .arg("--format=csv,noheader,nounits")
            .arg("--query-gpu=driver_version,name,utilization.gpu,temperature.gpu,memory.used,memory.total,clocks.current.graphics,power.draw")
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let lines = output_str.lines();

                for line in lines {
                    let parts: Vec<&str> = line.trim().split(',').collect();
                    if parts.len() == 8 {
                        let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                        let driver_version = parts[0].trim().to_string();
                        let name = parts[1].trim().to_string();
                        let utilization = f64::from_str(parts[2].trim()).unwrap_or(0.0);
                        let temperature = u32::from_str(parts[3].trim()).unwrap_or(0);
                        let used_memory = u64::from_str(parts[4].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let total_memory = u64::from_str(parts[5].trim()).unwrap_or(0) * 1024 * 1024; // Convert MiB to bytes
                        let frequency = u32::from_str(parts[6].trim()).unwrap_or(0); // Frequency in MHz
                        let power_consumption = f64::from_str(parts[7].trim()).unwrap_or(0.0); // Power consumption in W

                        let mut detail = HashMap::new();
                        detail.insert("driver_version".to_string(), driver_version);

                        gpu_info.push(GpuInfo {
                            time,
                            name,
                            utilization,
                            ane_utilization: 0.0,
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

    fn get_process_info(&self) -> Vec<ProcessInfo> {
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

                        process_list.push(ProcessInfo {
                            device_id: 0, // Actual GPU index would need additional logic
                            device_uuid,
                            pid,
                            process_name,
                            used_memory,
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