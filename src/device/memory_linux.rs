use chrono::Local;
use std::fs;

use crate::device::{MemoryInfo, MemoryReader};
use crate::utils::get_hostname;

pub struct LinuxMemoryReader;

impl LinuxMemoryReader {
    pub fn new() -> Self {
        LinuxMemoryReader
    }
}

impl MemoryReader for LinuxMemoryReader {
    fn get_memory_info(&self) -> Vec<MemoryInfo> {
        let mut memory_info = Vec::new();

        if let Ok(meminfo_content) = fs::read_to_string("/proc/meminfo") {
            let mut total_bytes = 0;
            let mut available_bytes = 0;
            let mut free_bytes = 0;
            let mut buffers_bytes = 0;
            let mut cached_bytes = 0;
            let mut swap_total_bytes = 0;
            let mut swap_free_bytes = 0;

            for line in meminfo_content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let key = parts[0].trim_end_matches(':');
                    if let Ok(value) = parts[1].parse::<u64>() {
                        let value_bytes = value * 1024; // Convert from kB to bytes

                        match key {
                            "MemTotal" => total_bytes = value_bytes,
                            "MemAvailable" => available_bytes = value_bytes,
                            "MemFree" => free_bytes = value_bytes,
                            "Buffers" => buffers_bytes = value_bytes,
                            "Cached" => cached_bytes = value_bytes,
                            "SwapTotal" => swap_total_bytes = value_bytes,
                            "SwapFree" => swap_free_bytes = value_bytes,
                            _ => {}
                        }
                    }
                }
            }

            // Calculate used memory
            let used_bytes = total_bytes - available_bytes;
            let swap_used_bytes = swap_total_bytes - swap_free_bytes;

            // Calculate utilization percentage
            let utilization = if total_bytes > 0 {
                (used_bytes as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };

            let hostname = get_hostname();
            let now = Local::now();

            memory_info.push(MemoryInfo {
                hostname: hostname.clone(),
                instance: hostname,
                total_bytes,
                used_bytes,
                available_bytes,
                free_bytes,
                buffers_bytes,
                cached_bytes,
                swap_total_bytes,
                swap_used_bytes,
                swap_free_bytes,
                utilization,
                time: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            });
        }

        memory_info
    }
}
