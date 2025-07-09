use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::device::powermetrics_parser::{parse_powermetrics_output, PowerMetricsData};

/// Manages a long-running powermetrics process to avoid repeated invocations
pub struct PowerMetricsManager {
    process: Arc<Mutex<Option<Child>>>,
    output_file: PathBuf,
    last_data: Arc<Mutex<Option<PowerMetricsData>>>,
    is_running: Arc<Mutex<bool>>,
}

impl PowerMetricsManager {
    /// Create a new PowerMetricsManager and start the powermetrics process
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Kill any existing powermetrics processes first
        Self::kill_existing_powermetrics_processes();

        // Generate unique filename with timestamp
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let output_file = PathBuf::from(format!("/tmp/all-smi_powermetrics_{}", timestamp));

        let manager = Self {
            process: Arc::new(Mutex::new(None)),
            output_file: output_file.clone(),
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        };

        manager.start_powermetrics()?;

        // Start a background thread to monitor the process
        let process_arc = manager.process.clone();
        let output_file_clone = output_file.clone();
        let is_running = manager.is_running.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(5));

                let should_restart = {
                    let mut process_guard = process_arc.lock().unwrap();
                    if let Some(ref mut child) = *process_guard {
                        match child.try_wait() {
                            Ok(Some(_)) => {
                                // Process has exited, need to restart
                                // Log to file instead of stderr to avoid breaking TUI
                                #[cfg(debug_assertions)]
                                eprintln!("powermetrics process died, restarting...");
                                true
                            }
                            Ok(None) => false, // Still running
                            Err(_e) => {
                                #[cfg(debug_assertions)]
                                eprintln!("Error checking powermetrics status: {}", _e);
                                true
                            }
                        }
                    } else {
                        false
                    }
                };

                if should_restart {
                    if let Ok(running) = is_running.lock() {
                        if !*running {
                            break; // Manager is shutting down
                        }
                    }

                    // Restart powermetrics
                    if let Err(_e) = Self::restart_powermetrics(&process_arc, &output_file_clone) {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to restart powermetrics: {}", _e);
                    }
                }
            }
        });

        Ok(manager)
    }

    /// Start the powermetrics subprocess
    fn start_powermetrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Determine output flag based on macOS version
        let output_flag = self.get_output_flag();

        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10", // Lower priority to reduce system impact
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            &output_flag,
            &self.output_file.to_string_lossy(),
            "-i",
            "1000", // 1 second interval
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

        let child = cmd.spawn()?;

        let mut process_guard = self.process.lock().unwrap();
        *process_guard = Some(child);

        let mut is_running = self.is_running.lock().unwrap();
        *is_running = true;

        // Wait for powermetrics to write initial data
        // Need to wait longer than the sampling interval (1000ms) plus processing time
        thread::sleep(Duration::from_millis(2500));

        Ok(())
    }

    /// Restart the powermetrics process
    fn restart_powermetrics(
        process_arc: &Arc<Mutex<Option<Child>>>,
        output_file: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Kill existing process if any
        {
            let mut process_guard = process_arc.lock().unwrap();
            if let Some(mut child) = process_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        // Determine output flag based on macOS version
        let output_flag = Self::get_output_flag_static();

        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10",
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            &output_flag,
            &output_file.to_string_lossy(),
            "-i",
            "1000",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

        let child = cmd.spawn()?;

        let mut process_guard = process_arc.lock().unwrap();
        *process_guard = Some(child);

        Ok(())
    }

    /// Get the latest powermetrics data
    pub fn get_latest_data(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        // Try to read from the file
        if let Ok(contents) = fs::read_to_string(&self.output_file) {
            // Find the last complete powermetrics output in the file
            // powermetrics outputs are separated by "*** Sampled system activity"
            let sections: Vec<&str> = contents.split("*** Sampled system activity").collect();

            // We need at least 2 sections to have one complete section
            if sections.len() >= 2 {
                // If we have exactly 2 sections, the first might be complete
                // If we have 3+, use the second-to-last which is definitely complete
                let last_complete = if sections.len() == 2 {
                    sections[1]
                } else {
                    sections[sections.len() - 2]
                };

                // Parse the data
                if let Ok(data) = parse_powermetrics_output(last_complete) {
                    // Cache the data
                    let mut last_data = self.last_data.lock().unwrap();
                    *last_data = Some(data.clone());
                    return Ok(data);
                }
            }
        }

        // If we can't read fresh data, return cached data if available
        if let Some(cached) = self.last_data.lock().unwrap().clone() {
            return Ok(cached);
        }

        Err("No powermetrics data available".into())
    }

    /// Get process information from the latest powermetrics data
    pub fn get_process_info(&self) -> Vec<(String, u32, f64)> {
        let mut processes = Vec::new();

        if let Ok(contents) = fs::read_to_string(&self.output_file) {
            // Find the last complete powermetrics output
            let sections: Vec<&str> = contents.split("*** Sampled system activity").collect();

            // We need at least 2 sections to have one complete section
            if sections.len() >= 2 {
                // If we have exactly 2 sections, the first might be complete
                // If we have 3+, use the second-to-last which is definitely complete
                let last_complete = if sections.len() == 2 {
                    sections[1]
                } else {
                    sections[sections.len() - 2]
                };

                // Debug logging removed to prevent breaking TUI layout

                // Parse process information from powermetrics output
                // Format: Name ID CPU ms/s User% Deadlines Wakeups GPU ms/s
                for line in last_complete.lines() {
                    // Skip header lines and section markers
                    if line.contains("***")
                        || line.contains("Name")
                        || line.contains("ID")
                        || line.trim().is_empty()
                    {
                        continue;
                    }

                    // The GPU ms/s is the last column, so we need to handle lines with spaces in process names
                    let line = line.trim();

                    // Split the line and look for the pattern
                    let parts: Vec<&str> = line.split_whitespace().collect();

                    // We need at least 8 parts for a valid process line with GPU usage
                    if parts.len() >= 8 {
                        // The last part should be GPU ms/s
                        if let Ok(gpu_ms) = parts[parts.len() - 1].parse::<f64>() {
                            // Find where the numeric columns start (after process name)
                            let mut pid_index = None;
                            for (i, part) in parts.iter().enumerate() {
                                if i > 0 && part.parse::<i32>().is_ok() {
                                    pid_index = Some(i);
                                    break;
                                }
                            }

                            if let Some(idx) = pid_index {
                                if let Ok(pid) = parts[idx].parse::<u32>() {
                                    // Reconstruct process name from parts before PID
                                    let process_name = parts[0..idx].join(" ");

                                    // Only include processes with GPU usage > 0
                                    if gpu_ms > 0.0 {
                                        processes.push((process_name, pid, gpu_ms));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        processes
    }

    /// Determine the output flag based on macOS version
    fn get_output_flag(&self) -> String {
        Self::get_output_flag_static()
    }

    fn get_output_flag_static() -> String {
        // Check macOS version to determine correct flag
        // Older versions use -u, newer use -o
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                let parts: Vec<&str> = version.trim().split('.').collect();
                if let Some(major) = parts.get(0).and_then(|v| v.parse::<u32>().ok()) {
                    if major >= 13 {
                        return "-o".to_string();
                    }
                }
            }
        }
        "-u".to_string() // Default to older flag
    }

    /// Kill any existing powermetrics processes spawned by all-smi
    fn kill_existing_powermetrics_processes() {
        if let Ok(output) = Command::new("ps").args(["aux"]).output() {
            let ps_output = String::from_utf8_lossy(&output.stdout);
            for line in ps_output.lines() {
                // Look for powermetrics processes with our specific arguments
                if line.contains("powermetrics") && line.contains("all-smi_powermetrics") {
                    // Extract PID (second column)
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            // Kill the process
                            let _ = Command::new("sudo")
                                .args(["kill", "-9", &pid.to_string()])
                                .output();
                        }
                    }
                }
            }
        }
    }
}

impl Drop for PowerMetricsManager {
    fn drop(&mut self) {
        // Stop the monitoring flag
        if let Ok(mut is_running) = self.is_running.lock() {
            *is_running = false;
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = self.process.lock() {
            if let Some(mut child) = process_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        // Clean up the temporary file
        let _ = fs::remove_file(&self.output_file);
    }
}

// Global singleton instance
use once_cell::sync::Lazy;
static POWERMETRICS_MANAGER: Lazy<Mutex<Option<Arc<PowerMetricsManager>>>> =
    Lazy::new(|| Mutex::new(None));

/// Initialize the global PowerMetricsManager
pub fn initialize_powermetrics_manager() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
    if manager_guard.is_none() {
        // Clean up any stale powermetrics files before starting
        cleanup_stale_powermetrics_files();

        let manager = PowerMetricsManager::new()?;
        *manager_guard = Some(Arc::new(manager));
    }
    Ok(())
}

/// Clean up stale powermetrics temporary files
fn cleanup_stale_powermetrics_files() {
    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            if let Some(filename) = entry.file_name().to_str() {
                if filename.starts_with("all-smi_powermetrics_") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// Get the global PowerMetricsManager instance
pub fn get_powermetrics_manager() -> Option<Arc<PowerMetricsManager>> {
    POWERMETRICS_MANAGER.lock().unwrap().clone()
}

/// Shutdown the global PowerMetricsManager
pub fn shutdown_powermetrics_manager() {
    let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
    *manager_guard = None; // This will trigger Drop
}
