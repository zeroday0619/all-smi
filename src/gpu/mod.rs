pub mod apple_silicon;
pub mod nvidia;

use std::process::Command;

pub trait GpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
}

#[derive(Debug)]
pub struct GpuInfo {
    pub time: String,
    pub name: String,
    pub utilization: f64,
    pub temperature: u32,
    pub used_memory: u64,
    pub total_memory: u64,
    pub frequency: u32,          // Added frequency field
    pub power_consumption: f64,  // Added power consumption field
}

pub fn get_gpu_readers() -> Vec<Box<dyn GpuReader>> {
    let mut readers: Vec<Box<dyn GpuReader>> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            if has_nvidia() {
                readers.push(Box::new(nvidia::NvidiaGpuReader {}));
            }
        },
        "macos" => {
            if is_apple_silicon() {
                readers.push(Box::new(apple_silicon::AppleSiliconGpuReader {}));
            }
        },
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