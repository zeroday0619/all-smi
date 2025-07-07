use std::collections::HashMap;

use chrono::Local;
use regex::Regex;

use crate::device::{AppleSiliconCpuInfo, CpuInfo, CpuPlatformType, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;

pub struct MetricsParser;

impl MetricsParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse_metrics(
        &self,
        text: &str,
        host: &str,
        re: &Regex,
    ) -> (
        Vec<GpuInfo>,
        Vec<CpuInfo>,
        Vec<MemoryInfo>,
        Vec<StorageInfo>,
    ) {
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

                let labels = self.parse_labels(labels_str);

                // Extract instance name from the first metric that has it
                if host_instance_name.is_none() {
                    if let Some(instance) = labels.get("instance") {
                        host_instance_name = Some(instance.clone());
                    }
                }

                // Process different metric types
                if metric_name.starts_with("gpu_") || metric_name == "ane_utilization" {
                    self.process_gpu_metrics(&mut gpu_info_map, metric_name, &labels, value, host);
                } else if metric_name.starts_with("cpu_") {
                    self.process_cpu_metrics(&mut cpu_info_map, metric_name, &labels, value, host);
                } else if metric_name.starts_with("memory_") {
                    self.process_memory_metrics(
                        &mut memory_info_map,
                        metric_name,
                        &labels,
                        value,
                        host,
                    );
                } else if metric_name.starts_with("storage_") {
                    self.process_storage_metrics(
                        &mut storage_info_map,
                        metric_name,
                        &labels,
                        value,
                        host,
                    );
                }
            }
        }

        // Update hostnames to use instance name if available
        if let Some(instance_name) = host_instance_name {
            self.update_hostnames(
                &mut gpu_info_map,
                &mut cpu_info_map,
                &mut memory_info_map,
                &mut storage_info_map,
                &instance_name,
            );
        }

        (
            gpu_info_map.into_values().collect(),
            cpu_info_map.into_values().collect(),
            memory_info_map.into_values().collect(),
            storage_info_map.into_values().collect(),
        )
    }

    fn parse_labels(&self, labels_str: &str) -> HashMap<String, String> {
        let mut labels: HashMap<String, String> = HashMap::new();
        for label in labels_str.split(',') {
            let label_parts: Vec<&str> = label.split('=').collect();
            if label_parts.len() == 2 {
                let key = label_parts[0].trim().to_string();
                let value = label_parts[1].replace('"', "").to_string();
                labels.insert(key, value);
            }
        }
        labels
    }

    fn process_gpu_metrics(
        &self,
        gpu_info_map: &mut HashMap<String, GpuInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        let gpu_name = labels.get("gpu").cloned().unwrap_or_default();
        let gpu_uuid = labels.get("uuid").cloned().unwrap_or_default();
        let gpu_index = labels.get("index").cloned().unwrap_or_default();

        if gpu_name.is_empty() || gpu_uuid.is_empty() {
            return;
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
            "gpu_utilization" => gpu_info.utilization = value,
            "gpu_memory_used_bytes" => gpu_info.used_memory = value as u64,
            "gpu_memory_total_bytes" => gpu_info.total_memory = value as u64,
            "gpu_temperature_celsius" => gpu_info.temperature = value as u32,
            "gpu_power_consumption_watts" => gpu_info.power_consumption = value,
            "gpu_frequency_mhz" => gpu_info.frequency = value as u32,
            "ane_utilization" => gpu_info.ane_utilization = value,
            _ => {}
        }
    }

    fn process_cpu_metrics(
        &self,
        cpu_info_map: &mut HashMap<String, CpuInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        let cpu_model = labels.get("cpu_model").cloned().unwrap_or_default();
        let hostname = host.split(':').next().unwrap_or_default().to_string();
        let cpu_index = labels.get("index").cloned().unwrap_or("0".to_string());

        let cpu_key = format!("{host}:{cpu_index}");

        let cpu_info = cpu_info_map.entry(cpu_key).or_insert_with(|| {
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
                architecture: "".to_string(),
                platform_type,
                socket_count: 1,
                total_cores: 0,
                total_threads: 0,
                base_frequency_mhz: 0,
                max_frequency_mhz: 0,
                cache_size_mb: 0,
                utilization: 0.0,
                temperature: None,
                power_consumption: None,
                per_socket_info: Vec::new(),
                apple_silicon_info: None,
                time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            }
        });

        match metric_name {
            "cpu_utilization" => cpu_info.utilization = value,
            "cpu_socket_count" => cpu_info.socket_count = value as u32,
            "cpu_core_count" => cpu_info.total_cores = value as u32,
            "cpu_thread_count" => cpu_info.total_threads = value as u32,
            "cpu_frequency_mhz" => {
                cpu_info.base_frequency_mhz = value as u32;
                cpu_info.max_frequency_mhz = value as u32;
            }
            "cpu_temperature_celsius" => cpu_info.temperature = Some(value as u32),
            "cpu_power_consumption_watts" => cpu_info.power_consumption = Some(value),
            "cpu_p_core_count" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.p_core_count = value as u32;
                }
            }
            "cpu_e_core_count" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.e_core_count = value as u32;
                }
            }
            "cpu_p_core_utilization" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.p_core_utilization = value;
                }
            }
            "cpu_e_core_utilization" => {
                self.ensure_apple_silicon_info(cpu_info);
                if let Some(ref mut apple_info) = cpu_info.apple_silicon_info {
                    apple_info.e_core_utilization = value;
                }
            }
            _ => {}
        }
    }

    fn process_memory_metrics(
        &self,
        memory_info_map: &mut HashMap<String, MemoryInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
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
            "memory_total_bytes" => memory_info.total_bytes = value as u64,
            "memory_used_bytes" => memory_info.used_bytes = value as u64,
            "memory_available_bytes" => memory_info.available_bytes = value as u64,
            "memory_buffers_bytes" => memory_info.buffers_bytes = value as u64,
            "memory_cached_bytes" => memory_info.cached_bytes = value as u64,
            "memory_utilization" => memory_info.utilization = value,
            _ => {}
        }
    }

    fn process_storage_metrics(
        &self,
        storage_info_map: &mut HashMap<String, StorageInfo>,
        metric_name: &str,
        labels: &HashMap<String, String>,
        value: f64,
        host: &str,
    ) {
        let hostname = host.split(':').next().unwrap_or_default().to_string();
        let mount_point = labels.get("mount_point").cloned().unwrap_or_default();
        let storage_index = labels.get("index").cloned().unwrap_or("0".to_string());

        if mount_point.is_empty() {
            return;
        }

        let storage_key = format!("{host}:{mount_point}");
        let storage_info = storage_info_map
            .entry(storage_key)
            .or_insert_with(|| StorageInfo {
                hostname: hostname.clone(),
                mount_point: mount_point.clone(),
                total_bytes: 0,
                available_bytes: 0,
                index: storage_index.parse().unwrap_or(0),
            });

        match metric_name {
            "storage_total_bytes" => storage_info.total_bytes = value as u64,
            "storage_available_bytes" => storage_info.available_bytes = value as u64,
            _ => {}
        }
    }

    fn ensure_apple_silicon_info(&self, cpu_info: &mut CpuInfo) {
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
    }

    fn update_hostnames(
        &self,
        gpu_info_map: &mut HashMap<String, GpuInfo>,
        cpu_info_map: &mut HashMap<String, CpuInfo>,
        memory_info_map: &mut HashMap<String, MemoryInfo>,
        storage_info_map: &mut HashMap<String, StorageInfo>,
        instance_name: &str,
    ) {
        for gpu_info in gpu_info_map.values_mut() {
            gpu_info.hostname = instance_name.to_string();
        }
        for cpu_info in cpu_info_map.values_mut() {
            cpu_info.hostname = instance_name.to_string();
        }
        for memory_info in memory_info_map.values_mut() {
            memory_info.hostname = instance_name.to_string();
        }
        for storage_info in storage_info_map.values_mut() {
            storage_info.hostname = instance_name.to_string();
        }
    }
}

impl Default for MetricsParser {
    fn default() -> Self {
        Self::new()
    }
}
