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
        metrics.push_str(&format!(
            "# HELP all_smi_gpu_utilization GPU utilization percentage\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_utilization gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.utilization
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_memory_used_bytes GPU memory used in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_memory_used_bytes gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_memory_used_bytes{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.used_memory
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_memory_total_bytes GPU memory total in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_memory_total_bytes gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_memory_total_bytes{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.total_memory
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_temperature_celsius GPU temperature in celsius\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_temperature_celsius gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_temperature_celsius{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.temperature
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_power_consumption_watts GPU power consumption in watts\n"
        ));
        metrics.push_str(&format!(
            "# TYPE all_smi_gpu_power_consumption_watts gauge\n"
        ));
        metrics.push_str(&format!(
            "all_smi_gpu_power_consumption_watts{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.power_consumption
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_frequency_mhz GPU frequency in MHz\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_frequency_mhz gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_frequency_mhz{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name, info.instance, info.uuid, i, info.frequency
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_ane_utilization ANE utilization in watts\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_ane_utilization gauge\n"));
        metrics.push_str(&format!(
            "all_smi_ane_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
            info.name,
            info.instance,
            info.uuid,
            i,
            info.ane_utilization / 1000.0
        ));

        if let Some(dla_util) = info.dla_utilization {
            metrics.push_str(&format!(
                "# HELP all_smi_dla_utilization DLA utilization percentage\n"
            ));
            metrics.push_str(&format!("# TYPE all_smi_dla_utilization gauge\n"));
            metrics.push_str(&format!(
                "all_smi_dla_utilization{{gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"}} {}\n",
                info.name, info.instance, info.uuid, i, dla_util
            ));
        }
    }

    if !state.process_info.is_empty() {
        metrics.push_str(&format!(
            "# HELP all_smi_process_memory_used_bytes Process memory used in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_process_memory_used_bytes gauge\n"));
        for process in &state.process_info {
            metrics.push_str(&format!(
                "all_smi_process_memory_used_bytes{{pid=\"{}\", name=\"{}\", device_id=\"{}\", device_uuid=\"{}\"}} {}\n",
                process.pid, process.process_name, process.device_id, process.device_uuid, process.used_memory
            ));
        }
    }

    // Use instance name for disk metrics to ensure consistency with GPU metrics
    let instance = state
        .gpu_info
        .first()
        .map(|info| info.instance.clone())
        .unwrap_or_else(|| get_hostname());
    let disks = Disks::new_with_refreshed_list();

    for (index, disk) in disks.iter().enumerate() {
        let mount_point_str = disk.mount_point().to_string_lossy();
        if !should_include_disk(&mount_point_str) {
            continue;
        }
        metrics.push_str(&format!(
            "# HELP all_smi_disk_total_bytes Total disk space in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_disk_total_bytes gauge\n"));
        metrics.push_str(&format!(
            "all_smi_disk_total_bytes{{instance=\"{}\", mount_point=\"{}\", index=\"{}\"}} {}\n",
            instance,
            disk.mount_point().to_string_lossy(),
            index,
            disk.total_space()
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_disk_available_bytes Available disk space in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_disk_available_bytes gauge\n"));
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
