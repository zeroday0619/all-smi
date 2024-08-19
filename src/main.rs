mod gpu;

use std::time::{Duration, Instant};
use std::cmp::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use crate::gpu::{get_gpu_readers, GpuInfo, ProcessInfo};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size, enable_raw_mode, disable_raw_mode},
};
use chrono::Local;
use std::io::{stdout, Write};
use std::process::Command;

// GpuReader 트레이트를 가져옵니다.
use crate::gpu::GpuReader;

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
        execute!(
            stdout,
            SetForegroundColor(fg_color),
            SetBackgroundColor(bg),
            Print(adjusted_text),
            ResetColor
        )
        .unwrap();
    } else {
        execute!(
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

    execute!(stdout, Print("] ")).unwrap();
}

fn print_gpu_info<W: Write>(stdout: &mut W, index: usize, info: &GpuInfo, half_width: usize) {
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

    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }

    draw_bar(stdout, "GPU", info.utilization, 100.0, half_width, Some(gpu_percentage_text));
    draw_bar(stdout, "MEM", used_memory_gib, total_memory_gib, half_width, Some(memory_text));

    execute!(stdout, Print("\r\n")).unwrap(); // Move cursor to the start of the next line
}

fn print_function_keys<W: Write>(stdout: &mut W, cols: usize) {
    let key_width: usize = 3; // Width for each function key label
    let total_width: usize = cols; // Total width of the terminal
    let min_label_width: usize = 5; // Minimum width for label text
    let label_width = (total_width / 10).saturating_sub(key_width).max(min_label_width); // Ensure label_width is at least min_label_width

    let function_keys = vec!["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

    let labels = vec![
        "Help", "", "", "", "", "", "", "", "", "Quit",
    ];

    execute!(stdout, cursor::MoveTo(0, (cols.saturating_sub(1) - 1) as u16)).unwrap();

    for (index, key) in function_keys.iter().enumerate() {
        print_colored_text(stdout, key, Color::White, Some(Color::Black), Some(key_width));
        print_colored_text(stdout, labels[index], Color::Black, Some(Color::Cyan), Some(label_width));
    }
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
    let process_width: u16 = cols - id_width - uuid_width - pid_width - mem_width - 2;

    // Print the header
    execute!(stdout, cursor::MoveTo(0, rows)).unwrap();
    let header = format!(
        "{:<id_width$}{:<uuid_width$}{:<pid_width$}{:<process_width$}{:<mem_width$}",
        "ID", "UUID", "PID", "Process", "Memory",
        id_width = id_width as usize,
        uuid_width = uuid_width as usize,
        pid_width = pid_width as usize,
        process_width = process_width as usize,
        mem_width = mem_width as usize,
    );
    print_colored_text(stdout, &header, Color::Black, Some(Color::Green), None);

    // Print each process
    for (i, process) in processes.iter().enumerate().skip(start_index).take((rows as usize) / 2) {
        let uuid_display = if process.device_uuid.len() > uuid_width as usize {
            &process.device_uuid[..uuid_width as usize]
        } else {
            &process.device_uuid
        };

        let process_display = if process.process_name.len() > process_width as usize {
            &process.process_name[..process_width as usize]
        } else {
            &process.process_name
        };

        let row = format!(
            "{:<id_width$}{:<uuid_width$}{:<pid_width$}{:<process_width$}{:<mem_width$}",
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

        let fg_color = if i == selected_process_index {
            Color::Black
        } else {
            Color::White
        };

        let bg_color = if i == selected_process_index {
            Some(Color::Cyan)
        } else {
            None
        };

        execute!(stdout, cursor::MoveTo(0, rows + 1 + i as u16)).unwrap();
        print_colored_text(stdout, &row, fg_color, bg_color, None);
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
    ensure_sudo_permissions(); // Check for sudo permissions on macOS

    let mut stdout = stdout();

    enable_raw_mode().unwrap(); // Enable raw mode to prevent key echo
    execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .unwrap();

    let mut selected_process_index: usize = 0;
    let mut start_index: usize = 0;
    let mut sort_criteria = SortCriteria::Pid;
    let (_cols, rows) = size().unwrap();

    // GPU 정보와 프로세스 정보를 저장할 공간
    let gpu_info = Arc::new(Mutex::new(Vec::<GpuInfo>::new()));
    let process_info = Arc::new(Mutex::new(Vec::<ProcessInfo>::new()));

    // GPU 정보와 프로세스 정보를 업데이트하는 스레드 생성
    let gpu_info_clone = gpu_info.clone();
    let process_info_clone = process_info.clone();
    let gpu_readers = Arc::new(Mutex::new(get_gpu_readers()));
    let update_thread = thread::spawn(move || {
        loop {
            let mut gpu_info_lock = gpu_info_clone.lock().unwrap();
            let mut process_info_lock = process_info_clone.lock().unwrap();
            let gpu_readers_lock = gpu_readers.lock().unwrap();

            // GPU 정보 업데이트
            *gpu_info_lock = gpu_readers_lock
                .iter()
                .flat_map(|reader| reader.get_gpu_info())
                .collect();

            // 프로세스 정보 업데이트
            *process_info_lock = gpu_readers_lock
                .iter()
                .flat_map(|reader| reader.get_process_info())
                .collect();

            // 500밀리초마다 업데이트
            thread::sleep(Duration::from_millis(2000));
        }
    });

    loop {
        let start_time = Instant::now();

        // 키 입력을 비블로킹 방식으로 확인
        if event::poll(Duration::from_millis(10)).unwrap() {
            // 키 입력을 처리합니다.
            if let Ok(Event::Key(key_event)) = event::read() {
                match key_event.code {
                    KeyCode::Esc | KeyCode::F(10) => break,
                    KeyCode::Char(c) if c.to_ascii_lowercase() == 'q' => break,
                    KeyCode::Up => {
                        if selected_process_index > 0 {
                            selected_process_index -= 1;
                        }
                        if selected_process_index < start_index {
                            start_index -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected_process_index < usize::MAX {
                            selected_process_index = selected_process_index.saturating_add(1);
                        }
                        if selected_process_index >= start_index + (rows / 2) as usize {
                            start_index += 1;
                        }
                    }
                    KeyCode::Char('p') => sort_criteria = SortCriteria::Pid,
                    KeyCode::Char('m') => sort_criteria = SortCriteria::Memory,
                    _ => {}
                }
            }
        }

        execute!(stdout, cursor::MoveTo(0, 0)).unwrap();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        print_colored_text(&mut stdout, &format!("{}\r\n", current_time), Color::White, None, None);

        let (cols, rows) = size().unwrap();
        let half_width = (cols / 2 - 2) as usize;
        let half_rows = rows / 2;

        // GPU 정보 출력
        let gpu_info_lock = gpu_info.lock().unwrap();
        for (index, info) in gpu_info_lock.iter().enumerate() {
            print_gpu_info(&mut stdout, index, info, half_width);

            if index < gpu_info_lock.len() - 1 {
                execute!(stdout, Print("\r\n")).unwrap();
            }
        }

        // 프로세스 정보 출력
        let process_info_lock = process_info.lock().unwrap();
        let mut sorted_process_info = process_info_lock.clone();
        sorted_process_info.sort_by(|a, b| sort_criteria.sort(a, b));

        print_process_info(
            &mut stdout,
            &sorted_process_info,
            selected_process_index,
            start_index,
            half_rows,
            cols,
        );

        print_function_keys(&mut stdout, cols as usize);

        stdout.flush().unwrap(); // Ensure all output is flushed to the terminal

        let elapsed_time = start_time.elapsed();
        let update_interval = Duration::from_millis(500); // 100밀리초마다 업데이트

        if elapsed_time < update_interval {
            std::thread::sleep(update_interval - elapsed_time);
        }
    }

    // 스레드 종료
    update_thread.join().unwrap();

    execute!(stdout, LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap(); // Disable raw mode
}