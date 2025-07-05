use regex::Regex;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod gpu;

use std::cmp::Ordering;
use std::fs;
use std::io::{stdout, Write};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use axum::{extract::State, routing::get, Router};
use chrono::Local;
use clap::{Parser, Subcommand};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, size, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::{Any, CorsLayer};
use sysinfo::Disks;

use crate::gpu::{get_gpu_readers, GpuInfo, ProcessInfo};

fn get_hostname() -> String {
    let output = Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

// Filter out unnecessary disk partitions
fn should_include_disk(mount_point: &str) -> bool {
    // Exclude common system partitions that don't need monitoring
    let excluded_patterns = [
        "/System/Volumes/Data",  // macOS system partition
        "/System/Volumes/VM",    // macOS VM partition
        "/System/Volumes/Preboot", // macOS preboot partition
        "/System/Volumes/Update", // macOS update partition
        "/System/Volumes/xarts", // macOS xarts partition
        "/System/Volumes/iSCPreboot", // macOS iSC preboot partition
        "/System/Volumes/Hardware", // macOS hardware partition
        "/System/Volumes/Data/home", // macOS auto_home mount
        "/boot/efi",             // Linux EFI boot partition
        "/boot",                 // Linux boot partition
        "/dev",                  // Device filesystem
        "/proc",                 // Process filesystem
        "/sys",                  // System filesystem
        "/run",                  // Runtime filesystem
        "/snap/",                // Snap package mounts
        "/var/lib/docker/",      // Docker overlay mounts
    ];
    
    for pattern in &excluded_patterns {
        if mount_point.starts_with(pattern) {
            return false;
        }
    }
    
    // Include root filesystem and /Volumes/ mounts (external drives, etc.)
    // Exclude temporary filesystems and virtual filesystems
    if mount_point == "/" {
        return true;
    }
    if mount_point.starts_with("/Volumes/") {
        return true;
    }
    if mount_point.starts_with("/home") || mount_point.starts_with("/var") || mount_point.starts_with("/usr") {
        return true;
    }
    
    // For other mount points, be more selective
    mount_point.starts_with('/') && 
        !mount_point.starts_with("/tmp") && 
        !mount_point.starts_with("/var/tmp") &&
        !mount_point.contains("/snap/") &&
        !mount_point.contains("/docker/")
}

#[derive(Clone)]
struct StorageInfo {
    mount_point: String,
    total_bytes: u64,
    available_bytes: u64,
    hostname: String,
    index: u32,
}

/// A command-line tool to monitor GPU usage, similar to nvidia-smi, but for all GPUs.
#[derive(Parser)]
#[command(author, version, about, long_about = None, arg_required_else_help(true))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run in API mode, exposing metrics in Prometheus format.
    Api(ApiArgs),
    /// Run in view mode, displaying a TUI. (default)
    View(ViewArgs),
}

/// Arguments for the API mode.
#[derive(clap::Args)]
struct ApiArgs {
    /// The port to listen on for the API server.
    #[arg(short, long, default_value_t = 9090)]
    port: u16,
    /// The interval in seconds at which to update the GPU information.
    #[arg(short, long, default_value_t = 3)]
    interval: u64,
    /// Include the process list in the API output.
    #[arg(long)]
    processes: bool,
}

/// Arguments for the view mode.
#[derive(clap::Args, Clone)]
struct ViewArgs {
    /// A list of host addresses to connect to for remote monitoring.
    #[arg(long, num_args = 1..)]
    hosts: Option<Vec<String>>,
    /// A file containing a list of host addresses to connect to for remote monitoring.
    #[arg(long)]
    hostfile: Option<String>,
}

#[derive(Clone)]
struct AppState {
    gpu_info: Vec<GpuInfo>,
    process_info: Vec<ProcessInfo>,
    selected_process_index: usize,
    start_index: usize,
    sort_criteria: SortCriteria,
    loading: bool,
    tabs: Vec<String>,
    current_tab: usize,
    gpu_scroll_offset: usize,
    tab_scroll_offset: usize,
    device_name_scroll_offsets: std::collections::HashMap<String, usize>,
    hostname_scroll_offsets: std::collections::HashMap<String, usize>,
    frame_counter: u64,
    storage_info: Vec<StorageInfo>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            gpu_info: Vec::new(),
            process_info: Vec::new(),
            selected_process_index: 0,
            start_index: 0,
            sort_criteria: SortCriteria::Pid,
            loading: true,
            tabs: vec!["All".to_string()],
            current_tab: 0,
            gpu_scroll_offset: 0,
            tab_scroll_offset: 0,
            device_name_scroll_offsets: std::collections::HashMap::new(),
            hostname_scroll_offsets: std::collections::HashMap::new(),
            frame_counter: 0,
            storage_info: Vec::new(),
        }
    }
}

fn ensure_sudo_permissions() {
    if cfg!(target_os = "macos") {
        let status = Command::new("sudo")
            .arg("-v")
            .status()
            .expect("Failed to execute sudo command");

        if !status.success() {
            println!("Failed to acquire sudo privileges.");
            std::process::exit(1);
        }
    }
}

fn print_colored_text<W: Write>(
    stdout: &mut W,
    text: &str,
    fg_color: Color,
    bg_color: Option<Color>,
    width: Option<usize>,
) {
    let adjusted_text = if let Some(w) = width {
        if text.len() > w {
            text.chars().take(w).collect::<String>()
        } else {
            format!("{:<width$}", text, width = w)
        }
    } else {
        text.to_string()
    };

    if let Some(bg) = bg_color {
        queue!(
            stdout,
            SetForegroundColor(fg_color),
            SetBackgroundColor(bg),
            Print(adjusted_text),
            ResetColor
        )
        .unwrap();
    } else {
        queue!(
            stdout,
            SetForegroundColor(fg_color),
            Print(adjusted_text),
            ResetColor
        )
        .unwrap();
    }
}

fn draw_bar<W: Write>(
    stdout: &mut W,
    label: &str,
    value: f64,
    max_value: f64,
    width: usize,
    show_text: Option<String>,
) {
    let label_width = label.len();
    let text_width = show_text.as_ref().map_or(0, |text| text.len());
    let available_bar_width = width.saturating_sub(label_width + 4);

    let full_blocks = (value / max_value * available_bar_width as f64).floor() as usize;
    let remainder = (value / max_value * available_bar_width as f64) - full_blocks as f64;
    let filled_char = match remainder {
        r if r > 0.875 => "▉",
        r if r > 0.625 => "▊",
        r if r > 0.375 => "▋",
        r if r > 0.125 => "▌",
        _ => "▏",
    };
    let empty_width = available_bar_width - full_blocks - text_width;

    let filled_bar = format!(
        "{}{}",
        "▉".repeat(full_blocks),
        if full_blocks < available_bar_width {
            filled_char
        } else {
            ""
        }
    );
    let empty_bar = "▏".repeat(empty_width);

    // Use different colors for storage bars
    let (label_color, bar_color) = if label == "DSK" {
        (Color::Yellow, Color::Yellow)
    } else {
        (Color::Blue, Color::Green)
    };

    print_colored_text(stdout, &format!("{}: [", label), label_color, None, None);
    print_colored_text(stdout, &filled_bar, bar_color, None, None);
    print_colored_text(stdout, &empty_bar, bar_color, None, None);

    if let Some(text) = show_text {
        print_colored_text(stdout, &text, Color::White, None, Some(text_width));
    }

    queue!(stdout, Print("] ")).unwrap();
}

fn draw_tabs<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    queue!(stdout, cursor::MoveTo(0, 10)).unwrap();

    // Always draw the 'All' tab
    let (fg_color, bg_color) = if state.current_tab == 0 {
        (Color::Black, Some(Color::Cyan))
    } else {
        (Color::White, None)
    };
    print_colored_text(stdout, " All ", fg_color, bg_color, None);

    let mut available_width = cols.saturating_sub(5);

    for (i, tab) in state
        .tabs
        .iter()
        .enumerate()
        .skip(1)
        .skip(state.tab_scroll_offset)
    {
        let tab_text = format!(" {} ", tab);
        let tab_width = tab_text.len() as u16;

        if available_width < tab_width {
            break;
        }

        let (fg_color, bg_color) = if i == state.current_tab {
            (Color::Black, Some(Color::Cyan))
        } else {
            (Color::White, None)
        };
        print_colored_text(stdout, &tab_text, fg_color, bg_color, None);

        available_width -= tab_width;
    }

    queue!(stdout, Print("\r\n")).unwrap();
}

fn print_gpu_info<W: Write>(
    stdout: &mut W,
    index: usize,
    info: &GpuInfo,
    width: usize,
    device_name_scroll_offset: usize,
    hostname_scroll_offset: usize,
) {
    const GIB_DIVISOR: f64 = 1024.0 * 1024.0 * 1024.0;

    let used_memory_gib = info.used_memory as f64 / GIB_DIVISOR;
    let total_memory_gib = info.total_memory as f64 / GIB_DIVISOR;
    let memory_text = format!("{:.2}/{:.2}Gi", used_memory_gib, total_memory_gib);
    let gpu_percentage_text = format!("{:.2}%", info.utilization);
    let freq_text = format!("{} MHz", info.frequency);
    let power_text = format!("{:.2} W", info.power_consumption);
    let _time = &info.time; // Keep for other device support

    let mut labels = Vec::new();

    // Helper function to add a label and value pair to the labels vector
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
    ) {
        labels.push((label.to_string(), label_color));
        labels.push((value, Color::White));
    }

    // Adding device, memory, temperature, frequency, and power information
    let hostname = if info.hostname.len() > 9 {
        let extended_hostname = format!("{}   ", info.hostname);
        let start = hostname_scroll_offset % extended_hostname.len();
        let scrolled_name = extended_hostname.chars().cycle().skip(start).take(9).collect::<String>();
        scrolled_name
    } else {
        format!("{:<9}", info.hostname)
    };

    add_label(
        &mut labels,
        "HOST: ",
        format!("{}  ", hostname),
        Color::Blue,
    );

    let device_name = if info.name.len() > 15 {
        let extended_name = format!("{}   ", info.name);
        let start = device_name_scroll_offset % extended_name.len();
        let scrolled_name = extended_name.chars().cycle().skip(start).take(15).collect::<String>();
        scrolled_name
    } else {
        format!("{:<15}", info.name)
    };

    add_label(
        &mut labels,
        &format!("DEVICE {}: ", index + 1),
        format!("{}  ", device_name),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "Total: ",
        format!("{:.2} GiB  ", total_memory_gib),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "Used: ",
        format!("{:.2} GiB  ", used_memory_gib),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "Temp.: ",
        format!("{}°C  ", info.temperature),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "FREQ: ",
        format!("{}  ", freq_text),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "POW: ",
        format!("{} ", power_text),
        Color::Blue,
    );

    // Check if driver_version exists in the detail map and add it to labels
    if let Some(driver_version) = info.detail.get("driver_version") {
        add_label(
            &mut labels,
            "DRIV: ",
            format!("{} ", driver_version),
            Color::Blue,
        );
    }

    labels.push((String::from("\r\n"), Color::White));

    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }

    // The overflow is 2 characters per bar.
    let w1 = (width / 3).saturating_sub(2);
    let w2 = (width / 3).saturating_sub(2);
    let w3 = (width - (width / 3) * 2).saturating_sub(2);

    draw_bar(
        stdout,
        "GPU",
        info.utilization,
        100.0,
        w1,
        Some(gpu_percentage_text),
    );

    if let Some(dla_util) = info.dla_utilization {
        draw_bar(
            stdout,
            "DLA",
            dla_util,
            100.0,
            w2,
            Some(format!("{:.2}%", dla_util)),
        );
    } else if info.name.starts_with("Apple") {
        draw_bar(
            stdout,
            "ANE",
            info.ane_utilization,
            1000.0,
            w2,
            Some(format!("{:.2}W", info.ane_utilization / 1000.0)),
        );
    }

    draw_bar(
        stdout,
        "MEM",
        used_memory_gib,
        total_memory_gib,
        w3,
        Some(memory_text),
    );

    queue!(stdout, Print("\r\n")).unwrap(); // Move cursor to the start of the next line
}

fn print_storage_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &StorageInfo,
    width: usize,
) {
    const GIB_DIVISOR: f64 = 1024.0 * 1024.0 * 1024.0;
    const TIB_DIVISOR: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

    let used_bytes = info.total_bytes - info.available_bytes;
    let used_gib = used_bytes as f64 / GIB_DIVISOR;
    let total_gib = info.total_bytes as f64 / GIB_DIVISOR;
    let used_tib = used_bytes as f64 / TIB_DIVISOR;
    let total_tib = info.total_bytes as f64 / TIB_DIVISOR;

    let (used_text, total_text, storage_text) = if total_tib >= 1.0 {
        (
            format!("{:.1}T", used_tib),
            format!("{:.1}T", total_tib),
            format!("{:.1}/{:.1}T", used_tib, total_tib),
        )
    } else {
        (
            format!("{:.0}G", used_gib),
            format!("{:.0}G", total_gib),
            format!("{:.0}/{:.0}G", used_gib, total_gib),
        )
    };

    let mut labels = Vec::new();

    // Helper function to add a label and value pair to the labels vector
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
    ) {
        labels.push((label.to_string(), label_color));
        labels.push((value, Color::White));
    }

    // Show mount point more prominently for multiple disks
    let mount_display = if info.mount_point == "/" {
        "Root".to_string()
    } else if info.mount_point.len() > 20 {
        format!("{}...", &info.mount_point[..17])
    } else {
        info.mount_point.clone()
    };

    add_label(
        &mut labels,
        &format!("DISK {}: ", info.index + 1),
        format!("{}  ", mount_display),
        Color::Yellow,
    );
    add_label(
        &mut labels,
        "Total: ",
        format!("{}  ", total_text),
        Color::Yellow,
    );
    add_label(
        &mut labels,
        "Used: ",
        format!("{}  ", used_text),
        Color::Yellow,
    );

    labels.push((String::from("\r\n"), Color::White));

    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }

    // Use full width for storage bar
    let w = width.saturating_sub(2);

    draw_bar(
        stdout,
        "DSK",
        used_bytes as f64,
        info.total_bytes as f64,
        w,
        Some(storage_text),
    );

    queue!(stdout, Print("\r\n")).unwrap(); // Move cursor to the start of the next line
}

fn draw_node_square<W: Write>(
    stdout: &mut W,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    utilization: f64,
    is_selected: bool,
) {
    // Add minimum baseline for visibility (ensure at least 20% is always shown for single-unit height)
    let adjusted_utilization = if utilization == 0.0 { 20.0 } else { utilization };
    let fill_height = height as f64 * adjusted_utilization / 100.0;
    let full_rows = fill_height.floor() as u16;
    let partial_fill = fill_height - full_rows as f64;

    let partial_chars = [" ", " ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    let partial_char_index = (partial_fill * 8.0).round() as usize;
    let partial_char = partial_chars[partial_char_index.min(partial_chars.len() - 1)];

    let color = if is_selected {
        Color::Yellow
    } else if utilization == 0.0 {
        // Use a dimmer color for idle nodes to distinguish from active ones
        Color::DarkGreen
    } else {
        Color::Green
    };

    for i in 0..height {
        let current_row_y = y + height - 1 - i;
        queue!(stdout, cursor::MoveTo(x, current_row_y)).unwrap();
        if i < full_rows {
            print_colored_text(stdout, &"█".repeat(width as usize), color, None, None);
        } else if i == full_rows {
            print_colored_text(
                stdout,
                &partial_char.repeat(width as usize),
                color,
                None,
                None,
            );
        } else {
            print_colored_text(
                stdout,
                &"░".repeat(width as usize),
                Color::DarkGrey,
                None,
                None,
            );
        }
    }
}

fn draw_system_view<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let mut host_utilization: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    
    // Initialize all known hosts from tabs (excluding "All" tab)
    for tab in &state.tabs {
        if tab != "All" {
            host_utilization.insert(tab.clone(), (0.0, 0));
        }
    }
    
    // Update with actual GPU utilization data
    for gpu in &state.gpu_info {
        let entry = host_utilization
            .entry(gpu.hostname.clone())
            .or_insert((0.0, 0));
        entry.0 += gpu.utilization;
        entry.1 += 1;
    }

    let mut host_avg_utilization: Vec<(String, f64)> = host_utilization
        .into_iter()
        .map(|(host, (total_util, count))| {
            if count > 0 {
                (host, total_util / count as f64)
            } else {
                // Node with no GPUs or all GPUs idle - show 0% utilization
                (host, 0.0)
            }
        })
        .collect();

    host_avg_utilization.sort_by(|a, b| a.0.cmp(&b.0));

    const SQUARE_WIDTH: u16 = 1;
    const SQUARE_HEIGHT: u16 = 1;
    const NODE_COL_SPACING: u16 = 1;
    const MAX_Y: u16 = 4;

    let mut x: u16 = 1;
    let mut y: u16 = 2;
    let max_x = cols / 2;

    for (_hostname, avg_util) in &host_avg_utilization {
        if x + SQUARE_WIDTH > max_x {
            break; // No more space horizontally
        }

        let is_selected = if state.current_tab > 0 {
            _hostname == &state.tabs[state.current_tab]
        } else {
            false
        };

        draw_node_square(
            stdout,
            x,
            y,
            SQUARE_WIDTH,
            SQUARE_HEIGHT,
            *avg_util,
            is_selected,
        );

        y += SQUARE_HEIGHT;
        if y > MAX_Y {
            y = 2;
            x += SQUARE_WIDTH + NODE_COL_SPACING;
        }
    }
}

fn draw_dashboard_items<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let num_nodes = state.gpu_info.iter().map(|g| &g.hostname).collect::<std::collections::HashSet<_>>().len();
    let total_gpus = state.gpu_info.len();
    let total_utilization: f64 = state.gpu_info.iter().map(|g| g.utilization).sum();
    let avg_gpu_util = if total_gpus > 0 { total_utilization / total_gpus as f64 } else { 0.0 };
    let total_used_memory: u64 = state.gpu_info.iter().map(|g| g.used_memory).sum();
    let total_total_memory: u64 = state.gpu_info.iter().map(|g| g.total_memory).sum();
    let avg_mem_util = if total_total_memory > 0 { (total_used_memory as f64 / total_total_memory as f64) * 100.0 } else { 0.0 };
    let total_power: f64 = state.gpu_info.iter().map(|g| g.power_consumption).sum();
    let hottest_gpu = state.gpu_info.iter().max_by(|a, b| a.temperature.cmp(&b.temperature));

    let dashboard_x = cols / 2 + 2;
    let mut y = 1;

    let power_text = format!("Total Power: {:.2}kW", total_power / 1000.0);
    let hottest_text = if let Some(gpu) = hottest_gpu {
        let mut base_text = format!("Hottest GPU: {}°C", gpu.temperature);
        let remaining_space = 24_usize.saturating_sub(base_text.len());

        if remaining_space > 3 { // Need at least space for ' (…)'
            let max_hostname_len = remaining_space.saturating_sub(3);
            let mut hostname = gpu.hostname.clone();
            if hostname.len() > max_hostname_len {
                hostname.truncate(max_hostname_len.saturating_sub(1));
                hostname.push('…');
            }
            base_text.push_str(&format!(" ({})", hostname));
        }
        base_text
    } else {
        "Hottest GPU: N/A".to_string()
    };

    queue!(stdout, cursor::MoveTo(dashboard_x, y)).unwrap();
    print_colored_text(stdout, "┌──────────────────────────┐", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x, y + 1)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", power_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x, y + 2)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", hottest_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x, y + 3)).unwrap();
    print_colored_text(stdout, "└──────────────────────────┘", Color::DarkGrey, None, None);

    let dashboard_x2 = dashboard_x + 30;
    y = 1;

    let nodes_text = format!("Nodes: {}", num_nodes);
    let gpus_text = format!("Total GPUs: {}", total_gpus);
    let avg_gpu_text = format!("Avg GPU Util: {:.2}%", avg_gpu_util);
    let avg_mem_text = format!("Avg Mem Util: {:.2}%", avg_mem_util);

    queue!(stdout, cursor::MoveTo(dashboard_x2, y)).unwrap();
    print_colored_text(stdout, "┌──────────────────────────┐", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 1)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", nodes_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 2)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", gpus_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 3)).unwrap();
    print_colored_text(stdout, "├──────────────────────────┤", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 4)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", avg_gpu_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 5)).unwrap();
    print_colored_text(stdout, "│ ", Color::DarkGrey, None, None);
    print_colored_text(stdout, &format!("{:<24}", avg_mem_text), Color::White, None, None);
    print_colored_text(stdout, " │", Color::DarkGrey, None, None);
    queue!(stdout, cursor::MoveTo(dashboard_x2, y + 6)).unwrap();
    print_colored_text(stdout, "└──────────────────────────┘", Color::DarkGrey, None, None);
}

fn print_function_keys<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    let key_width: usize = 3; // Width for each function key label
    let total_width: usize = cols as usize; // Total width of the terminal
    let min_label_width: usize = 5; // Minimum width for label text
    let label_width = (total_width / 10)
        .saturating_sub(key_width)
        .max(min_label_width); // Ensure label_width is at least min_label_width

    let function_keys = vec!["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

    let labels = vec!["Help", "", "", "", "", "", "", "", "", "Quit"];

    queue!(stdout, cursor::MoveTo(0, rows.saturating_sub(1))).unwrap();

    for (index, key) in function_keys.iter().enumerate() {
        print_colored_text(
            stdout,
            key,
            Color::White,
            Some(Color::Black),
            Some(key_width),
        );
        print_colored_text(
            stdout,
            labels[index],
            Color::Black,
            Some(Color::Cyan),
            Some(label_width),
        );
    }
}

fn print_loading_indicator<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    let loading_text = "Loading...";
    let text_len = loading_text.len() as u16;
    let x = (cols - text_len) / 2;
    let y = rows / 2;
    queue!(stdout, cursor::MoveTo(x, y)).unwrap();
    print_colored_text(stdout, loading_text, Color::White, None, None);
}

fn print_process_info<
    W: Write,
>(
    stdout: &mut W,
    processes: &[ProcessInfo],
    selected_process_index: usize,
    start_index: usize,
    rows: u16,
    cols: u16,
) {
    let id_width: u16 = 4;
    let uuid_width: u16 = 30;
    let pid_width: u16 = 8;
    let mem_width: u16 = 12;
    let process_width: u16 = cols - id_width - uuid_width - pid_width - mem_width - 3;

    let header_start_row = rows;
    queue!(stdout, cursor::MoveTo(0, header_start_row)).unwrap();
    let header = format!(
        "{:<id_width$}{:<uuid_width$}{:<pid_width$}{:<process_width$} {:<mem_width$}",
        "ID",
        "UUID",
        "PID",
        "Process",
        "Memory",
        id_width = id_width as usize,
        uuid_width = uuid_width as usize,
        pid_width = pid_width as usize,
        process_width = process_width as usize,
        mem_width = mem_width as usize,
    );
    print_colored_text(stdout, &header, Color::Black, Some(Color::Green), None);

    let process_list_start_row = header_start_row + 1;
    let total_rows = size().unwrap().1;
    let available_rows_for_processes = total_rows
        .saturating_sub(process_list_start_row)
        .saturating_sub(1);

    let processes_to_render: Vec<_> = processes
        .iter()
        .skip(start_index)
        .take(available_rows_for_processes as usize)
        .collect();

    for (i, process) in processes_to_render.iter().enumerate() {
        let global_index = start_index + i;
        let uuid_display = if process.device_uuid.len() > uuid_width as usize {
            &process.device_uuid[..uuid_width as usize]
        } else {
            &process.device_uuid
        };

        let process_display = if process.process_name.len() > process_width as usize {
            format!(
                "{}...",
                &process.process_name[..process_width as usize - 3]
            )
        } else {
            process.process_name.clone()
        };

        let row = format!(
            "{:<id_width$}{:<uuid_width$}{:<pid_width$}{:<process_width$} {:<mem_width$}",
            process.device_id.to_string(),
            uuid_display,
            process.pid.to_string(),
            process_display,
            format!("{:.2} MiB", process.used_memory as f64 / (1024.0 * 1024.0)),
            id_width = id_width as usize,
            uuid_width = uuid_width as usize,
            pid_width = pid_width as usize,
            process_width = process_width as usize,
            mem_width = mem_width as usize,
        );

        let fg_color = if global_index == selected_process_index {
            Color::Black
        } else {
            Color::White
        };

        let bg_color = if global_index == selected_process_index {
            Some(Color::Cyan)
        } else {
            None
        };

        queue!(
            stdout,
            cursor::MoveTo(0, process_list_start_row + i as u16)
        )
        .unwrap();
        print_colored_text(stdout, &row, fg_color, bg_color, None);
    }

    let num_rendered = processes_to_render.len();
    for i in num_rendered..(available_rows_for_processes as usize) {
        queue!(
            stdout,
            cursor::MoveTo(0, process_list_start_row + i as u16),
            terminal::Clear(ClearType::CurrentLine)
        )
        .unwrap();
    }
}

#[derive(Clone, Copy, Deserialize)]
enum SortCriteria {
    Pid,
    Memory,
}

impl SortCriteria {
    fn sort(&self, a: &ProcessInfo, b: &ProcessInfo) -> Ordering {
        match self {
            SortCriteria::Pid => a.pid.cmp(&b.pid),
            SortCriteria::Memory => b.used_memory.cmp(&a.used_memory),
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Api(args)) => {
            ensure_sudo_permissions();
            run_api_mode(args).await;
        }
        Some(Commands::View(args)) => {
            if args.hosts.is_none() && args.hostfile.is_none() {
                ensure_sudo_permissions();
            }
            run_view_mode(args).await;
        }
        None => {
            ensure_sudo_permissions();
            run_view_mode(&ViewArgs {
                hosts: None,
                hostfile: None,
            })
            .await;
        }
    }
}

async fn run_view_mode(args: &ViewArgs) {
    let app_state = Arc::new(Mutex::new(AppState::new()));
    let app_state_clone = Arc::clone(&app_state);
    let args_clone = args.clone();

    tokio::spawn(async move {
        let hosts = args_clone.hosts.unwrap_or_default();
        let hostfile = args_clone.hostfile;

        if hosts.is_empty() && hostfile.is_none() {
            // Local mode
            let gpu_readers = get_gpu_readers();
            loop {
                let all_gpu_info: Vec<GpuInfo> = gpu_readers
                    .iter()
                    .flat_map(|reader| reader.get_gpu_info())
                    .collect();

                let all_processes: Vec<ProcessInfo> = gpu_readers
                    .iter()
                    .flat_map(|reader| reader.get_process_info())
                    .collect();

                // Collect local storage information
                let mut all_storage_info = Vec::new();
                let disks = Disks::new_with_refreshed_list();
                let hostname = get_hostname();
                
                for (index, disk) in disks.iter().enumerate() {
                    let mount_point_str = disk.mount_point().to_string_lossy();
                    if should_include_disk(&mount_point_str) {
                        all_storage_info.push(StorageInfo {
                            mount_point: mount_point_str.to_string(),
                            total_bytes: disk.total_space(),
                            available_bytes: disk.available_space(),
                            hostname: hostname.clone(),
                            index: index as u32,
                        });
                    }
                }

                let mut state = app_state_clone.lock().await;
                if state.gpu_info.is_empty() {
                    state.gpu_info = all_gpu_info;
                } else {
                    for new_info in all_gpu_info {
                        if let Some(old_info) = state.gpu_info.iter_mut().find(|info| info.uuid == new_info.uuid) {
                            *old_info = new_info;
                        }
                    }
                }
                state.process_info = all_processes;
                state.storage_info = all_storage_info;
                let mut tabs = vec!["All".to_string()];
                let mut hostnames: Vec<String> = state
                    .gpu_info
                    .iter()
                    .map(|info| info.hostname.clone())
                    .collect::<std::collections::HashSet<_>>() // Collect into HashSet to get unique hostnames
                    .into_iter()
                    .collect(); // Convert back to Vec
                hostnames.sort(); // Sort hostnames alphabetically
                tabs.extend(hostnames);
                state.tabs = tabs;

                if state.loading {
                    state.loading = false;
                }

                drop(state);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        } else {
            // Remote mode
            let mut all_hosts = hosts;
            if let Some(file_path) = hostfile {
                if let Ok(content) = fs::read_to_string(file_path) {
                    all_hosts.extend(content.lines().map(|s| s.to_string()));
                }
            }

            let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            loop {
                let mut all_gpu_info = Vec::new();
                let mut all_storage_info = Vec::new();
                // Create mapping between host addresses and actual instance names
                let mut host_to_instance: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                
                for host in &all_hosts {
                    let url = if host.starts_with("http://") || host.starts_with("https://") {
                        format!("{}/metrics", host)
                    } else {
                        format!("http://{}/metrics", host)
                    };
                    if let Ok(response) = client.get(&url).send().await {
                        if let Ok(text) = response.text().await {
                            let mut gpu_info_map: std::collections::HashMap<String, GpuInfo> =
                                std::collections::HashMap::new();
                            let mut storage_info_map: std::collections::HashMap<String, StorageInfo> =
                                std::collections::HashMap::new();
                            let mut host_instance_name: Option<String> = None;

                            for line in text.lines() {
                                if let Some(cap) = re.captures(line.trim()) {
                                    let metric_name = &cap[1];
                                    let labels_str = &cap[2];
                                    let value = cap[3].parse::<f64>().unwrap_or(0.0);

                                    let mut labels: std::collections::HashMap<String, String> =
                                        std::collections::HashMap::new();
                                    for label in labels_str.split(',') {
                                        let label_parts: Vec<&str> = label.split('=').collect();
                                        if label_parts.len() == 2 {
                                            labels.insert(
                                                label_parts[0].to_string(),
                                                label_parts[1].replace("\"", "").to_string(),
                                            );
                                        }
                                    }
                                    
                                    // Extract instance name from the first metric that has it
                                    if host_instance_name.is_none() {
                                        if let Some(instance) = labels.get("instance") {
                                            host_instance_name = Some(instance.clone());
                                            host_to_instance.insert(host.clone(), instance.clone());
                                        }
                                    }

                                    // Only process GPU metrics if this line contains GPU-related data
                                    if metric_name.starts_with("gpu_") || metric_name == "ane_utilization" {
                                        let gpu_name =
                                            labels.get("gpu").cloned().unwrap_or_default();
                                        // Skip if gpu_name is empty (shouldn't happen for valid GPU metrics)
                                        if gpu_name.is_empty() {
                                            continue;
                                        }
                                        let gpu_info =
                                            gpu_info_map.entry(gpu_name.clone()).or_insert(GpuInfo {
                                                uuid: labels.get("uuid").cloned().unwrap_or_default(),
                                                time: Local::now()
                                                    .format("%Y-%m-%d %H:%M:%S")
                                                    .to_string(),
                                                name: gpu_name,
                                                hostname: host.split(':').next().unwrap_or_default().to_string(),
                                                instance: host.clone(),
                                                utilization: 0.0,
                                                ane_utilization: 0.0,
                                                dla_utilization: None,
                                                temperature: 0,
                                                used_memory: 0,
                                                total_memory: 0,
                                                frequency: 0,
                                                power_consumption: 0.0,
                                                detail: Default::default(),
                                            });

                                        match metric_name {
                                            "gpu_utilization" => {
                                                gpu_info.utilization = value;
                                            }
                                            "gpu_memory_used_bytes" => {
                                                gpu_info.used_memory = value as u64;
                                            }
                                            "gpu_memory_total_bytes" => {
                                                gpu_info.total_memory = value as u64;
                                            }
                                            "gpu_temperature_celsius" => {
                                                gpu_info.temperature = value as u32;
                                            }
                                            "gpu_power_consumption_watts" => {
                                                gpu_info.power_consumption = value;
                                            }
                                            "gpu_frequency_mhz" => {
                                                gpu_info.frequency = value as u32;
                                            }
                                            "ane_utilization" => {
                                                gpu_info.ane_utilization = value;
                                            }
                                            _ => {}
                                        }
                                    } else if metric_name.starts_with("disk_") {
                                        // Handle disk metrics separately
                                        let mount_point = labels.get("mount_point").cloned().unwrap_or_default();
                                        // Initial hostname (will be updated to instance name later)
                                        let hostname = host.split(':').next().unwrap_or_default().to_string();
                                        
                                        match metric_name {
                                            "disk_total_bytes" => {
                                                // Include host in key to prevent collisions when same machine is accessed via multiple addresses
                                                let storage_key = format!("{}:{}", host, mount_point);
                                                let index = labels.get("index").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                                                let storage_info = storage_info_map.entry(storage_key)
                                                    .or_insert(StorageInfo {
                                                        mount_point: mount_point.clone(),
                                                        total_bytes: 0,
                                                        available_bytes: 0,
                                                        hostname: hostname.clone(),
                                                        index,
                                                    });
                                                storage_info.total_bytes = value as u64;
                                            }
                                            "disk_available_bytes" => {
                                                // Include host in key to prevent collisions when same machine is accessed via multiple addresses
                                                let storage_key = format!("{}:{}", host, mount_point);
                                                let index = labels.get("index").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                                                let storage_info = storage_info_map.entry(storage_key)
                                                    .or_insert(StorageInfo {
                                                        mount_point: mount_point.clone(),
                                                        total_bytes: 0,
                                                        available_bytes: 0,
                                                        hostname: hostname.clone(),
                                                        index,
                                                    });
                                                storage_info.available_bytes = value as u64;
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            
                            // Update all GPU and storage entries with the correct instance hostname
                            if let Some(instance_name) = host_instance_name {
                                // Update GPU hostnames to use instance name
                                for gpu_info in gpu_info_map.values_mut() {
                                    gpu_info.hostname = instance_name.clone();
                                }
                                // Update storage hostnames to use instance name
                                for storage_info in storage_info_map.values_mut() {
                                    storage_info.hostname = instance_name.clone();
                                }
                            }
                            
                            all_gpu_info.extend(gpu_info_map.into_values());
                            all_storage_info.extend(storage_info_map.into_values());
                        }
                    }
                }

                // Deduplicate storage info by instance and mount_point to handle same machine accessed via multiple addresses
                let mut deduplicated_storage: std::collections::HashMap<String, StorageInfo> = std::collections::HashMap::new();
                for storage in all_storage_info {
                    let dedup_key = format!("{}:{}", storage.hostname, storage.mount_point);
                    deduplicated_storage.insert(dedup_key, storage);
                }
                let final_storage_info: Vec<StorageInfo> = deduplicated_storage.into_values().collect();

                let mut state = app_state_clone.lock().await;
                state.gpu_info = all_gpu_info;
                state.storage_info = final_storage_info;
                let mut tabs = vec!["All".to_string()];
                let mut hostnames: std::collections::HashSet<String> = std::collections::HashSet::new();
                
                // Collect hostnames from GPU info
                for info in &state.gpu_info {
                    hostnames.insert(info.hostname.clone());
                }
                
                // Collect hostnames from storage info
                for info in &state.storage_info {
                    hostnames.insert(info.hostname.clone());
                }
                
                let mut sorted_hostnames: Vec<String> = hostnames.into_iter().collect();
                sorted_hostnames.sort(); // Sort hostnames alphabetically
                tabs.extend(sorted_hostnames);
                state.tabs = tabs;
                state.process_info = Vec::new(); // No process info in remote mode
                if state.loading {
                    state.loading = false;
                }

                drop(state);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });

    let mut stdout = stdout();
    enable_raw_mode().unwrap();
    execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .unwrap();

    loop {
        if event::poll(Duration::from_millis(50)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                let mut state = app_state.lock().await;
                match key_event.code {
                    KeyCode::Esc | KeyCode::F(10) | KeyCode::Char('q') => break,
                    KeyCode::Left => {
                        if state.current_tab > 0 {
                            state.current_tab -= 1;
                            if state.current_tab < state.tab_scroll_offset + 1 && state.tab_scroll_offset > 0 {
                                state.tab_scroll_offset -= 1;
                            }
                        }
                        state.gpu_scroll_offset = 0;
                    }
                    KeyCode::Right => {
                        if state.current_tab < state.tabs.len() - 1 {
                            state.current_tab += 1;
                            let (cols, _) = size().unwrap();
                            let mut available_width = cols.saturating_sub(5);
                            let mut last_visible_tab = state.tab_scroll_offset;
                            for (i, tab) in state.tabs.iter().enumerate().skip(1).skip(state.tab_scroll_offset) {
                                let tab_width = tab.len() as u16 + 2;
                                if available_width < tab_width {
                                    break;
                                }
                                available_width -= tab_width;
                                last_visible_tab = i;
                            }
                            if state.current_tab > last_visible_tab {
                                state.tab_scroll_offset += 1;
                            }
                        }
                        state.gpu_scroll_offset = 0;
                    }
                    _ if !state.loading => {
                        // Only handle other keys if not loading
                        match key_event.code {
                            KeyCode::Up => {
                                if state.current_tab > 0 {
                                    if state.gpu_scroll_offset > 0 {
                                        state.gpu_scroll_offset -= 1;
                                    }
                                } else {
                                    if state.selected_process_index > 0 {
                                        state.selected_process_index -= 1;
                                    }
                                    if state.selected_process_index < state.start_index {
                                        state.start_index = state.selected_process_index;
                                    }
                                }
                            }
                            KeyCode::Down => {
                                if state.current_tab > 0 {
                                    let gpu_info_for_tab = state
                                        .gpu_info
                                        .iter()
                                        .filter(|info| info.hostname == state.tabs[state.current_tab])
                                        .count();
                                    if state.gpu_scroll_offset < gpu_info_for_tab - 1 {
                                        state.gpu_scroll_offset += 1;
                                    }
                                } else {
                                    if !state.process_info.is_empty()
                                        && state.selected_process_index
                                            < state.process_info.len() - 1
                                    {
                                        state.selected_process_index += 1;
                                    }
                                    let (_cols, rows) = size().unwrap();
                                    let half_rows = rows / 2;
                                    let visible_process_rows =
                                        half_rows.saturating_sub(1) as usize;
                                    if state.selected_process_index
                                        >= state.start_index + visible_process_rows
                                    {
                                        state.start_index =
                                            state.selected_process_index - visible_process_rows
                                                + 1;
                                    }
                                }
                            }
                            KeyCode::PageUp => {
                                let (_cols, rows) = size().unwrap();
                                let half_rows = rows / 2;
                                let page_size = half_rows.saturating_sub(1) as usize;
                                state.selected_process_index =
                                    state.selected_process_index.saturating_sub(page_size);
                                if state.selected_process_index < state.start_index {
                                    state.start_index = state.selected_process_index;
                                }
                            }
                            KeyCode::PageDown => {
                                if !state.process_info.is_empty() {
                                    let (_cols, rows) = size().unwrap();
                                    let half_rows = rows / 2;
                                    let page_size = half_rows.saturating_sub(1) as usize;
                                    state.selected_process_index = (state.selected_process_index
                                        + page_size)
                                        .min(state.process_info.len() - 1);
                                    let visible_process_rows =
                                        half_rows.saturating_sub(1) as usize;
                                    if state.selected_process_index
                                        >= state.start_index + visible_process_rows
                                    {
                                        state.start_index = state.selected_process_index
                                            - visible_process_rows
                                            + 1;
                                    }
                                }
                            }
                            KeyCode::Char('p') => state.sort_criteria = SortCriteria::Pid,
                            KeyCode::Char('m') => state.sort_criteria = SortCriteria::Memory,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut state = app_state.lock().await;
        state.frame_counter += 1;
        if state.frame_counter % 2 == 0 {
            // Update scroll offsets
            let mut new_device_name_scroll_offsets = state.device_name_scroll_offsets.clone();
            let mut new_hostname_scroll_offsets = state.hostname_scroll_offsets.clone();
            let mut processed_hostnames = std::collections::HashSet::new();

            for gpu in &state.gpu_info {
                if gpu.name.len() > 15 {
                    let offset = new_device_name_scroll_offsets.entry(gpu.uuid.clone()).or_insert(0);
                    *offset = (*offset + 1) % (gpu.name.len() + 3);
                }
                if gpu.hostname.len() > 9 && processed_hostnames.insert(gpu.hostname.clone()) {
                    let offset = new_hostname_scroll_offsets.entry(gpu.hostname.clone()).or_insert(0);
                    *offset = (*offset + 1) % (gpu.hostname.len() + 3);
                }
            }
            state.device_name_scroll_offsets = new_device_name_scroll_offsets;
            state.hostname_scroll_offsets = new_hostname_scroll_offsets;
        }

        let (cols, rows) = size().unwrap();

        queue!(stdout, cursor::Hide, cursor::MoveTo(0, 0)).unwrap();

        if state.loading {
            print_function_keys(&mut stdout, cols, rows);
            print_loading_indicator(&mut stdout, cols, rows);
        } else {
            let width = cols as usize;
            let half_rows = rows / 2;

            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            print_colored_text(
                &mut stdout,
                &format!("all-smi - {}\r\n", current_time),
                Color::White,
                None,
                None,
            );

            print_colored_text(&mut stdout, "Clusters\r\n", Color::Cyan, None, None);
            draw_system_view(&mut stdout, &state, cols);
            draw_dashboard_items(&mut stdout, &state, cols);
            draw_tabs(&mut stdout, &state, cols);

            // Clear the GPU info area before drawing
            for i in 11..half_rows {
                queue!(
                    stdout,
                    cursor::MoveTo(0, i),
                    terminal::Clear(ClearType::CurrentLine)
                )
                .unwrap();
            }
            queue!(stdout, cursor::MoveTo(0, 11)).unwrap();

            let gpu_info_to_display: Vec<_> = if state.current_tab == 0 {
                state.gpu_info.iter().collect()
            } else {
                state
                    .gpu_info
                    .iter()
                    .filter(|info| info.hostname == state.tabs[state.current_tab])
                    .collect()
            };

            for (index, info) in gpu_info_to_display
                .iter()
                .skip(state.gpu_scroll_offset)
                .enumerate()
            {
                let device_name_scroll_offset = state.device_name_scroll_offsets.get(&info.uuid).cloned().unwrap_or(0);
                let hostname_scroll_offset = state.hostname_scroll_offsets.get(&info.hostname).cloned().unwrap_or(0);
                print_gpu_info(&mut stdout, index, info, width, device_name_scroll_offset, hostname_scroll_offset);
                if index < gpu_info_to_display.len() - 1 {
                    queue!(stdout, Print("\r\n")).unwrap();
                }
            }

            // Display storage information for node-specific tabs
            if state.current_tab > 0 && !state.storage_info.is_empty() {
                let current_hostname = &state.tabs[state.current_tab];
                let storage_info_to_display: Vec<_> = state
                    .storage_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect();

                if !storage_info_to_display.is_empty() {
                    queue!(stdout, Print("\r\n")).unwrap();
                    // Sort storage info by index first, then by mount point for consistent display
                    let mut sorted_storage: Vec<_> = storage_info_to_display.clone();
                    sorted_storage.sort_by(|a, b| {
                        a.index.cmp(&b.index)
                            .then_with(|| a.mount_point.cmp(&b.mount_point))
                    });
                    
                    for (index, info) in sorted_storage.iter().enumerate() {
                        print_storage_info(&mut stdout, index, info, width);
                        // Add spacing between disks for better visual separation
                        if index < sorted_storage.len() - 1 {
                            queue!(stdout, Print("\r\n")).unwrap();
                        }
                    }
                }
            }

            let is_remote = args.hosts.is_some() || args.hostfile.is_some();
            if !state.process_info.is_empty() && !is_remote {
                let mut sorted_process_info = state.process_info.clone();
                sorted_process_info.sort_by(|a, b| state.sort_criteria.sort(a, b));

                print_process_info(
                    &mut stdout,
                    &sorted_process_info,
                    state.selected_process_index,
                    state.start_index,
                    half_rows,
                    cols,
                );
            }

            print_function_keys(&mut stdout, cols, rows);
        }

        queue!(stdout, cursor::Show).unwrap();
        stdout.flush().unwrap();
    }

    execute!(stdout, LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap();
}

type SharedState = Arc<RwLock<AppState>>;

async fn run_api_mode(args: &ApiArgs) {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "all_smi=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("Starting API mode...");
    let state = SharedState::new(RwLock::new(AppState::new()));
    let state_clone = state.clone();
    let processes = args.processes;
    let interval = args.interval;

    tokio::spawn(async move {
        let gpu_readers = get_gpu_readers();
        loop {
            let all_gpu_info: Vec<GpuInfo> = gpu_readers
                .iter()
                .flat_map(|reader| reader.get_gpu_info())
                .collect();

            let all_processes: Vec<ProcessInfo> = if processes {
                gpu_readers
                    .iter()
                    .flat_map(|reader| reader.get_process_info())
                    .collect()
            } else {
                Vec::new()
            };

            let mut state = state_clone.write().await;
            state.gpu_info = all_gpu_info;
            state.process_info = all_processes;
            if state.loading {
                state.loading = false;
            }

            drop(state);
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });

    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http());

    let listener = TcpListener::bind(&format!("0.0.0.0:{}", args.port))
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn metrics_handler(State(state): State<SharedState>) -> String {
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
        metrics.push_str(&format!(
            "# TYPE all_smi_gpu_temperature_celsius gauge\n"
        ));
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
        metrics.push_str(&format!(
            "# TYPE all_smi_process_memory_used_bytes gauge\n"
        ));
        for process in &state.process_info {
            metrics.push_str(&format!(
                "all_smi_process_memory_used_bytes{{pid=\"{}\", name=\"{}\", device_id=\"{}\", device_uuid=\"{}\"}} {}\n",
                process.pid, process.process_name, process.device_id, process.device_uuid, process.used_memory
            ));
        }
    }

    // Use instance name for disk metrics to ensure consistency with GPU metrics
    let instance = state.gpu_info.first().map(|info| info.instance.clone()).unwrap_or_else(|| get_hostname());
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