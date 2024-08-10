mod gpu;

use std::time::{Duration, Instant};
use crate::gpu::{get_gpu_readers, GpuInfo};
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
    let _time = &info.time;

    let labels = vec![
        (format!("DEVICE {}: ", index + 1), Color::Blue),
        (format!("{}  ", info.name), Color::White),
        (String::from("Total: "), Color::Blue),
        (format!("{:.2} GiB  ", total_memory_gib), Color::White),
        (String::from("Used: "), Color::Blue),
        (format!("{:.2} GiB  ", used_memory_gib), Color::White),
        (String::from("Temp.: "), Color::Blue),
        (format!("{}°C  ", info.temperature), Color::White),
        (String::from("FREQ: "), Color::Blue),
        (format!("{}  ", freq_text), Color::White),
        (String::from("POW: "), Color::Blue),
        (format!("{}\r\n", power_text), Color::White),
    ];

    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }

    draw_bar(stdout, "GPU", info.utilization, 100.0, half_width, Some(gpu_percentage_text));
    draw_bar(stdout, "MEM", used_memory_gib, total_memory_gib, half_width, Some(memory_text));

    execute!(stdout, Print("\r\n")).unwrap(); // Move cursor to the start of the next line
}

fn print_function_keys<W: Write>(stdout: &mut W, cols: u16) {
    let key_width = 3; // Width for each function key label
    let total_width = cols as usize; // Total width of the terminal
    let min_label_width = 5; // Minimum width for label text
    let label_width = (total_width / 10).saturating_sub(key_width).max(min_label_width); // Ensure label_width is at least min_label_width

    let function_keys = vec!["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"];

    let labels = vec![
        "Help", "", "", "", "", "", "", "", "", "Quit",
    ];

    execute!(stdout, cursor::MoveTo(0, cols.saturating_sub(1) - 1)).unwrap();

    for (index, key) in function_keys.iter().enumerate() {
        print_colored_text(stdout, key, Color::White, Some(Color::Black), Some(key_width));
        print_colored_text(stdout, labels[index], Color::Black, Some(Color::Cyan), Some(label_width));
    }
}

fn main() {
    ensure_sudo_permissions(); // Check for sudo permissions on macOS

    let gpu_readers = get_gpu_readers();
    let mut stdout = stdout();

    enable_raw_mode().unwrap(); // Enable raw mode to prevent key echo
    execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .unwrap();

    loop {
        let start_time = Instant::now();

        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                if key_event.code == KeyCode::Esc || key_event.code == KeyCode::F(10) {
                    break;
                }
                if let KeyCode::Char(c) = key_event.code {
                    if c.to_ascii_lowercase() == 'q' {
                        break;
                    }
                }
            }
        }

        execute!(stdout, cursor::MoveTo(0, 0)).unwrap();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        print_colored_text(&mut stdout, &format!("{}\r\n", current_time), Color::White, None, None);

        let (cols, _rows) = size().unwrap();
        let half_width = (cols / 2 - 2) as usize;

        let all_gpu_info: Vec<GpuInfo> = gpu_readers
            .iter()
            .flat_map(|reader| reader.get_gpu_info())
            .collect();

        for (index, info) in all_gpu_info.iter().enumerate() {
            print_gpu_info(&mut stdout, index, info, half_width);

            if index < all_gpu_info.len() - 1 {
                execute!(stdout, Print("\r\n")).unwrap();
            }
        }

        print_function_keys(&mut stdout, cols);

        stdout.flush().unwrap(); // Ensure all output is flushed to the terminal

        // Calculate elapsed time and sleep for the remaining time of the interval
        let elapsed_time = start_time.elapsed();
        let update_interval = Duration::from_secs(1.5);

        if elapsed_time < update_interval {
            std::thread::sleep(update_interval - elapsed_time);
        }
    }
    // Exit alternate screen mode and restore terminal settings
    execute!(stdout, LeaveAlternateScreen).unwrap();
    disable_raw_mode().unwrap(); // Disable raw mode
}