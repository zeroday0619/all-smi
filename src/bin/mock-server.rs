use anyhow::Result;
use clap::Parser;
use futures_util::future::join_all;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use rand::Rng;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::interval;

const DEFAULT_GPU_NAME: &str = "NVIDIA H200 144GB HBM3";
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
        short,
        long,
        default_value = "hosts.csv",
        help = "Output CSV file name"
    )]
    o: String,
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
}

// High-performance template-based mock node
#[allow(dead_code)]
struct MockNode {
    instance_name: String,
    gpu_name: String,
    gpus: Vec<GpuMetrics>,
    disk_available_bytes: u64,
    disk_total_bytes: u64,
    response_template: String,
    rendered_response: String,
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
const PLACEHOLDER_DISK_AVAIL: &str = "{{DISK_AVAIL}}";
const PLACEHOLDER_DISK_TOTAL: &str = "{{DISK_TOTAL}}";

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
    fn new(instance_name: String, gpu_name: String) -> Self {
        let gpu_memory_gb = Self::extract_gpu_memory_gb(&gpu_name);
        let memory_total_bytes = gpu_memory_gb * 1024 * 1024 * 1024;

        let mut rng = rand::thread_rng();

        // Choose random disk size from options
        let disk_sizes = [DISK_SIZE_1TB, DISK_SIZE_4TB, DISK_SIZE_12TB];
        let disk_total_bytes = disk_sizes[rng.gen_range(0..disk_sizes.len())];

        let gpus: Vec<GpuMetrics> = (0..NUM_GPUS)
            .map(|_| GpuMetrics {
                uuid: generate_uuid(),
                utilization: rng.gen_range(10.0..90.0),
                memory_used_bytes: rng
                    .gen_range(memory_total_bytes / 10..memory_total_bytes * 9 / 10),
                memory_total_bytes,
                temperature_celsius: rng.gen_range(45..75),
                power_consumption_watts: rng.gen_range(200.0..600.0),
                frequency_mhz: rng.gen_range(1000..1980),
            })
            .collect();

        // Build response template once during initialization
        let response_template = Self::build_response_template(&instance_name, &gpu_name, &gpus);

        let mut node = Self {
            instance_name,
            gpu_name,
            gpus,
            disk_available_bytes: rng.gen_range(disk_total_bytes / 10..disk_total_bytes * 9 / 10),
            disk_total_bytes,
            response_template,
            rendered_response: String::new(),
        };

        // Render initial response
        node.render_response();
        node
    }

    fn extract_gpu_memory_gb(gpu_name: &str) -> u64 {
        // Extract memory size from GPU name (e.g., "NVIDIA H200 144GB HBM3" -> 144)
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
    fn build_response_template(instance_name: &str, gpu_name: &str, gpus: &[GpuMetrics]) -> String {
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
            template.push_str(&format!("# HELP {} {}\n", metric_name, help_text));
            template.push_str(&format!("# TYPE {} gauge\n", metric_name));

            for (i, gpu) in gpus.iter().enumerate() {
                let labels = format!(
                    "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
                    gpu_name, instance_name, gpu.uuid, i
                );

                let placeholder = match metric_name {
                    "all_smi_gpu_utilization" => format!("{{{{UTIL_{}}}}}", i),
                    "all_smi_gpu_memory_used_bytes" => format!("{{{{MEM_USED_{}}}}}", i),
                    "all_smi_gpu_memory_total_bytes" => format!("{{{{MEM_TOTAL_{}}}}}", i),
                    "all_smi_gpu_temperature_celsius" => format!("{{{{TEMP_{}}}}}", i),
                    "all_smi_gpu_power_consumption_watts" => format!("{{{{POWER_{}}}}}", i),
                    "all_smi_gpu_frequency_mhz" => format!("{{{{FREQ_{}}}}}", i),
                    _ => "0".to_string(),
                };

                template.push_str(&format!("{}{{{}}} {}\n", metric_name, labels, placeholder));
            }
        }

        // Disk metrics
        template.push_str("# HELP all_smi_disk_total_bytes Total disk space in bytes\n");
        template.push_str("# TYPE all_smi_disk_total_bytes gauge\n");
        let disk_labels = format!(
            "instance=\"{}\", mount_point=\"/\", index=\"0\"",
            instance_name
        );
        template.push_str(&format!(
            "all_smi_disk_total_bytes{{{}}} {}\n",
            disk_labels, PLACEHOLDER_DISK_TOTAL
        ));

        template.push_str("# HELP all_smi_disk_available_bytes Available disk space in bytes\n");
        template.push_str("# TYPE all_smi_disk_available_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_disk_available_bytes{{{}}} {}\n",
            disk_labels, PLACEHOLDER_DISK_AVAIL
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
                    &format!("{{{{UTIL_{}}}}}", i),
                    &format!("{:.2}", gpu.utilization),
                )
                .replace(
                    &format!("{{{{MEM_USED_{}}}}}", i),
                    &gpu.memory_used_bytes.to_string(),
                )
                .replace(
                    &format!("{{{{MEM_TOTAL_{}}}}}", i),
                    &gpu.memory_total_bytes.to_string(),
                )
                .replace(
                    &format!("{{{{TEMP_{}}}}}", i),
                    &gpu.temperature_celsius.to_string(),
                )
                .replace(
                    &format!("{{{{POWER_{}}}}}", i),
                    &format!("{:.3}", gpu.power_consumption_watts),
                )
                .replace(
                    &format!("{{{{FREQ_{}}}}}", i),
                    &gpu.frequency_mhz.to_string(),
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

            // GPU temperature: 40-80°C, change by less than 2°C
            let temp_delta = rng.gen_range(-2..=2);
            gpu.temperature_celsius =
                (gpu.temperature_celsius as i32 + temp_delta).clamp(40, 80) as u32;

            // GPU power: change by less than 50W
            let power_delta = rng.gen_range(-50.0..50.0);
            gpu.power_consumption_watts =
                (gpu.power_consumption_watts + power_delta).clamp(100.0, 700.0);

            // GPU frequency: small changes
            let freq_delta = rng.gen_range(-50..50);
            gpu.frequency_mhz = (gpu.frequency_mhz as i32 + freq_delta).clamp(1000, 1980) as u32;
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
    _req: Request<Body>,
    nodes: Arc<Mutex<HashMap<u16, MockNode>>>,
    port: u16,
) -> Result<Response<Body>, Infallible> {
    // Copy response data to own it (avoiding lifetime issues)
    let metrics = {
        let nodes_guard = nodes.lock().unwrap();
        let node = nodes_guard.get(&port).unwrap();
        node.get_response().to_string() // Copy the string to own it
    };

    // Build optimized HTTP response with performance headers
    let response = Response::builder()
        .status(200)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "max-age=2, must-revalidate") // Cache for 2 seconds
        .header("Connection", "keep-alive") // Enable connection reuse
        .header("Keep-Alive", "timeout=60, max=1000") // Keep connections alive
        .header("Content-Length", metrics.len().to_string()) // Explicit content length
        .body(Body::from(metrics))
        .unwrap();

    Ok(response)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let port_range = match args.port_range {
        Some(range) => parse_port_range(&range)?,
        None => 10001..=10010,
    };

    let nodes = Arc::new(Mutex::new(HashMap::new()));
    let mut file = File::create(&args.o)?;
    let mut instance_counter = 1;

    for port in port_range.clone() {
        let instance_name = format!("node-{:04}", instance_counter);
        let node = MockNode::new(instance_name, args.gpu_name.clone());
        nodes.lock().unwrap().insert(port, node);
        writeln!(file, "localhost:{}", port).unwrap();
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

    let mut servers = vec![];
    for port in port_range {
        let nodes_clone = Arc::clone(&nodes);
        let make_svc = make_service_fn(move |_conn| {
            let nodes_clone = Arc::clone(&nodes_clone);
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    handle_request(req, Arc::clone(&nodes_clone), port)
                }))
            }
        });

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let server = Server::bind(&addr).serve(make_svc);
        println!("Listening on http://{}", addr);
        servers.push(tokio::spawn(server));
    }

    println!(
        "Started {} servers with background updater (updates every {}s)",
        servers.len(),
        UPDATE_INTERVAL_SECS
    );

    // Run servers and updater concurrently
    servers.push(updater_task);
    join_all(servers).await;

    Ok(())
}
