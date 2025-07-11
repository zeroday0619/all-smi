use std::io::Write;

use crossterm::{queue, style::Color, style::Print};

use crate::device::ProcessInfo;
use crate::ui::text::{print_colored_text, truncate_to_width};

pub fn print_process_info<W: Write>(
    stdout: &mut W,
    processes: &[ProcessInfo],
    selected_index: usize,
    start_index: usize,
    half_rows: u16,
    cols: u16,
    horizontal_scroll_offset: usize,
    current_user: &str,
) {
    queue!(stdout, Print("\r\nProcesses:\r\n")).unwrap();

    let width = cols as usize;

    // Fixed column widths based on actual data sizes
    // PID: 7 (up to 9999999), USER: 12, PRI: 3, NI: 3, VIRT: 6, RES: 6, S: 1,
    // CPU%: 5, MEM%: 5, GPU%: 5, GPUMEM: 7, TIME+: 8, Command: remaining
    let fixed_widths = [7, 12, 3, 3, 6, 6, 1, 5, 5, 5, 7, 8];
    let num_gaps = fixed_widths.len(); // Gaps between columns (not after last column)
    let fixed_total: usize = fixed_widths.iter().sum::<usize>() + num_gaps;

    // Give remaining space to command column, ensure at least 20 chars
    let _command_w = if width > fixed_total + 20 {
        width - fixed_total
    } else {
        20
    };

    let (pid_w, user_w, pri_w, ni_w, virt_w, res_w, s_w, cpu_w, mem_w, gpu_w, gpu_mem_w, time_w) = (
        fixed_widths[0],  // PID: 7
        fixed_widths[1],  // USER: 12
        fixed_widths[2],  // PRI: 3
        fixed_widths[3],  // NI: 3
        fixed_widths[4],  // VIRT: 6
        fixed_widths[5],  // RES: 6
        fixed_widths[6],  // S: 1
        fixed_widths[7],  // CPU%: 5
        fixed_widths[8],  // MEM%: 5
        fixed_widths[9],  // GPU%: 5
        fixed_widths[10], // GPUMEM: 7
        fixed_widths[11], // TIME+: 8
    );

    // Build header format string with proper alignment
    let header_format = format!(
        "{:>pid_w$} {:<user_w$} {:>pri_w$} {:>ni_w$} {:>virt_w$} {:>res_w$} {:<s_w$} {:>cpu_w$} {:>mem_w$} {:>gpu_w$} {:>gpu_mem_w$} {:>time_w$} {}",
        "PID", "USER", "PRI", "NI", "VIRT", "RES", "S", "CPU%", "MEM%", "GPU%", "GPUMEM", "TIME+", "Command",
        pid_w = pid_w,
        user_w = user_w,
        pri_w = pri_w,
        ni_w = ni_w,
        virt_w = virt_w,
        res_w = res_w,
        s_w = s_w,
        cpu_w = cpu_w,
        mem_w = mem_w,
        gpu_w = gpu_w,
        gpu_mem_w = gpu_mem_w,
        time_w = time_w,
    );

    // Apply horizontal scrolling
    let visible_header = if horizontal_scroll_offset < header_format.len() {
        let scrolled = &header_format[horizontal_scroll_offset..];
        // Pad the header to full width to clear any previous content
        format!(
            "{:<width$}",
            truncate_to_width(scrolled, width),
            width = width
        )
    } else {
        // Clear the entire line when scrolled past the content
        " ".repeat(width)
    };

    print_colored_text(stdout, &visible_header, Color::White, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Print separator line
    let separator = "─".repeat(width.min(120));
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate how many processes we can display
    let available_rows = half_rows.saturating_sub(3) as usize; // Reserve 3 rows for header and separator
    let end_index = (start_index + available_rows).min(processes.len());

    // Print process information
    for i in start_index..end_index {
        if let Some(process) = processes.get(i) {
            let is_selected = i == selected_index;

            // Format process information with proper truncation
            let pid = format!("{}", process.pid);
            let user = truncate_to_width(&process.user, user_w);
            let priority = format!("{}", process.priority);
            let nice = format!("{:+}", process.nice_value); // Show + for positive values

            // Format memory sizes
            let virt = format_memory_size(process.memory_vms);
            let res = format_memory_size(process.memory_rss);

            let state = truncate_to_width(&process.state, s_w);
            let cpu_percent = format!("{:.1}", process.cpu_percent);
            let mem_percent = format!("{:.1}", process.memory_percent);

            // Format GPU utilization
            let gpu_percent = if process.uses_gpu && process.gpu_utilization > 0.0 {
                format!("{:.1}", process.gpu_utilization)
            } else if process.uses_gpu {
                "-".to_string()
            } else {
                "".to_string()
            };

            // Format GPU memory usage
            let gpu_mem = if process.used_memory > 0 {
                let gpu_mem_mb = process.used_memory as f64 / (1024.0 * 1024.0);
                if gpu_mem_mb >= 1024.0 {
                    format!("{:.1}G", gpu_mem_mb / 1024.0)
                } else {
                    format!("{gpu_mem_mb:.0}M")
                }
            } else if process.uses_gpu {
                "-".to_string()
            } else {
                "".to_string()
            };

            // Format CPU time
            let time_plus = format_cpu_time(process.cpu_time);

            let command = process.command.clone();

            // Build the row with proper formatting and padding
            let row_format = format!(
                "{:>pid_w$} {:<user_w$} {:>pri_w$} {:>ni_w$} {:>virt_w$} {:>res_w$} {:<s_w$} {:>cpu_w$} {:>mem_w$} {:>gpu_w$} {:>gpu_mem_w$} {:>time_w$} {}",
                pid,
                truncate_to_width(&user, user_w),
                priority,
                nice,
                virt,
                res,
                state,
                cpu_percent,
                mem_percent,
                gpu_percent,
                gpu_mem,
                time_plus,
                command,
                pid_w = pid_w,
                user_w = user_w,
                pri_w = pri_w,
                ni_w = ni_w,
                virt_w = virt_w,
                res_w = res_w,
                s_w = s_w,
                cpu_w = cpu_w,
                mem_w = mem_w,
                gpu_w = gpu_w,
                gpu_mem_w = gpu_mem_w,
                time_w = time_w,
            );

            // Apply horizontal scrolling
            let visible_row = if horizontal_scroll_offset < row_format.len() {
                let scrolled = &row_format[horizontal_scroll_offset..];
                // Pad the row to full width to clear any previous content
                format!(
                    "{:<width$}",
                    truncate_to_width(scrolled, width),
                    width = width
                )
            } else {
                // Clear the entire line when scrolled past the content
                " ".repeat(width)
            };

            // Print with selection highlight
            if is_selected {
                print_colored_text(stdout, &visible_row, Color::Black, Some(Color::White), None);
            } else {
                // Determine base color based on user
                let is_current_user = process.user == current_user;
                let is_root = process.user == "root";

                // Enhanced color coding based on resource utilization and user
                let text_color = if process.cpu_percent >= 90.0 || process.memory_percent >= 90.0 {
                    // Extremely high utilization (90%+) - Critical
                    Color::Red
                } else if process.cpu_percent >= 80.0 || process.memory_percent >= 80.0 {
                    // Very high utilization (80-90%) - Also red for visibility
                    Color::Rgb {
                        r: 255,
                        g: 100,
                        b: 100,
                    } // Bright red
                } else if process.cpu_percent >= 70.0 || process.memory_percent >= 70.0 {
                    // High utilization (70-80%) - Danger zone
                    Color::Yellow
                } else if process.cpu_percent >= 50.0 || process.memory_percent >= 50.0 {
                    // Moderate utilization (50-70%) - Warning
                    Color::Rgb {
                        r: 255,
                        g: 200,
                        b: 0,
                    } // Orange/amber
                } else if process.uses_gpu
                    && (process.cpu_percent >= 30.0 || process.memory_percent >= 30.0)
                {
                    // GPU process with notable system resource usage
                    Color::Cyan
                } else if process.uses_gpu {
                    // GPU process with low system resource usage
                    Color::Green
                } else if is_current_user {
                    // Current user's process
                    Color::White
                } else if is_root || process.user == "unknown" {
                    // Root or unknown user's process
                    Color::DarkGrey
                } else {
                    // Other users' processes
                    Color::DarkGrey
                };
                print_colored_text(stdout, &visible_row, text_color, None, None);
            }

            queue!(stdout, Print("\r\n")).unwrap();
        }
    }

    // Show navigation info if there are more processes
    if processes.len() > available_rows {
        let nav_info = format!(
            "Showing {}-{} of {} processes (Use ↑↓ to navigate, PgUp/PgDn for pages)",
            start_index + 1,
            end_index,
            processes.len()
        );
        print_colored_text(stdout, &nav_info, Color::DarkGrey, None, None);
        queue!(stdout, Print("\r\n")).unwrap();
    }

    // Show process statistics
    if !processes.is_empty() {
        let total_gpu_mem: u64 = processes.iter().map(|p| p.used_memory).sum();
        let gpu_mem_gb = total_gpu_mem as f64 / (1024.0 * 1024.0 * 1024.0);

        let active_processes = processes.iter().filter(|p| p.cpu_percent > 0.1).count();
        let gpu_processes = processes.iter().filter(|p| p.used_memory > 0).count();

        let stats = format!(
            "Active: {active_processes} | GPU: {gpu_processes} | Total GPU Memory: {gpu_mem_gb:.1}GB"
        );
        print_colored_text(stdout, &stats, Color::Cyan, None, None);
        queue!(stdout, Print("\r\n")).unwrap();
    }
}

/// Format memory size in human-readable format (e.g., 187T, 123G, 500M, 16K)
fn format_memory_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0".to_string();
    }

    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let mb = bytes as f64 / (1024.0 * 1024.0);
    let kb = bytes as f64 / 1024.0;

    if gb >= 1000.0 {
        // Only show TB if >= 1000GB
        let tb = gb / 1024.0;
        format!("{tb:.0}T")
    } else if gb >= 1.0 {
        format!("{gb:.0}G")
    } else if mb >= 1.0 {
        format!("{mb:.0}M")
    } else if kb >= 1.0 {
        format!("{kb:.0}K")
    } else {
        format!("{bytes}")
    }
}

/// Format CPU time in TIME+ format (e.g., 12:34.56, 1:23:45)
/// For extremely long-running basic system processes, show as 0:00:00
fn format_cpu_time(seconds: u64) -> String {
    if seconds == 0 {
        return "0:00:00".to_string();
    }

    // If the process has been running for more than 365 days (basic system process)
    // show as 0:00:00 to avoid clutter
    if seconds > 365 * 24 * 3600 {
        return "0:00:00".to_string();
    }

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, secs)
    }
}
