use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{stdout, Write};
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use crossterm::{
    cursor,
    event::{self, Event},
    execute, queue,
    style::{Color, Print},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, size, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use regex::Regex;
use sysinfo::Disks;
use tokio::sync::Mutex;

use crate::app_state::{AppState, SortCriteria};
use crate::cli::ViewArgs;
use crate::device::{
    get_cpu_readers, get_gpu_readers, get_memory_readers, get_nvml_status_message, CpuInfo,
    GpuInfo, MemoryInfo, ProcessInfo,
};
use crate::storage::info::StorageInfo;
use crate::ui::buffer::{BufferWriter, DifferentialRenderer};
// use crate::ui::help::print_help_popup; // Not needed with differential rendering
use crate::ui::dashboard::{draw_dashboard_items, draw_system_view};
use crate::ui::renderer::{
    print_cpu_info, print_function_keys, print_gpu_info, print_loading_indicator,
    print_memory_info, print_process_info, print_storage_info,
};
use crate::ui::tabs::draw_tabs;
use crate::ui::text::print_colored_text;
use crate::utils::{calculate_adaptive_interval, get_hostname, should_include_disk};
use crate::view::event_handler::handle_key_event;

pub async fn run_view_mode(args: &ViewArgs) {
    let mut initial_state = AppState::new();
    // Disable loading indicator for remote mode
    let is_remote_mode = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote_mode {
        initial_state.loading = false;
    }

    let app_state = Arc::new(Mutex::new(initial_state));
    let app_state_clone = Arc::clone(&app_state);
    let args_clone = args.clone();

    // Background data collection task
    tokio::spawn(async move {
        let hosts = args_clone.hosts.clone().unwrap_or_default();
        let hostfile = args_clone.hostfile.clone();

        if hosts.is_empty() && hostfile.is_none() {
            // Local mode
            run_local_mode(app_state_clone, args_clone).await;
        } else {
            // Remote mode
            run_remote_mode(app_state_clone, args_clone, hosts, hostfile).await;
        }
    });

    // Main UI loop
    run_ui_loop(app_state, args).await;
}

async fn run_local_mode(app_state: Arc<Mutex<AppState>>, args: ViewArgs) {
    let gpu_readers = get_gpu_readers();
    let cpu_readers = get_cpu_readers();
    let memory_readers = get_memory_readers();
    loop {
        let all_gpu_info: Vec<GpuInfo> = gpu_readers
            .iter()
            .flat_map(|reader| reader.get_gpu_info())
            .collect();

        let all_cpu_info: Vec<CpuInfo> = cpu_readers
            .iter()
            .flat_map(|reader| reader.get_cpu_info())
            .collect();

        let all_memory_info: Vec<MemoryInfo> = memory_readers
            .iter()
            .flat_map(|reader| reader.get_memory_info())
            .collect();

        let all_processes: Vec<ProcessInfo> = gpu_readers
            .iter()
            .flat_map(|reader| reader.get_process_info())
            .collect();

        // Collect local storage information
        let mut all_storage_info = Vec::new();
        let disks = Disks::new_with_refreshed_list();
        let hostname = get_hostname();

        for (index, disk) in disks.iter().enumerate() {
            let mount_point_str = disk.mount_point().to_string_lossy();
            if should_include_disk(&mount_point_str) {
                all_storage_info.push(StorageInfo {
                    mount_point: mount_point_str.to_string(),
                    total_bytes: disk.total_space(),
                    available_bytes: disk.available_space(),
                    hostname: hostname.clone(),
                    index: index as u32,
                });
            }
        }

        let mut state = app_state.lock().await;
        if state.gpu_info.is_empty() {
            state.gpu_info = all_gpu_info;
        } else {
            for new_info in all_gpu_info {
                if let Some(old_info) = state
                    .gpu_info
                    .iter_mut()
                    .find(|info| info.uuid == new_info.uuid)
                {
                    *old_info = new_info;
                }
            }
        }
        state.cpu_info = all_cpu_info;
        state.memory_info = all_memory_info;
        state.process_info = all_processes;
        state.storage_info = all_storage_info;

        // Update notifications (remove expired ones)
        state.notifications.update();

        // Check for NVML status message and show as notification once
        if let Some(nvml_message) = get_nvml_status_message() {
            // Only show notification if we haven't shown it before
            if !state.nvml_notification_shown {
                if let Err(e) = state.notifications.warning(nvml_message) {
                    eprintln!("Failed to show NVML notification: {e}");
                }
                state.nvml_notification_shown = true;
            }
        }

        // Update utilization history
        update_utilization_history(&mut state);

        // Collect unique hostnames
        let mut hostnames: Vec<String> = state
            .gpu_info
            .iter()
            .map(|info| info.hostname.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        hostnames.sort();

        // For single node, skip "All" tab and go directly to node tab
        let mut tabs = if hostnames.len() <= 1 {
            hostnames.clone()
        } else {
            let mut tabs = vec!["All".to_string()];
            tabs.extend(hostnames);
            tabs
        };

        // Ensure we have at least one tab
        if tabs.is_empty() {
            tabs.push("Local".to_string());
        }

        state.tabs = tabs;

        // Always clear loading state in local mode after first iteration
        state.loading = false;

        drop(state);

        // Use adaptive interval for local mode
        let interval = args
            .interval
            .unwrap_or_else(|| calculate_adaptive_interval(1));
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn run_remote_mode(
    app_state: Arc<Mutex<AppState>>,
    args: ViewArgs,
    mut hosts: Vec<String>,
    hostfile: Option<String>,
) {
    // Load hosts from file if specified
    if let Some(file_path) = hostfile {
        if let Ok(content) = fs::read_to_string(&file_path) {
            let file_hosts: Vec<String> = content
                .lines()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .filter(|s| !s.starts_with('#'))
                .map(|s| s.to_string())
                .collect();
            hosts.extend(file_hosts);
        }
    }

    let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(200)
        .tcp_keepalive(Duration::from_secs(30))
        .http2_keep_alive_interval(Duration::from_secs(30))
        .build()
        .unwrap();

    // Create semaphore to limit concurrent connections
    let max_concurrent_connections = std::cmp::min(hosts.len(), 64);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_connections));

    loop {
        let (all_gpu_info, all_cpu_info, all_memory_info, all_storage_info) =
            fetch_remote_data(&hosts, &client, &semaphore, &re).await;

        // Deduplicate storage info by instance and mount_point
        let mut deduplicated_storage: HashMap<String, StorageInfo> = HashMap::new();
        for storage in all_storage_info {
            let dedup_key = format!("{}:{}", storage.hostname, storage.mount_point);
            deduplicated_storage.insert(dedup_key, storage);
        }
        let final_storage_info: Vec<StorageInfo> = deduplicated_storage.into_values().collect();

        let mut state = app_state.lock().await;
        state.gpu_info = all_gpu_info;
        state.cpu_info = all_cpu_info;
        state.memory_info = all_memory_info;
        state.storage_info = final_storage_info;

        // Update utilization history
        update_utilization_history(&mut state);

        let mut hostnames: HashSet<String> = HashSet::new();

        // Collect hostnames from GPU info
        for info in &state.gpu_info {
            hostnames.insert(info.hostname.clone());
        }

        // Collect hostnames from CPU info
        for info in &state.cpu_info {
            hostnames.insert(info.hostname.clone());
        }

        // Collect hostnames from memory info
        for info in &state.memory_info {
            hostnames.insert(info.hostname.clone());
        }

        // Collect hostnames from storage info
        for info in &state.storage_info {
            hostnames.insert(info.hostname.clone());
        }

        let mut sorted_hostnames: Vec<String> = hostnames.into_iter().collect();
        sorted_hostnames.sort();

        // For single node, skip "All" tab and go directly to node tab
        let tabs = if sorted_hostnames.len() <= 1 {
            sorted_hostnames
        } else {
            let mut tabs = vec!["All".to_string()];
            tabs.extend(sorted_hostnames);
            tabs
        };

        state.tabs = tabs;
        state.process_info = Vec::new(); // No process info in remote mode

        // Always clear loading state in remote mode after first iteration
        state.loading = false;

        drop(state);

        // Use adaptive interval for remote mode based on node count
        let interval = args
            .interval
            .unwrap_or_else(|| calculate_adaptive_interval(hosts.len()));
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
}

async fn fetch_remote_data(
    hosts: &[String],
    client: &reqwest::Client,
    semaphore: &Arc<tokio::sync::Semaphore>,
    re: &Regex,
) -> (
    Vec<GpuInfo>,
    Vec<CpuInfo>,
    Vec<MemoryInfo>,
    Vec<StorageInfo>,
) {
    let mut all_gpu_info = Vec::new();
    let mut all_cpu_info = Vec::new();
    let mut all_memory_info = Vec::new();
    let mut all_storage_info = Vec::new();

    // Parallel data collection with concurrency limiting and retries
    let total_hosts = hosts.len();
    let fetch_tasks: Vec<_> = hosts
        .iter()
        .enumerate()
        .map(|(i, host)| {
            let client = client.clone();
            let host = host.clone();
            let semaphore = semaphore.clone();
            tokio::spawn(async move {
                // Stagger connection attempts to avoid overwhelming the listen queue
                let stagger_delay = (i as u64 * 500) / total_hosts as u64;
                tokio::time::sleep(Duration::from_millis(stagger_delay)).await;

                // Acquire semaphore permit to limit concurrency
                let _permit = semaphore.acquire().await.unwrap();

                let url = if host.starts_with("http://") || host.starts_with("https://") {
                    format!("{host}/metrics")
                } else {
                    format!("http://{host}/metrics")
                };

                // Retry logic - 3 attempts with exponential backoff
                for attempt in 1..=3 {
                    match client.get(&url).send().await {
                        Ok(response) => {
                            if response.status().is_success() {
                                match response.text().await {
                                    Ok(text) => return Some((host, text, None)),
                                    Err(e) => {
                                        if attempt == 3 {
                                            return Some((
                                                host,
                                                String::new(),
                                                Some(format!("Text parse error: {e}")),
                                            ));
                                        }
                                    }
                                }
                            } else if attempt == 3 {
                                return Some((
                                    host,
                                    String::new(),
                                    Some(format!("HTTP {}", response.status())),
                                ));
                            }
                        }
                        Err(e) => {
                            if attempt == 3 {
                                return Some((
                                    host,
                                    String::new(),
                                    Some(format!("Connection error after {attempt} attempts: {e}")),
                                ));
                            }
                        }
                    }

                    // Exponential backoff: 50ms, 100ms, 150ms
                    tokio::time::sleep(Duration::from_millis(50 * attempt as u64)).await;
                }

                Some((
                    host,
                    String::new(),
                    Some("All retry attempts failed".to_string()),
                ))
            })
        })
        .collect();

    // Wait for all fetch tasks to complete
    let fetch_results = futures_util::future::join_all(fetch_tasks).await;

    // Process all fetch results with error tracking
    let mut successful_connections = 0;
    let mut failed_connections = 0;
    for task_result in fetch_results {
        match task_result {
            Ok(Some((host, text, error))) => {
                if error.is_some() {
                    failed_connections += 1;
                    continue;
                }
                successful_connections += 1;

                if text.is_empty() {
                    continue;
                }

                let (gpu_info, cpu_info, memory_info, storage_info) =
                    parse_metrics(&text, &host, re);
                all_gpu_info.extend(gpu_info);
                all_cpu_info.extend(cpu_info);
                all_memory_info.extend(memory_info);
                all_storage_info.extend(storage_info);
            }
            Ok(None) => {
                failed_connections += 1;
            }
            Err(_) => {
                failed_connections += 1;
            }
        }
    }

    // Debug logging for connection success rate
    if failed_connections > 0 {
        eprintln!(
            "Connection stats: {successful_connections} successful, {failed_connections} failed out of {total_hosts} total"
        );
    }

    (
        all_gpu_info,
        all_cpu_info,
        all_memory_info,
        all_storage_info,
    )
}

fn parse_metrics(
    text: &str,
    host: &str,
    re: &Regex,
) -> (
    Vec<GpuInfo>,
    Vec<CpuInfo>,
    Vec<MemoryInfo>,
    Vec<StorageInfo>,
) {
    use crate::device::{AppleSiliconCpuInfo, CpuPlatformType, CpuSocketInfo};

    let mut gpu_info_map: HashMap<String, GpuInfo> = HashMap::new();
    let mut cpu_info_map: HashMap<String, CpuInfo> = HashMap::new();
    let mut memory_info_map: HashMap<String, MemoryInfo> = HashMap::new();
    let mut storage_info_map: HashMap<String, StorageInfo> = HashMap::new();
    let mut host_instance_name: Option<String> = None;

    for line in text.lines() {
        if let Some(cap) = re.captures(line.trim()) {
            let metric_name = &cap[1];
            let labels_str = &cap[2];
            let value = cap[3].parse::<f64>().unwrap_or(0.0);

            let mut labels: HashMap<String, String> = HashMap::new();
            for label in labels_str.split(',') {
                let label_parts: Vec<&str> = label.split('=').collect();
                if label_parts.len() == 2 {
                    let key = label_parts[0].trim().to_string();
                    let value = label_parts[1].replace('"', "").to_string();
                    labels.insert(key, value);
                }
            }

            // Extract instance name from the first metric that has it
            if host_instance_name.is_none() {
                if let Some(instance) = labels.get("instance") {
                    host_instance_name = Some(instance.clone());
                }
            }

            // Process GPU metrics
            if metric_name.starts_with("gpu_") || metric_name == "ane_utilization" {
                let gpu_name = labels.get("gpu").cloned().unwrap_or_default();
                let gpu_uuid = labels.get("uuid").cloned().unwrap_or_default();
                let gpu_index = labels.get("index").cloned().unwrap_or_default();

                if gpu_name.is_empty() || gpu_uuid.is_empty() {
                    continue;
                }

                let gpu_info = gpu_info_map.entry(gpu_uuid.clone()).or_insert_with(|| {
                    let mut detail = HashMap::new();
                    detail.insert("index".to_string(), gpu_index.clone());
                    GpuInfo {
                        uuid: gpu_uuid.clone(),
                        time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        name: gpu_name,
                        hostname: host.split(':').next().unwrap_or_default().to_string(),
                        instance: host.to_string(),
                        utilization: 0.0,
                        ane_utilization: 0.0,
                        dla_utilization: None,
                        temperature: 0,
                        used_memory: 0,
                        total_memory: 0,
                        frequency: 0,
                        power_consumption: 0.0,
                        detail,
                    }
                });

                match metric_name {
                    "gpu_utilization" => {
                        gpu_info.utilization = value;
                    }
                    "gpu_memory_used_bytes" => {
                        gpu_info.used_memory = value as u64;
                    }
                    "gpu_memory_total_bytes" => {
                        gpu_info.total_memory = value as u64;
                    }
                    "gpu_temperature_celsius" => {
                        gpu_info.temperature = value as u32;
                    }
                    "gpu_power_consumption_watts" => {
                        gpu_info.power_consumption = value;
                    }
                    "gpu_frequency_mhz" => {
                        gpu_info.frequency = value as u32;
                    }
                    "ane_utilization" => {
                        gpu_info.ane_utilization = value;
                    }
                    _ => {}
                }
            } else if metric_name.starts_with("cpu_") {
                // Handle CPU metrics
                let cpu_model = labels.get("cpu_model").cloned().unwrap_or_default();
                let hostname = host.split(':').next().unwrap_or_default().to_string();
                let cpu_index = labels.get("index").cloned().unwrap_or("0".to_string());

                let cpu_key = format!("{host}:{cpu_index}");

                let cpu_info = cpu_info_map.entry(cpu_key).or_insert_with(|| {
                    // Determine platform type from CPU model
                    let platform_type = if cpu_model.contains("Apple") {
                        CpuPlatformType::AppleSilicon
                    } else if cpu_model.contains("Intel") {
                        CpuPlatformType::Intel
                    } else if cpu_model.contains("AMD") {
                        CpuPlatformType::Amd
                    } else {
                        CpuPlatformType::Other("Unknown".to_string())
                    };

                    CpuInfo {
                        hostname: hostname.clone(),
                        instance: host.to_string(),
                        cpu_model: cpu_model.clone(),
                        architecture: "".to_string(), // Will be filled from metrics if available
                        platform_type,
                        socket_count: 1,             // Will be updated from metrics
                        total_cores: 0,              // Will be updated from metrics
                        total_threads: 0,            // Will be updated from metrics
                        base_frequency_mhz: 0,       // Will be updated from metrics
                        max_frequency_mhz: 0,        // Same as base for now
                        cache_size_mb: 0,            // Not available from metrics
                        utilization: 0.0,            // Will be updated from metrics
                        temperature: None,           // Will be updated from metrics
                        power_consumption: None,     // Will be updated from metrics
                        per_socket_info: Vec::new(), // Will be populated from socket metrics
                        apple_silicon_info: None,    // Will be populated for Apple Silicon
                        time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    }
                });

                match metric_name {
                    "cpu_utilization" => {
                        cpu_info.utilization = value;
                    }
                    "cpu_socket_count" => {
                        cpu_info.socket_count = value as u32;
                    }
                    "cpu_core_count" => {
                        cpu_info.total_cores = value as u32;
                    }
                    "cpu_thread_count" => {
                        cpu_info.total_threads = value as u32;
                    }
                    "cpu_frequency_mhz" => {
                        cpu_info.base_frequency_mhz = value as u32;
                        cpu_info.max_frequency_mhz = value as u32;
                    }
                    "cpu_temperature_celsius" => {
                        cpu_info.temperature = Some(value as u32);
                    }
                    "cpu_power_consumption_watts" => {
                        cpu_info.power_consumption = Some(value);
                    }
                    "cpu_p_core_count" => {
                        // Apple Silicon specific - ensure apple_silicon_info exists
                        if cpu_info.apple_silicon_info.is_none() {
                            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                                p_core_count: 0,
                                e_core_count: 0,
                                gpu_core_count: 0,
                                p_core_utilization: 0.0,
                                e_core_utilization: 0.0,
                                ane_ops_per_second: None,
                            });
                        }
                        if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                            apple_info.p_core_count = value as u32;
                        }
                    }
                    "cpu_e_core_count" => {
                        if cpu_info.apple_silicon_info.is_none() {
                            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                                p_core_count: 0,
                                e_core_count: 0,
                                gpu_core_count: 0,
                                p_core_utilization: 0.0,
                                e_core_utilization: 0.0,
                                ane_ops_per_second: None,
                            });
                        }
                        if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                            apple_info.e_core_count = value as u32;
                        }
                    }
                    "cpu_gpu_core_count" => {
                        if cpu_info.apple_silicon_info.is_none() {
                            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                                p_core_count: 0,
                                e_core_count: 0,
                                gpu_core_count: 0,
                                p_core_utilization: 0.0,
                                e_core_utilization: 0.0,
                                ane_ops_per_second: None,
                            });
                        }
                        if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                            apple_info.gpu_core_count = value as u32;
                        }
                    }
                    "cpu_p_core_utilization" => {
                        if cpu_info.apple_silicon_info.is_none() {
                            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                                p_core_count: 0,
                                e_core_count: 0,
                                gpu_core_count: 0,
                                p_core_utilization: 0.0,
                                e_core_utilization: 0.0,
                                ane_ops_per_second: None,
                            });
                        }
                        if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                            apple_info.p_core_utilization = value;
                        }
                    }
                    "cpu_e_core_utilization" => {
                        if cpu_info.apple_silicon_info.is_none() {
                            cpu_info.apple_silicon_info = Some(AppleSiliconCpuInfo {
                                p_core_count: 0,
                                e_core_count: 0,
                                gpu_core_count: 0,
                                p_core_utilization: 0.0,
                                e_core_utilization: 0.0,
                                ane_ops_per_second: None,
                            });
                        }
                        if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                            apple_info.e_core_utilization = value;
                        }
                    }
                    "cpu_socket_utilization" => {
                        // Per-socket utilization metrics
                        if let Some(socket_id_str) = labels.get("socket_id") {
                            if let Ok(socket_id) = socket_id_str.parse::<u32>() {
                                // Ensure per_socket_info has enough entries
                                while cpu_info.per_socket_info.len() <= socket_id as usize {
                                    cpu_info.per_socket_info.push(CpuSocketInfo {
                                        socket_id: cpu_info.per_socket_info.len() as u32,
                                        utilization: 0.0,
                                        cores: 0,
                                        threads: 0,
                                        temperature: None,
                                        frequency_mhz: 0,
                                    });
                                }
                                if let Some(socket_info) =
                                    cpu_info.per_socket_info.get_mut(socket_id as usize)
                                {
                                    socket_info.utilization = value;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            } else if metric_name.starts_with("memory_") || metric_name.starts_with("swap_") {
                // Handle memory metrics
                let hostname = host.split(':').next().unwrap_or_default().to_string();
                let memory_index = labels.get("index").cloned().unwrap_or("0".to_string());

                let memory_key = format!("{host}:{memory_index}");

                let memory_info = memory_info_map
                    .entry(memory_key)
                    .or_insert_with(|| MemoryInfo {
                        hostname: hostname.clone(),
                        instance: host.to_string(),
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
                        time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    });

                match metric_name {
                    "memory_total_bytes" => {
                        memory_info.total_bytes = value as u64;
                    }
                    "memory_used_bytes" => {
                        memory_info.used_bytes = value as u64;
                    }
                    "memory_available_bytes" => {
                        memory_info.available_bytes = value as u64;
                    }
                    "memory_free_bytes" => {
                        memory_info.free_bytes = value as u64;
                    }
                    "memory_buffers_bytes" => {
                        memory_info.buffers_bytes = value as u64;
                    }
                    "memory_cached_bytes" => {
                        memory_info.cached_bytes = value as u64;
                    }
                    "memory_utilization" => {
                        memory_info.utilization = value;
                    }
                    "swap_total_bytes" => {
                        memory_info.swap_total_bytes = value as u64;
                    }
                    "swap_used_bytes" => {
                        memory_info.swap_used_bytes = value as u64;
                    }
                    "swap_free_bytes" => {
                        memory_info.swap_free_bytes = value as u64;
                    }
                    _ => {}
                }
            } else if metric_name.starts_with("disk_") {
                // Handle disk metrics
                let mount_point = labels.get("mount_point").cloned().unwrap_or_default();
                let hostname = host.split(':').next().unwrap_or_default().to_string();
                let index = labels
                    .get("index")
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);

                let storage_key = format!("{host}:{mount_point}:{index}");

                match metric_name {
                    "disk_total_bytes" => {
                        let storage_info =
                            storage_info_map.entry(storage_key).or_insert(StorageInfo {
                                mount_point: mount_point.clone(),
                                total_bytes: 0,
                                available_bytes: 0,
                                hostname: hostname.clone(),
                                index,
                            });
                        storage_info.total_bytes = value as u64;
                    }
                    "disk_available_bytes" => {
                        let storage_info =
                            storage_info_map.entry(storage_key).or_insert(StorageInfo {
                                mount_point: mount_point.clone(),
                                total_bytes: 0,
                                available_bytes: 0,
                                hostname: hostname.clone(),
                                index,
                            });
                        storage_info.available_bytes = value as u64;
                    }
                    _ => {}
                }
            }
        }
    }

    // Update all GPU, CPU, and storage entries with the correct instance hostname
    if let Some(instance_name) = host_instance_name {
        // Update GPU hostnames to use instance name
        for gpu_info in gpu_info_map.values_mut() {
            gpu_info.hostname = instance_name.clone();
        }
        // Update CPU hostnames to use instance name
        for cpu_info in cpu_info_map.values_mut() {
            cpu_info.hostname = instance_name.clone();
        }
        // Update memory hostnames to use instance name
        for memory_info in memory_info_map.values_mut() {
            memory_info.hostname = instance_name.clone();
        }
        // Update storage hostnames to use instance name
        for storage_info in storage_info_map.values_mut() {
            storage_info.hostname = instance_name.clone();
        }
    }

    (
        gpu_info_map.into_values().collect(),
        cpu_info_map.into_values().collect(),
        memory_info_map.into_values().collect(),
        storage_info_map.into_values().collect(),
    )
}

async fn run_ui_loop(app_state: Arc<Mutex<AppState>>, args: &ViewArgs) {
    let mut stdout = stdout();
    if enable_raw_mode().is_err() {
        eprintln!("Failed to enable raw mode - terminal not available");
        return;
    }
    if execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .is_err()
    {
        eprintln!("Failed to initialize terminal display");
        let _ = disable_raw_mode();
        return;
    }

    // Create differential renderer to eliminate flickering
    let mut differential_renderer = match DifferentialRenderer::new() {
        Ok(renderer) => renderer,
        Err(_) => {
            eprintln!("Failed to create differential renderer");
            let _ = disable_raw_mode();
            return;
        }
    };

    // Track previous display mode to force clear when switching
    let mut previous_show_help = false;
    let mut previous_loading = false;
    let mut previous_tab = 0;

    loop {
        if let Ok(has_event) = event::poll(Duration::from_millis(50)) {
            if has_event {
                if let Ok(event) = event::read() {
                    match event {
                        Event::Key(key_event) => {
                            let mut state = app_state.lock().await;
                            let should_break = handle_key_event(key_event, &mut state, args).await;
                            if should_break {
                                break;
                            }
                            drop(state);
                        }
                        Event::Mouse(_) => {
                            // Mouse events won't occur since mouse capture is disabled
                        }
                        Event::Resize(_, _) => {
                            // Ignore resize events - the display will adjust automatically
                        }
                        Event::FocusGained | Event::FocusLost => {
                            // Ignore focus events
                        }
                        Event::Paste(_) => {
                            // Ignore paste events
                        }
                    }
                }
            }
        }

        // Update display
        let mut state = app_state.lock().await;
        state.frame_counter += 1;

        // Update scroll offsets for long text
        if state.frame_counter % 2 == 0 {
            update_scroll_offsets(&mut state);
        }

        let (cols, rows) = match size() {
            Ok((c, r)) => (c, r),
            Err(_) => {
                eprintln!("Failed to get terminal size");
                break;
            }
        };
        if queue!(stdout, cursor::Hide).is_err() {
            break;
        }

        // Check if we need to force clear due to mode change or tab change
        let force_clear = state.show_help != previous_show_help
            || state.loading != previous_loading
            || state.current_tab != previous_tab;

        if force_clear && differential_renderer.force_clear().is_err() {
            break;
        }

        // Create content using buffer, then render differentially
        let content = if state.show_help {
            render_help_popup_content(&state, args, cols, rows)
        } else if state.loading {
            let is_remote = args.hosts.is_some() || args.hostfile.is_some();
            render_loading_content(&state, is_remote, cols, rows)
        } else {
            render_main_content(&state, args, cols, rows)
        };

        // Use differential rendering to update only changed lines
        if differential_renderer.render_differential(&content).is_err() {
            break;
        }

        // Update previous state
        previous_show_help = state.show_help;
        previous_loading = state.loading;
        previous_tab = state.current_tab;

        if queue!(stdout, cursor::Show).is_err() {
            break;
        }
        if stdout.flush().is_err() {
            break;
        }
    }

    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = disable_raw_mode();
}

fn update_scroll_offsets(state: &mut AppState) {
    let mut new_device_name_scroll_offsets = state.device_name_scroll_offsets.clone();
    let mut new_hostname_scroll_offsets = state.hostname_scroll_offsets.clone();
    let mut processed_hostnames = HashSet::new();

    for gpu in &state.gpu_info {
        if gpu.name.len() > 15 {
            let offset = new_device_name_scroll_offsets
                .entry(gpu.uuid.clone())
                .or_insert(0);
            *offset = (*offset + 1) % (gpu.name.len() + 3);
        }
        if gpu.hostname.len() > 9 && processed_hostnames.insert(gpu.hostname.clone()) {
            let offset = new_hostname_scroll_offsets
                .entry(gpu.hostname.clone())
                .or_insert(0);
            *offset = (*offset + 1) % (gpu.hostname.len() + 3);
        }
    }
    state.device_name_scroll_offsets = new_device_name_scroll_offsets;
    state.hostname_scroll_offsets = new_hostname_scroll_offsets;
}

fn render_help_popup_content(state: &AppState, args: &ViewArgs, cols: u16, rows: u16) -> String {
    let is_remote = args.hosts.is_some() || args.hostfile.is_some();
    crate::ui::help::generate_help_popup_content(cols, rows, state, is_remote)
}

fn render_loading_content(state: &AppState, is_remote: bool, cols: u16, rows: u16) -> String {
    let mut buffer = BufferWriter::new();
    print_function_keys(&mut buffer, cols, rows, state, is_remote);
    print_loading_indicator(&mut buffer, cols, rows);
    buffer.get_buffer().to_string()
}

fn render_main_content(state: &AppState, args: &ViewArgs, cols: u16, rows: u16) -> String {
    let width = cols as usize;

    // Use double buffering to reduce flickering
    let mut buffer = BufferWriter::new();

    // Write time/date header to buffer first
    let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let version = env!("CARGO_PKG_VERSION");
    let header_text = format!("all-smi - {current_time}");
    let version_text = format!("v{version}");

    // Calculate spacing to right-align version
    let total_width = cols as usize;
    let content_length = header_text.len() + version_text.len();
    let spacing = if total_width > content_length {
        " ".repeat(total_width - content_length)
    } else {
        " ".to_string()
    };

    print_colored_text(
        &mut buffer,
        &format!("{header_text}{spacing}{version_text}\r\n"),
        Color::White,
        None,
        None,
    );

    // Write remaining header content to buffer
    print_colored_text(&mut buffer, "Cluster Overview\r\n", Color::Cyan, None, None);
    draw_system_view(&mut buffer, state, cols);

    draw_dashboard_items(&mut buffer, state, cols);
    draw_tabs(&mut buffer, state, cols);

    let is_remote = args.hosts.is_some() || args.hostfile.is_some();

    let mut gpu_info_to_display: Vec<_> =
        if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] == "All" {
            state.gpu_info.iter().collect()
        } else {
            state
                .gpu_info
                .iter()
                .filter(|info| info.hostname == state.tabs[state.current_tab])
                .collect()
        };

    // Sort GPUs based on current sort criteria
    gpu_info_to_display.sort_by(|a, b| state.sort_criteria.sort_gpus(a, b));

    // Calculate dynamic header size for accurate space allocation
    let header_lines = calculate_header_lines(state);
    let content_start_row = header_lines;
    let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize;

    // Calculate storage display rows
    let storage_items_count = if !state.storage_info.is_empty() {
        if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                state
                    .storage_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .count()
            } else {
                0
            }
        } else {
            // Local mode: show all storage info
            state.storage_info.len()
        }
    } else {
        0
    };

    let storage_display_rows = if storage_items_count > 0 {
        // Each storage item takes 1 line (labels + usage bar on same line)
        storage_items_count + 2 // Extra 2 for headers/padding
    } else {
        0
    };

    // Calculate GPU display area
    let gpu_display_rows = if is_remote {
        if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] == "All" {
            // "All" tab: no storage display, use full available space
            available_rows
        } else {
            // Specific host tab: reserve space for storage if needed
            available_rows.saturating_sub(storage_display_rows)
        }
    } else {
        // In local mode, reserve space for storage if available, then split remaining for GPU/processes
        if state.process_info.is_empty() {
            available_rows.saturating_sub(storage_display_rows)
        } else {
            (available_rows.saturating_sub(storage_display_rows)) / 2
        }
    };

    // Each GPU takes 2 lines (labels + progress bars on same line)
    let lines_per_gpu = 2;
    let max_gpu_items = gpu_display_rows / lines_per_gpu;

    // Render GPU info
    for (index, info) in gpu_info_to_display
        .iter()
        .skip(state.gpu_scroll_offset)
        .take(max_gpu_items)
        .enumerate()
    {
        let device_name_scroll_offset = state
            .device_name_scroll_offsets
            .get(&info.uuid)
            .cloned()
            .unwrap_or(0);
        let hostname_scroll_offset = state
            .hostname_scroll_offsets
            .get(&info.hostname)
            .cloned()
            .unwrap_or(0);
        print_gpu_info(
            &mut buffer,
            index,
            info,
            width,
            device_name_scroll_offset,
            hostname_scroll_offset,
        );
    }

    // Display CPU information for remote mode (node-specific tabs) and local mode
    if !state.cpu_info.is_empty() {
        let cpu_info_to_display: Vec<_> = if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                state
                    .cpu_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            // Local mode: show all CPU info
            state.cpu_info.iter().collect()
        };

        if !cpu_info_to_display.is_empty() {
            queue!(buffer, Print("\r\n")).unwrap();
            for (index, info) in cpu_info_to_display.iter().enumerate() {
                print_cpu_info(&mut buffer, index, info, width);
                if index < cpu_info_to_display.len() - 1 {
                    queue!(buffer, Print("\r\n")).unwrap();
                }
            }
        }
    }

    // Display memory information for remote mode (node-specific tabs) and local mode
    if !state.memory_info.is_empty() {
        let memory_info_to_display: Vec<_> = if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                state
                    .memory_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            // Local mode: show all memory info
            state.memory_info.iter().collect()
        };

        if !memory_info_to_display.is_empty() {
            queue!(buffer, Print("\r\n")).unwrap();
            for (index, info) in memory_info_to_display.iter().enumerate() {
                print_memory_info(&mut buffer, index, info, width);
                if index < memory_info_to_display.len() - 1 {
                    queue!(buffer, Print("\r\n")).unwrap();
                }
            }
        }
    }

    // Display storage information for remote mode (node-specific tabs) and local mode
    if !state.storage_info.is_empty() {
        let storage_info_to_display: Vec<_> = if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                state
                    .storage_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            // Local mode: show all storage info
            state.storage_info.iter().collect()
        };

        if !storage_info_to_display.is_empty() {
            queue!(buffer, Print("\r\n")).unwrap();
            let mut sorted_storage: Vec<_> = storage_info_to_display.clone();
            sorted_storage.sort_by(|a, b| {
                a.hostname
                    .cmp(&b.hostname)
                    .then_with(|| a.index.cmp(&b.index))
                    .then_with(|| a.mount_point.cmp(&b.mount_point))
            });

            let remaining_rows = available_rows.saturating_sub(gpu_display_rows);
            for (index, info) in sorted_storage
                .iter()
                .skip(state.storage_scroll_offset)
                .take(remaining_rows.saturating_sub(2))
                .enumerate()
            {
                print_storage_info(&mut buffer, index, info, width);
                if index < sorted_storage.len() - 1 {
                    queue!(buffer, Print("\r\n")).unwrap();
                }
            }
        }
    }

    // Calculate actual rows used by other sections to determine remaining space for processes
    let mut used_rows = 0;

    // GPU section rows
    let actual_gpu_items = std::cmp::min(
        gpu_info_to_display
            .len()
            .saturating_sub(state.gpu_scroll_offset),
        max_gpu_items,
    );
    used_rows += actual_gpu_items * lines_per_gpu;

    // CPU section rows (if displayed in remote mode or local mode)
    if !state.cpu_info.is_empty() {
        if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                let cpu_info_to_display: Vec<_> = state
                    .cpu_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect();
                if !cpu_info_to_display.is_empty() {
                    used_rows += cpu_info_to_display.len() * 2; // Each CPU takes 2 lines (labels + gauges)
                    used_rows += 1; // Extra line for spacing between sections
                }
            }
        } else {
            // Local mode: count all CPU rows
            used_rows += state.cpu_info.len() * 2; // Each CPU takes 2 lines
            used_rows += 1; // Extra line for spacing between sections
        }
    }

    // Memory section rows (if displayed in remote mode or local mode)
    if !state.memory_info.is_empty() {
        if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                let current_hostname = &state.tabs[state.current_tab];
                let memory_info_to_display: Vec<_> = state
                    .memory_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect();
                if !memory_info_to_display.is_empty() {
                    used_rows += memory_info_to_display.len() * 2; // Each memory section takes 2 lines
                    used_rows += 1; // Extra line for spacing between sections
                }
            }
        } else {
            // Local mode: count all memory rows
            used_rows += state.memory_info.len() * 2; // Each memory section takes 2 lines
            used_rows += 1; // Extra line for spacing between sections
        }
    }

    // Storage section rows (if displayed in remote mode or local mode)
    if !state.storage_info.is_empty() {
        if is_remote {
            if state.current_tab < state.tabs.len() && state.tabs[state.current_tab] != "All" {
                used_rows += storage_display_rows;
                used_rows += 1; // Extra line for spacing between sections
            }
        } else {
            // Local mode: always count storage rows if storage info exists
            used_rows += storage_display_rows;
            used_rows += 1; // Extra line for spacing between sections
        }
    }

    // Display process info for local mode
    if !state.process_info.is_empty() && !is_remote {
        let mut sorted_process_info = state.process_info.clone();
        sorted_process_info.sort_by(|a, b| state.sort_criteria.sort_processes(a, b));

        // Calculate remaining available rows for processes
        let remaining_rows = available_rows.saturating_sub(used_rows);
        let process_rows = std::cmp::max(remaining_rows, 10) as u16; // Minimum 10 rows for processes

        print_process_info(
            &mut buffer,
            &sorted_process_info,
            state.selected_process_index,
            state.start_index,
            process_rows,
            cols,
        );
    }

    // Add function keys to buffer
    print_function_keys(&mut buffer, cols, rows, state, is_remote);

    // Return the complete buffer content for differential rendering
    buffer.get_buffer().to_string()
}

fn update_utilization_history(state: &mut AppState) {
    const MAX_HISTORY_SIZE: usize = 60; // Keep last 60 data points

    if state.gpu_info.is_empty() {
        return;
    }

    // Calculate cluster-wide averages
    let total_gpus = state.gpu_info.len() as f64;
    let avg_utilization = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.utilization)
        .sum::<f64>()
        / total_gpus;

    let total_memory: u64 = state.gpu_info.iter().map(|gpu| gpu.total_memory).sum();
    let used_memory: u64 = state.gpu_info.iter().map(|gpu| gpu.used_memory).sum();
    let memory_percent = if total_memory > 0 {
        (used_memory as f64 / total_memory as f64) * 100.0
    } else {
        0.0
    };

    let avg_temperature = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.temperature as f64)
        .sum::<f64>()
        / total_gpus;

    // Add to history
    state.utilization_history.push_back(avg_utilization);
    state.memory_history.push_back(memory_percent);
    state.temperature_history.push_back(avg_temperature);

    // Keep history size manageable
    if state.utilization_history.len() > MAX_HISTORY_SIZE {
        state.utilization_history.pop_front();
    }
    if state.memory_history.len() > MAX_HISTORY_SIZE {
        state.memory_history.pop_front();
    }
    if state.temperature_history.len() > MAX_HISTORY_SIZE {
        state.temperature_history.pop_front();
    }
}

impl SortCriteria {
    fn sort_processes(&self, a: &ProcessInfo, b: &ProcessInfo) -> std::cmp::Ordering {
        match self {
            SortCriteria::Pid => a.pid.cmp(&b.pid),
            SortCriteria::Memory => b.used_memory.cmp(&a.used_memory),
            _ => a.pid.cmp(&b.pid), // Default to PID for non-process sorting
        }
    }

    fn sort_gpus(&self, a: &GpuInfo, b: &GpuInfo) -> std::cmp::Ordering {
        match self {
            SortCriteria::Default => {
                // Sort by hostname first, then by index (original behavior)
                a.hostname.cmp(&b.hostname).then_with(|| {
                    let a_index = a
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    let b_index = b
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    a_index.cmp(&b_index)
                })
            }
            SortCriteria::Utilization => {
                // Sort by utilization (descending), then by hostname and index
                b.utilization
                    .partial_cmp(&a.utilization)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::GpuMemory => {
                // Sort by memory usage (descending), then by hostname and index
                b.used_memory
                    .cmp(&a.used_memory)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::Power => {
                // Sort by power consumption (descending), then by hostname and index
                b.power_consumption
                    .partial_cmp(&a.power_consumption)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::Temperature => {
                // Sort by temperature (descending), then by hostname and index
                b.temperature
                    .cmp(&a.temperature)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            _ => {
                // For process sorting criteria, fall back to default GPU sorting
                self.sort_gpus_default(a, b)
            }
        }
    }

    fn sort_gpus_default(&self, a: &GpuInfo, b: &GpuInfo) -> std::cmp::Ordering {
        a.hostname.cmp(&b.hostname).then_with(|| {
            let a_index = a
                .detail
                .get("index")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            let b_index = b
                .detail
                .get("index")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0);
            a_index.cmp(&b_index)
        })
    }
}

fn calculate_header_lines(state: &AppState) -> u16 {
    // Calculate the actual number of lines used by the header content
    let mut lines = 0u16;

    // Line 1: "all-smi - {timestamp}"
    lines += 1;

    // Line 2: "Cluster Overview"
    lines += 1;

    // Lines 3-6: draw_system_view (2 dashboard rows, each takes 2 lines)
    lines += 4;

    // Line 7: Separator line in draw_dashboard_items
    lines += 1;

    // Lines 8-12: draw_utilization_history
    // This shows live statistics with node view and history gauges
    if !state.utilization_history.is_empty() {
        lines += 5; // Header + node view + history display
    } else {
        lines += 1; // Just header line when no history
    }

    // Line N: draw_tabs (tabs line + separator)
    lines += 2;

    lines
}
