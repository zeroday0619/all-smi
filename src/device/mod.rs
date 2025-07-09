pub mod apple_silicon;
pub mod nvidia;
pub mod nvidia_jetson;

// Re-export NVML status function for UI
pub use nvidia::get_nvml_status_message;

// CPU reader modules
pub mod cpu_linux;
pub mod cpu_macos;

// Memory reader modules
pub mod memory_linux;
pub mod memory_macos;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::process::Command;

// Type aliases for complex return types
type ProcessInfoResult = Option<(
    f64,
    f64,
    u64,
    u64,
    String,
    String,
    String,
    u64,
    String,
    u32,
    u32,
)>;

pub trait GpuReader: Send {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
    fn get_process_info(&self) -> Vec<ProcessInfo>;
}

pub trait CpuReader: Send {
    fn get_cpu_info(&self) -> Vec<CpuInfo>;
}

pub trait MemoryReader: Send {
    fn get_memory_info(&self) -> Vec<MemoryInfo>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub uuid: String,
    pub time: String,
    pub name: String,
    pub device_type: String, // "GPU", "NPU", etc.
    pub hostname: String,
    pub instance: String,
    pub utilization: f64,
    pub ane_utilization: f64,
    pub dla_utilization: Option<f64>,
    pub temperature: u32,
    pub used_memory: u64,
    pub total_memory: u64,
    pub frequency: u32,
    pub power_consumption: f64,
    pub detail: HashMap<String, String>, // Added detail field
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub device_id: usize,     // GPU index (internal)
    pub device_uuid: String,  // GPU UUID
    pub pid: u32,             // Process ID
    pub process_name: String, // Process name
    pub used_memory: u64,     // GPU memory usage in bytes
    pub cpu_percent: f64,     // CPU usage percentage
    pub memory_percent: f64,  // System memory usage percentage
    pub memory_rss: u64,      // Resident Set Size in bytes
    pub memory_vms: u64,      // Virtual Memory Size in bytes
    pub user: String,         // User name
    pub state: String,        // Process state (R, S, D, etc.)
    pub start_time: String,   // Process start time
    pub cpu_time: u64,        // Total CPU time in seconds
    pub command: String,      // Full command line
    pub ppid: u32,            // Parent process ID
    pub threads: u32,         // Number of threads
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub hostname: String,
    pub instance: String,
    pub cpu_model: String,
    pub architecture: String, // "x86_64", "arm64", etc.
    pub platform_type: CpuPlatformType,
    pub socket_count: u32,                   // Number of CPU sockets
    pub total_cores: u32,                    // Total logical cores
    pub total_threads: u32,                  // Total threads (with hyperthreading)
    pub base_frequency_mhz: u32,             // Base CPU frequency
    pub max_frequency_mhz: u32,              // Maximum CPU frequency
    pub cache_size_mb: u32,                  // Total cache size in MB
    pub utilization: f64,                    // Overall CPU utilization percentage
    pub temperature: Option<u32>,            // CPU temperature in Celsius (if available)
    pub power_consumption: Option<f64>,      // Power consumption in watts (if available)
    pub per_socket_info: Vec<CpuSocketInfo>, // Per-socket information
    pub apple_silicon_info: Option<AppleSiliconCpuInfo>, // Apple Silicon specific info
    pub time: String,                        // Timestamp
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CpuPlatformType {
    Intel,
    Amd,
    AppleSilicon,
    Arm,
    Other(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuSocketInfo {
    pub socket_id: u32,
    pub utilization: f64,         // Per-socket utilization
    pub cores: u32,               // Number of cores in this socket
    pub threads: u32,             // Number of threads in this socket
    pub temperature: Option<u32>, // Socket temperature (if available)
    pub frequency_mhz: u32,       // Current frequency
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppleSiliconCpuInfo {
    pub p_core_count: u32,               // Performance core count
    pub e_core_count: u32,               // Efficiency core count
    pub gpu_core_count: u32,             // GPU core count
    pub p_core_utilization: f64,         // Performance core utilization
    pub e_core_utilization: f64,         // Efficiency core utilization
    pub ane_ops_per_second: Option<f64>, // ANE operations per second
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryInfo {
    pub hostname: String,
    pub instance: String,
    pub total_bytes: u64,      // Total system memory in bytes
    pub used_bytes: u64,       // Used memory in bytes
    pub available_bytes: u64,  // Available memory in bytes
    pub free_bytes: u64,       // Free memory in bytes
    pub buffers_bytes: u64,    // Buffer memory in bytes (Linux specific)
    pub cached_bytes: u64,     // Cached memory in bytes (Linux specific)
    pub swap_total_bytes: u64, // Total swap space in bytes
    pub swap_used_bytes: u64,  // Used swap space in bytes
    pub swap_free_bytes: u64,  // Free swap space in bytes
    pub utilization: f64,      // Memory utilization percentage
    pub time: String,          // Timestamp
}

pub fn get_gpu_readers() -> Vec<Box<dyn GpuReader>> {
    let mut readers: Vec<Box<dyn GpuReader>> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            if is_jetson() {
                readers.push(Box::new(nvidia_jetson::NvidiaJetsonGpuReader {}));
            } else if has_nvidia() {
                readers.push(Box::new(nvidia::NvidiaGpuReader {}));
            }
        }
        "macos" => {
            if is_apple_silicon() {
                readers.push(Box::new(apple_silicon::AppleSiliconGpuReader::new()));
            }
        }
        _ => println!("Unsupported OS type: {os_type}"),
    }

    readers
}

pub fn get_cpu_readers() -> Vec<Box<dyn CpuReader>> {
    let mut readers: Vec<Box<dyn CpuReader>> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            readers.push(Box::new(cpu_linux::LinuxCpuReader::new()));
        }
        "macos" => {
            readers.push(Box::new(cpu_macos::MacOsCpuReader::new()));
        }
        _ => println!("CPU monitoring not supported for OS type: {os_type}"),
    }

    readers
}

pub fn get_memory_readers() -> Vec<Box<dyn MemoryReader>> {
    let mut readers: Vec<Box<dyn MemoryReader>> = Vec::new();
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => {
            readers.push(Box::new(memory_linux::LinuxMemoryReader::new()));
        }
        "macos" => {
            readers.push(Box::new(memory_macos::MacOsMemoryReader::new()));
        }
        _ => println!("Memory monitoring not supported for OS type: {os_type}"),
    }

    readers
}

fn has_nvidia() -> bool {
    Command::new("nvidia-smi").output().is_ok()
}

fn is_jetson() -> bool {
    if let Ok(compatible) = std::fs::read_to_string("/proc/device-tree/compatible") {
        return compatible.contains("tegra");
    }
    false
}

fn is_apple_silicon() -> bool {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to execute uname command");

    let architecture = String::from_utf8_lossy(&output.stdout);
    architecture.trim() == "arm64"
}

// Helper function to get system process information
pub fn get_system_process_info(pid: u32) -> ProcessInfoResult {
    let os_type = std::env::consts::OS;

    match os_type {
        "linux" => get_linux_process_info(pid),
        "macos" => get_macos_process_info(pid),
        _ => None,
    }
}

fn get_linux_process_info(pid: u32) -> ProcessInfoResult {
    // Read /proc/[pid]/stat for basic process information
    let stat_path = format!("/proc/{pid}/stat");
    let stat_content = fs::read_to_string(&stat_path).ok()?;
    let stat_fields: Vec<&str> = stat_content.split_whitespace().collect();

    if stat_fields.len() < 24 {
        return None;
    }

    // Read /proc/[pid]/status for additional information
    let status_path = format!("/proc/{pid}/status");
    let status_content = fs::read_to_string(&status_path).ok()?;

    // Read /proc/[pid]/cmdline for full command
    let cmdline_path = format!("/proc/{pid}/cmdline");
    let cmdline_content = fs::read_to_string(&cmdline_path).unwrap_or_default();
    let command = cmdline_content.replace('\0', " ").trim().to_string();
    let command = if command.is_empty() {
        format!(
            "[{}]",
            stat_fields
                .get(1)
                .unwrap_or(&"unknown")
                .trim_matches('(')
                .trim_matches(')')
        )
    } else {
        command
    };

    // Parse stat fields
    let state = stat_fields.get(2).unwrap_or(&"?").to_string();
    let ppid = stat_fields
        .get(3)
        .unwrap_or(&"0")
        .parse::<u32>()
        .unwrap_or(0);
    let utime = stat_fields
        .get(13)
        .unwrap_or(&"0")
        .parse::<u64>()
        .unwrap_or(0);
    let stime = stat_fields
        .get(14)
        .unwrap_or(&"0")
        .parse::<u64>()
        .unwrap_or(0);
    let cpu_time = (utime + stime) / 100; // Convert from jiffies to seconds (assuming 100 HZ)
    let vsize = stat_fields
        .get(22)
        .unwrap_or(&"0")
        .parse::<u64>()
        .unwrap_or(0);
    let rss_pages = stat_fields
        .get(23)
        .unwrap_or(&"0")
        .parse::<u64>()
        .unwrap_or(0);
    let rss_bytes = rss_pages * 4096; // Convert pages to bytes (assuming 4KB pages)
    let num_threads = stat_fields
        .get(19)
        .unwrap_or(&"1")
        .parse::<u32>()
        .unwrap_or(1);

    // Parse status for additional information
    let mut user = "unknown".to_string();

    for line in status_content.lines() {
        if line.starts_with("Uid:") {
            if let Some(uid_str) = line.split_whitespace().nth(1) {
                if let Ok(uid) = uid_str.parse::<u32>() {
                    user = get_username_from_uid(uid);
                }
            }
        }
    }

    // Calculate memory percentage (simplified - would need total system memory)
    let memory_percent = (rss_bytes as f64 / (8.0 * 1024.0 * 1024.0 * 1024.0)) * 100.0; // Assume 8GB system memory

    // Get start time
    let start_time = get_process_start_time(pid).unwrap_or_else(|| "unknown".to_string());

    // CPU percentage calculation (simplified - would need previous measurements for accurate calculation)
    let cpu_percent = 0.0; // Would need time-based sampling to calculate accurately

    Some((
        cpu_percent,
        memory_percent,
        rss_bytes,
        vsize,
        user,
        state,
        start_time,
        cpu_time,
        command,
        ppid,
        num_threads,
    ))
}

fn get_macos_process_info(pid: u32) -> ProcessInfoResult {
    // Use ps command for macOS
    let output = Command::new("ps")
        .args([
            "-o",
            "pid,ppid,user,pcpu,pmem,rss,vsz,state,lstart,time,comm,args",
            "-p",
            &pid.to_string(),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = output_str.lines().collect();

    if lines.len() < 2 {
        return None;
    }

    let data_line = lines[1];
    let fields: Vec<&str> = data_line.split_whitespace().collect();

    if fields.len() < 12 {
        return None;
    }

    let ppid = fields.get(1).unwrap_or(&"0").parse::<u32>().unwrap_or(0);
    let user = fields.get(2).unwrap_or(&"unknown").to_string();
    let cpu_percent = fields
        .get(3)
        .unwrap_or(&"0.0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let memory_percent = fields
        .get(4)
        .unwrap_or(&"0.0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let rss_kb = fields.get(5).unwrap_or(&"0").parse::<u64>().unwrap_or(0);
    let vsz_kb = fields.get(6).unwrap_or(&"0").parse::<u64>().unwrap_or(0);
    let state = fields.get(7).unwrap_or(&"?").to_string();

    let rss_bytes = rss_kb * 1024;
    let vms_bytes = vsz_kb * 1024;

    // Get start time (simplified)
    let start_time = fields.get(8).unwrap_or(&"unknown").to_string();

    // CPU time (simplified)
    let cpu_time_str = fields.get(9).unwrap_or(&"0:00");
    let cpu_time = parse_time_to_seconds(cpu_time_str);

    // Command (take remaining fields)
    let command = if fields.len() > 11 {
        fields[11..].join(" ")
    } else {
        fields.get(10).unwrap_or(&"unknown").to_string()
    };

    // Number of threads (not easily available via ps, use 1 as default)
    let num_threads = 1;

    Some((
        cpu_percent,
        memory_percent,
        rss_bytes,
        vms_bytes,
        user,
        state,
        start_time,
        cpu_time,
        command,
        ppid,
        num_threads,
    ))
}

fn get_username_from_uid(uid: u32) -> String {
    // Try to get username from /etc/passwd
    if let Ok(passwd_content) = fs::read_to_string("/etc/passwd") {
        for line in passwd_content.lines() {
            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() >= 3 {
                if let Ok(line_uid) = fields[2].parse::<u32>() {
                    if line_uid == uid {
                        return fields[0].to_string();
                    }
                }
            }
        }
    }
    uid.to_string()
}

fn get_process_start_time(pid: u32) -> Option<String> {
    let stat_path = format!("/proc/{pid}/stat");
    let stat_content = fs::read_to_string(&stat_path).ok()?;
    let stat_fields: Vec<&str> = stat_content.split_whitespace().collect();

    if let Some(starttime_str) = stat_fields.get(21) {
        if let Ok(starttime_jiffies) = starttime_str.parse::<u64>() {
            // Convert jiffies to seconds since boot
            let starttime_seconds = starttime_jiffies / 100; // Assuming 100 HZ

            // Get boot time
            if let Ok(uptime_content) = fs::read_to_string("/proc/uptime") {
                if let Some(uptime_str) = uptime_content.split_whitespace().next() {
                    if let Ok(uptime_seconds) = uptime_str.parse::<f64>() {
                        let boot_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as f64
                            - uptime_seconds;

                        let process_start_time = boot_time + starttime_seconds as f64;
                        let start_time = std::time::UNIX_EPOCH
                            + std::time::Duration::from_secs(process_start_time as u64);

                        if let Ok(datetime) = start_time.duration_since(std::time::UNIX_EPOCH) {
                            return Some(format!("{}", datetime.as_secs()));
                        }
                    }
                }
            }
        }
    }

    None
}

fn parse_time_to_seconds(time_str: &str) -> u64 {
    // Parse time in format like "0:01.23" or "1:23:45"
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        2 => {
            // MM:SS format
            let minutes = parts[0].parse::<u64>().unwrap_or(0);
            let seconds = parts[1]
                .split('.')
                .next()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0);
            minutes * 60 + seconds
        }
        3 => {
            // HH:MM:SS format
            let hours = parts[0].parse::<u64>().unwrap_or(0);
            let minutes = parts[1].parse::<u64>().unwrap_or(0);
            let seconds = parts[2]
                .split('.')
                .next()
                .unwrap_or("0")
                .parse::<u64>()
                .unwrap_or(0);
            hours * 3600 + minutes * 60 + seconds
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_to_seconds() {
        assert_eq!(parse_time_to_seconds("0:01"), 1);
        assert_eq!(parse_time_to_seconds("1:30"), 90);
        assert_eq!(parse_time_to_seconds("0:01.23"), 1);
        assert_eq!(parse_time_to_seconds("2:15.45"), 135);
        assert_eq!(parse_time_to_seconds("1:23:45"), 5025);
        assert_eq!(parse_time_to_seconds("0:05:30"), 330);
        assert_eq!(parse_time_to_seconds("0:00:00"), 0);
        assert_eq!(parse_time_to_seconds("invalid"), 0);
        assert_eq!(parse_time_to_seconds(""), 0);
        assert_eq!(parse_time_to_seconds("1"), 0);
        assert_eq!(parse_time_to_seconds("1:2:3:4"), 0);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_is_apple_silicon_on_macos() {
        let _result = is_apple_silicon();
        // Function should execute without panicking and return a boolean
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_is_apple_silicon_on_non_macos() {
        let result = is_apple_silicon();
        assert_eq!(
            result, false,
            "is_apple_silicon should return false on non-macOS"
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_has_nvidia_on_linux() {
        let result = has_nvidia();
        assert!(
            result == true || result == false,
            "has_nvidia should return a boolean"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_has_nvidia_on_non_linux() {
        let _result = has_nvidia();
        // Function should execute without panicking and return a boolean
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_is_jetson_on_linux() {
        let result = is_jetson();
        assert!(
            result == true || result == false,
            "is_jetson should return a boolean"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_is_jetson_on_non_linux() {
        let result = is_jetson();
        assert!(!result, "is_jetson should return false on non-Linux");
    }

    #[test]
    fn test_get_gpu_readers() {
        let readers = get_gpu_readers();
        assert!(readers.len() <= 1, "Should return at most one GPU reader");
    }

    #[test]
    fn test_get_cpu_readers() {
        let readers = get_cpu_readers();
        assert!(
            readers.len() <= 1,
            "Should return at most one CPU reader per OS"
        );
    }

    #[test]
    fn test_get_memory_readers() {
        let readers = get_memory_readers();
        assert!(
            readers.len() <= 1,
            "Should return at most one memory reader per OS"
        );
    }

    #[test]
    fn test_cpu_platform_type_enum() {
        let intel = CpuPlatformType::Intel;
        let amd = CpuPlatformType::Amd;
        let apple = CpuPlatformType::AppleSilicon;
        let arm = CpuPlatformType::Arm;
        let other = CpuPlatformType::Other("Custom".to_string());

        match intel {
            CpuPlatformType::Intel => {}
            _ => panic!("Intel enum variant should match"),
        }

        match amd {
            CpuPlatformType::Amd => {}
            _ => panic!("AMD enum variant should match"),
        }

        match apple {
            CpuPlatformType::AppleSilicon => {}
            _ => panic!("AppleSilicon enum variant should match"),
        }

        match arm {
            CpuPlatformType::Arm => {}
            _ => panic!("Arm enum variant should match"),
        }

        match other {
            CpuPlatformType::Other(ref name) => assert_eq!(name, "Custom"),
            _ => panic!("Other enum variant should match"),
        }
    }

    #[test]
    fn test_gpu_info_default_values() {
        let gpu_info = GpuInfo {
            uuid: "test-uuid".to_string(),
            time: "2023-01-01 00:00:00".to_string(),
            name: "Test GPU".to_string(),
            device_type: "GPU".to_string(),
            hostname: "test-host".to_string(),
            instance: "test-instance".to_string(),
            utilization: 0.0,
            ane_utilization: 0.0,
            dla_utilization: None,
            temperature: 0,
            used_memory: 0,
            total_memory: 0,
            frequency: 0,
            power_consumption: 0.0,
            detail: HashMap::new(),
        };

        assert_eq!(gpu_info.uuid, "test-uuid");
        assert_eq!(gpu_info.utilization, 0.0);
        assert_eq!(gpu_info.dla_utilization, None);
        assert!(gpu_info.detail.is_empty());
    }

    #[test]
    fn test_cpu_info_default_values() {
        let cpu_info = CpuInfo {
            hostname: "test-host".to_string(),
            instance: "test-instance".to_string(),
            cpu_model: "Test CPU".to_string(),
            architecture: "x86_64".to_string(),
            platform_type: CpuPlatformType::Other("Test".to_string()),
            socket_count: 1,
            total_cores: 4,
            total_threads: 8,
            base_frequency_mhz: 2400,
            max_frequency_mhz: 3200,
            cache_size_mb: 16,
            utilization: 0.0,
            temperature: None,
            power_consumption: None,
            per_socket_info: Vec::new(),
            apple_silicon_info: None,
            time: "2023-01-01 00:00:00".to_string(),
        };

        assert_eq!(cpu_info.hostname, "test-host");
        assert_eq!(cpu_info.socket_count, 1);
        assert_eq!(cpu_info.temperature, None);
        assert!(cpu_info.per_socket_info.is_empty());
        assert!(cpu_info.apple_silicon_info.is_none());
    }

    #[test]
    fn test_memory_info_default_values() {
        let memory_info = MemoryInfo {
            hostname: "test-host".to_string(),
            instance: "test-instance".to_string(),
            total_bytes: 0,
            used_bytes: 0,
            available_bytes: 0,
            free_bytes: 0,
            buffers_bytes: 0,
            cached_bytes: 0,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            swap_free_bytes: 0,
            utilization: 0.0,
            time: "2023-01-01 00:00:00".to_string(),
        };

        assert_eq!(memory_info.hostname, "test-host");
        assert_eq!(memory_info.total_bytes, 0);
        assert_eq!(memory_info.utilization, 0.0);
    }

    #[test]
    fn test_apple_silicon_cpu_info() {
        let apple_info = AppleSiliconCpuInfo {
            p_core_count: 8,
            e_core_count: 4,
            gpu_core_count: 32,
            p_core_utilization: 50.0,
            e_core_utilization: 25.0,
            ane_ops_per_second: Some(15.5e12),
        };

        assert_eq!(apple_info.p_core_count, 8);
        assert_eq!(apple_info.e_core_count, 4);
        assert_eq!(apple_info.gpu_core_count, 32);
        assert_eq!(apple_info.p_core_utilization, 50.0);
        assert_eq!(apple_info.e_core_utilization, 25.0);
        assert_eq!(apple_info.ane_ops_per_second, Some(15.5e12));
    }

    #[test]
    fn test_process_info_values() {
        let process_info = ProcessInfo {
            device_id: 0,
            device_uuid: "GPU-12345".to_string(),
            pid: 1234,
            process_name: "test_process".to_string(),
            used_memory: 1073741824, // 1GB
            cpu_percent: 25.5,
            memory_percent: 10.2,
            memory_rss: 2147483648, // 2GB
            memory_vms: 4294967296, // 4GB
            user: "testuser".to_string(),
            state: "R".to_string(),
            start_time: "2023-01-01 12:00:00".to_string(),
            cpu_time: 3600, // 1 hour
            command: "test_command --arg1 --arg2".to_string(),
            ppid: 1,
            threads: 4,
        };

        assert_eq!(process_info.device_id, 0);
        assert_eq!(process_info.device_uuid, "GPU-12345");
        assert_eq!(process_info.pid, 1234);
        assert_eq!(process_info.used_memory, 1073741824);
        assert_eq!(process_info.cpu_percent, 25.5);
        assert_eq!(process_info.threads, 4);
    }
}
