use crate::gpu::{GpuInfo, GpuReader};
use chrono::Local;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};

pub struct AppleSiliconGpuReader;

impl GpuReader for AppleSiliconGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::<GpuInfo>::new();

        let total_memory = get_total_memory();
        let used_memory = get_used_memory();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let name = "Apple Silicon GPU".to_string();

        let utilization = get_gpu_utilization().unwrap_or(0.0); // 0.0 if utilization is not available
        gpu_info.push(GpuInfo {
            time: current_time,
            name,
            utilization,
            temperature: 0, // Temperature not available
            used_memory,
            total_memory,
        });

        gpu_info
    }
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

fn get_gpu_utilization() -> Option<f64> {
    let output = Command::new("sudo")
        .arg("powermetrics")
        .arg("-n")
        .arg("1")
        .arg("-i")
        .arg("1000")
        .arg("--samplers")
        .arg("gpu_power")
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute powermetrics");

    let reader = BufReader::new(output.stdout.as_slice());

    for line in reader.lines() {
        if let Ok(line) = line {
            if line.contains("GPU HW active residency:") {
                // Example parsing: "GPU HW active residency:  12.79%"
                if let Some(usage_str) = line.split(':').nth(1) {
                    if let Ok(usage) = usage_str.trim().split_whitespace().next().unwrap_or("0").trim_end_matches('%').parse::<f64>() {
                        return Some(usage);
                    }
                }
            }
        }
    }

    None
}