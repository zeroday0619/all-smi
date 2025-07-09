use crate::device::types::ProcessInfo;
use std::collections::HashSet;
use sysinfo::{ProcessStatus, System};

/// Get all system processes with GPU usage information
pub fn get_all_processes(system: &System, gpu_pids: &HashSet<u32>) -> Vec<ProcessInfo> {
    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        let pid_u32 = pid.as_u32();
        let uses_gpu = gpu_pids.contains(&pid_u32);

        // Get process information
        let process_info = ProcessInfo {
            device_id: 0, // Will be set by GPU-specific code if uses_gpu
            device_uuid: if uses_gpu {
                "GPU".to_string()
            } else {
                String::new()
            },
            pid: pid_u32,
            process_name: process.name().to_string_lossy().to_string(),
            used_memory: 0, // GPU memory, will be set by GPU-specific code
            cpu_percent: process.cpu_usage() as f64,
            memory_percent: (process.memory() as f64 / system.total_memory() as f64) * 100.0,
            memory_rss: process.memory() * 1024, // Convert from KB to bytes
            memory_vms: process.virtual_memory() * 1024, // Convert from KB to bytes
            user: get_process_user(process),
            state: convert_process_state(process.status()),
            start_time: format!("{}", process.start_time()),
            cpu_time: process.run_time(),
            command: get_process_command(process),
            ppid: process.parent().map(|p| p.as_u32()).unwrap_or(0),
            threads: 1, // sysinfo doesn't provide thread count directly
            uses_gpu,
        };

        processes.push(process_info);
    }

    // Sort by PID for consistent ordering
    processes.sort_by_key(|p| p.pid);
    processes
}

/// Convert sysinfo ProcessStatus to standard Unix state code
fn convert_process_state(status: ProcessStatus) -> String {
    // Convert the status to string and then map to single-letter codes
    let status_str = status.to_string();

    match status_str.as_str() {
        "Run" | "Runnable" | "Running" => "R",            // Running
        "Sleep" | "Sleeping" => "S",                      // Sleeping
        "Idle" => "I",                                    // Idle
        "Stop" | "Stopped" => "T",                        // Stopped (traced)
        "Zombie" => "Z",                                  // Zombie
        "Dead" => "X",                                    // Dead
        "Disk Sleep" | "UninterruptibleDiskSleep" => "D", // Uninterruptible disk sleep
        "Unknown" => "?",                                 // Unknown
        _ => "?",                                         // Any other state
    }
    .to_string()
}

/// Get process user name
fn get_process_user(process: &sysinfo::Process) -> String {
    if let Some(user_id) = process.user_id() {
        // Try to get username from user ID
        #[cfg(unix)]
        {
            use std::ffi::CStr;
            unsafe {
                let passwd = libc::getpwuid(**user_id);
                if !passwd.is_null() {
                    if let Ok(name) = CStr::from_ptr((*passwd).pw_name).to_str() {
                        return name.to_string();
                    }
                }
            }
        }
        user_id.to_string()
    } else {
        "unknown".to_string()
    }
}

/// Get process command line
fn get_process_command(process: &sysinfo::Process) -> String {
    let cmd = process.cmd();
    if cmd.is_empty() {
        format!("[{}]", process.name().to_string_lossy())
    } else {
        // Convert OsStr arguments to String
        cmd.iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Merge GPU process information with system process list
pub fn merge_gpu_processes(all_processes: &mut [ProcessInfo], gpu_processes: Vec<ProcessInfo>) {
    // Create a map of GPU processes by PID
    let gpu_map: std::collections::HashMap<u32, ProcessInfo> =
        gpu_processes.into_iter().map(|p| (p.pid, p)).collect();

    // Update matching processes with GPU information
    for process in all_processes.iter_mut() {
        if let Some(gpu_process) = gpu_map.get(&process.pid) {
            process.device_id = gpu_process.device_id;
            process.device_uuid = gpu_process.device_uuid.clone();
            process.used_memory = gpu_process.used_memory;
            process.uses_gpu = true;
        }
    }
}
