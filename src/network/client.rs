use std::sync::Arc;
use std::time::Duration;

use regex::Regex;

use crate::common::config::{AppConfig, EnvConfig};
use crate::device::{CpuInfo, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;

pub struct NetworkClient {
    client: reqwest::Client,
}

impl NetworkClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(AppConfig::CONNECTION_TIMEOUT_SECS))
            .pool_idle_timeout(Duration::from_secs(AppConfig::POOL_IDLE_TIMEOUT_SECS))
            .pool_max_idle_per_host(AppConfig::POOL_MAX_IDLE_PER_HOST)
            .tcp_keepalive(Duration::from_secs(AppConfig::TCP_KEEPALIVE_SECS))
            .http2_keep_alive_interval(Duration::from_secs(AppConfig::HTTP2_KEEPALIVE_SECS))
            .build()
            .unwrap();

        Self { client }
    }

    pub async fn fetch_remote_data(
        &self,
        hosts: &[String],
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
                let client = self.client.clone();
                let host = host.clone();
                let semaphore = semaphore.clone();
                tokio::spawn(async move {
                    // Stagger connection attempts to avoid overwhelming the listen queue
                    let stagger_delay = EnvConfig::connection_stagger_delay(i, total_hosts);
                    tokio::time::sleep(Duration::from_millis(stagger_delay)).await;

                    // Acquire semaphore permit to limit concurrency
                    let _permit = semaphore.acquire().await.unwrap();

                    let url = if host.starts_with("http://") || host.starts_with("https://") {
                        format!("{host}/metrics")
                    } else {
                        format!("http://{host}/metrics")
                    };

                    // Retry logic with exponential backoff
                    for attempt in 1..=AppConfig::RETRY_ATTEMPTS {
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
                                        Some(format!(
                                            "Connection error after {attempt} attempts: {e}"
                                        )),
                                    ));
                                }
                            }
                        }

                        // Exponential backoff
                        tokio::time::sleep(Duration::from_millis(EnvConfig::retry_delay(attempt)))
                            .await;
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

                    let parser = super::metrics_parser::MetricsParser::new();
                    let (gpu_info, cpu_info, memory_info, storage_info) =
                        parser.parse_metrics(&text, &host, re);
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
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}
