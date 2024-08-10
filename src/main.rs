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
use std::process::Command; // Command 타입 임포트

fn ensure_sudo_permissions() {
    if cfg!(target_os = "macos") {
        // Attempt to update sudo timestamp or prompt for password
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

fn draw_bar<W: Write>(
    stdout: &mut W,
    label: &str,
    value: f64,
    max_value: f64,
    width: usize,
    show_text: Option<String>,
) {
    let label_width = label.len();
    let text_width = show_text.as_ref().map_or(0, |text| text.len()); // Calculate text length
    let available_bar_width = width.saturating_sub(label_width + 4); // 4 for label and surrounding characters

    let _percentage = (value / max_value) * 100.0;
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

    execute!(
        stdout,
        SetForegroundColor(Color::Blue),
        Print(format!("{}: [", label)),
        SetForegroundColor(Color::Green),
        Print(filled_bar),
        Print(empty_bar),
        ResetColor
    )
    .unwrap();

    if let Some(text) = show_text {
        execute!(
            stdout,
            SetForegroundColor(Color::White),
            Print(format!("{:>width$}", text, width = text_width)),
            ResetColor,
            Print("] ")
        )
        .unwrap();
    } else {
        execute!(stdout, Print("] ")).unwrap();
    }
}

fn main() {
    ensure_sudo_permissions(); // Check for sudo permissions on macOS

    let gpu_readers = get_gpu_readers();
    let mut stdout = stdout();

    // Initialize the terminal screen and switch to alternate screen mode
    execute!(
        stdout,
        EnterAlternateScreen,
        terminal::Clear(ClearType::All)
    )
    .unwrap();

    loop {
        // Check if the ESC key is pressed
        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                if key_event.code == KeyCode::Esc {
                    break;
                }
            }
        }

        // Move the cursor to the top and avoid clearing the screen
        execute!(stdout, cursor::MoveTo(0, 0)).unwrap();

        // Print the current time
        let current_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        execute!(stdout, Print(format!("{}\n", current_time))).unwrap();

        // Get the current terminal size
        let (cols, _) = size().unwrap();
        let half_width = (cols / 2 - 2) as usize; // Adjusted width to prevent overflow

        let mut all_gpu_info: Vec<GpuInfo> = Vec::new();

        for reader in &gpu_readers {
            let gpu_info = reader.get_gpu_info();
            all_gpu_info.extend(gpu_info);
        }

        // Print GPU information
        for (index, info) in all_gpu_info.iter().enumerate() {
            let used_memory_gib = info.used_memory as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_memory_gib = info.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);
            let memory_text = format!("{:.2}/{:.2}Gi", used_memory_gib, total_memory_gib);
            let gpu_percentage_text = format!("{:.2}%", info.utilization);
            let freq_text = format!("{} MHz", info.frequency);
            let power_text = format!("{:.2} W", info.power_consumption);

            execute!(
                stdout,
                SetForegroundColor(Color::Blue),
                Print(format!("DEVICE {}: ", index + 1)),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{}  ", info.name)),
                SetForegroundColor(Color::Blue),
                Print("Total: "),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{:.2} GiB  ", total_memory_gib)),
                SetForegroundColor(Color::Blue),
                Print("Used: "),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{:.2} GiB  ", used_memory_gib)),
                SetForegroundColor(Color::Blue),
                Print("Temp.: "),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{}°C  ", info.temperature)),
                SetForegroundColor(Color::Blue),
                Print("FREQ: "),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{}  ", freq_text)),
                SetForegroundColor(Color::Blue),
                Print("POW: "),
                ResetColor,
                SetForegroundColor(Color::White),
                Print(format!("{}\n", power_text)),
                ResetColor
            )
            .unwrap();

            // Print GPU utilization bar (Adjusted for terminal width)
            draw_bar(
                &mut stdout,
                "GPU",
                info.utilization,
                100.0,
                half_width,
                Some(gpu_percentage_text),
            );

            // Print MEM utilization bar (Adjusted for terminal width)
            draw_bar(
                &mut stdout,
                "MEM",
                used_memory_gib,
                total_memory_gib,
                half_width,
                Some(memory_text),
            );

            // Add a blank line between devices
            if index < all_gpu_info.len() - 1 {
                execute!(stdout, Print("\n\n")).unwrap();
            }
        }

        // Wait for 1 second
        thread::sleep(Duration::from_secs(1));
    }

    // Exit alternate screen mode and restore the original screen
    execute!(stdout, LeaveAlternateScreen).unwrap();
}