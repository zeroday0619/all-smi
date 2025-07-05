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

use crate::gpu::{get_gpu_readers, GpuInfo, ProcessInfo};

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

    print_colored_text(stdout, &format!("{}: [", label), Color::Blue, None, None);
    print_colored_text(stdout, &filled_bar, Color::Green, None, None);
    print_colored_text(stdout, &empty_bar, Color::Green, None, None);

    if let Some(text) = show_text {
        print_colored_text(stdout, &text, Color::White, None, Some(text_width));
    }

    queue!(stdout, Print("] ")).unwrap();
}

fn print_gpu_info<W: Write>(stdout: &mut W, index: usize, info: &GpuInfo, width: usize) {
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
    add_label(
        &mut labels,
        &format!("DEVICE {}: ", index + 1),
        format!("{}  ", info.name),
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

    let ane_percentage_text = format!("{:.2}W", info.ane_utilization / 1000.0);

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
    draw_bar(
        stdout,
        "ANE",
        info.ane_utilization,
        1000.0,
        w2,
        Some(ane_percentage_text),
    );
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
    let x = cols.saturating_sub(text_len).saturating_sub(1);
    let y = rows.saturating_sub(1);
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
    ensure_sudo_permissions();
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Api(args)) => {
            run_api_mode(args).await;
        }
        Some(Commands::View(args)) => {
            run_view_mode(args).await;
        }
        None => {
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

                let mut state = app_state_clone.lock().await;
                state.gpu_info = all_gpu_info;
                state.process_info = all_processes;
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

            loop {
                let mut all_gpu_info = Vec::new();
                for host in &all_hosts {
                    let url = if host.starts_with("http://") || host.starts_with("https://") {
                        format!("{}/metrics", host)
                    } else {
                        format!("http://{}/metrics", host)
                    };
                    if let Ok(response) = reqwest::get(&url).await {
                        if let Ok(text) = response.text().await {
                            let mut gpu_info_map: std::collections::HashMap<String, GpuInfo> =
                                std::collections::HashMap::new();

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

                                    let gpu_name =
                                        labels.get("gpu").cloned().unwrap_or_default();
                                    let gpu_info =
                                        gpu_info_map.entry(gpu_name.clone()).or_insert(GpuInfo {
                                            time: Local::now()
                                                .format("%Y-%m-%d %H:%M:%S")
                                                .to_string(),
                                            name: gpu_name,
                                            utilization: 0.0,
                                            ane_utilization: 0.0,
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
                                }
                            }
                            all_gpu_info.extend(gpu_info_map.into_values());
                        }
                    }
                }

                let mut state = app_state_clone.lock().await;
                state.gpu_info = all_gpu_info;
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
                    _ if !state.loading => {
                        // Only handle other keys if not loading
                        match key_event.code {
                            KeyCode::Up => {
                                if state.selected_process_index > 0 {
                                    state.selected_process_index -= 1;
                                }
                                if state.selected_process_index < state.start_index {
                                    state.start_index = state.selected_process_index;
                                }
                            }
                            KeyCode::Down => {
                                if !state.process_info.is_empty()
                                    && state.selected_process_index < state.process_info.len() - 1
                                {
                                    state.selected_process_index += 1;
                                }
                                let (_cols, rows) = size().unwrap();
                                let half_rows = rows / 2;
                                let visible_process_rows = half_rows.saturating_sub(1) as usize;
                                if state.selected_process_index
                                    >= state.start_index + visible_process_rows
                                {
                                    state.start_index =
                                        state.selected_process_index - visible_process_rows + 1;
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

        let state = app_state.lock().await.clone();
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
                &format!("{}\r\n", current_time),
                Color::White,
                None,
                None,
            );

            for (index, info) in state.gpu_info.iter().enumerate() {
                print_gpu_info(&mut stdout, index, info, width);
                if index < state.gpu_info.len() - 1 {
                    queue!(stdout, Print("\r\n")).unwrap();
                }
            }

            if !state.process_info.is_empty() {
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
            "all_smi_gpu_utilization{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.utilization
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_memory_used_bytes GPU memory used in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_memory_used_bytes gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_memory_used_bytes{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.used_memory
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_memory_total_bytes GPU memory total in bytes\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_memory_total_bytes gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_memory_total_bytes{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.total_memory
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_temperature_celsius GPU temperature in celsius\n"
        ));
        metrics.push_str(&format!(
            "# TYPE all_smi_gpu_temperature_celsius gauge\n"
        ));
        metrics.push_str(&format!(
            "all_smi_gpu_temperature_celsius{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.temperature
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_power_consumption_watts GPU power consumption in watts\n"
        ));
        metrics.push_str(&format!(
            "# TYPE all_smi_gpu_power_consumption_watts gauge\n"
        ));
        metrics.push_str(&format!(
            "all_smi_gpu_power_consumption_watts{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.power_consumption
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_gpu_frequency_mhz GPU frequency in MHz\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_gpu_frequency_mhz gauge\n"));
        metrics.push_str(&format!(
            "all_smi_gpu_frequency_mhz{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name, i, info.frequency
        ));

        metrics.push_str(&format!(
            "# HELP all_smi_ane_utilization ANE utilization in watts\n"
        ));
        metrics.push_str(&format!("# TYPE all_smi_ane_utilization gauge\n"));
        metrics.push_str(&format!(
            "all_smi_ane_utilization{{gpu=\"{}\", index=\"{}\"}} {}\n",
            info.name,
            i,
            info.ane_utilization / 1000.0
        ));
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

    metrics
}