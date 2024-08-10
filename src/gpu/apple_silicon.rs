#[cfg(target_os = "macos")]
use chrono::Local;
#[cfg(target_os = "macos")]
use metal::*;
#[cfg(target_os = "macos")]
use std::time::Instant;

use crate::gpu::{GpuInfo, GpuReader};

pub struct AppleSiliconGpuReader;

impl GpuReader for AppleSiliconGpuReader {
    fn get_gpu_info(&self) -> Vec<GpuInfo> {
        #[cfg(target_os = "macos")]
        {
            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

            // Measure GPU utilization by performing a simple compute operation
            let device = Device::system_default().expect("No Metal device found");
            let command_queue = device.new_command_queue();
            let command_buffer = command_queue.new_command_buffer();

            // Start measuring time
            let start = Instant::now();

            // Perform a simple GPU operation
            command_buffer.commit();
            command_buffer.wait_until_completed();

            // Measure the time taken
            let duration = start.elapsed();
            let utilization = calculate_utilization(duration);

            vec![GpuInfo {
                time: current_time,
                name: device.name().to_string(),
                utilization,
                temperature: 0,
                used_memory: 0,
                total_memory: 0,
            }]
        }
        #[cfg(not(target_os = "macos"))]
        {
            vec![]
        }
    }
}

// Function to estimate GPU utilization based on task duration
#[cfg(target_os = "macos")]
fn calculate_utilization(duration: std::time::Duration) -> f64 {
    let max_duration = std::time::Duration::from_millis(100); // Example max duration
    let utilization = (duration.as_secs_f64() / max_duration.as_secs_f64()) * 100.0;
    utilization.min(100.0)
}