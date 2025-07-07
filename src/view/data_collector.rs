use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use regex::Regex;
use sysinfo::Disks;
use tokio::sync::Mutex;

use crate::app_state::AppState;
use crate::cli::ViewArgs;
use crate::common::config::{AppConfig, EnvConfig};
use crate::device::{
    get_cpu_readers, get_gpu_readers, get_memory_readers, get_nvml_status_message, CpuInfo,
    GpuInfo, MemoryInfo, ProcessInfo,
};
use crate::network::NetworkClient;
use crate::storage::info::StorageInfo;
use crate::utils::{get_hostname, should_include_disk};

pub struct DataCollector {
    app_state: Arc<Mutex<AppState>>,
}

impl DataCollector {
    pub fn new(app_state: Arc<Mutex<AppState>>) -> Self {
        Self { app_state }
    }

    pub async fn run_local_mode(&self, args: ViewArgs) {
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
            let all_storage_info = self.collect_local_storage_info().await;

            self.update_local_state(
                all_gpu_info,
                all_cpu_info,
                all_memory_info,
                all_processes,
                all_storage_info,
            )
            .await;

            // Use adaptive interval for local mode
            let interval = args
                .interval
                .unwrap_or_else(|| EnvConfig::adaptive_interval(1));
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    }

    pub async fn run_remote_mode(
        &self,
        args: ViewArgs,
        mut hosts: Vec<String>,
        hostfile: Option<String>,
    ) {
        // Load hosts from file if specified
        if let Some(file_path) = hostfile {
            hosts = self.load_hosts_from_file(hosts, file_path).await;
        }

        let client = self.create_http_client();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(std::cmp::min(hosts.len(), 64)));
        let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();

        loop {
            let (all_gpu_info, all_cpu_info, all_memory_info, all_storage_info) = self
                .fetch_remote_data(&hosts, &client, &semaphore, &re)
                .await;

            self.update_remote_state(
                all_gpu_info,
                all_cpu_info,
                all_memory_info,
                all_storage_info,
            )
            .await;

            // Use adaptive interval for remote mode based on node count
            let interval = args
                .interval
                .unwrap_or_else(|| EnvConfig::adaptive_interval(hosts.len()));
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    }

    async fn collect_local_storage_info(&self) -> Vec<StorageInfo> {
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

        all_storage_info
    }

    async fn load_hosts_from_file(&self, mut hosts: Vec<String>, file_path: String) -> Vec<String> {
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
        hosts
    }

    fn create_http_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_idle_timeout(Duration::from_secs(60))
            .pool_max_idle_per_host(200)
            .tcp_keepalive(Duration::from_secs(30))
            .http2_keep_alive_interval(Duration::from_secs(30))
            .build()
            .unwrap()
    }

    async fn update_local_state(
        &self,
        all_gpu_info: Vec<GpuInfo>,
        all_cpu_info: Vec<CpuInfo>,
        all_memory_info: Vec<MemoryInfo>,
        all_processes: Vec<ProcessInfo>,
        all_storage_info: Vec<StorageInfo>,
    ) {
        let mut state = self.app_state.lock().await;

        // Update GPU info with UUID matching
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
            if !state.nvml_notification_shown {
                if let Err(e) = state.notifications.warning(nvml_message) {
                    eprintln!("Failed to show NVML notification: {e}");
                }
                state.nvml_notification_shown = true;
            }
        }

        // Update utilization history
        self.update_utilization_history(&mut state);

        // Update tabs
        self.update_tabs(&mut state);

        // Always clear loading state in local mode after first iteration
        state.loading = false;
    }

    async fn update_remote_state(
        &self,
        all_gpu_info: Vec<GpuInfo>,
        all_cpu_info: Vec<CpuInfo>,
        all_memory_info: Vec<MemoryInfo>,
        all_storage_info: Vec<StorageInfo>,
    ) {
        // Deduplicate storage info by instance and mount_point
        let mut deduplicated_storage: HashMap<String, StorageInfo> = HashMap::new();
        for storage in all_storage_info {
            let dedup_key = format!("{}:{}", storage.hostname, storage.mount_point);
            deduplicated_storage.insert(dedup_key, storage);
        }
        let final_storage_info: Vec<StorageInfo> = deduplicated_storage.into_values().collect();

        let mut state = self.app_state.lock().await;
        state.gpu_info = all_gpu_info;
        state.cpu_info = all_cpu_info;
        state.memory_info = all_memory_info;
        state.storage_info = final_storage_info;

        // Update utilization history
        self.update_utilization_history(&mut state);

        // Update tabs from all device hostnames
        self.update_remote_tabs(&mut state);

        state.process_info = Vec::new(); // No process info in remote mode
        state.loading = false;
    }

    fn update_utilization_history(&self, state: &mut AppState) {
        if !state.gpu_info.is_empty() {
            let avg_utilization = state
                .gpu_info
                .iter()
                .map(|gpu| gpu.utilization)
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            let avg_memory = state
                .gpu_info
                .iter()
                .map(|gpu| {
                    if gpu.total_memory > 0 {
                        (gpu.used_memory as f64 / gpu.total_memory as f64) * 100.0
                    } else {
                        0.0
                    }
                })
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            let avg_temperature = state
                .gpu_info
                .iter()
                .map(|gpu| gpu.temperature as f64)
                .sum::<f64>()
                / state.gpu_info.len() as f64;

            state.utilization_history.push_back(avg_utilization);
            state.memory_history.push_back(avg_memory);
            state.temperature_history.push_back(avg_temperature);

            // Keep only last N entries as configured
            if state.utilization_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.utilization_history.pop_front();
            }
            if state.memory_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.memory_history.pop_front();
            }
            if state.temperature_history.len() > AppConfig::HISTORY_MAX_ENTRIES {
                state.temperature_history.pop_front();
            }
        }
    }

    fn update_tabs(&self, state: &mut AppState) {
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
    }

    fn update_remote_tabs(&self, state: &mut AppState) {
        let mut hostnames: HashSet<String> = HashSet::new();

        // Collect hostnames from all device types
        for info in &state.gpu_info {
            hostnames.insert(info.hostname.clone());
        }
        for info in &state.cpu_info {
            hostnames.insert(info.hostname.clone());
        }
        for info in &state.memory_info {
            hostnames.insert(info.hostname.clone());
        }
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
    }

    async fn fetch_remote_data(
        &self,
        hosts: &[String],
        _client: &reqwest::Client,
        semaphore: &Arc<tokio::sync::Semaphore>,
        re: &Regex,
    ) -> (
        Vec<GpuInfo>,
        Vec<CpuInfo>,
        Vec<MemoryInfo>,
        Vec<StorageInfo>,
    ) {
        let network_client = NetworkClient::new();
        network_client.fetch_remote_data(hosts, semaphore, re).await
    }
}
