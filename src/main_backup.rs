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

use crate::device::{get_gpu_readers, GpuInfo, ProcessInfo};

// Buffer writer for double buffering to reduce flickering
struct BufferWriter {
    buffer: String,
}

impl BufferWriter {
    fn new() -> Self {
        Self {
            buffer: String::with_capacity(1024 * 1024), // Pre-allocate 1MB
        }
    }
    
    
    fn get_buffer(&self) -> &str {
        &self.buffer
    }
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8")
        })?;
        self.buffer.push_str(s);
        Ok(buf.len())
    }
    
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn get_hostname() -> String {
    let output = Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn calculate_adaptive_interval(node_count: usize) -> u64 {
    // Adaptive interval based on node count to prevent overwhelming the network
    // For 1-10 nodes: 2 seconds
    // For 11-50 nodes: 3 seconds  
    // For 51-100 nodes: 4 seconds
    // For 101-200 nodes: 5 seconds
    // For 201+ nodes: 6 seconds
    match node_count {
        0..=10 => 2,
        11..=50 => 3,
        51..=100 => 4,
        101..=200 => 5,
        _ => 6,
    }
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
#[command(author, version, about, long_about = None)]
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
    /// The interval in seconds at which to update the GPU information. If not specified, uses adaptive interval based on node count.
    #[arg(short, long)]
    interval: Option<u64>,
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
    storage_scroll_offset: usize,
    tab_scroll_offset: usize,
    device_name_scroll_offsets: std::collections::HashMap<String, usize>,
    hostname_scroll_offsets: std::collections::HashMap<String, usize>,
    frame_counter: u64,
    storage_info: Vec<StorageInfo>,
    show_help: bool,
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
            storage_scroll_offset: 0,
            tab_scroll_offset: 0,
            device_name_scroll_offsets: std::collections::HashMap::new(),
            hostname_scroll_offsets: std::collections::HashMap::new(),
            frame_counter: 0,
            storage_info: Vec::new(),
            show_help: false,
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
    let available_bar_width = width.saturating_sub(label_width + 4); // 4 for ": [" and "] "
    
    // Calculate the filled portion
    let fill_ratio = (value / max_value).min(1.0);
    let filled_width = (fill_ratio * available_bar_width as f64).floor() as usize;
    let remainder = (fill_ratio * available_bar_width as f64) - filled_width as f64;
    
    // Choose partial block character based on remainder
    let partial_char = match remainder {
        r if r > 0.875 => "▉",
        r if r > 0.625 => "▊", 
        r if r > 0.375 => "▋",
        r if r > 0.125 => "▌",
        _ => "▏",
    };
    
    // Create the bar content with text positioned at the right end
    let text_to_show = show_text.unwrap_or_default();
    let text_len = text_to_show.len();
    let text_start_pos = available_bar_width.saturating_sub(text_len);
    
    // Build the bar character by character
    let mut bar_chars = Vec::new();
    for i in 0..available_bar_width {
        if i < filled_width {
            // Filled area
            if i >= text_start_pos && i < text_start_pos + text_len {
                // Text position in filled area
                let text_char = text_to_show.chars().nth(i - text_start_pos).unwrap_or(' ');
                bar_chars.push((text_char, true, true)); // (char, is_filled, is_text)
            } else {
                bar_chars.push(('▉', true, false));
            }
        } else if i == filled_width && remainder > 0.125 {
            // Partial fill position
            if i >= text_start_pos && i < text_start_pos + text_len {
                // Text position in partial area
                let text_char = text_to_show.chars().nth(i - text_start_pos).unwrap_or(' ');
                bar_chars.push((text_char, false, true));
            } else {
                bar_chars.push((partial_char.chars().next().unwrap(), false, false));
            }
        } else {
            // Empty area
            if i >= text_start_pos && i < text_start_pos + text_len {
                // Text position in empty area
                let text_char = text_to_show.chars().nth(i - text_start_pos).unwrap_or(' ');
                bar_chars.push((text_char, false, true));
            } else {
                bar_chars.push(('▏', false, false));
            }
        }
    }

    // Use different colors for storage bars
    let (label_color, bar_color) = if label == "DSK" {
        (Color::Yellow, Color::Yellow)
    } else {
        (Color::Blue, Color::Green)
    };

    // Print label and opening bracket
    print_colored_text(stdout, &format!("{}: [", label), label_color, None, None);
    
    // Print each character with appropriate coloring
    for (ch, is_filled, is_text) in bar_chars {
        if is_text {
            // Text should be white on appropriate background
            let bg_color = if is_filled { Some(bar_color) } else { None };
            print_colored_text(stdout, &ch.to_string(), Color::White, bg_color, None);
        } else {
            // Bar characters
            print_colored_text(stdout, &ch.to_string(), bar_color, None, None);
        }
    }
    
    // Print closing bracket and space
    queue!(stdout, Print("] ")).unwrap();
}

fn draw_tabs<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    queue!(stdout, cursor::MoveTo(0, 10)).unwrap();
    queue!(stdout, terminal::Clear(ClearType::CurrentLine)).unwrap();

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
    
    // Draw horizontal separator line below tabs
    let separator = "─".repeat(cols as usize);
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
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
    let memory_text = format!("{:.1}/{:.1}Gi", used_memory_gib, total_memory_gib);
    let gpu_percentage_text = format!("{:.2}%", info.utilization);
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
        format!("{:>6.1} GiB  ", total_memory_gib),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "Used: ",
        format!("{:>6.1} GiB  ", used_memory_gib),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "Temp.: ",
        format!("{:>3}°C  ", info.temperature),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "FREQ: ",
        format!("{:>4} MHz  ", info.frequency),
        Color::Blue,
    );
    add_label(
        &mut labels,
        "POW: ",
        format!("{:>5.1}W  ", info.power_consumption),
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
            Some(format!("{:.1}W", info.ane_utilization / 1000.0)),
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

fn print_help_popup<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    // Fill the entire screen with the help popup
    let border_color = Color::Cyan;
    let bg_color = Color::Black;
    let text_color = Color::White;
    let highlight_color = Color::Yellow;
    
    // Clear the entire screen first
    queue!(stdout, terminal::Clear(ClearType::All)).unwrap();
    
    // Draw the outer border
    queue!(stdout, cursor::MoveTo(0, 0)).unwrap();
    print_colored_text(stdout, &format!("┌{}┐", "─".repeat((cols - 2) as usize)), border_color, Some(bg_color), None);
    
    // Draw side borders and fill with background
    for row in 1..rows-1 {
        queue!(stdout, cursor::MoveTo(0, row)).unwrap();
        print_colored_text(stdout, "│", border_color, Some(bg_color), None);
        print_colored_text(stdout, &" ".repeat((cols - 2) as usize), text_color, Some(bg_color), None);
        print_colored_text(stdout, "│", border_color, Some(bg_color), None);
    }
    
    // Draw bottom border
    queue!(stdout, cursor::MoveTo(0, rows - 1)).unwrap();
    print_colored_text(stdout, &format!("└{}┘", "─".repeat((cols - 2) as usize)), border_color, Some(bg_color), None);
    
    // Content area calculations
    let content_width = (cols - 4) as usize; // -4 for borders and padding
    let _content_height = (rows - 2) as usize; // -2 for top and bottom borders
    let start_x = 2;
    let start_y = 1;
    
    // Section 1: Title (top section)
    let title_y = start_y + 2;
    let title_text = "all-smi Help";
    let title_x = start_x + (content_width - title_text.len()) / 2;
    queue!(stdout, cursor::MoveTo(title_x as u16, title_y)).unwrap();
    print_colored_text(stdout, title_text, highlight_color, Some(bg_color), None);
    
    // Title underline
    queue!(stdout, cursor::MoveTo(title_x as u16, title_y + 1)).unwrap();
    print_colored_text(stdout, &"═".repeat(title_text.len()), highlight_color, Some(bg_color), None);
    
    // Section 2: Cheat sheet (middle section)
    let cheat_start_y = title_y + 4;
    let cheat_sheet_lines = vec![
        "Navigation & Controls:",
        "  ←/→        Switch between tabs (All, Node1, Node2, etc.)",
        "",
        "Remote Mode (monitoring remote hosts):",
        "  ↑/↓        Navigate GPU/Storage list one item at a time",
        "  PgUp/PgDn  Page through GPU list (full page at a time)",
        "",
        "Local Mode (single machine):",
        "  ↑/↓        Navigate process list one item at a time",
        "  PgUp/PgDn  Page through process list (full page at a time)",
        "",
        "Sorting (Local mode only):",
        "  p          Sort processes by PID",
        "  m          Sort processes by Memory usage",
        "",
        "Application:",
        "  F1 / h     Show/hide this help",
        "  F10 / q    Quit application",
        "  Esc        Exit help or quit application",
    ];
    
    for (i, line) in cheat_sheet_lines.iter().enumerate() {
        let y = cheat_start_y + i as u16;
        if y < rows - 8 { // Leave space for bottom section
            queue!(stdout, cursor::MoveTo(start_x as u16, y)).unwrap();
            if line.starts_with("  ") {
                // Indent lines - highlight the key combinations
                let parts: Vec<&str> = line.splitn(2, ' ').collect();
                if parts.len() == 2 && parts[0].trim() != "" {
                    let key_part = parts[0].trim();
                    let desc_part = parts[1];
                    print_colored_text(stdout, "  ", text_color, Some(bg_color), None);
                    print_colored_text(stdout, key_part, highlight_color, Some(bg_color), None);
                    print_colored_text(stdout, &format!("  {}", desc_part), text_color, Some(bg_color), None);
                } else {
                    print_colored_text(stdout, line, text_color, Some(bg_color), None);
                }
            } else if line.ends_with(":") {
                // Section headers
                print_colored_text(stdout, line, Color::Cyan, Some(bg_color), None);
            } else {
                print_colored_text(stdout, line, text_color, Some(bg_color), None);
            }
        }
    }
    
    // Section 3: Terminal options (bottom section)
    let terminal_start_y = rows - 6;
    let terminal_lines = vec![
        "Terminal Usage:",
        "  View Mode:  all-smi view [--hosts host1 host2] [--hostfile hosts.csv]",
        "  API Mode:   all-smi api [--port 9090] [--interval 3]",
    ];
    
    for (i, line) in terminal_lines.iter().enumerate() {
        let y = terminal_start_y + i as u16;
        if y < rows - 1 {
            queue!(stdout, cursor::MoveTo(start_x as u16, y)).unwrap();
            if line.starts_with("  ") {
                // Command examples
                print_colored_text(stdout, "  ", text_color, Some(bg_color), None);
                let command_part = &line[2..];
                print_colored_text(stdout, command_part, Color::Green, Some(bg_color), None);
            } else if line.ends_with(":") {
                // Section header
                print_colored_text(stdout, line, Color::Cyan, Some(bg_color), None);
            } else {
                print_colored_text(stdout, line, text_color, Some(bg_color), None);
            }
        }
    }
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

    match cli.command {
        Some(Commands::Api(args)) => {
            ensure_sudo_permissions();
            run_api_mode(&args).await;
        }
        Some(Commands::View(args)) => {
            if args.hosts.is_none() && args.hostfile.is_none() {
                ensure_sudo_permissions();
            }
            run_view_mode(&args).await;
        }
        None => {
            // This will be the default behavior
            ensure_sudo_permissions();
            run_view_mode(&ViewArgs {
                hosts: None,
                hostfile: None,
                interval: None,
            })
            .await;
        }
    }
}

async fn run_view_mode(args: &ViewArgs) {
    let mut initial_state = AppState::new();
    // Disable loading indicator for remote mode
    let is_remote_mode = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote_mode {
        initial_state.loading = false;
    }
    
    let app_state = Arc::new(Mutex::new(initial_state));
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

                // Always clear loading state in local mode after first iteration  
                state.loading = false;

                drop(state);
                
                // Use adaptive interval for local mode
                let interval = args_clone.interval.unwrap_or_else(|| calculate_adaptive_interval(1));
                tokio::time::sleep(Duration::from_secs(interval)).await;
            }
        } else {
            // Remote mode
            let mut all_hosts = hosts;
            if let Some(file_path) = hostfile {
                if let Ok(content) = fs::read_to_string(&file_path) {
                    let file_hosts: Vec<String> = content
                        .lines()
                        .map(|s| s.trim()) // Remove whitespace
                        .filter(|s| !s.is_empty()) // Filter out empty lines
                        .filter(|s| !s.starts_with('#')) // Filter out comments
                        .map(|s| s.to_string())
                        .collect();
                    all_hosts.extend(file_hosts);
                }
            }

            let re = Regex::new(r"^all_smi_([^\{]+)\{([^}]+)\} ([\d\.]+)$").unwrap();
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5)) // Reduced timeout for better performance
                .pool_idle_timeout(Duration::from_secs(60))
                .pool_max_idle_per_host(200) // Increased pool size significantly
                .tcp_keepalive(Duration::from_secs(30))
                .http2_keep_alive_interval(Duration::from_secs(30))
                .build()
                .unwrap();
                
            // Create semaphore to limit concurrent connections - respect system limits
            // macOS kern.ipc.somaxconn is 128, so limit to 64 concurrent connections to avoid queue drops
            let max_concurrent_connections = std::cmp::min(all_hosts.len(), 64);
            let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_connections));

            loop {
                let mut all_gpu_info = Vec::new();
                let mut all_storage_info = Vec::new();
                // Create mapping between host addresses and actual instance names
                let mut host_to_instance: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                
                
                // Parallel data collection with concurrency limiting and retries
                let total_hosts = all_hosts.len();
                let fetch_tasks: Vec<_> = all_hosts.iter().enumerate().map(|(i, host)| {
                    let client = client.clone();
                    let host = host.clone();
                    let semaphore = semaphore.clone();
                    let _re = re.clone(); // Keep for potential future use in parallel processing
                    tokio::spawn(async move {
                        // Stagger connection attempts to avoid overwhelming the listen queue
                        // Spread connections over 500ms window
                        let stagger_delay = (i as u64 * 500) / total_hosts as u64;
                        tokio::time::sleep(Duration::from_millis(stagger_delay)).await;
                        
                        // Acquire semaphore permit to limit concurrency
                        let _permit = semaphore.acquire().await.unwrap();
                        
                        let url = if host.starts_with("http://") || host.starts_with("https://") {
                            format!("{}/metrics", host)
                        } else {
                            format!("http://{}/metrics", host)
                        };
                        
                        // Retry logic - 3 attempts with exponential backoff (better for real remote nodes)
                        for attempt in 1..=3 {
                            match client.get(&url).send().await {
                                Ok(response) => {
                                    if response.status().is_success() {
                                        match response.text().await {
                                            Ok(text) => return Some((host, text, None)), // Success
                                            Err(e) => {
                                                if attempt == 3 {
                                                    return Some((host, String::new(), Some(format!("Text parse error: {}", e))));
                                                }
                                            }
                                        }
                                    } else {
                                        if attempt == 3 {
                                            return Some((host, String::new(), Some(format!("HTTP {}", response.status()))));
                                        }
                                    }
                                },
                                Err(e) => {
                                    if attempt == 3 {
                                        return Some((host, String::new(), Some(format!("Connection error after {} attempts: {}", attempt, e))));
                                    }
                                }
                            }
                            
                            // Exponential backoff: 50ms, 100ms, 150ms (slightly more aggressive than original)
                            tokio::time::sleep(Duration::from_millis(50 * attempt as u64)).await;
                        }
                        
                        Some((host, String::new(), Some("All retry attempts failed".to_string())))
                    })
                }).collect();

                // Wait for all fetch tasks to complete  
                let fetch_results = futures_util::future::join_all(fetch_tasks).await;
                
                // Process all fetch results with error tracking
                let mut successful_connections = 0;
                let mut failed_connections = 0;
                for task_result in fetch_results {
                    match task_result {
                        Ok(Some((host, text, error))) => {
                            if let Some(_err_msg) = error {
                                failed_connections += 1;
                                continue;
                            }
                            successful_connections += 1;
                            
                            if text.is_empty() {
                                continue;
                            }
                            
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
                                            let key = label_parts[0].trim().to_string(); // Trim whitespace from key
                                            let value = label_parts[1].replace("\"", "").to_string();
                                            labels.insert(key.clone(), value.clone());
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
                                        let gpu_uuid = labels.get("uuid").cloned().unwrap_or_default();
                                        let gpu_index = labels.get("index").cloned().unwrap_or_default();
                                        // Skip if gpu_name or uuid is empty (shouldn't happen for valid GPU metrics)
                                        if gpu_name.is_empty() || gpu_uuid.is_empty() {
                                            continue;
                                        }
                                        // Use UUID as the unique key for each GPU
                                        let gpu_info =
                                            gpu_info_map.entry(gpu_uuid.clone()).or_insert_with(|| {
                                                let mut detail = std::collections::HashMap::new();
                                                detail.insert("index".to_string(), gpu_index.clone());
                                                GpuInfo {
                                                uuid: gpu_uuid.clone(),
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
                                                detail,
                                            }});

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
                                        let index = labels.get("index").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                                        
                                        
                                        // Create storage key that includes both host and mount point to handle multiple disks
                                        let storage_key = format!("{}:{}:{}", host, mount_point, index);
                                        
                                        match metric_name {
                                            "disk_total_bytes" => {
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
                        Ok(None) => {
                            // Task returned None - ignore
                            failed_connections += 1;
                        }
                        Err(_e) => {
                            // Task error - ignore
                            failed_connections += 1;
                        }
                    }
                }
                
                // Debug logging for connection success rate
                if failed_connections > 0 {
                    eprintln!("Connection stats: {} successful, {} failed out of {} total", 
                             successful_connections, failed_connections, total_hosts);
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
                
                // Always clear loading state in remote mode after first iteration
                state.loading = false;

                drop(state);
                
                // Use adaptive interval for remote mode based on node count
                let interval = args_clone.interval.unwrap_or_else(|| calculate_adaptive_interval(all_hosts.len()));
                tokio::time::sleep(Duration::from_secs(interval)).await;
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
                    KeyCode::Esc => {
                        if state.show_help {
                            state.show_help = false;
                        } else {
                            break;
                        }
                    }
                    KeyCode::F(10) | KeyCode::Char('q') => break,
                    KeyCode::F(1) | KeyCode::Char('h') => {
                        state.show_help = !state.show_help;
                    }
                    KeyCode::Left => {
                        if !state.show_help {
                            if state.current_tab > 0 {
                                state.current_tab -= 1;
                                if state.current_tab < state.tab_scroll_offset + 1 && state.tab_scroll_offset > 0 {
                                    state.tab_scroll_offset -= 1;
                                }
                            }
                            state.gpu_scroll_offset = 0;
                            state.storage_scroll_offset = 0;
                        }
                    }
                    KeyCode::Right => {
                        if !state.show_help {
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
                            state.storage_scroll_offset = 0;
                        }
                    }
                    _ if !state.loading && !state.show_help => {
                        // Only handle other keys if not loading
                        match key_event.code {
                            KeyCode::Up => {
                                let is_remote = args.hosts.is_some() || args.hostfile.is_some();
                                if is_remote {
                                    // Unified scrolling for remote mode
                                    if state.gpu_scroll_offset > 0 {
                                        state.gpu_scroll_offset -= 1;
                                        state.storage_scroll_offset = 0; // Reset storage scroll when in GPU area
                                    } else if state.storage_scroll_offset > 0 {
                                        state.storage_scroll_offset -= 1;
                                    }
                                } else {
                                    // Local mode - process list scrolling
                                    if state.selected_process_index > 0 {
                                        state.selected_process_index -= 1;
                                    }
                                    if state.selected_process_index < state.start_index {
                                        state.start_index = state.selected_process_index;
                                    }
                                }
                            }
                            KeyCode::Down => {
                                let is_remote = args.hosts.is_some() || args.hostfile.is_some();
                                if is_remote {
                                    // Unified scrolling for remote mode
                                    let gpu_count = if state.current_tab == 0 {
                                        state.gpu_info.len()
                                    } else {
                                        state.gpu_info.iter()
                                            .filter(|info| info.hostname == state.tabs[state.current_tab])
                                            .count()
                                    };
                                    
                                    let storage_count = if state.current_tab == 0 {
                                        // No storage on 'All' tab
                                        0
                                    } else {
                                        state.storage_info.iter()
                                            .filter(|info| info.hostname == state.tabs[state.current_tab])
                                            .count()
                                    };
                                    
                                    if state.gpu_scroll_offset < gpu_count.saturating_sub(1) {
                                        state.gpu_scroll_offset += 1;
                                        state.storage_scroll_offset = 0; // Reset storage scroll when in GPU area
                                    } else if state.storage_scroll_offset < storage_count.saturating_sub(1) {
                                        state.storage_scroll_offset += 1;
                                    }
                                } else {
                                    // Local mode - process list scrolling
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
                                let is_remote = args.hosts.is_some() || args.hostfile.is_some();
                                if is_remote {
                                    // Remote mode - page up through GPU list
                                    let (_cols, rows) = size().unwrap();
                                    let content_start_row = 12;
                                    let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize;
                                    
                                    // Calculate storage display space for current tab
                                    let storage_items_count = if state.current_tab > 0 && !state.storage_info.is_empty() {
                                        let current_hostname = &state.tabs[state.current_tab];
                                        state.storage_info.iter()
                                            .filter(|info| info.hostname == *current_hostname)
                                            .count()
                                    } else {
                                        0
                                    };
                                    let storage_display_rows = if storage_items_count > 0 {
                                        storage_items_count + 2
                                    } else {
                                        0
                                    };
                                    
                                    let gpu_display_rows = available_rows.saturating_sub(storage_display_rows);
                                    let max_gpu_items = gpu_display_rows / 2; // Each GPU takes 2 rows
                                    let page_size = max_gpu_items.max(1); // At least 1 item per page
                                    
                                    state.gpu_scroll_offset = state.gpu_scroll_offset.saturating_sub(page_size);
                                    state.storage_scroll_offset = 0; // Reset storage scroll when paging GPU list
                                } else {
                                    // Local mode - page up through process list
                                    let (_cols, rows) = size().unwrap();
                                    let half_rows = rows / 2;
                                    let page_size = half_rows.saturating_sub(1) as usize;
                                    state.selected_process_index =
                                        state.selected_process_index.saturating_sub(page_size);
                                    if state.selected_process_index < state.start_index {
                                        state.start_index = state.selected_process_index;
                                    }
                                }
                            }
                            KeyCode::PageDown => {
                                let is_remote = args.hosts.is_some() || args.hostfile.is_some();
                                if is_remote {
                                    // Remote mode - page down through GPU list
                                    let (_cols, rows) = size().unwrap();
                                    let content_start_row = 12;
                                    let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize;
                                    
                                    // Calculate storage display space for current tab
                                    let storage_items_count = if state.current_tab > 0 && !state.storage_info.is_empty() {
                                        let current_hostname = &state.tabs[state.current_tab];
                                        state.storage_info.iter()
                                            .filter(|info| info.hostname == *current_hostname)
                                            .count()
                                    } else {
                                        0
                                    };
                                    let storage_display_rows = if storage_items_count > 0 {
                                        storage_items_count + 2
                                    } else {
                                        0
                                    };
                                    
                                    let gpu_display_rows = available_rows.saturating_sub(storage_display_rows);
                                    let max_gpu_items = gpu_display_rows / 2; // Each GPU takes 2 rows
                                    let page_size = max_gpu_items.max(1); // At least 1 item per page
                                    
                                    // Calculate total GPUs for current tab
                                    let total_gpus = if state.current_tab == 0 {
                                        state.gpu_info.len()
                                    } else {
                                        state.gpu_info.iter()
                                            .filter(|info| info.hostname == state.tabs[state.current_tab])
                                            .count()
                                    };
                                    
                                    if total_gpus > 0 {
                                        let max_offset = total_gpus.saturating_sub(max_gpu_items);
                                        state.gpu_scroll_offset = (state.gpu_scroll_offset + page_size).min(max_offset);
                                        state.storage_scroll_offset = 0; // Reset storage scroll when paging GPU list
                                    }
                                } else {
                                    // Local mode - page down through process list
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

        if state.show_help {
            print_help_popup(&mut stdout, cols, rows);
        } else if state.loading {
            print_function_keys(&mut stdout, cols, rows);
            print_loading_indicator(&mut stdout, cols, rows);
        } else {
            let width = cols as usize;
            let half_rows = rows / 2;

            // Use double buffering to reduce flickering - write everything to buffer
            let mut buffer = BufferWriter::new();
            
            // Write time/date header to buffer first
            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            print_colored_text(
                &mut buffer,
                &format!("all-smi - {}\r\n", current_time),
                Color::White,
                None,
                None,
            );
            
            // Write remaining header content to buffer
            print_colored_text(&mut buffer, "Clusters\r\n", Color::Cyan, None, None);
            draw_system_view(&mut buffer, &state, cols);
            draw_dashboard_items(&mut buffer, &state, cols);
            draw_tabs(&mut buffer, &state, cols);
            
            let is_remote = args.hosts.is_some() || args.hostfile.is_some();

            let mut gpu_info_to_display: Vec<_> = if state.current_tab == 0 {
                state.gpu_info.iter().collect()
            } else {
                state
                    .gpu_info
                    .iter()
                    .filter(|info| info.hostname == state.tabs[state.current_tab])
                    .collect()
            };
            
            // Sort GPUs by hostname first, then by index to ensure consistent ordering
            gpu_info_to_display.sort_by(|a, b| {
                a.hostname.cmp(&b.hostname).then_with(|| {
                    let a_index = a.detail.get("index").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                    let b_index = b.detail.get("index").and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                    a_index.cmp(&b_index)
                })
            });

            // Calculate available display area for GPU list
            // Content area starts at row 12 (after tabs + separator), reserve 1 row for function keys
            let content_start_row = 12;
            let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize; // -1 for function keys
            
            // Calculate how many storage items will be displayed (only for node-specific tabs)
            let storage_items_count = if is_remote && state.current_tab > 0 && !state.storage_info.is_empty() {
                let current_hostname = &state.tabs[state.current_tab];
                state.storage_info.iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .count()
            } else {
                0
            };
            
            // Reserve space for storage display (each storage item + separator + header)
            let storage_display_rows = if storage_items_count > 0 {
                storage_items_count + 2 // +1 for header newline, +1 for spacing
            } else {
                0
            };
            
            // Calculate GPU display area
            let gpu_display_rows = if is_remote {
                // In remote mode, use available area minus storage area
                available_rows.saturating_sub(storage_display_rows)
            } else {
                // In local mode, use only half for GPU list (other half for process list)
                available_rows / 2
            };
            
            // Each GPU takes 2 rows (info line + progress bars), so divide available rows by 2
            let max_gpu_items = gpu_display_rows / 2;
            
            // Render GPU info to buffer
            for (index, info) in gpu_info_to_display
                .iter()
                .skip(state.gpu_scroll_offset)
                .take(max_gpu_items)
                .enumerate()
            {
                let device_name_scroll_offset = state.device_name_scroll_offsets.get(&info.uuid).cloned().unwrap_or(0);
                let hostname_scroll_offset = state.hostname_scroll_offsets.get(&info.hostname).cloned().unwrap_or(0);
                print_gpu_info(&mut buffer, index, info, width, device_name_scroll_offset, hostname_scroll_offset);
            }

            // Display storage information only for node-specific tabs in remote mode (not 'All' tab)
            let is_remote = args.hosts.is_some() || args.hostfile.is_some();
            if is_remote && state.current_tab > 0 && !state.storage_info.is_empty() {
                let current_hostname = &state.tabs[state.current_tab];
                let storage_info_to_display: Vec<_> = state
                    .storage_info
                    .iter()
                    .filter(|info| info.hostname == *current_hostname)
                    .collect();

                if !storage_info_to_display.is_empty() {
                    queue!(buffer, Print("\r\n")).unwrap();
                    // Sort storage info by hostname first, then by index, then by mount point for consistent display
                    let mut sorted_storage: Vec<_> = storage_info_to_display.clone();
                    sorted_storage.sort_by(|a, b| {
                        a.hostname.cmp(&b.hostname)
                            .then_with(|| a.index.cmp(&b.index))
                            .then_with(|| a.mount_point.cmp(&b.mount_point))
                    });
                    
                    // Calculate remaining display area for storage (ensure it doesn't overflow)
                    let remaining_rows = available_rows.saturating_sub(gpu_display_rows);
                    for (index, info) in sorted_storage.iter().skip(state.storage_scroll_offset).take(remaining_rows.saturating_sub(2)).enumerate() {
                        print_storage_info(&mut buffer, index, info, width);
                        // Add spacing between disks for better visual separation
                        if index < sorted_storage.len() - 1 {
                            queue!(buffer, Print("\r\n")).unwrap();
                        }
                    }
                }
            }

            let is_remote = args.hosts.is_some() || args.hostfile.is_some();
            if !state.process_info.is_empty() && !is_remote {
                let mut sorted_process_info = state.process_info.clone();
                sorted_process_info.sort_by(|a, b| state.sort_criteria.sort(a, b));

                print_process_info(
                    &mut buffer,
                    &sorted_process_info,
                    state.selected_process_index,
                    state.start_index,
                    half_rows,
                    cols,
                );
            }

            // Output the entire buffer to stdout in one operation (header + content)
            queue!(stdout, cursor::MoveTo(0, 0)).unwrap();
            queue!(stdout, terminal::Clear(ClearType::FromCursorDown)).unwrap();
            print!("{}", buffer.get_buffer());
            
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