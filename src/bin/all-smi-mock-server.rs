use anyhow::Result;
use clap::Parser;
use futures_util::future::join_all;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use hyper_util::server::conn::auto::Builder;
use rand::Rng;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::interval;

const DEFAULT_GPU_NAME: &str = "NVIDIA H200 141GB HBM3";
const NUM_GPUS: usize = 8;
const UPDATE_INTERVAL_SECS: u64 = 3;

// Disk size options in bytes
const DISK_SIZE_1TB: u64 = 1024 * 1024 * 1024 * 1024;
const DISK_SIZE_4TB: u64 = 4 * 1024 * 1024 * 1024 * 1024;
const DISK_SIZE_12TB: u64 = 12 * 1024 * 1024 * 1024 * 1024;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, help = "Port range, e.g., 10001-10010 or 10001")]
    port_range: Option<String>,

    #[arg(long, default_value = DEFAULT_GPU_NAME, help = "GPU name")]
    gpu_name: String,

    #[arg(
        long,
        default_value = "nvidia",
        help = "Platform type: nvidia, apple, jetson, intel, amd"
    )]
    platform: String,

    #[arg(
        short,
        long,
        default_value = "hosts.csv",
        help = "Output CSV file name"
    )]
    o: String,

    #[arg(
        long,
        default_value_t = 0,
        help = "Number of nodes to simulate random failures (0 = no failures)"
    )]
    failure_nodes: u32,
}

#[derive(Clone)]
struct GpuMetrics {
    uuid: String,
    utilization: f32,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    temperature_celsius: u32,
    power_consumption_watts: f32,
    frequency_mhz: u32,
    ane_utilization_watts: f32, // ANE power consumption in watts (Apple Silicon only)
}

#[derive(Clone)]
struct CpuMetrics {
    model: String,
    utilization: f32,
    socket_count: u32,
    core_count: u32,
    thread_count: u32,
    frequency_mhz: u32,
    temperature_celsius: Option<u32>,
    power_consumption_watts: Option<f32>,
    // Per-socket utilization for multi-socket systems
    socket_utilizations: Vec<f32>,
    // Apple Silicon specific fields
    p_core_count: Option<u32>,
    e_core_count: Option<u32>,
    gpu_core_count: Option<u32>,
    p_core_utilization: Option<f32>,
    e_core_utilization: Option<f32>,
}

#[derive(Clone)]
struct MemoryMetrics {
    total_bytes: u64,
    used_bytes: u64,
    available_bytes: u64,
    free_bytes: u64,
    buffers_bytes: u64,
    cached_bytes: u64,
    swap_total_bytes: u64,
    swap_used_bytes: u64,
    swap_free_bytes: u64,
    utilization: f32,
}

#[derive(Clone, Debug, PartialEq)]
enum PlatformType {
    Nvidia,
    Apple,
    Jetson,
    Intel,
    Amd,
}

// High-performance template-based mock node
#[allow(dead_code)]
struct MockNode {
    instance_name: String,
    gpu_name: String,
    gpus: Vec<GpuMetrics>,
    cpu: CpuMetrics,
    memory: MemoryMetrics,
    platform_type: PlatformType,
    disk_available_bytes: u64,
    disk_total_bytes: u64,
    response_template: String,
    rendered_response: String,
    is_responding: bool, // Whether this node should respond to requests
}

// Template placeholders for fast string replacement
#[allow(dead_code)]
const PLACEHOLDER_UTILIZATION: &str = "{{UTIL_";
#[allow(dead_code)]
const PLACEHOLDER_MEMORY_USED: &str = "{{MEM_USED_";
#[allow(dead_code)]
const PLACEHOLDER_MEMORY_TOTAL: &str = "{{MEM_TOTAL_";
#[allow(dead_code)]
const PLACEHOLDER_TEMPERATURE: &str = "{{TEMP_";
#[allow(dead_code)]
const PLACEHOLDER_POWER: &str = "{{POWER_";
#[allow(dead_code)]
const PLACEHOLDER_FREQUENCY: &str = "{{FREQ_";
#[allow(dead_code)]
const PLACEHOLDER_ANE: &str = "{{ANE_";
const PLACEHOLDER_DISK_AVAIL: &str = "{{DISK_AVAIL}}";
const PLACEHOLDER_DISK_TOTAL: &str = "{{DISK_TOTAL}}";

// CPU placeholders
const PLACEHOLDER_CPU_UTIL: &str = "{{CPU_UTIL}}";
const PLACEHOLDER_CPU_SOCKET0_UTIL: &str = "{{CPU_SOCKET0_UTIL}}";
const PLACEHOLDER_CPU_SOCKET1_UTIL: &str = "{{CPU_SOCKET1_UTIL}}";
const PLACEHOLDER_CPU_P_CORE_UTIL: &str = "{{CPU_P_CORE_UTIL}}";
const PLACEHOLDER_CPU_E_CORE_UTIL: &str = "{{CPU_E_CORE_UTIL}}";
const PLACEHOLDER_CPU_TEMP: &str = "{{CPU_TEMP}}";
const PLACEHOLDER_CPU_POWER: &str = "{{CPU_POWER}}";

// System memory placeholders
const PLACEHOLDER_SYS_MEMORY_USED: &str = "{{SYS_MEMORY_USED}}";
const PLACEHOLDER_SYS_MEMORY_AVAILABLE: &str = "{{SYS_MEMORY_AVAILABLE}}";
const PLACEHOLDER_SYS_MEMORY_FREE: &str = "{{SYS_MEMORY_FREE}}";
const PLACEHOLDER_SYS_MEMORY_UTIL: &str = "{{SYS_MEMORY_UTIL}}";
const PLACEHOLDER_SYS_SWAP_USED: &str = "{{SYS_SWAP_USED}}";
const PLACEHOLDER_SYS_SWAP_FREE: &str = "{{SYS_SWAP_FREE}}";
const PLACEHOLDER_SYS_MEMORY_BUFFERS: &str = "{{SYS_MEMORY_BUFFERS}}";
const PLACEHOLDER_SYS_MEMORY_CACHED: &str = "{{SYS_MEMORY_CACHED}}";

fn generate_uuid() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "GPU-{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

impl MockNode {
    fn new(instance_name: String, gpu_name: String, platform: PlatformType) -> Self {
        let gpu_memory_gb = Self::extract_gpu_memory_gb(&gpu_name);
        let memory_total_bytes = gpu_memory_gb * 1024 * 1024 * 1024;

        let mut rng = rand::thread_rng();

        // Choose random disk size from options
        let disk_sizes = [DISK_SIZE_1TB, DISK_SIZE_4TB, DISK_SIZE_12TB];
        let disk_total_bytes = disk_sizes[rng.gen_range(0..disk_sizes.len())];

        let gpus: Vec<GpuMetrics> = (0..NUM_GPUS)
            .map(|_| {
                let utilization = rng.gen_range(10.0..90.0);
                let memory_used_bytes =
                    rng.gen_range(memory_total_bytes / 10..memory_total_bytes * 9 / 10);
                let memory_usage_percent =
                    (memory_used_bytes as f32 / memory_total_bytes as f32) * 100.0;

                // Calculate realistic initial power consumption
                let base_power = rng.gen_range(80.0..120.0);
                let util_power_contribution = utilization * rng.gen_range(4.0..6.0);
                let memory_power_contribution = memory_usage_percent * rng.gen_range(1.0..2.0);
                let gpu_bias = rng.gen_range(-30.0..30.0);
                let power_consumption_watts =
                    (base_power + util_power_contribution + memory_power_contribution + gpu_bias)
                        .clamp(80.0, 700.0);

                // Calculate realistic initial temperature
                let base_temp = 45.0;
                let util_temp_contribution = utilization * 0.25;
                let power_temp_contribution = (power_consumption_watts - 200.0) * 0.05;
                let temperature_celsius = (base_temp
                    + util_temp_contribution
                    + power_temp_contribution)
                    .clamp(35.0, 85.0) as u32;

                // Calculate realistic initial frequency
                let base_freq = 1200.0;
                let util_freq_contribution = utilization * 6.0;
                let frequency_mhz =
                    (base_freq + util_freq_contribution).clamp(1000.0, 1980.0) as u32;

                // ANE utilization only for Apple Silicon
                let ane_utilization_watts = if platform == PlatformType::Apple {
                    rng.gen_range(0.0..2.5) // ANE power consumption 0-2.5W
                } else {
                    0.0
                };

                GpuMetrics {
                    uuid: generate_uuid(),
                    utilization,
                    memory_used_bytes,
                    memory_total_bytes,
                    temperature_celsius,
                    power_consumption_watts,
                    frequency_mhz,
                    ane_utilization_watts,
                }
            })
            .collect();

        // Initialize CPU metrics based on platform
        let cpu = Self::create_cpu_metrics(&platform, &mut rng);

        // Initialize memory metrics
        let memory = Self::create_memory_metrics(&mut rng);

        // Build response template once during initialization
        let response_template = Self::build_response_template(
            &instance_name,
            &gpu_name,
            &gpus,
            &cpu,
            &memory,
            &platform,
        );

        let mut node = Self {
            instance_name,
            gpu_name,
            gpus,
            cpu,
            memory,
            platform_type: platform,
            disk_available_bytes: rng.gen_range(disk_total_bytes / 10..disk_total_bytes * 9 / 10),
            disk_total_bytes,
            response_template,
            rendered_response: String::new(),
            is_responding: true, // Start with all nodes responding
        };

        // Render initial response
        node.render_response();
        node
    }

    fn create_cpu_metrics(platform: &PlatformType, rng: &mut rand::rngs::ThreadRng) -> CpuMetrics {
        match platform {
            PlatformType::Apple => {
                // Apple Silicon M1/M2/M3
                let models = [
                    "Apple M1",
                    "Apple M2",
                    "Apple M2 Pro",
                    "Apple M2 Max",
                    "Apple M3",
                ];
                let model = models[rng.gen_range(0..models.len())].to_string();

                let (p_cores, e_cores, gpu_cores) = match model.as_str() {
                    "Apple M1" => (4, 4, 8),
                    "Apple M2" => (4, 4, 10),
                    "Apple M2 Pro" => (8, 4, 19),
                    "Apple M2 Max" => (8, 4, 38),
                    "Apple M3" => (4, 4, 10),
                    _ => (4, 4, 8),
                };

                CpuMetrics {
                    model,
                    utilization: rng.gen_range(15.0..75.0),
                    socket_count: 1,
                    core_count: p_cores + e_cores,
                    thread_count: p_cores + e_cores, // Apple Silicon doesn't use hyperthreading
                    frequency_mhz: rng.gen_range(3000..3500),
                    temperature_celsius: Some(rng.gen_range(45..70)),
                    power_consumption_watts: Some(rng.gen_range(15.0..35.0)),
                    socket_utilizations: vec![rng.gen_range(15.0..75.0)],
                    p_core_count: Some(p_cores),
                    e_core_count: Some(e_cores),
                    gpu_core_count: Some(gpu_cores),
                    p_core_utilization: Some(rng.gen_range(10.0..80.0)),
                    e_core_utilization: Some(rng.gen_range(5.0..40.0)),
                }
            }
            PlatformType::Intel => {
                let models = [
                    "Intel Xeon Gold 6248R",
                    "Intel Xeon Platinum 8280",
                    "Intel Core i9-13900K",
                    "Intel Xeon E5-2699 v4",
                ];
                let model = models[rng.gen_range(0..models.len())].to_string();

                let socket_count = if model.contains("Xeon") {
                    rng.gen_range(1..=2)
                } else {
                    1
                };
                let cores_per_socket = rng.gen_range(8..32);
                let total_cores = socket_count * cores_per_socket;
                let total_threads = total_cores * 2; // Intel hyperthreading

                let socket_utilizations: Vec<f32> = (0..socket_count)
                    .map(|_| rng.gen_range(20.0..80.0))
                    .collect();
                let overall_util =
                    socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

                CpuMetrics {
                    model,
                    utilization: overall_util,
                    socket_count,
                    core_count: total_cores,
                    thread_count: total_threads,
                    frequency_mhz: rng.gen_range(2400..3800),
                    temperature_celsius: Some(rng.gen_range(55..85)),
                    power_consumption_watts: Some(rng.gen_range(150.0..400.0)),
                    socket_utilizations,
                    p_core_count: None,
                    e_core_count: None,
                    gpu_core_count: None,
                    p_core_utilization: None,
                    e_core_utilization: None,
                }
            }
            PlatformType::Amd => {
                let models = [
                    "AMD EPYC 7742",
                    "AMD Ryzen 9 7950X",
                    "AMD EPYC 9554",
                    "AMD Threadripper PRO 5995WX",
                ];
                let model = models[rng.gen_range(0..models.len())].to_string();

                let socket_count = if model.contains("EPYC") {
                    rng.gen_range(1..=2)
                } else {
                    1
                };
                let cores_per_socket = rng.gen_range(16..64);
                let total_cores = socket_count * cores_per_socket;
                let total_threads = total_cores * 2; // AMD SMT

                let socket_utilizations: Vec<f32> = (0..socket_count)
                    .map(|_| rng.gen_range(25.0..85.0))
                    .collect();
                let overall_util =
                    socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

                CpuMetrics {
                    model,
                    utilization: overall_util,
                    socket_count,
                    core_count: total_cores,
                    thread_count: total_threads,
                    frequency_mhz: rng.gen_range(2200..4500),
                    temperature_celsius: Some(rng.gen_range(50..80)),
                    power_consumption_watts: Some(rng.gen_range(180.0..500.0)),
                    socket_utilizations,
                    p_core_count: None,
                    e_core_count: None,
                    gpu_core_count: None,
                    p_core_utilization: None,
                    e_core_utilization: None,
                }
            }
            PlatformType::Jetson => {
                // NVIDIA Jetson platforms
                let models = [
                    "NVIDIA Jetson AGX Orin",
                    "NVIDIA Jetson Xavier NX",
                    "NVIDIA Jetson Nano",
                ];
                let model = models[rng.gen_range(0..models.len())].to_string();

                let (cores, threads) = match model.as_str() {
                    "NVIDIA Jetson AGX Orin" => (12, 12),
                    "NVIDIA Jetson Xavier NX" => (6, 6),
                    "NVIDIA Jetson Nano" => (4, 4),
                    _ => (6, 6),
                };

                CpuMetrics {
                    model,
                    utilization: rng.gen_range(20.0..70.0),
                    socket_count: 1,
                    core_count: cores,
                    thread_count: threads,
                    frequency_mhz: rng.gen_range(1400..2200),
                    temperature_celsius: Some(rng.gen_range(55..75)),
                    power_consumption_watts: Some(rng.gen_range(10.0..60.0)),
                    socket_utilizations: vec![rng.gen_range(20.0..70.0)],
                    p_core_count: None,
                    e_core_count: None,
                    gpu_core_count: None,
                    p_core_utilization: None,
                    e_core_utilization: None,
                }
            }
            PlatformType::Nvidia => {
                // Default NVIDIA GPU server (Intel/AMD CPU)
                let models = ["Intel Xeon Gold 6248R", "AMD EPYC 7742"];
                let model = models[rng.gen_range(0..models.len())].to_string();

                let socket_count = 2;
                let cores_per_socket = rng.gen_range(16..32);
                let total_cores = socket_count * cores_per_socket;
                let total_threads = total_cores * 2;

                let socket_utilizations: Vec<f32> = (0..socket_count)
                    .map(|_| rng.gen_range(30.0..85.0))
                    .collect();
                let overall_util =
                    socket_utilizations.iter().sum::<f32>() / socket_utilizations.len() as f32;

                CpuMetrics {
                    model,
                    utilization: overall_util,
                    socket_count,
                    core_count: total_cores,
                    thread_count: total_threads,
                    frequency_mhz: rng.gen_range(2400..3600),
                    temperature_celsius: Some(rng.gen_range(60..80)),
                    power_consumption_watts: Some(rng.gen_range(200.0..450.0)),
                    socket_utilizations,
                    p_core_count: None,
                    e_core_count: None,
                    gpu_core_count: None,
                    p_core_utilization: None,
                    e_core_utilization: None,
                }
            }
        }
    }

    fn create_memory_metrics(rng: &mut rand::rngs::ThreadRng) -> MemoryMetrics {
        // Memory size options: 256GB, 512GB, 1TB, 2TB, 4TB
        let memory_sizes_gb = [256, 512, 1024, 2048, 4096];
        let total_gb = memory_sizes_gb[rng.gen_range(0..memory_sizes_gb.len())];
        let total_bytes = total_gb as u64 * 1024 * 1024 * 1024;

        // Start used memory at 40%+ and make it fluctuate
        let base_utilization = rng.gen_range(40.0..80.0);
        let utilization = base_utilization as f32;
        let used_bytes = (total_bytes as f64 * utilization as f64 / 100.0) as u64;
        let available_bytes = total_bytes - used_bytes;
        let free_bytes = rng.gen_range(available_bytes / 4..available_bytes * 3 / 4);

        // Linux-specific memory breakdown
        let buffers_bytes = rng.gen_range(total_bytes / 100..total_bytes / 20); // 1-5% for buffers
        let cached_bytes = rng.gen_range(total_bytes / 50..total_bytes / 10); // 2-10% for cache

        // Swap configuration (some nodes have swap, others don't)
        let (swap_total_bytes, swap_used_bytes, swap_free_bytes) = if rng.gen_bool(0.7) {
            // 70% chance of having swap
            // Swap size: min(1/8 of total memory, 32GB)
            let max_swap_32gb = 32 * 1024 * 1024 * 1024; // 32GB in bytes
            let max_swap_eighth = total_bytes / 8; // 1/8 of total memory
            let swap_total = std::cmp::min(max_swap_32gb, max_swap_eighth);

            // Swap is only used when memory usage is at 100%
            // Since we start at 40-80% usage, no swap is used initially
            let swap_used = 0;
            let swap_free = swap_total;
            (swap_total, swap_used, swap_free)
        } else {
            (0, 0, 0)
        };

        MemoryMetrics {
            total_bytes,
            used_bytes,
            available_bytes,
            free_bytes,
            buffers_bytes,
            cached_bytes,
            swap_total_bytes,
            swap_used_bytes,
            swap_free_bytes,
            utilization,
        }
    }

    fn extract_gpu_memory_gb(gpu_name: &str) -> u64 {
        // Extract memory size from GPU name (e.g., "NVIDIA H200 141GB HBM3" -> 141)
        if let Some(gb_pos) = gpu_name.find("GB") {
            let before_gb = &gpu_name[..gb_pos];
            if let Some(space_pos) = before_gb.rfind(' ') {
                if let Ok(gb) = before_gb[space_pos + 1..].parse::<u64>() {
                    return gb;
                }
            }
            // Try to find number right before GB
            let mut end = gb_pos;
            while end > 0 && gpu_name.chars().nth(end - 1).unwrap().is_ascii_digit() {
                end -= 1;
            }
            if let Ok(gb) = gpu_name[end..gb_pos].parse::<u64>() {
                return gb;
            }
        }
        // Default to 24GB if can't parse
        24
    }

    // Build static response template with placeholders (called once during init)
    fn build_response_template(
        instance_name: &str,
        gpu_name: &str,
        gpus: &[GpuMetrics],
        cpu: &CpuMetrics,
        memory: &MemoryMetrics,
        platform: &PlatformType,
    ) -> String {
        let mut template = String::with_capacity(16384); // Pre-allocate 16KB

        // GPU Metrics headers
        let gpu_metrics = [
            ("all_smi_gpu_utilization", "GPU utilization percentage"),
            ("all_smi_gpu_memory_used_bytes", "GPU memory used in bytes"),
            (
                "all_smi_gpu_memory_total_bytes",
                "GPU memory total in bytes",
            ),
            (
                "all_smi_gpu_temperature_celsius",
                "GPU temperature in celsius",
            ),
            (
                "all_smi_gpu_power_consumption_watts",
                "GPU power consumption in watts",
            ),
            ("all_smi_gpu_frequency_mhz", "GPU frequency in MHz"),
        ];

        for (metric_name, help_text) in gpu_metrics {
            template.push_str(&format!("# HELP {metric_name} {help_text}\n"));
            template.push_str(&format!("# TYPE {metric_name} gauge\n"));

            for (i, gpu) in gpus.iter().enumerate() {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                    gpu_name, instance_name, gpu.uuid, i
                );

                let placeholder = match metric_name {
                    "all_smi_gpu_utilization" => format!("{{{{UTIL_{i}}}}}"),
                    "all_smi_gpu_memory_used_bytes" => format!("{{{{MEM_USED_{i}}}}}"),
                    "all_smi_gpu_memory_total_bytes" => format!("{{{{MEM_TOTAL_{i}}}}}"),
                    "all_smi_gpu_temperature_celsius" => format!("{{{{TEMP_{i}}}}}"),
                    "all_smi_gpu_power_consumption_watts" => format!("{{{{POWER_{i}}}}}"),
                    "all_smi_gpu_frequency_mhz" => format!("{{{{FREQ_{i}}}}}"),
                    _ => "0".to_string(),
                };

                template.push_str(&format!("{metric_name}{{{labels}}} {placeholder}\n"));
            }
        }

        // ANE utilization metrics (Apple Silicon only)
        if let PlatformType::Apple = platform {
            template.push_str("# HELP all_smi_ane_utilization ANE utilization in watts\n");
            template.push_str("# TYPE all_smi_ane_utilization gauge\n");

            for (i, gpu) in gpus.iter().enumerate() {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                    gpu_name, instance_name, gpu.uuid, i
                );
                let placeholder = format!("{{{{ANE_{i}}}}}");
                template.push_str(&format!(
                    "all_smi_ane_utilization{{{labels}}} {placeholder}\n"
                ));
            }
        }

        // CPU metrics
        let cpu_labels = format!(
            "cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", index=\"0\"",
            cpu.model, instance_name, instance_name
        );

        // Basic CPU metrics
        template.push_str("# HELP all_smi_cpu_utilization CPU utilization percentage\n");
        template.push_str("# TYPE all_smi_cpu_utilization gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_UTIL}\n"
        ));

        template.push_str("# HELP all_smi_cpu_socket_count Number of CPU sockets\n");
        template.push_str("# TYPE all_smi_cpu_socket_count gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_socket_count{{{cpu_labels}}} {}\n",
            cpu.socket_count
        ));

        template.push_str("# HELP all_smi_cpu_core_count Total number of CPU cores\n");
        template.push_str("# TYPE all_smi_cpu_core_count gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_core_count{{{cpu_labels}}} {}\n",
            cpu.core_count
        ));

        template.push_str("# HELP all_smi_cpu_thread_count Total number of CPU threads\n");
        template.push_str("# TYPE all_smi_cpu_thread_count gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_thread_count{{{cpu_labels}}} {}\n",
            cpu.thread_count
        ));

        template.push_str("# HELP all_smi_cpu_frequency_mhz CPU frequency in MHz\n");
        template.push_str("# TYPE all_smi_cpu_frequency_mhz gauge\n");
        template.push_str(&format!(
            "all_smi_cpu_frequency_mhz{{{cpu_labels}}} {}\n",
            cpu.frequency_mhz
        ));

        // Optional CPU metrics (temperature and power)
        if cpu.temperature_celsius.is_some() {
            template
                .push_str("# HELP all_smi_cpu_temperature_celsius CPU temperature in celsius\n");
            template.push_str("# TYPE all_smi_cpu_temperature_celsius gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_temperature_celsius{{{cpu_labels}}} {PLACEHOLDER_CPU_TEMP}\n"
            ));
        }

        if cpu.power_consumption_watts.is_some() {
            template.push_str(
                "# HELP all_smi_cpu_power_consumption_watts CPU power consumption in watts\n",
            );
            template.push_str("# TYPE all_smi_cpu_power_consumption_watts gauge\n");
            template.push_str(&format!(
                "all_smi_cpu_power_consumption_watts{{{cpu_labels}}} {PLACEHOLDER_CPU_POWER}\n"
            ));
        }

        // Per-socket metrics for multi-socket systems
        if cpu.socket_count > 1 {
            for (socket_id, _) in cpu.socket_utilizations.iter().enumerate() {
                let socket_labels = format!(
                    "cpu_model=\"{}\", instance=\"{}\", hostname=\"{}\", cpu_index=\"0\", socket_id=\"{}\"",
                    cpu.model, instance_name, instance_name, socket_id
                );

                template.push_str(
                    "# HELP all_smi_cpu_socket_utilization Per-socket CPU utilization percentage\n",
                );
                template.push_str("# TYPE all_smi_cpu_socket_utilization gauge\n");
                let placeholder = if socket_id == 0 {
                    PLACEHOLDER_CPU_SOCKET0_UTIL
                } else {
                    PLACEHOLDER_CPU_SOCKET1_UTIL
                };
                template.push_str(&format!(
                    "all_smi_cpu_socket_utilization{{{socket_labels}}} {placeholder}\n"
                ));
            }
        }

        // Apple Silicon specific metrics
        if let PlatformType::Apple = platform {
            if let (Some(p_count), Some(e_count), Some(gpu_count)) =
                (cpu.p_core_count, cpu.e_core_count, cpu.gpu_core_count)
            {
                template.push_str("# HELP all_smi_cpu_p_core_count Apple Silicon P-core count\n");
                template.push_str("# TYPE all_smi_cpu_p_core_count gauge\n");
                template.push_str(&format!(
                    "all_smi_cpu_p_core_count{{{cpu_labels}}} {p_count}\n"
                ));

                template.push_str("# HELP all_smi_cpu_e_core_count Apple Silicon E-core count\n");
                template.push_str("# TYPE all_smi_cpu_e_core_count gauge\n");
                template.push_str(&format!(
                    "all_smi_cpu_e_core_count{{{cpu_labels}}} {e_count}\n"
                ));

                template
                    .push_str("# HELP all_smi_cpu_gpu_core_count Apple Silicon GPU core count\n");
                template.push_str("# TYPE all_smi_cpu_gpu_core_count gauge\n");
                template.push_str(&format!(
                    "all_smi_cpu_gpu_core_count{{{cpu_labels}}} {gpu_count}\n"
                ));

                template.push_str("# HELP all_smi_cpu_p_core_utilization Apple Silicon P-core utilization percentage\n");
                template.push_str("# TYPE all_smi_cpu_p_core_utilization gauge\n");
                template.push_str(&format!(
                    "all_smi_cpu_p_core_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_P_CORE_UTIL}\n"
                ));

                template.push_str("# HELP all_smi_cpu_e_core_utilization Apple Silicon E-core utilization percentage\n");
                template.push_str("# TYPE all_smi_cpu_e_core_utilization gauge\n");
                template.push_str(&format!(
                    "all_smi_cpu_e_core_utilization{{{cpu_labels}}} {PLACEHOLDER_CPU_E_CORE_UTIL}\n"
                ));
            }
        }

        // Memory metrics
        let memory_labels =
            format!("instance=\"{instance_name}\", hostname=\"{instance_name}\", index=\"0\"");

        template.push_str("# HELP all_smi_memory_total_bytes Total system memory in bytes\n");
        template.push_str("# TYPE all_smi_memory_total_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_total_bytes{{{memory_labels}}} {}\n",
            memory.total_bytes
        ));

        template.push_str("# HELP all_smi_memory_used_bytes Used system memory in bytes\n");
        template.push_str("# TYPE all_smi_memory_used_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_used_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_USED}\n"
        ));

        template
            .push_str("# HELP all_smi_memory_available_bytes Available system memory in bytes\n");
        template.push_str("# TYPE all_smi_memory_available_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_available_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_AVAILABLE}\n"
        ));

        template.push_str("# HELP all_smi_memory_free_bytes Free system memory in bytes\n");
        template.push_str("# TYPE all_smi_memory_free_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_free_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_FREE}\n"
        ));

        template.push_str("# HELP all_smi_memory_utilization Memory utilization percentage\n");
        template.push_str("# TYPE all_smi_memory_utilization gauge\n");
        template.push_str(&format!(
            "all_smi_memory_utilization{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_UTIL}\n"
        ));

        // Swap metrics if available
        if memory.swap_total_bytes > 0 {
            template.push_str("# HELP all_smi_swap_total_bytes Total swap space in bytes\n");
            template.push_str("# TYPE all_smi_swap_total_bytes gauge\n");
            template.push_str(&format!(
                "all_smi_swap_total_bytes{{{memory_labels}}} {}\n",
                memory.swap_total_bytes
            ));

            template.push_str("# HELP all_smi_swap_used_bytes Used swap space in bytes\n");
            template.push_str("# TYPE all_smi_swap_used_bytes gauge\n");
            template.push_str(&format!(
                "all_smi_swap_used_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_SWAP_USED}\n"
            ));

            template.push_str("# HELP all_smi_swap_free_bytes Free swap space in bytes\n");
            template.push_str("# TYPE all_smi_swap_free_bytes gauge\n");
            template.push_str(&format!(
                "all_smi_swap_free_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_SWAP_FREE}\n"
            ));
        }

        // Linux-specific metrics
        if memory.buffers_bytes > 0 {
            template
                .push_str("# HELP all_smi_memory_buffers_bytes Memory used for buffers in bytes\n");
            template.push_str("# TYPE all_smi_memory_buffers_bytes gauge\n");
            template.push_str(&format!(
                "all_smi_memory_buffers_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_BUFFERS}\n"
            ));
        }

        if memory.cached_bytes > 0 {
            template
                .push_str("# HELP all_smi_memory_cached_bytes Memory used for cache in bytes\n");
            template.push_str("# TYPE all_smi_memory_cached_bytes gauge\n");
            template.push_str(&format!(
                "all_smi_memory_cached_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_CACHED}\n"
            ));
        }

        // Disk metrics
        template.push_str("# HELP all_smi_disk_total_bytes Total disk space in bytes\n");
        template.push_str("# TYPE all_smi_disk_total_bytes gauge\n");
        let disk_labels = format!("instance=\"{instance_name}\", mount_point=\"/\", index=\"0\"");
        template.push_str(&format!(
            "all_smi_disk_total_bytes{{{disk_labels}}} {PLACEHOLDER_DISK_TOTAL}\n"
        ));

        template.push_str("# HELP all_smi_disk_available_bytes Available disk space in bytes\n");
        template.push_str("# TYPE all_smi_disk_available_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_disk_available_bytes{{{disk_labels}}} {PLACEHOLDER_DISK_AVAIL}\n"
        ));

        template
    }

    // Fast response rendering using string replacement (called every 3 seconds)
    fn render_response(&mut self) {
        let mut response = self.response_template.clone();

        // Replace GPU metrics
        for (i, gpu) in self.gpus.iter().enumerate() {
            response = response
                .replace(
                    &format!("{{{{UTIL_{i}}}}}"),
                    &format!("{:.2}", gpu.utilization),
                )
                .replace(
                    &format!("{{{{MEM_USED_{i}}}}}"),
                    &gpu.memory_used_bytes.to_string(),
                )
                .replace(
                    &format!("{{{{MEM_TOTAL_{i}}}}}"),
                    &gpu.memory_total_bytes.to_string(),
                )
                .replace(
                    &format!("{{{{TEMP_{i}}}}}"),
                    &gpu.temperature_celsius.to_string(),
                )
                .replace(
                    &format!("{{{{POWER_{i}}}}}"),
                    &format!("{:.3}", gpu.power_consumption_watts),
                )
                .replace(&format!("{{{{FREQ_{i}}}}}"), &gpu.frequency_mhz.to_string());

            // Replace ANE metrics for Apple Silicon
            if let PlatformType::Apple = self.platform_type {
                response = response.replace(
                    &format!("{{{{ANE_{i}}}}}"),
                    &format!("{:.3}", gpu.ane_utilization_watts),
                );
            }
        }

        // Replace CPU metrics
        response = response
            .replace(
                PLACEHOLDER_CPU_UTIL,
                &format!("{:.2}", self.cpu.utilization),
            )
            .replace(
                PLACEHOLDER_CPU_SOCKET0_UTIL,
                &format!(
                    "{:.2}",
                    self.cpu.socket_utilizations.first().copied().unwrap_or(0.0)
                ),
            )
            .replace(
                PLACEHOLDER_CPU_SOCKET1_UTIL,
                &format!(
                    "{:.2}",
                    self.cpu.socket_utilizations.get(1).copied().unwrap_or(0.0)
                ),
            );

        if let Some(temp) = self.cpu.temperature_celsius {
            response = response.replace(PLACEHOLDER_CPU_TEMP, &temp.to_string());
        }

        if let Some(power) = self.cpu.power_consumption_watts {
            response = response.replace(PLACEHOLDER_CPU_POWER, &format!("{power:.3}"));
        }

        // Apple Silicon specific replacements
        if let PlatformType::Apple = self.platform_type {
            if let (Some(p_util), Some(e_util)) =
                (self.cpu.p_core_utilization, self.cpu.e_core_utilization)
            {
                response = response
                    .replace(PLACEHOLDER_CPU_P_CORE_UTIL, &format!("{p_util:.2}"))
                    .replace(PLACEHOLDER_CPU_E_CORE_UTIL, &format!("{e_util:.2}"));
            }
        }

        // Replace memory metrics
        response = response
            .replace(
                PLACEHOLDER_SYS_MEMORY_USED,
                &self.memory.used_bytes.to_string(),
            )
            .replace(
                PLACEHOLDER_SYS_MEMORY_AVAILABLE,
                &self.memory.available_bytes.to_string(),
            )
            .replace(
                PLACEHOLDER_SYS_MEMORY_FREE,
                &self.memory.free_bytes.to_string(),
            )
            .replace(
                PLACEHOLDER_SYS_MEMORY_UTIL,
                &format!("{:.2}", self.memory.utilization),
            );

        // Replace swap metrics if available
        if self.memory.swap_total_bytes > 0 {
            response = response
                .replace(
                    PLACEHOLDER_SYS_SWAP_USED,
                    &self.memory.swap_used_bytes.to_string(),
                )
                .replace(
                    PLACEHOLDER_SYS_SWAP_FREE,
                    &self.memory.swap_free_bytes.to_string(),
                );
        }

        // Replace buffer and cache metrics if available
        if self.memory.buffers_bytes > 0 {
            response = response.replace(
                PLACEHOLDER_SYS_MEMORY_BUFFERS,
                &self.memory.buffers_bytes.to_string(),
            );
        }

        if self.memory.cached_bytes > 0 {
            response = response.replace(
                PLACEHOLDER_SYS_MEMORY_CACHED,
                &self.memory.cached_bytes.to_string(),
            );
        }

        // Replace disk metrics
        response = response
            .replace(PLACEHOLDER_DISK_TOTAL, &self.disk_total_bytes.to_string())
            .replace(
                PLACEHOLDER_DISK_AVAIL,
                &self.disk_available_bytes.to_string(),
            );

        self.rendered_response = response;
    }

    fn update(&mut self) {
        let mut rng = rand::thread_rng();

        for gpu in &mut self.gpus {
            // GPU utilization: gradual changes
            let utilization_delta = rng.gen_range(-5.0..5.0);
            gpu.utilization = (gpu.utilization + utilization_delta).clamp(0.0, 100.0);

            // GPU memory: change by less than 3GB
            let memory_delta = rng.gen_range(-(3 * 1024 * 1024 * 1024)..(3 * 1024 * 1024 * 1024));
            gpu.memory_used_bytes = gpu
                .memory_used_bytes
                .saturating_add_signed(memory_delta)
                .min(gpu.memory_total_bytes);

            // Calculate realistic power consumption based on utilization and memory usage
            let memory_usage_percent =
                (gpu.memory_used_bytes as f32 / gpu.memory_total_bytes as f32) * 100.0;

            // Base power consumption (idle state) - varies by GPU type
            let base_power = rng.gen_range(80.0..120.0);

            // Power contribution from GPU utilization (strong correlation)
            let util_power_contribution = gpu.utilization * rng.gen_range(4.0..6.0); // 4-6W per % utilization

            // Power contribution from memory usage (moderate correlation)
            let memory_power_contribution = memory_usage_percent * rng.gen_range(1.0..2.0); // 1-2W per % memory usage

            // Individual GPU bias (some GPUs naturally consume more/less power)
            let gpu_bias = rng.gen_range(-30.0..30.0);

            // Random variation (±15W)
            let random_variation = rng.gen_range(-15.0..15.0);

            // Calculate total power consumption
            gpu.power_consumption_watts = (base_power
                + util_power_contribution
                + memory_power_contribution
                + gpu_bias
                + random_variation)
                .clamp(80.0, 700.0);

            // GPU temperature: correlate with power consumption and utilization
            let base_temp = 45.0;
            let util_temp_contribution = gpu.utilization * 0.25; // 0.25°C per % utilization
            let power_temp_contribution = (gpu.power_consumption_watts - 200.0) * 0.05; // Temperature increases with power
            let temp_variation = rng.gen_range(-3.0..3.0);

            gpu.temperature_celsius =
                (base_temp + util_temp_contribution + power_temp_contribution + temp_variation)
                    .clamp(35.0, 85.0) as u32;

            // GPU frequency: correlate with utilization (higher util = higher freq)
            let base_freq = 1200.0;
            let util_freq_contribution = gpu.utilization * 6.0; // Up to 600MHz boost at 100% util
            let freq_variation = rng.gen_range(-100.0..100.0);

            gpu.frequency_mhz =
                (base_freq + util_freq_contribution + freq_variation).clamp(1000.0, 1980.0) as u32;

            // Update ANE utilization for Apple Silicon
            if let PlatformType::Apple = self.platform_type {
                let ane_delta = rng.gen_range(-0.3..0.3);
                gpu.ane_utilization_watts = (gpu.ane_utilization_watts + ane_delta).clamp(0.0, 3.0);
            }
        }

        // Update CPU metrics
        let cpu_utilization_delta = rng.gen_range(-3.0..3.0);
        self.cpu.utilization = (self.cpu.utilization + cpu_utilization_delta).clamp(0.0, 100.0);

        // Update per-socket utilizations
        for socket_util in &mut self.cpu.socket_utilizations {
            let socket_delta = rng.gen_range(-3.0..3.0);
            *socket_util = (*socket_util + socket_delta).clamp(0.0, 100.0);
        }

        // Update CPU temperature if available
        if let Some(ref mut temp) = self.cpu.temperature_celsius {
            let temp_delta = rng.gen_range(-2..3);
            *temp = (*temp as i32 + temp_delta).clamp(35, 85) as u32;
        }

        // Update CPU power consumption if available
        if let Some(ref mut power) = self.cpu.power_consumption_watts {
            let power_delta = rng.gen_range(-10.0..10.0);
            *power = (*power + power_delta).clamp(10.0, 500.0);
        }

        // Update Apple Silicon specific metrics
        if let (Some(ref mut p_util), Some(ref mut e_util)) = (
            &mut self.cpu.p_core_utilization,
            &mut self.cpu.e_core_utilization,
        ) {
            let p_delta = rng.gen_range(-4.0..4.0);
            let e_delta = rng.gen_range(-2.0..2.0);
            *p_util = (*p_util + p_delta).clamp(0.0, 100.0);
            *e_util = (*e_util + e_delta).clamp(0.0, 100.0);
        }

        // Update memory metrics with gradual fluctuation
        let memory_util_delta = rng.gen_range(-2.0..2.0);
        // Allow memory utilization to occasionally reach 100% to trigger swap usage
        self.memory.utilization = (self.memory.utilization + memory_util_delta).clamp(30.0, 102.0);

        // Calculate memory usage, accounting for potential over-allocation
        let target_used_bytes =
            (self.memory.total_bytes as f64 * self.memory.utilization as f64 / 100.0) as u64;

        if target_used_bytes > self.memory.total_bytes {
            // Memory usage exceeds physical memory - use swap
            self.memory.used_bytes = self.memory.total_bytes;
            self.memory.available_bytes = 0;
            self.memory.free_bytes = 0;

            // Calculate swap usage based on excess memory demand
            if self.memory.swap_total_bytes > 0 {
                let excess_bytes = target_used_bytes - self.memory.total_bytes;
                self.memory.swap_used_bytes = excess_bytes.min(self.memory.swap_total_bytes);
                self.memory.swap_free_bytes =
                    self.memory.swap_total_bytes - self.memory.swap_used_bytes;

                // Memory utilization should show 100% when physical memory is full
                self.memory.utilization = 100.0;
            } else {
                // No swap available, cap at 100% physical memory
                self.memory.utilization = 100.0;
            }
        } else {
            // Normal memory usage - no swap needed
            self.memory.used_bytes = target_used_bytes;
            self.memory.available_bytes = self.memory.total_bytes - target_used_bytes;

            // Update free bytes (a portion of available bytes)
            let free_ratio = rng.gen_range(0.3..0.8);
            self.memory.free_bytes = (self.memory.available_bytes as f64 * free_ratio) as u64;

            // No swap usage when memory is below 100%
            if self.memory.swap_total_bytes > 0 {
                self.memory.swap_used_bytes = 0;
                self.memory.swap_free_bytes = self.memory.swap_total_bytes;
            }
        }

        // Small fluctuations in buffers and cache
        if self.memory.buffers_bytes > 0 {
            let buffer_delta = rng.gen_range(
                -(self.memory.total_bytes as i64 / 200)..(self.memory.total_bytes as i64 / 200),
            );
            self.memory.buffers_bytes = self
                .memory
                .buffers_bytes
                .saturating_add_signed(buffer_delta)
                .min(self.memory.total_bytes / 20);
        }

        if self.memory.cached_bytes > 0 {
            let cache_delta = rng.gen_range(
                -(self.memory.total_bytes as i64 / 100)..(self.memory.total_bytes as i64 / 100),
            );
            self.memory.cached_bytes = self
                .memory
                .cached_bytes
                .saturating_add_signed(cache_delta)
                .min(self.memory.total_bytes / 5);
        }

        // Change disk available bytes by a small amount, up to 1 GiB
        let delta = rng.gen_range(-(1024 * 1024 * 1024)..(1024 * 1024 * 1024));
        self.disk_available_bytes = self
            .disk_available_bytes
            .saturating_add_signed(delta)
            .min(self.disk_total_bytes);

        // Re-render response with new values
        self.render_response();
    }

    // Instant response serving (no processing, just return pre-rendered string)
    fn get_response(&self) -> &str {
        &self.rendered_response
    }
}

fn parse_port_range(range_str: &str) -> Result<RangeInclusive<u16>> {
    if let Some((start, end)) = range_str.split_once('-') {
        Ok(start.parse()?..=end.parse()?)
    } else {
        let port = range_str.parse()?;
        Ok(port..=port)
    }
}

async fn handle_request(
    _req: Request<hyper::body::Incoming>,
    nodes: Arc<Mutex<HashMap<u16, MockNode>>>,
    port: u16,
) -> Result<Response<String>, Infallible> {
    // Check if node is responding and copy response data
    let (is_responding, metrics) = {
        let nodes_guard = nodes.lock().unwrap();
        let node = nodes_guard.get(&port).unwrap();
        (node.is_responding, node.get_response().to_string())
    };

    // If node is not responding, simulate a connection timeout/error
    if !is_responding {
        // Return a 503 Service Unavailable to simulate failure
        let response = Response::builder()
            .status(503)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body("Service temporarily unavailable".to_string())
            .unwrap();
        return Ok(response);
    }

    // Build optimized HTTP response with performance headers
    let response = Response::builder()
        .status(200)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "max-age=2, must-revalidate") // Cache for 2 seconds
        .header("Connection", "keep-alive") // Enable connection reuse
        .header("Keep-Alive", "timeout=60, max=1000") // Keep connections alive
        .header("Content-Length", metrics.len().to_string()) // Explicit content length
        .body(metrics)
        .unwrap();

    Ok(response)
}

fn parse_platform_type(platform_str: &str) -> PlatformType {
    match platform_str.to_lowercase().as_str() {
        "nvidia" => PlatformType::Nvidia,
        "apple" => PlatformType::Apple,
        "jetson" => PlatformType::Jetson,
        "intel" => PlatformType::Intel,
        "amd" => PlatformType::Amd,
        _ => {
            eprintln!("Unknown platform '{platform_str}', defaulting to nvidia");
            PlatformType::Nvidia
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let port_range = match args.port_range {
        Some(range) => parse_port_range(&range)?,
        None => 10001..=10010,
    };

    let platform_type = parse_platform_type(&args.platform);
    let nodes = Arc::new(Mutex::new(HashMap::new()));
    let mut file = File::create(&args.o)?;
    let mut instance_counter = 1;

    for port in port_range.clone() {
        let instance_name = format!("node-{instance_counter:04}");
        let node = MockNode::new(instance_name, args.gpu_name.clone(), platform_type.clone());
        nodes.lock().unwrap().insert(port, node);
        writeln!(file, "localhost:{port}").unwrap();
        instance_counter += 1;
    }

    println!("Outputting server list to {}", args.o);

    // Start background updater task - updates all nodes every 3 seconds
    let nodes_updater = Arc::clone(&nodes);
    let updater_task = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(UPDATE_INTERVAL_SECS));
        loop {
            interval.tick().await;
            let mut nodes_guard = nodes_updater.lock().unwrap();
            for node in nodes_guard.values_mut() {
                node.update();
            }
        }
    });

    // Start failure simulation task if failure_nodes > 0
    let failure_task = if args.failure_nodes > 0 {
        let nodes_failure = Arc::clone(&nodes);
        let failure_count = args.failure_nodes;
        Some(tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // Every 10 seconds
            loop {
                interval.tick().await;
                let mut rng = rand::thread_rng(); // Create RNG inside the loop to avoid Send issues
                let mut nodes_guard = nodes_failure.lock().unwrap();
                let port_list: Vec<u16> = nodes_guard.keys().cloned().collect();

                if port_list.len() as u32 >= failure_count {
                    // Randomly select nodes to fail
                    let mut selected_ports = Vec::new();
                    while selected_ports.len() < failure_count as usize {
                        let port = port_list[rng.gen_range(0..port_list.len())];
                        if !selected_ports.contains(&port) {
                            selected_ports.push(port);
                        }
                    }

                    // Toggle failure state for all nodes
                    for (port, node) in nodes_guard.iter_mut() {
                        if selected_ports.contains(port) {
                            // Randomly fail/recover selected nodes
                            node.is_responding = rng.gen_bool(0.3); // 30% chance to be responding
                        } else {
                            // Non-selected nodes have higher chance to be responding
                            node.is_responding = rng.gen_bool(0.9); // 90% chance to be responding
                        }
                    }

                    let responding_count = nodes_guard.values().filter(|n| n.is_responding).count();
                    let total_count = nodes_guard.len();
                    println!(
                        "Failure simulation: {responding_count}/{total_count} nodes responding"
                    );
                }
            }
        }))
    } else {
        None
    };

    let mut servers = vec![];
    for port in port_range {
        let nodes_clone = Arc::clone(&nodes);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = TcpListener::bind(addr).await?;
        println!("Listening on http://{addr}");

        let server = tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let io = TokioIo::new(tcp);
                let nodes_clone = Arc::clone(&nodes_clone);

                let service =
                    service_fn(move |req| handle_request(req, Arc::clone(&nodes_clone), port));

                tokio::spawn(async move {
                    let builder = Builder::new(hyper_util::rt::TokioExecutor::new());
                    let conn = builder.serve_connection(io, service);

                    if let Err(err) = conn.await {
                        eprintln!("Connection failed: {err:?}");
                    }
                });
            }
        });
        servers.push(server);
    }

    if args.failure_nodes > 0 {
        println!(
            "Started {} servers with background updater (updates every {}s) and failure simulation ({} nodes)",
            servers.len(),
            UPDATE_INTERVAL_SECS,
            args.failure_nodes
        );
    } else {
        println!(
            "Started {} servers with background updater (updates every {}s)",
            servers.len(),
            UPDATE_INTERVAL_SECS
        );
    }

    // Run servers, updater, and failure simulation concurrently
    servers.push(updater_task);
    if let Some(failure_task) = failure_task {
        servers.push(failure_task);
    }
    join_all(servers).await;

    Ok(())
}
