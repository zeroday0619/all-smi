use crate::device::{get_system_process_info, GpuInfo, GpuReader, ProcessInfo};
use chrono::Local;
use std::collections::HashMap;
use std::process::Command;
use std::str::FromStr;

pub struct NvidiaGpuReader;

fn get_hostname() -> String {
    let output = Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

impl GpuReader for NvidiaGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
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
