mod gpu;

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration};
use std::cmp::Ordering;
use crate::gpu::{get_gpu_readers, GpuInfo, ProcessInfo};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size, enable_raw_mode, disable_raw_mode},
};
use chrono::Local;
use std::io::{stdout, Write};
use std::process::Command;

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
        if full_blocks < available_bar_width { filled_char } else { "" }
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
    fn add_label(labels: &mut Vec<(String, Color)>, label: &str, value: String, label_color: Color) {
        labels.push((label.to_string(), label_color));
        labels.push((value, Color::White));
    }

    // Adding device, memory, temperature, frequency, and power information
    add_label(&mut labels, &format!("DEVICE {}: ", index + 1), format!("{}  ", info.name), Color::Blue);
    add_label(&mut labels, "Total: ", format!("{:.2} GiB  ", total_memory_gib), Color::Blue);
    add_label(&mut labels, "Used: ", format!("{:.2} GiB  ", used_memory_gib), Color::Blue);
    add_label(&mut labels, "Temp.: ", format!("{}°C  ", info.temperature), Color::Blue);
    add_label(&mut labels, "FREQ: ", format!("{}  ", freq_text), Color::Blue);
    add_label(&mut labels, "POW: ", format!("{} ", power_text), Color::Blue);

    // Check if driver_version exists in the detail map and add it to labels
    if let Some(driver_version) = info.detail.get("driver_version") {
        add_label(&mut labels, "DRIV: ", format!("{} ", driver_version), Color::Blue);
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

    draw_bar(stdout, "GPU", info.utilization, 100.0, w1, Some(gpu_percentage_text));
    draw_bar(stdout, "ANE", info.ane_utilization, 1000.0, w2, Some(ane_percentage_text));
    draw_bar(stdout, "MEM", used_memory_gib, total_memory_gib, w3, Some(memory_text));

    queue!(stdout, Print("\r\n")).unwrap(); // Move cursor to the start of the next line
}

fn print_function_keys<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    let key_width: usize = 3; // Width for each function key label
    let total_width: usize = cols as usize; // Total width of the terminal
    let min_label_width: usize = 5; // Minimum width for label text
    let label_width = (total_width / 10).saturating_sub(key_width).max(min_label_width); // Ensure label_width is at least min_label_width

    let function_keys = vec!["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

    let labels = vec![
        "Help", "", "", "", "", "", "", "", "", "Quit",
    ];

    queue!(stdout, cursor::MoveTo(0, rows.saturating_sub(1))).unwrap();

    for (index, key) in function_keys.iter().enumerate() {
        print_colored_text(stdout, key, Color::White, Some(Color::Black), Some(key_width));
        print_colored_text(stdout, labels[index], Color::Black, Some(Color::Cyan), Some(label_width));
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

fn print_process_info<W: Write>(
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
        "ID", "UUID", "PID", "Process", "Memory",
        id_width = id_width as usize,
        uuid_width = uuid_width as usize,
        pid_width = pid_width as usize,
        process_width = process_width as usize,
        mem_width = mem_width as usize,
    );
    print_colored_text(stdout, &header, Color::Black, Some(Color::Green), None);

    let process_list_start_row = header_start_row + 1;
    let total_rows = size().unwrap().1;
    let available_rows_for_processes = total_rows.saturating_sub(process_list_start_row).saturating_sub(1);

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
            format!("{}...", &process.process_name[..process_width as usize - 3])
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

        queue!(stdout, cursor::MoveTo(0, process_list_start_row + i as u16)).unwrap();
        print_colored_text(stdout, &row, fg_color, bg_color, None);
    }

    let num_rendered = processes_to_render.len();
    for i in num_rendered..(available_rows_for_processes as usize) {
        queue!(
            stdout,
            cursor::MoveTo(0, process_list_start_row + i as u16),
            terminal::Clear(ClearType::CurrentLine)
        ).unwrap();
    }
}

#[derive(Clone, Copy)]
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

fn main() {
    ensure_sudo_permissions();

    let app_state = Arc::new(Mutex::new(AppState::new()));
    let app_state_clone = Arc::clone(&app_state);

    thread::spawn(move || {
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

            let mut state = app_state_clone.lock().unwrap();
            state.gpu_info = all_gpu_info;
            state.process_info = all_processes;
            if state.loading {
                state.loading = false;
            }
            
            drop(state);
            thread::sleep(Duration::from_secs(2));
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
                let mut state = app_state.lock().unwrap();
                match key_event.code {
                    KeyCode::Esc | KeyCode::F(10) | KeyCode::Char('q') => break,
                    _ if !state.loading => { // Only handle other keys if not loading
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
                                if state.selected_process_index < state.process_info.len() - 1 {
                                    state.selected_process_index += 1;
                                }
                                let (_cols, rows) = size().unwrap();
                                let half_rows = rows / 2;
                                let visible_process_rows = half_rows.saturating_sub(1) as usize;
                                if state.selected_process_index >= state.start_index + visible_process_rows {
                                    state.start_index = state.selected_process_index - visible_process_rows + 1;
                                }
                            }
                            KeyCode::PageUp => {
                                let (_cols, rows) = size().unwrap();
                                let half_rows = rows / 2;
                                let page_size = half_rows.saturating_sub(1) as usize;
                                state.selected_process_index = state.selected_process_index.saturating_sub(page_size);
                                if state.selected_process_index < state.start_index {
                                    state.start_index = state.selected_process_index;
                                }
                            }
                            KeyCode::PageDown => {
                                let (_cols, rows) = size().unwrap();
                                let half_rows = rows / 2;
                                let page_size = half_rows.saturating_sub(1) as usize;
                                state.selected_process_index = (state.selected_process_index + page_size).min(state.process_info.len() - 1);
                                let visible_process_rows = half_rows.saturating_sub(1) as usize;
                                if state.selected_process_index >= state.start_index + visible_process_rows {
                                    state.start_index = state.selected_process_index - visible_process_rows + 1;
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

        let state = app_state.lock().unwrap();
        let (cols, rows) = size().unwrap();
        
        queue!(stdout, cursor::Hide, cursor::MoveTo(0, 0)).unwrap();

        if state.loading {
            print_function_keys(&mut stdout, cols, rows);
            print_loading_indicator(&mut stdout, cols, rows);
        } else {
            let width = cols as usize;
            let half_rows = rows / 2;

            let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            print_colored_text(&mut stdout, &format!("{}\r\n", current_time), Color::White, None, None);

            for (index, info) in state.gpu_info.iter().enumerate() {
                print_gpu_info(&mut stdout, index, info, width);
                if index < state.gpu_info.len() - 1 {
                    queue!(stdout, Print("\r\n")).unwrap();
                }
            }

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

            print_function_keys(&mut stdout, cols, rows);
        }
        
        queue!(stdout, cursor::Show).unwrap();
        stdout.flush().unwrap();
    }

    execute!(stdout, LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap();
}