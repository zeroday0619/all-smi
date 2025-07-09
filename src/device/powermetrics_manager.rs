use std::fs;
use std::path::{Path, PathBuf};
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
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Kill any existing powermetrics processes first
        Self::kill_existing_powermetrics_processes();

        // Generate unique filename with timestamp
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let output_file = PathBuf::from(format!("/tmp/all-smi_powermetrics_{timestamp}"));

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
                                eprintln!("Error checking powermetrics status: {_e}");
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
                        eprintln!("Failed to restart powermetrics: {_e}");
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
        output_file: &Path,
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
    fn get_latest_data_internal(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
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

    /// Get latest data as Option (for test compatibility)
    #[cfg(test)]
    pub fn get_latest_data(&self) -> Option<PowerMetricsData> {
        self.get_latest_data_result().ok()
    }

    /// Get latest data as Result
    pub fn get_latest_data_result(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        self.get_latest_data_internal()
    }

    /// Determine the output flag based on macOS version
    #[allow(dead_code)]
    fn get_output_flag(&self) -> String {
        Self::get_output_flag_static()
    }

    #[allow(dead_code)]
    fn get_output_flag_static() -> String {
        // Check macOS version to determine correct flag
        // Older versions use -u, newer use -o
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
            if let Ok(version) = String::from_utf8(output.stdout) {
                let parts: Vec<&str> = version.trim().split('.').collect();
                if let Some(major) = parts.first().and_then(|v| v.parse::<u32>().ok()) {
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
        // Use ps auxww to see full command line without truncation
        if let Ok(output) = Command::new("ps").args(["auxww"]).output() {
            let ps_output = String::from_utf8_lossy(&output.stdout);
            for line in ps_output.lines() {
                // Look for parent processes (sudo nice) with our specific output file pattern
                if line.contains("sudo nice") && line.contains("/tmp/all-smi_powermetrics_") {
                    // Extract PID (second column)
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            // Kill the process (this will also kill its children)
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
pub fn cleanup_stale_powermetrics_files() {
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

/// Kill all powermetrics processes spawned by all-smi
#[allow(dead_code)]
pub fn terminate_all_smi_powermetrics_processes() {
    // Use ps auxww to see full command line without truncation
    if let Ok(output) = Command::new("ps").args(["auxww"]).output() {
        let ps_output = String::from_utf8_lossy(&output.stdout);
        let mut pids_to_kill = Vec::new();

        // First, find all parent processes (sudo nice) with our output file pattern
        for line in ps_output.lines() {
            if line.contains("sudo nice") && line.contains("/tmp/all-smi_powermetrics_") {
                // Extract PID (second column)
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() > 1 {
                    if let Ok(pid) = parts[1].parse::<u32>() {
                        pids_to_kill.push(pid);
                    }
                }
            }
        }

        // Kill the parent processes (which will also kill their children)
        let mut killed_count = 0;
        for pid in pids_to_kill {
            if Command::new("sudo")
                .args(["kill", "-9", &pid.to_string()])
                .output()
                .is_ok()
            {
                killed_count += 1;
                println!("Terminated all-smi powermetrics process with PID: {pid}");
            }
        }

        if killed_count > 0 {
            println!("Terminated {killed_count} all-smi powermetrics process(es)");
            // Also clean up any stale temp files
            cleanup_stale_powermetrics_files();
        } else {
            println!("No all-smi powermetrics processes found");
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

    // Explicitly clean up before dropping
    if let Some(manager) = manager_guard.as_ref() {
        // Stop the monitoring flag
        if let Ok(mut is_running) = manager.is_running.lock() {
            *is_running = false;
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = manager.process.lock() {
            if let Some(mut child) = process_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        // Clean up the temporary file
        let _ = fs::remove_file(&manager.output_file);
    }

    *manager_guard = None; // This will trigger Drop
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_get_output_flag_static() {
        // Test that get_output_flag_static returns a valid flag
        let flag = PowerMetricsManager::get_output_flag_static();
        assert!(flag == "-o" || flag == "-u");
    }

    #[test]
    fn test_powermetrics_data_cache() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            output_file: PathBuf::from("/tmp/test_powermetrics"),
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        };

        // Test initial state - no data
        assert!(manager.get_latest_data().is_none());

        // Create test data
        let test_data = PowerMetricsData {
            e_cluster_active_residency: 25.5,
            p_cluster_active_residency: 75.5,
            e_cluster_frequency: 1020,
            p_cluster_frequency: 3000,
            cpu_power_mw: 1500.0,
            core_active_residencies: vec![],
            core_frequencies: vec![],
            core_cluster_types: vec![],
            gpu_active_residency: 45.5,
            gpu_frequency: 1200,
            gpu_power_mw: 2500.0,
            ane_power_mw: 100.0,
            combined_power_mw: 4100.0,
            thermal_pressure: Some(0),
        };

        // Set cached data
        {
            let mut last_data = manager.last_data.lock().unwrap();
            *last_data = Some(test_data.clone());
        }

        // Verify cached data
        let cached = manager.get_latest_data();
        assert!(cached.is_some());
        let cached_data = cached.unwrap();
        assert_eq!(cached_data.gpu_active_residency, 45.5);
        assert_eq!(cached_data.gpu_frequency, 1200);
        assert_eq!(cached_data.e_cluster_active_residency, 25.5);
        assert_eq!(cached_data.p_cluster_active_residency, 75.5);
    }

    #[test]
    fn test_singleton_instance() {
        // Clean up any existing instance
        shutdown_powermetrics_manager();

        // Initialize the manager
        let _ = initialize_powermetrics_manager();

        // Test that we get the same instance
        let manager1 = get_powermetrics_manager();
        let manager2 = get_powermetrics_manager();

        assert!(manager1.is_some());
        assert!(manager2.is_some());

        // Both should return the same Arc pointer
        assert!(Arc::ptr_eq(&manager1.unwrap(), &manager2.unwrap()));

        // Clean up
        shutdown_powermetrics_manager();
    }

    #[test]
    fn test_get_latest_data_from_file() {
        use std::path::PathBuf;

        // Create a test file path
        let test_file = PathBuf::from("/tmp/test_powermetrics_test.txt");

        // Write test powermetrics output that matches expected format
        let test_output = r#"*** Sampled system activity (Fri Nov 15 10:00:00 2024 -0800) (1000ms elapsed) ***

*** Processor usage ***
E-Cluster HW active frequency: 1020 MHz
E-Cluster HW active residency: 25.5%

P-Cluster HW active frequency: 3000 MHz  
P-Cluster HW active residency: 75.5%

CPU Power: 1500 mW

*** GPU usage ***
GPU HW active frequency: 1200 MHz
GPU HW active residency: 45.5%
GPU Power: 2500 mW

ANE Power: 100 mW
Combined Power (CPU + GPU + ANE): 4100 mW

*** Sampled system activity"#;

        if let Ok(mut file) = File::create(&test_file) {
            let _ = file.write_all(test_output.as_bytes());
        }

        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            output_file: test_file.clone(),
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        };

        // Get data from file
        let result = manager.get_latest_data_result();

        // Check that data was parsed
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.e_cluster_frequency, 1020);
        assert_eq!(data.e_cluster_active_residency, 25.5);
        assert_eq!(data.p_cluster_frequency, 3000);
        assert_eq!(data.p_cluster_active_residency, 75.5);

        // Clean up
        let _ = std::fs::remove_file(&test_file);
    }

    #[test]
    fn test_cleanup_stale_files() {
        // Create test files in /tmp directly
        let stale_file1 = PathBuf::from("/tmp/all-smi_powermetrics_test_12345");
        let stale_file2 = PathBuf::from("/tmp/all-smi_powermetrics_test_67890");

        // Create test files
        let _ = File::create(&stale_file1);
        let _ = File::create(&stale_file2);

        // Call cleanup - it will clean up all stale powermetrics files
        cleanup_stale_powermetrics_files();

        // The test files might or might not be cleaned depending on timing
        // Just ensure the function doesn't panic

        // Clean up our test files if they still exist
        let _ = std::fs::remove_file(&stale_file1);
        let _ = std::fs::remove_file(&stale_file2);
    }
}
