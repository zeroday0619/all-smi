use crate::gpu::{GpuInfo, GpuReader};
use chrono::Local;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::collections::HashMap;

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
}

impl GpuReader for AppleSiliconGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        let mut gpu_info = Vec::<GpuInfo>::new();

        let total_memory = get_total_memory();
        let used_memory = get_used_memory();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Get the GPU metrics from powermetrics
        let gpu_metrics = get_gpu_metrics();

        let utilization = gpu_metrics.utilization.unwrap_or(0.0);
        let frequency = gpu_metrics.frequency.unwrap_or(0);
        let power_consumption = gpu_metrics.power_consumption.unwrap_or(0.0);
        let mut detail = HashMap::new();

        // Add driver version to detail map
        if let Some(version) = &self.driver_version {
            detail.insert("driver_version".to_string(), version.clone());
        }

        gpu_info.push(GpuInfo {
            time: current_time,
            name: self.name.clone(),
            utilization,
            temperature: 0,
            used_memory,
            total_memory,
            frequency,
            power_consumption,
            detail,
        });

        gpu_info
    }
}

struct GpuMetrics {
    utilization: Option<f64>,
    frequency: Option<u32>,
    power_consumption: Option<f64>,
}

fn get_gpu_metrics() -> GpuMetrics {
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

    let mut utilization: Option<f64> = None;
    let mut frequency: Option<u32> = None;
    let mut power_consumption: Option<f64> = None;

    for line in reader.lines() {
        if let Ok(line) = line {
            if line.contains("GPU HW active residency:") {
                if let Some(usage_str) = line.split(':').nth(1) {
                    utilization = usage_str
                        .trim()
                        .split_whitespace()
                        .next()
                        .unwrap_or("0")
                        .trim_end_matches('%')
                        .parse::<f64>()
                        .ok();
                }
            } else if line.contains("GPU HW active frequency:") {
                if let Some(freq_str) = line.split(':').nth(1) {
                    frequency = freq_str.trim().split_whitespace().next().unwrap_or("0").parse::<u32>().ok();
                }
            } else if line.contains("GPU Power:") {
                if let Some(power_str) = line.split(':').nth(1) {
                    power_consumption = power_str.trim().split_whitespace().next().unwrap_or("0").parse::<f64>().ok();
                    // Convert power from mW to W
                    if let Some(p) = power_consumption {
                        power_consumption = Some(p / 1000.0);
                    }
                }
            }
        }
    }

    GpuMetrics {
        utilization,
        frequency,
        power_consumption,
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