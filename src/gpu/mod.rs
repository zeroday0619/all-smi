pub mod apple_silicon;
pub mod nvidia;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

pub trait GpuReader: Send {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
    fn get_process_info(&self) -> Vec<ProcessInfo>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub time: String,
    pub name: String,
    pub utilization: f64,
    pub ane_utilization: f64,
    pub temperature: u32,
    pub used_memory: u64,
    pub total_memory: u64,
    pub frequency: u32,
    pub power_consumption: f64,
    pub detail: HashMap<String, String>, // Added detail field
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub device_id: usize,    // GPU index (internal)
    pub device_uuid: String, // GPU UUID
    pub pid: u32,            // Process ID
    pub process_name: String, // Process name
    pub used_memory: u64,
}

pub fn get_gpu_readers() -> Vec<Box<dyn GpuReader>> {
    let mut readers: Vec<Box<dyn GpuReader>> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            if has_nvidia() {
                readers.push(Box::new(nvidia::NvidiaGpuReader {}));
            }
        }
        "macos" => {
            if is_apple_silicon() {
                readers.push(Box::new(apple_silicon::AppleSiliconGpuReader::new()));
            }
        }
        _ => println!("Unsupported OS type: {}", os_type),
    }

    readers
}

fn has_nvidia() -> bool {
    Command::new("nvidia-smi").output().is_ok()
}

fn is_apple_silicon() -> bool {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to execute uname command");

    let architecture = String::from_utf8_lossy(&output.stdout);
    architecture.trim() == "arm64"
}