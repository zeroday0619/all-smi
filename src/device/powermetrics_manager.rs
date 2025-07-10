use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::device::powermetrics_parser::{parse_powermetrics_output, PowerMetricsData};

/// Manages a long-running powermetrics process with in-memory circular buffer
pub struct PowerMetricsManager {
    process: Arc<Mutex<Option<Child>>>,
    // Circular buffer storing complete powermetrics sections
    data_buffer: Arc<Mutex<VecDeque<String>>>,
    // Channel for sending commands to reader thread
    command_tx: Option<Sender<ReaderCommand>>,
    last_data: Arc<Mutex<Option<PowerMetricsData>>>,
    is_running: Arc<Mutex<bool>>,
}

#[derive(Debug)]
enum ReaderCommand {
    Shutdown,
}

impl PowerMetricsManager {
    /// Create a new PowerMetricsManager and start the powermetrics process
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Kill any existing powermetrics processes first
        Self::kill_existing_powermetrics_processes();

        let (command_tx, command_rx) = mpsc::channel();

        let manager = Self {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(120))), // 2 minutes of data
            command_tx: Some(command_tx),
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        };

        manager.start_powermetrics(command_rx)?;

        // Start a background thread to monitor the process
        let process_arc = manager.process.clone();
        let data_buffer = manager.data_buffer.clone();
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

                    // Create new channel for the restarted process
                    let (_new_tx, new_rx) = mpsc::channel();

                    // Restart powermetrics
                    if let Err(_e) = Self::restart_powermetrics(&process_arc, &data_buffer, new_rx)
                    {
                        #[cfg(debug_assertions)]
                        eprintln!("Failed to restart powermetrics: {_e}");
                    }
                }
            }
        });

        Ok(manager)
    }

    /// Start the powermetrics subprocess with stdout piping
    fn start_powermetrics(
        &self,
        command_rx: Receiver<ReaderCommand>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10", // Lower priority to reduce system impact
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            "-i",
            "1000", // 1 second interval
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped()) // Capture stdout instead of writing to file
        .stderr(Stdio::null());

        // On Unix, create a new process group so we can kill all children
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        // Start reader thread
        let data_buffer = self.data_buffer.clone();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer, command_rx);
        });

        let mut process_guard = self.process.lock().unwrap();
        *process_guard = Some(child);

        let mut is_running = self.is_running.lock().unwrap();
        *is_running = true;

        // Wait for initial data to be collected
        thread::sleep(Duration::from_millis(2500));

        Ok(())
    }

    /// Reader thread that processes stdout from powermetrics
    fn reader_thread(
        stdout: std::process::ChildStdout,
        data_buffer: Arc<Mutex<VecDeque<String>>>,
        command_rx: Receiver<ReaderCommand>,
    ) {
        let reader = BufReader::new(stdout);
        let mut current_section = String::new();
        let mut in_section = false;

        for line in reader.lines() {
            // Check for shutdown command
            if let Ok(ReaderCommand::Shutdown) = command_rx.try_recv() {
                break;
            }

            let line = match line {
                Ok(l) => l,
                Err(_) => break, // Pipe broken, process died
            };

            // Detect start of new section
            if line.contains("*** Sampled system activity") {
                // If we have a complete section, store it
                if in_section && !current_section.is_empty() {
                    let mut buffer = data_buffer.lock().unwrap();
                    if buffer.len() >= 120 {
                        buffer.pop_front(); // Remove oldest
                    }
                    buffer.push_back(current_section.clone());
                }
                // Start new section
                current_section.clear();
                in_section = true;
            }

            if in_section {
                current_section.push_str(&line);
                current_section.push('\n');
            }
        }
    }

    /// Restart the powermetrics process
    fn restart_powermetrics(
        process_arc: &Arc<Mutex<Option<Child>>>,
        data_buffer: &Arc<Mutex<VecDeque<String>>>,
        command_rx: Receiver<ReaderCommand>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Kill existing process if any
        {
            let mut process_guard = process_arc.lock().unwrap();
            if let Some(mut child) = process_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        let mut cmd = Command::new("sudo");
        cmd.args([
            "nice",
            "-n",
            "10",
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal,tasks",
            "--show-process-gpu",
            "-i",
            "1000",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        // On Unix, create a new process group so we can kill all children
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;

        // Start new reader thread
        let data_buffer_clone = data_buffer.clone();
        thread::spawn(move || {
            Self::reader_thread(stdout, data_buffer_clone, command_rx);
        });

        let mut process_guard = process_arc.lock().unwrap();
        *process_guard = Some(child);

        Ok(())
    }

    /// Get the latest powermetrics data from the circular buffer
    fn get_latest_data_internal(&self) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Parse the data
            if let Ok(data) = parse_powermetrics_output(&section) {
                // Cache the data
                let mut last_data = self.last_data.lock().unwrap();
                *last_data = Some(data.clone());
                return Ok(data);
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

        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Find the "*** Running tasks ***" section
            if let Some(tasks_start) = section.find("*** Running tasks ***") {
                let tasks_section = &section[tasks_start..];

                // Find the next section marker or use the rest of the content
                let tasks_end = tasks_section[20..]
                    .find("***")
                    .unwrap_or(tasks_section.len() - 20)
                    + 20;
                let tasks_content = &tasks_section[..tasks_end];

                let mut in_header = false;

                // Parse process information from powermetrics output
                // Format: Name ID CPU ms/s User% Deadlines Deadlines Wakeups Wakeups GPU ms/s
                for line in tasks_content.lines() {
                    let line = line.trim();

                    // Skip empty lines and section markers
                    if line.is_empty() || line.contains("***") {
                        continue;
                    }

                    // Detect and skip header line
                    if line.contains("Name") && line.contains("ID") && line.contains("GPU ms/s") {
                        in_header = true;
                        continue;
                    }

                    // Skip lines until we're past the header
                    if in_header {
                        in_header = false;
                        continue;
                    }

                    // Process data lines - the format has fixed column positions
                    // Name (0-34), ID (35-41), CPU ms/s (42-51), User% (52-58),
                    // Deadlines (59-66, 67-74), Wakeups (75-82, 83-90), GPU ms/s (91-100)

                    // For robustness, we'll use a different approach:
                    // Split by whitespace but handle the known column structure
                    let parts: Vec<&str> = line.split_whitespace().collect();

                    // We need at least 9 parts (name, id, cpu, user%, 2 deadlines, 2 wakeups, gpu)
                    if parts.len() >= 9 {
                        // Try to find the PID - it should be an integer after the process name
                        let mut pid_index = None;
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                // Check if this could be a PID (positive integer)
                                if let Ok(pid) = part.parse::<u32>() {
                                    // Verify it's in a reasonable PID range
                                    if pid > 0 && pid < 100000 {
                                        pid_index = Some(i);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(idx) = pid_index {
                            // We expect 8 numeric values after PID
                            if parts.len() >= idx + 8 {
                                if let Ok(pid) = parts[idx].parse::<u32>() {
                                    // GPU ms/s is the last value
                                    if let Ok(gpu_ms) = parts[idx + 7].parse::<f64>() {
                                        // Reconstruct process name from parts before PID
                                        let process_name = parts[0..idx].join(" ");

                                        // Include all processes for better visibility
                                        // We'll let the caller decide what to filter
                                        processes.push((process_name, pid, gpu_ms));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by GPU usage (highest first)
        processes.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        processes
    }

    /// Get process information with both CPU and GPU metrics
    #[allow(dead_code)]
    pub fn get_process_info_detailed(&self) -> Vec<(String, u32, f64, f64)> {
        let mut processes = Vec::new();

        // Get the most recent complete section from the buffer
        let latest_section = {
            let buffer = self.data_buffer.lock().unwrap();
            buffer.back().cloned()
        };

        if let Some(section) = latest_section {
            // Find the "*** Running tasks ***" section
            if let Some(tasks_start) = section.find("*** Running tasks ***") {
                let tasks_section = &section[tasks_start..];

                let tasks_end = tasks_section[20..]
                    .find("***")
                    .unwrap_or(tasks_section.len() - 20)
                    + 20;
                let tasks_content = &tasks_section[..tasks_end];

                let mut in_header = false;

                for line in tasks_content.lines() {
                    let line = line.trim();

                    if line.is_empty() || line.contains("***") {
                        continue;
                    }

                    if line.contains("Name") && line.contains("ID") && line.contains("GPU ms/s") {
                        in_header = true;
                        continue;
                    }

                    if in_header {
                        in_header = false;
                        continue;
                    }

                    let parts: Vec<&str> = line.split_whitespace().collect();

                    if parts.len() >= 9 {
                        let mut pid_index = None;
                        for (i, part) in parts.iter().enumerate() {
                            if i > 0 {
                                if let Ok(pid) = part.parse::<u32>() {
                                    if pid > 0 && pid < 100000 {
                                        pid_index = Some(i);
                                        break;
                                    }
                                }
                            }
                        }

                        if let Some(idx) = pid_index {
                            if parts.len() >= idx + 8 {
                                if let Ok(pid) = parts[idx].parse::<u32>() {
                                    // CPU ms/s is at idx + 1
                                    if let Ok(cpu_ms) = parts[idx + 1].parse::<f64>() {
                                        // GPU ms/s is at idx + 7
                                        if let Ok(gpu_ms) = parts[idx + 7].parse::<f64>() {
                                            let process_name = parts[0..idx].join(" ");
                                            processes.push((process_name, pid, cpu_ms, gpu_ms));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by GPU usage first, then by CPU usage
        processes.sort_by(|a, b| {
            match b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal) {
                std::cmp::Ordering::Equal => {
                    b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
                }
                other => other,
            }
        });

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

    /// Kill any existing powermetrics processes spawned by all-smi
    fn kill_existing_powermetrics_processes() {
        // Use ps auxww to see full command line without truncation
        if let Ok(output) = Command::new("ps").args(["auxww"]).output() {
            let ps_output = String::from_utf8_lossy(&output.stdout);
            let mut parent_pids = Vec::new();
            let mut all_pids = Vec::new();

            for line in ps_output.lines() {
                // Look for powermetrics processes spawned by all-smi
                if line.contains("sudo nice")
                    && line.contains("powermetrics")
                    && line.contains("--samplers")
                    && line.contains("cpu_power")
                {
                    // Extract PID (second column)
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            parent_pids.push(pid);
                            all_pids.push(pid);
                        }
                    }
                }
                // Also look for powermetrics processes that might be orphaned
                else if line.contains("powermetrics")
                    && line.contains("--samplers")
                    && line.contains("cpu_power")
                    && line.contains("gpu_power")
                    && !line.contains("grep")
                {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() > 1 {
                        if let Ok(pid) = parts[1].parse::<u32>() {
                            all_pids.push(pid);
                        }
                    }
                }
            }

            // Kill parent processes with their process groups first
            for pid in &parent_pids {
                // Kill the entire process group (negative PID)
                let _ = Command::new("sudo")
                    .args(["kill", "-TERM", &format!("-{pid}")])
                    .output();
            }

            // Wait a moment for processes to terminate gracefully
            thread::sleep(Duration::from_millis(200));

            // Force kill any remaining processes individually
            for pid in all_pids {
                let _ = Command::new("sudo")
                    .args(["kill", "-9", &pid.to_string()])
                    .output();
            }
        }
    }
}

impl Drop for PowerMetricsManager {
    fn drop(&mut self) {
        // Debug: PowerMetricsManager Drop called
        // Stop the monitoring flag
        if let Ok(mut is_running) = self.is_running.lock() {
            *is_running = false;
        }

        // Send shutdown command to reader thread
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(ReaderCommand::Shutdown);
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = self.process.lock() {
            if let Some(mut child) = process_guard.take() {
                #[cfg(unix)]
                {
                    // Kill the entire process group
                    let pid = child.id();
                    // Use negative PID to kill the process group
                    let _ = Command::new("sudo")
                        .args(["kill", "-TERM", &format!("-{pid}")])
                        .output();
                    thread::sleep(Duration::from_millis(100));
                    let _ = Command::new("sudo")
                        .args(["kill", "-9", &format!("-{pid}")])
                        .output();
                }

                // Also try normal kill
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        // No temporary files to clean up with in-memory buffer approach
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
        let manager = PowerMetricsManager::new()?;
        *manager_guard = Some(Arc::new(manager));
    }
    Ok(())
}

/// Get the global PowerMetricsManager instance
pub fn get_powermetrics_manager() -> Option<Arc<PowerMetricsManager>> {
    POWERMETRICS_MANAGER.lock().unwrap().clone()
}

/// Shutdown the global PowerMetricsManager
pub fn shutdown_powermetrics_manager() {
    // Debug: shutdown_powermetrics_manager called

    // Take the manager out to ensure Drop is called
    let manager_arc = {
        let mut manager_guard = POWERMETRICS_MANAGER.lock().unwrap();
        manager_guard.take()
    };

    // If we had a manager, clean it up
    if let Some(manager) = manager_arc {
        // Debug: Shutting down PowerMetricsManager

        // Stop the monitoring flag
        if let Ok(mut is_running) = manager.is_running.lock() {
            *is_running = false;
        }

        // Send shutdown command to reader thread
        if let Some(tx) = &manager.command_tx {
            let _ = tx.send(ReaderCommand::Shutdown);
        }

        // Kill the powermetrics process
        if let Ok(mut process_guard) = manager.process.lock() {
            if let Some(mut child) = process_guard.take() {
                #[cfg(unix)]
                {
                    // Kill the entire process group
                    let pid = child.id();
                    // Use negative PID to kill the process group
                    let _ = Command::new("sudo")
                        .args(["kill", "-TERM", &format!("-{pid}")])
                        .output();
                    thread::sleep(Duration::from_millis(100));
                    let _ = Command::new("sudo")
                        .args(["kill", "-9", &format!("-{pid}")])
                        .output();
                }

                let _ = child.kill();
                let _ = child.wait();
            }
        }

        // The Arc will be dropped when this function ends
    }

    // Extra cleanup to catch any orphaned processes
    PowerMetricsManager::kill_existing_powermetrics_processes();

    // Give Drop a moment to execute
    thread::sleep(Duration::from_millis(100));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_powermetrics_data_cache() {
        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
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

        // Skip this test if not running as root (required for powermetrics)
        if std::env::var("USER").unwrap_or_default() != "root" {
            eprintln!("Skipping test that requires root privileges");
            return;
        }

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
    fn test_get_latest_data_from_buffer() {
        // Create test powermetrics output that matches expected format
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

        let manager = PowerMetricsManager {
            process: Arc::new(Mutex::new(None)),
            data_buffer: Arc::new(Mutex::new(VecDeque::new())),
            command_tx: None,
            last_data: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        };

        // Add test data to buffer
        {
            let mut buffer = manager.data_buffer.lock().unwrap();
            buffer.push_back(test_output.to_string());
        }

        // Get data from buffer
        let result = manager.get_latest_data_result();

        // Check that data was parsed
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.e_cluster_frequency, 1020);
        assert_eq!(data.e_cluster_active_residency, 25.5);
        assert_eq!(data.p_cluster_frequency, 3000);
        assert_eq!(data.p_cluster_active_residency, 75.5);
    }

    #[test]
    fn test_circular_buffer_limits() {
        let buffer = Arc::new(Mutex::new(VecDeque::with_capacity(120)));

        // Add more than capacity
        for i in 0..150 {
            let mut buf = buffer.lock().unwrap();
            if buf.len() >= 120 {
                buf.pop_front();
            }
            buf.push_back(format!("Sample {i}"));
        }

        // Should only have last 120 samples
        let buf = buffer.lock().unwrap();
        assert_eq!(buf.len(), 120);
        assert_eq!(buf.front().unwrap(), "Sample 30");
        assert_eq!(buf.back().unwrap(), "Sample 149");
    }
}
