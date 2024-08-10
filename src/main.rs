mod gpu;

use std::thread;
use std::time::Duration;
use crate::gpu::{get_gpu_readers, GpuInfo};

fn main() {
    let handle = thread::spawn(move || {
        let gpu_readers = get_gpu_readers();

        loop {
            let mut all_gpu_info: Vec<GpuInfo> = Vec::new();

            for reader in &gpu_readers {
                let gpu_info = reader.get_gpu_info();
                all_gpu_info.extend(gpu_info);
            }

            // Print all GPU information
            println!("GPU Information:");
            println!("--------------------------------------------------");
            for info in all_gpu_info {
                println!("Time: {}", info.time);
                println!("GPU Name: {}", info.name);
                println!("GPU Utilization: {:.2}%", info.utilization);
                println!("GPU Temperature: {}°C", info.temperature);
                println!("Used Memory: {} MB", info.used_memory / 1024 / 1024);
                println!("Total Memory: {} MB", info.total_memory / 1024 / 1024);
                println!("--------------------------------------------------");
            }

            thread::sleep(Duration::from_secs(1));
        }
    });

    handle.join().unwrap();
}