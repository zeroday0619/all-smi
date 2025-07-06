use chrono::Local;
use std::process::Command;

use crate::gpu::{MemoryInfo, MemoryReader};
use crate::utils::get_hostname;

pub struct MacOsMemoryReader;

impl MacOsMemoryReader {
    pub fn new() -> Self {
        MacOsMemoryReader
    }
}

impl MemoryReader for MacOsMemoryReader {
    fn get_memory_info(&self) -> Vec<MemoryInfo> {
        let mut memory_info = Vec::new();

        // Get memory information using vm_stat command
        if let Ok(output) = Command::new("vm_stat").output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let mut page_size = 4096; // Default page size
                let mut pages_free = 0;
                let mut pages_active = 0;
                let mut pages_inactive = 0;
                let mut pages_wired = 0;
                let mut pages_compressed = 0;

                for line in output_str.lines() {
                    if line.contains("page size of") {
                        if let Some(size_str) =
                            line.split_whitespace().find(|s| s.parse::<u64>().is_ok())
                        {
                            page_size = size_str.parse::<u64>().unwrap_or(4096);
                        }
                    } else if line.contains("Pages free:") {
                        if let Some(value) = extract_number_from_line(line) {
                            pages_free = value;
                        }
                    } else if line.contains("Pages active:") {
                        if let Some(value) = extract_number_from_line(line) {
                            pages_active = value;
                        }
                    } else if line.contains("Pages inactive:") {
                        if let Some(value) = extract_number_from_line(line) {
                            pages_inactive = value;
                        }
                    } else if line.contains("Pages wired down:") {
                        if let Some(value) = extract_number_from_line(line) {
                            pages_wired = value;
                        }
                    } else if line.contains("Pages stored in compressor:") {
                        if let Some(value) = extract_number_from_line(line) {
                            pages_compressed = value;
                        }
                    }
                }

                // Calculate memory values in bytes
                let free_bytes = pages_free * page_size;
                let active_bytes = pages_active * page_size;
                let inactive_bytes = pages_inactive * page_size;
                let wired_bytes = pages_wired * page_size;
                let compressed_bytes = pages_compressed * page_size;

                // Total memory calculation
                let total_bytes =
                    active_bytes + inactive_bytes + wired_bytes + compressed_bytes + free_bytes;
                let used_bytes = active_bytes + inactive_bytes + wired_bytes + compressed_bytes;
                let available_bytes = free_bytes + inactive_bytes;

                // Calculate utilization percentage
                let utilization = if total_bytes > 0 {
                    (used_bytes as f64 / total_bytes as f64) * 100.0
                } else {
                    0.0
                };

                // Get swap information (simplified - macOS doesn't expose swap info easily)
                let swap_info = get_swap_info();

                let hostname = get_hostname();
                let now = Local::now();

                memory_info.push(MemoryInfo {
                    hostname: hostname.clone(),
                    instance: hostname,
                    total_bytes,
                    used_bytes,
                    available_bytes,
                    free_bytes,
                    buffers_bytes: 0,             // Not applicable on macOS
                    cached_bytes: inactive_bytes, // Inactive pages can be considered as cached
                    swap_total_bytes: swap_info.0,
                    swap_used_bytes: swap_info.1,
                    swap_free_bytes: swap_info.0 - swap_info.1,
                    utilization,
                    time: now.format("%Y-%m-%d %H:%M:%S").to_string(),
                });
            }
        }

        memory_info
    }
}

fn extract_number_from_line(line: &str) -> Option<u64> {
    // Extract number from lines like "Pages free: 123456."
    let parts: Vec<&str> = line.split_whitespace().collect();
    if let Some(number_str) = parts.last() {
        let clean_number = number_str.trim_end_matches('.');
        clean_number.parse::<u64>().ok()
    } else {
        None
    }
}

fn get_swap_info() -> (u64, u64) {
    // Try to get swap information using sysctl
    if let Ok(output) = Command::new("sysctl").arg("vm.swapusage").output() {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Parse output like "vm.swapusage: total = 2048.00M  used = 1024.00M  free = 1024.00M"
            let mut total = 0;
            let mut used = 0;

            for part in output_str.split_whitespace() {
                if part.contains("total") {
                    continue;
                }
                if part.contains("used") {
                    continue;
                }
                if part.contains("free") {
                    continue;
                }
                if part.contains("=") {
                    continue;
                }

                // Parse values like "2048.00M"
                if let Some(value) = parse_memory_value(part) {
                    if total == 0 {
                        total = value;
                    } else if used == 0 {
                        used = value;
                        break;
                    }
                }
            }

            return (total, used);
        }
    }

    (0, 0)
}

fn parse_memory_value(value_str: &str) -> Option<u64> {
    // Parse values like "2048.00M", "1.5G", etc.
    if let Some(unit_pos) = value_str.find(|c: char| c.is_alphabetic()) {
        let (number_str, unit) = value_str.split_at(unit_pos);
        if let Ok(number) = number_str.parse::<f64>() {
            let multiplier = match unit.to_uppercase().as_str() {
                "K" => 1024,
                "M" => 1024 * 1024,
                "G" => 1024 * 1024 * 1024,
                _ => 1,
            };
            return Some((number * multiplier as f64) as u64);
        }
    }
    None
}
