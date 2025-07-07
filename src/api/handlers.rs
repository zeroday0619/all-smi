use axum::extract::State;
use std::sync::Arc;
use sysinfo::Disks;
use tokio::sync::RwLock;

use crate::app_state::AppState;
use crate::utils::disk::should_include_disk;
use crate::utils::system::get_hostname;

pub type SharedState = Arc<RwLock<AppState>>;

pub async fn metrics_handler(State(state): State<SharedState>) -> String {
    let state = state.read().await;
    let mut metrics = String::new();

    for (i, info) in state.gpu_info.iter().enumerate() {
        metrics.push_str("# HELP all_smi_gpu_utilization GPU utilization percentage\n");
        metrics.push_str("# TYPE all_smi_gpu_utilization gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.utilization
        ));

        metrics.push_str("# HELP all_smi_gpu_memory_used_bytes GPU memory used in bytes\n");
        metrics.push_str("# TYPE all_smi_gpu_memory_used_bytes gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_memory_used_bytes{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.used_memory
        ));

        metrics.push_str("# HELP all_smi_gpu_memory_total_bytes GPU memory total in bytes\n");
        metrics.push_str("# TYPE all_smi_gpu_memory_total_bytes gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_memory_total_bytes{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.total_memory
        ));

        metrics.push_str("# HELP all_smi_gpu_temperature_celsius GPU temperature in celsius\n");
        metrics.push_str("# TYPE all_smi_gpu_temperature_celsius gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_temperature_celsius{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.temperature
        ));

        metrics.push_str(
            "# HELP all_smi_gpu_power_consumption_watts GPU power consumption in watts\n",
        );
        metrics.push_str("# TYPE all_smi_gpu_power_consumption_watts gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_power_consumption_watts{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.power_consumption
        ));

        metrics.push_str("# HELP all_smi_gpu_frequency_mhz GPU frequency in MHz\n");
        metrics.push_str("# TYPE all_smi_gpu_frequency_mhz gauge\n");
        metrics.push_str(&format!(
            "all_smi_gpu_frequency_mhz{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.frequency
        ));

        metrics.push_str("# HELP all_smi_ane_utilization ANE utilization in watts\n");
        metrics.push_str("# TYPE all_smi_ane_utilization gauge\n");
        metrics.push_str(&format!(
            "all_smi_ane_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name,
            info.instance,
            info.uuid,
            i,
            info.ane_utilization
        ));

        if let Some(dla_util) = info.dla_utilization {
            metrics.push_str("# HELP all_smi_dla_utilization DLA utilization percentage\n");
            metrics.push_str("# TYPE all_smi_dla_utilization gauge\n");
            metrics.push_str(&format!(
                "all_smi_dla_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
                info.name, info.instance, info.uuid, i, dla_util
            ));
        }
    }

    if !state.process_info.is_empty() {
        metrics.push_str("# HELP all_smi_process_memory_used_bytes Process memory used in bytes\n");
        metrics.push_str("# TYPE all_smi_process_memory_used_bytes gauge\n");
        for process in &state.process_info {
            metrics.push_str(&format!(
                "all_smi_process_memory_used_bytes{{pid=\"{}\", name=\"{}\", device_id=\"{}\", device_uuid=\"{}\"}} {}\n",
                process.pid, process.process_name, process.device_id, process.device_uuid, process.used_memory
            ));
        }
    }

    // CPU metrics
    if !state.cpu_info.is_empty() {
        for (i, cpu_info) in state.cpu_info.iter().enumerate() {
            // CPU utilization
            metrics.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
            metrics.push_str("# TYPE all_smi_cpu_utilization gauge\n");
            metrics.push_str(&format!(
                "all_smi_cpu_utilization{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, cpu_info.utilization
            ));

            // CPU socket count
            metrics.push_str("# HELP all_smi_cpu_socket_count Number of CPU sockets\n");
            metrics.push_str("# TYPE all_smi_cpu_socket_count gauge\n");
            metrics.push_str(&format!(
                "all_smi_cpu_socket_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, cpu_info.socket_count
            ));

            // CPU core count
            metrics.push_str("# HELP all_smi_cpu_core_count Total number of CPU cores\n");
            metrics.push_str("# TYPE all_smi_cpu_core_count gauge\n");
            metrics.push_str(&format!(
                "all_smi_cpu_core_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, cpu_info.total_cores
            ));

            // CPU thread count
            metrics.push_str("# HELP all_smi_cpu_thread_count Total number of CPU threads\n");
            metrics.push_str("# TYPE all_smi_cpu_thread_count gauge\n");
            metrics.push_str(&format!(
                "all_smi_cpu_thread_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, cpu_info.total_threads
            ));

            // CPU frequency
            metrics.push_str("# HELP all_smi_cpu_frequency_mhz CPU frequency in MHz\n");
            metrics.push_str("# TYPE all_smi_cpu_frequency_mhz gauge\n");
            metrics.push_str(&format!(
                "all_smi_cpu_frequency_mhz{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, cpu_info.base_frequency_mhz
            ));

            // CPU temperature (if available)
            if let Some(temp) = cpu_info.temperature {
                metrics.push_str(
                    "# HELP all_smi_cpu_temperature_celsius CPU temperature in celsius\n",
                );
                metrics.push_str("# TYPE all_smi_cpu_temperature_celsius gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_temperature_celsius{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, temp
                ));
            }

            // CPU power consumption (if available)
            if let Some(power) = cpu_info.power_consumption {
                metrics.push_str(
                    "# HELP all_smi_cpu_power_consumption_watts CPU power consumption in watts\n",
                );
                metrics.push_str("# TYPE all_smi_cpu_power_consumption_watts gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_power_consumption_watts{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, power
                ));
            }

            // Per-socket metrics
            for socket_info in &cpu_info.per_socket_info {
                metrics.push_str(
                    "# HELP all_smi_cpu_socket_utilization Per-socket CPU utilization percentage\n",
                );
                metrics.push_str("# TYPE all_smi_cpu_socket_utilization gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_socket_utilization{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", cpu_index=\"{}\", socket_id=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, socket_info.socket_id, socket_info.utilization
                ));

                metrics.push_str(
                    "# HELP all_smi_cpu_socket_frequency_mhz Per-socket CPU frequency in MHz\n",
                );
                metrics.push_str("# TYPE all_smi_cpu_socket_frequency_mhz gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_socket_frequency_mhz{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", cpu_index=\"{}\", socket_id=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, socket_info.socket_id, socket_info.frequency_mhz
                ));

                if let Some(socket_temp) = socket_info.temperature {
                    metrics.push_str("# HELP all_smi_cpu_socket_temperature_celsius Per-socket CPU temperature in celsius\n");
                    metrics.push_str("# TYPE all_smi_cpu_socket_temperature_celsius gauge\n");
                    metrics.push_str(&format!(
                        "all_smi_cpu_socket_temperature_celsius{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", cpu_index=\"{}\", socket_id=\"{}\"}} {}\n",
                        cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, socket_info.socket_id, socket_temp
                    ));
                }
            }

            // Apple Silicon specific metrics
            if let Some(apple_info) = &cpu_info.apple_silicon_info {
                metrics.push_str("# HELP all_smi_cpu_p_core_count Apple Silicon P-core count\n");
                metrics.push_str("# TYPE all_smi_cpu_p_core_count gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_p_core_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, apple_info.p_core_count
                ));

                metrics.push_str("# HELP all_smi_cpu_e_core_count Apple Silicon E-core count\n");
                metrics.push_str("# TYPE all_smi_cpu_e_core_count gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_e_core_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, apple_info.e_core_count
                ));

                metrics
                    .push_str("# HELP all_smi_cpu_gpu_core_count Apple Silicon GPU core count\n");
                metrics.push_str("# TYPE all_smi_cpu_gpu_core_count gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_gpu_core_count{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, apple_info.gpu_core_count
                ));

                metrics.push_str("# HELP all_smi_cpu_p_core_utilization Apple Silicon P-core utilization percentage\n");
                metrics.push_str("# TYPE all_smi_cpu_p_core_utilization gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_p_core_utilization{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, apple_info.p_core_utilization
                ));

                metrics.push_str("# HELP all_smi_cpu_e_core_utilization Apple Silicon E-core utilization percentage\n");
                metrics.push_str("# TYPE all_smi_cpu_e_core_utilization gauge\n");
                metrics.push_str(&format!(
                    "all_smi_cpu_e_core_utilization{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, apple_info.e_core_utilization
                ));

                if let Some(ane_ops) = apple_info.ane_ops_per_second {
                    metrics.push_str("# HELP all_smi_cpu_ane_ops_per_second Apple Neural Engine operations per second\n");
                    metrics.push_str("# TYPE all_smi_cpu_ane_ops_per_second gauge\n");
                    metrics.push_str(&format!(
                        "all_smi_cpu_ane_ops_per_second{{cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                        cpu_info.cpu_model, cpu_info.instance, cpu_info.hostname, i, ane_ops
                    ));
                }
            }
        }
    }

    // Memory metrics
    if !state.memory_info.is_empty() {
        for (i, memory_info) in state.memory_info.iter().enumerate() {
            // Total memory
            metrics.push_str("# HELP all_smi_memory_total_bytes Total system memory in bytes\n");
            metrics.push_str("# TYPE all_smi_memory_total_bytes gauge\n");
            metrics.push_str(&format!(
                "all_smi_memory_total_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                memory_info.instance, memory_info.hostname, i, memory_info.total_bytes
            ));

            // Used memory
            metrics.push_str("# HELP all_smi_memory_used_bytes Used system memory in bytes\n");
            metrics.push_str("# TYPE all_smi_memory_used_bytes gauge\n");
            metrics.push_str(&format!(
                "all_smi_memory_used_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                memory_info.instance, memory_info.hostname, i, memory_info.used_bytes
            ));

            // Available memory
            metrics.push_str(
                "# HELP all_smi_memory_available_bytes Available system memory in bytes\n",
            );
            metrics.push_str("# TYPE all_smi_memory_available_bytes gauge\n");
            metrics.push_str(&format!(
                "all_smi_memory_available_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                memory_info.instance, memory_info.hostname, i, memory_info.available_bytes
            ));

            // Free memory
            metrics.push_str("# HELP all_smi_memory_free_bytes Free system memory in bytes\n");
            metrics.push_str("# TYPE all_smi_memory_free_bytes gauge\n");
            metrics.push_str(&format!(
                "all_smi_memory_free_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                memory_info.instance, memory_info.hostname, i, memory_info.free_bytes
            ));

            // Memory utilization
            metrics.push_str("# HELP all_smi_memory_utilization Memory utilization percentage\n");
            metrics.push_str("# TYPE all_smi_memory_utilization gauge\n");
            metrics.push_str(&format!(
                "all_smi_memory_utilization{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                memory_info.instance, memory_info.hostname, i, memory_info.utilization
            ));

            // Swap metrics if available
            if memory_info.swap_total_bytes > 0 {
                metrics.push_str("# HELP all_smi_swap_total_bytes Total swap space in bytes\n");
                metrics.push_str("# TYPE all_smi_swap_total_bytes gauge\n");
                metrics.push_str(&format!(
                    "all_smi_swap_total_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    memory_info.instance, memory_info.hostname, i, memory_info.swap_total_bytes
                ));

                metrics.push_str("# HELP all_smi_swap_used_bytes Used swap space in bytes\n");
                metrics.push_str("# TYPE all_smi_swap_used_bytes gauge\n");
                metrics.push_str(&format!(
                    "all_smi_swap_used_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    memory_info.instance, memory_info.hostname, i, memory_info.swap_used_bytes
                ));

                metrics.push_str("# HELP all_smi_swap_free_bytes Free swap space in bytes\n");
                metrics.push_str("# TYPE all_smi_swap_free_bytes gauge\n");
                metrics.push_str(&format!(
                    "all_smi_swap_free_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    memory_info.instance, memory_info.hostname, i, memory_info.swap_free_bytes
                ));
            }

            // Linux-specific metrics
            if memory_info.buffers_bytes > 0 {
                metrics.push_str(
                    "# HELP all_smi_memory_buffers_bytes Memory used for buffers in bytes\n",
                );
                metrics.push_str("# TYPE all_smi_memory_buffers_bytes gauge\n");
                metrics.push_str(&format!(
                    "all_smi_memory_buffers_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    memory_info.instance, memory_info.hostname, i, memory_info.buffers_bytes
                ));
            }

            if memory_info.cached_bytes > 0 {
                metrics.push_str(
                    "# HELP all_smi_memory_cached_bytes Memory used for cache in bytes\n",
                );
                metrics.push_str("# TYPE all_smi_memory_cached_bytes gauge\n");
                metrics.push_str(&format!(
                    "all_smi_memory_cached_bytes{{instance=\"{}\", hostname=\"{}\", index=\"{}\"}} {}\n",
                    memory_info.instance, memory_info.hostname, i, memory_info.cached_bytes
                ));
            }
        }
    }

    // Use instance name for disk metrics to ensure consistency with GPU metrics
    let instance = state
        .gpu_info
        .first()
        .map(|info| info.instance.clone())
        .unwrap_or_else(get_hostname);
    let disks = Disks::new_with_refreshed_list();

    for (index, disk) in disks.iter().enumerate() {
        let mount_point_str = disk.mount_point().to_string_lossy();
        if !should_include_disk(&mount_point_str) {
            continue;
        }
        metrics.push_str("# HELP all_smi_disk_total_bytes Total disk space in bytes\n");
        metrics.push_str("# TYPE all_smi_disk_total_bytes gauge\n");
        metrics.push_str(&format!(
            "all_smi_disk_total_bytes{{instance=\"{}\", mount_point=\"{}\", index=\"{}\"}} {}\n",
            instance,
            disk.mount_point().to_string_lossy(),
            index,
            disk.total_space()
        ));

        metrics.push_str("# HELP all_smi_disk_available_bytes Available disk space in bytes\n");
        metrics.push_str("# TYPE all_smi_disk_available_bytes gauge\n");
        metrics.push_str(&format!(
            "all_smi_disk_available_bytes{{instance=\"{}\", mount_point=\"{}\", index=\"{}\"}} {}\n",
            instance,
            disk.mount_point().to_string_lossy(),
            index,
            disk.available_space()
        ));
    }

    metrics
}
