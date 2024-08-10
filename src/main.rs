mod gpu;

use std::thread;
use std::time::Duration;
use crate::gpu::{get_gpu_readers, GpuInfo};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
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
    color: Color,
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

    execute!(
        stdout,
        SetForegroundColor(color),
        Print(adjusted_text),
        ResetColor
    )
    .unwrap();
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

    print_colored_text(stdout, &format!("{}: [", label), Color::Blue, None);
    print_colored_text(stdout, &filled_bar, Color::Green, None);
    print_colored_text(stdout, &empty_bar, Color::Green, None);

    if let Some(text) = show_text {
        print_colored_text(stdout, &text, Color::White, Some(text_width));
    }

    execute!(stdout, Print("] ")).unwrap();
}

fn print_gpu_info<W: Write>(stdout: &mut W, index: usize, info: &GpuInfo, half_width: usize) {
    let _time = &info.time; 
    let used_memory_gib = info.used_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_memory_gib = info.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let memory_text = format!("{:.2}/{:.2}Gi", used_memory_gib, total_memory_gib);
    let gpu_percentage_text = format!("{:.2}%", info.utilization);
    let freq_text = format!("{} MHz", info.frequency);
    let power_text = format!("{:.2} W", info.power_consumption);

    print_colored_text(stdout, &format!("DEVICE {}: ", index + 1), Color::Blue, None);
    print_colored_text(stdout, &format!("{}  ", info.name), Color::White, None);
    print_colored_text(stdout, "Total: ", Color::Blue, None);
    print_colored_text(stdout, &format!("{:.2} GiB  ", total_memory_gib), Color::White, None);
    print_colored_text(stdout, "Used: ", Color::Blue, None);
    print_colored_text(stdout, &format!("{:.2} GiB  ", used_memory_gib), Color::White, None);
    print_colored_text(stdout, "Temp.: ", Color::Blue, None);
    print_colored_text(stdout, &format!("{}°C  ", info.temperature), Color::White, None);
    print_colored_text(stdout, "FREQ: ", Color::Blue, None);
    print_colored_text(stdout, &format!("{}  ", freq_text), Color::White, None);
    print_colored_text(stdout, "POW: ", Color::Blue, None);
    print_colored_text(stdout, &format!("{}\n", power_text), Color::White, None);

    draw_bar(stdout, "GPU", info.utilization, 100.0, half_width, Some(gpu_percentage_text));
    draw_bar(stdout, "MEM", used_memory_gib, total_memory_gib, half_width, Some(memory_text));
}

fn main() {
    ensure_sudo_permissions(); // Check for sudo permissions on macOS

    let gpu_readers = get_gpu_readers();
    let mut stdout = stdout();

    execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .unwrap();

    loop {
        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                if key_event.code == KeyCode::Esc {
                    break;
                }
            }
        }

        execute!(stdout, cursor::MoveTo(0, 0)).unwrap();

        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        print_colored_text(&mut stdout, &format!("{}\n", current_time), Color::White, None);

        let (cols, _) = size().unwrap();
        let half_width = (cols / 2 - 2) as usize;

        let all_gpu_info: Vec<GpuInfo> = gpu_readers
            .iter()
            .flat_map(|reader| reader.get_gpu_info())
            .collect();

        for (index, info) in all_gpu_info.iter().enumerate() {
            print_gpu_info(&mut stdout, index, info, half_width);

            if index < all_gpu_info.len() - 1 {
                execute!(stdout, Print("\n\n")).unwrap();
            }
        }

        thread::sleep(Duration::from_secs(1));
    }

    execute!(stdout, LeaveAlternateScreen).unwrap();
}