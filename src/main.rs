use std::process::Command;
use std::str::FromStr;
use std::time::Duration;
use std::thread;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::io::AsyncReadExt; // AsyncReadExt 트레이트를 사용하기 위해 추가
use std::os::unix::process::CommandExt; // CommandExt 트레이트를 사용하기 위해 추가

#[derive(Debug)]
struct GpuInfo {
    time: String,
    name: String,
    utilization: f64,
    temperature: u32,
    used_memory: u64,
    total_memory: u64,
}

fn main() {
    // Get the type of GPU installed on the server
    let gpu_type = get_gpu_type();

    // Create a thread to periodically update GPU information
    let handle = thread::spawn(move || {
        // Create Tokio runtime.
        let rt = tokio::runtime::Runtime::new().unwrap();

        loop {
            // Get GPU information asynchronously
            let gpu_info = rt.block_on(get_gpu_info(&gpu_type));

            // Print GPU information
            println!("GPU Information:");
            println!("--------------------------------------------------");
            for info in gpu_info {
                println!("Time: {}", info.time);
                println!("GPU Name: {}", info.name);
                println!("GPU Utilization: {:.2}%", info.utilization);
                println!("GPU Temperature: {}°C", info.temperature);
                println!("Used Memory: {} MB", info.used_memory / 1024 / 1024);
                println!("Total Memory: {} MB", info.total_memory / 1024 / 1024);
                println!("--------------------------------------------------");
            }

            // Sleep for 1 second
            thread::sleep(Duration::from_secs(1));
        }
    });

    // Wait for the thread to finish
    handle.join().unwrap();
}

// Function to get the type of GPU installed on the server
fn get_gpu_type() -> String {
    // Execute the lshw -C display command to get GPU information
    let output = Command::new("lshw")
        .arg("-C")
        .arg("display")
        .output()
        .expect("Failed to execute lshw command");

    // Extract the GPU name from the output
    let output_str = String::from_utf8_lossy(&output.stdout);
    let gpu_type = output_str
        .lines()
        .find(|line| line.contains("product: NVIDIA"))
        .map(|line| line.split_whitespace().nth(1).unwrap().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    gpu_type
}

// Function to get GPU information asynchronously
async fn get_gpu_info(gpu_type: &str) -> Vec<GpuInfo> {
    let mut gpu_info = Vec::new();

    // Execute the appropriate command based on the GPU type
    match gpu_type {
        "NVIDIA" => {
            // Execute the nvidia-smi command to get GPU information
            let mut command = Command::new("nvidia-smi")
                .arg("--format=csv")
                .stdout(std::process::Stdio::piped())
                .spawn() // Uses spawn instead of spawn_async
                .expect("Failed to execute nvidia-smi command");

            // Read the output asynchronously
            let stdout = command.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            // Skip the first line as it is the header
            reader.read_line(&mut line).await.unwrap();

            // Read each line and extract GPU information
            while reader.read_line(&mut line).await.unwrap() > 0 {
                let parts: Vec<&str> = line.trim().split(',').collect();
                let time = parts[0].to_string();
                let name = parts[1].to_string();
                let utilization = f64::from_str(parts[2]).unwrap();
                let temperature = u32::from_str(parts[3]).unwrap();
                let used_memory = u64::from_str(parts[4]).unwrap();
                let total_memory = u64::from_str(parts[5]).unwrap();

                gpu_info.push(GpuInfo {
                    time,
                    name,
                    utilization,
                    temperature,
                    used_memory,
                    total_memory,
                });
            }
        }
        _ => {
            // Add handling for other GPU types
            println!("Unsupported GPU type.");
        }
    }

    gpu_info
}