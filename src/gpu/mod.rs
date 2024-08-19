pub mod apple_silicon;
pub mod nvidia;

use std::collections::HashMap;
use std::process::Command;

// GpuReader 트레이트
pub trait GpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
    fn get_process_info(&self) -> Vec<ProcessInfo>;
}

// GpuReader 트레이트를 구현하는 구조체
pub struct GpuReaderWrapper {
    reader: Box<dyn GpuReader>,
}

impl GpuReaderWrapper {
    pub fn new(reader: Box<dyn GpuReader>) -> Self {
        GpuReaderWrapper { reader }
    }
}

impl GpuReader for GpuReaderWrapper {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        self.reader.get_gpu_info()
    }

    fn get_process_info(&self) -> Vec<ProcessInfo> {
        self.reader.get_process_info()
    }
}

// Send 트레이트 구현
unsafe impl Send for GpuReaderWrapper {}

#[derive(Debug)]
pub struct GpuInfo {
    pub time: String,
    pub name: String,
    pub utilization: f64,
    pub temperature: u32,
    pub used_memory: u64,
    pub total_memory: u64,
    pub frequency: u32,
    pub power_consumption: f64,
    pub detail: HashMap<String, String>,  // Added detail field
}

#[derive(Clone)]
pub struct ProcessInfo {
    pub device_id: usize,        // GPU index (internal)
    pub device_uuid: String,     // GPU UUID
    pub pid: u32,                // Process ID
    pub process_name: String,    // Process name
    pub used_memory: u64,
}

pub fn get_gpu_readers() -> Vec<GpuReaderWrapper> {
    let mut readers: Vec<GpuReaderWrapper> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            if has_nvidia() {
                readers.push(GpuReaderWrapper::new(Box::new(nvidia::NvidiaGpuReader {})));
            }
        },
        "macos" => {
            if is_apple_silicon() {
                readers.push(GpuReaderWrapper::new(Box::new(apple_silicon::AppleSiliconGpuReader::new())));
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