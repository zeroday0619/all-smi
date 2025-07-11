//! Template building and rendering for Prometheus metrics

use crate::mock::constants::*;
use crate::mock::metrics::{CpuMetrics, GpuMetrics, MemoryMetrics, PlatformType};

/// Build static response template with placeholders (called once during init)
pub fn build_response_template(
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

    // GPU vendor info metrics (NVIDIA and Jetson)
    if matches!(platform, PlatformType::Nvidia | PlatformType::Jetson) {
        template.push_str("# HELP all_smi_gpu_info GPU vendor-specific information\n");
        template.push_str("# TYPE all_smi_gpu_info info\n");

        for (i, gpu) in gpus.iter().enumerate() {
            let mut labels = vec![
                format!("gpu=\"{}\"", gpu_name),
                format!("instance=\"{}\"", instance_name),
                format!("uuid=\"{}\"", gpu.uuid),
                format!("index=\"{}\"", i),
                format!("type=\"GPU\""), // Default to GPU, can be customized in the future
            ];

            // Add CUDA-specific labels based on platform
            match platform {
                PlatformType::Nvidia => {
                    // Determine architecture and capabilities based on GPU name
                    let (architecture, compute_capability, cuda_version, pcie_gen) =
                        if gpu_name.contains("H200") || gpu_name.contains("H100") {
                            ("Hopper", "9.0", "12.6", "5")
                        } else if gpu_name.contains("A100") || gpu_name.contains("A6000") {
                            ("Ampere", "8.0", "12.4", "4")
                        } else if gpu_name.contains("RTX 4090") || gpu_name.contains("RTX 4080") {
                            ("Ada Lovelace", "8.9", "12.3", "4")
                        } else if gpu_name.contains("V100") {
                            ("Volta", "7.0", "12.2", "3")
                        } else if gpu_name.contains("T4") {
                            ("Turing", "7.5", "12.1", "3")
                        } else {
                            ("Ampere", "8.6", "12.4", "4") // Default to A40-like specs
                        };

                    labels.push("driver_version=\"560.35.05\"".to_string());
                    labels.push(format!("cuda_version=\"{cuda_version}\""));
                    labels.push(format!("architecture=\"{architecture}\""));
                    labels.push(format!("compute_capability=\"{compute_capability}\""));
                    labels.push("compute_mode=\"Default\"".to_string());
                    labels.push("persistence_mode=\"Enabled\"".to_string());
                    labels.push("ecc_mode_current=\"Enabled\"".to_string());
                    labels.push("mig_mode_current=\"Disabled\"".to_string());
                    labels.push(format!("pcie_gen_current=\"{pcie_gen}\""));
                    labels.push(format!("pcie_gen_max=\"{pcie_gen}\""));
                    labels.push("pcie_width_current=\"16\"".to_string());
                    labels.push("pcie_width_max=\"16\"".to_string());
                    labels.push("performance_state=\"P0\"".to_string());
                    labels.push("vbios_version=\"96.00.61.00.01\"".to_string());
                }
                PlatformType::Jetson => {
                    // Determine Jetson variant based on GPU name
                    let (driver_version, cuda_version, compute_capability) =
                        if gpu_name.contains("Orin") {
                            ("36.2.0", "12.2", "8.7")
                        } else if gpu_name.contains("Xavier") {
                            ("35.4.1", "11.4", "7.2")
                        } else if gpu_name.contains("Nano") {
                            ("32.7.3", "10.2", "5.3")
                        } else {
                            ("35.4.1", "11.4", "7.2") // Default to Xavier
                        };

                    labels.push(format!("driver_version=\"{driver_version}\""));
                    labels.push(format!("cuda_version=\"{cuda_version}\""));
                    labels.push("architecture=\"Tegra\"".to_string());
                    labels.push(format!("compute_capability=\"{compute_capability}\""));
                }
                _ => {}
            }

            template.push_str(&format!("all_smi_gpu_info{{{}}} 1\n", labels.join(", ")));
        }

        // Add numeric metrics for NVIDIA GPUs
        if let PlatformType::Nvidia = platform {
            add_nvidia_numeric_metrics(&mut template, instance_name, gpu_name, gpus);
        }
    }

    // CPU metrics
    add_cpu_metrics(&mut template, instance_name, cpu, platform);

    // Memory metrics
    add_memory_metrics(&mut template, instance_name, memory);

    // Disk metrics
    add_disk_metrics(&mut template, instance_name);

    template
}

fn add_nvidia_numeric_metrics(
    template: &mut String,
    instance_name: &str,
    gpu_name: &str,
    gpus: &[GpuMetrics],
) {
    // Determine PCIe gen based on GPU model
    let pcie_gen = if gpu_name.contains("H200") || gpu_name.contains("H100") {
        5
    } else if gpu_name.contains("A100") || gpu_name.contains("A6000") || gpu_name.contains("RTX 40")
    {
        4
    } else {
        3
    };

    // Determine clock speeds and power limits based on GPU model
    let (max_graphics_clock, max_memory_clock, power_limit) =
        if gpu_name.contains("H200") || gpu_name.contains("H100") {
            (1980, 2619, 700)
        } else if gpu_name.contains("A100") {
            (1410, 1593, 400)
        } else if gpu_name.contains("RTX 4090") {
            (2520, 1313, 450)
        } else if gpu_name.contains("V100") {
            (1380, 877, 300)
        } else if gpu_name.contains("T4") {
            (1590, 1250, 70)
        } else {
            (1770, 1500, 300) // Default
        };

    // PCIe metrics
    template.push_str("# HELP all_smi_gpu_pcie_gen_current Current PCIe generation\n");
    template.push_str("# TYPE all_smi_gpu_pcie_gen_current gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_pcie_gen_current{{{labels}}} {pcie_gen}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_pcie_width_current Current PCIe link width\n");
    template.push_str("# TYPE all_smi_gpu_pcie_width_current gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!("all_smi_gpu_pcie_width_current{{{labels}}} 16\n"));
    }

    // Max clocks
    template.push_str("# HELP all_smi_gpu_clock_graphics_max_mhz Maximum graphics clock in MHz\n");
    template.push_str("# TYPE all_smi_gpu_clock_graphics_max_mhz gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_clock_graphics_max_mhz{{{labels}}} {max_graphics_clock}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_clock_memory_max_mhz Maximum memory clock in MHz\n");
    template.push_str("# TYPE all_smi_gpu_clock_memory_max_mhz gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_clock_memory_max_mhz{{{labels}}} {max_memory_clock}\n"
        ));
    }

    // Power limits
    template
        .push_str("# HELP all_smi_gpu_power_limit_current_watts Current power limit in watts\n");
    template.push_str("# TYPE all_smi_gpu_power_limit_current_watts gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_power_limit_current_watts{{{labels}}} {power_limit}\n"
        ));
    }

    template.push_str("# HELP all_smi_gpu_power_limit_max_watts Maximum power limit in watts\n");
    template.push_str("# TYPE all_smi_gpu_power_limit_max_watts gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        template.push_str(&format!(
            "all_smi_gpu_power_limit_max_watts{{{labels}}} {power_limit}\n"
        ));
    }

    // Performance state (dynamic based on utilization)
    template
        .push_str("# HELP all_smi_gpu_performance_state GPU performance state (P0=0, P1=1, ...)\n");
    template.push_str("# TYPE all_smi_gpu_performance_state gauge\n");
    for (i, gpu) in gpus.iter().enumerate() {
        let labels = format!(
            "gpu=\"{}\", instance=\"{}\", uuid=\"{}\", index=\"{}\"",
            gpu_name, instance_name, gpu.uuid, i
        );
        let placeholder = format!("{{{{PSTATE_{i}}}}}");
        template.push_str(&format!(
            "all_smi_gpu_performance_state{{{labels}}} {placeholder}\n"
        ));
    }
}

fn add_cpu_metrics(
    template: &mut String,
    instance_name: &str,
    cpu: &CpuMetrics,
    platform: &PlatformType,
) {
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
        template.push_str("# HELP all_smi_cpu_temperature_celsius CPU temperature in celsius\n");
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

            template.push_str("# HELP all_smi_cpu_gpu_core_count Apple Silicon GPU core count\n");
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
}

fn add_memory_metrics(template: &mut String, instance_name: &str, memory: &MemoryMetrics) {
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

    template.push_str("# HELP all_smi_memory_available_bytes Available system memory in bytes\n");
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
        template.push_str("# HELP all_smi_memory_buffers_bytes Memory used for buffers in bytes\n");
        template.push_str("# TYPE all_smi_memory_buffers_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_buffers_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_BUFFERS}\n"
        ));
    }

    if memory.cached_bytes > 0 {
        template.push_str("# HELP all_smi_memory_cached_bytes Memory used for cache in bytes\n");
        template.push_str("# TYPE all_smi_memory_cached_bytes gauge\n");
        template.push_str(&format!(
            "all_smi_memory_cached_bytes{{{memory_labels}}} {PLACEHOLDER_SYS_MEMORY_CACHED}\n"
        ));
    }
}

fn add_disk_metrics(template: &mut String, instance_name: &str) {
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
}

/// Render the response by replacing placeholders with actual values
pub fn render_response(
    template: &str,
    gpus: &[GpuMetrics],
    cpu: &CpuMetrics,
    memory: &MemoryMetrics,
    disk_available_bytes: u64,
    disk_total_bytes: u64,
    platform: &PlatformType,
) -> String {
    let mut response = template.to_string();

    // Replace GPU metrics
    for (i, gpu) in gpus.iter().enumerate() {
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
        if let PlatformType::Apple = platform {
            response = response.replace(
                &format!("{{{{ANE_{i}}}}}"),
                &format!("{:.3}", gpu.ane_utilization_watts),
            );
        }

        // Replace performance state for NVIDIA GPUs (based on utilization)
        if let PlatformType::Nvidia = platform {
            let pstate = if gpu.utilization > 80.0 {
                0 // P0 - Maximum performance
            } else if gpu.utilization > 50.0 {
                2 // P2 - Balanced
            } else if gpu.utilization > 20.0 {
                5 // P5 - Auto
            } else if gpu.utilization > 0.0 {
                8 // P8 - Adaptive
            } else {
                12 // P12 - Idle
            };
            response = response.replace(&format!("{{{{PSTATE_{i}}}}}"), &pstate.to_string());
        }
    }

    // Replace CPU metrics
    response = response
        .replace(PLACEHOLDER_CPU_UTIL, &format!("{:.2}", cpu.utilization))
        .replace(
            PLACEHOLDER_CPU_SOCKET0_UTIL,
            &format!(
                "{:.2}",
                cpu.socket_utilizations.first().copied().unwrap_or(0.0)
            ),
        )
        .replace(
            PLACEHOLDER_CPU_SOCKET1_UTIL,
            &format!(
                "{:.2}",
                cpu.socket_utilizations.get(1).copied().unwrap_or(0.0)
            ),
        );

    if let Some(temp) = cpu.temperature_celsius {
        response = response.replace(PLACEHOLDER_CPU_TEMP, &temp.to_string());
    }

    if let Some(power) = cpu.power_consumption_watts {
        response = response.replace(PLACEHOLDER_CPU_POWER, &format!("{power:.3}"));
    }

    // Apple Silicon specific replacements
    if let PlatformType::Apple = platform {
        if let (Some(p_util), Some(e_util)) = (cpu.p_core_utilization, cpu.e_core_utilization) {
            response = response
                .replace(PLACEHOLDER_CPU_P_CORE_UTIL, &format!("{p_util:.2}"))
                .replace(PLACEHOLDER_CPU_E_CORE_UTIL, &format!("{e_util:.2}"));
        }
    }

    // Replace memory metrics
    response = response
        .replace(PLACEHOLDER_SYS_MEMORY_USED, &memory.used_bytes.to_string())
        .replace(
            PLACEHOLDER_SYS_MEMORY_AVAILABLE,
            &memory.available_bytes.to_string(),
        )
        .replace(PLACEHOLDER_SYS_MEMORY_FREE, &memory.free_bytes.to_string())
        .replace(
            PLACEHOLDER_SYS_MEMORY_UTIL,
            &format!("{:.2}", memory.utilization),
        );

    // Replace swap metrics if available
    if memory.swap_total_bytes > 0 {
        response = response
            .replace(
                PLACEHOLDER_SYS_SWAP_USED,
                &memory.swap_used_bytes.to_string(),
            )
            .replace(
                PLACEHOLDER_SYS_SWAP_FREE,
                &memory.swap_free_bytes.to_string(),
            );
    }

    // Replace buffer and cache metrics if available
    if memory.buffers_bytes > 0 {
        response = response.replace(
            PLACEHOLDER_SYS_MEMORY_BUFFERS,
            &memory.buffers_bytes.to_string(),
        );
    }

    if memory.cached_bytes > 0 {
        response = response.replace(
            PLACEHOLDER_SYS_MEMORY_CACHED,
            &memory.cached_bytes.to_string(),
        );
    }

    // Replace disk metrics
    response = response
        .replace(PLACEHOLDER_DISK_TOTAL, &disk_total_bytes.to_string())
        .replace(PLACEHOLDER_DISK_AVAIL, &disk_available_bytes.to_string());

    response
}
