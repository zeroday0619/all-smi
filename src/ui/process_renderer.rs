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
) {
    queue!(stdout, Print("\r\nProcesses:\r\n")).unwrap();

    let width = cols as usize;

    // Calculate column widths dynamically based on terminal width
    let min_widths = [6, 12, 8, 6, 8, 4, 8, 8, 10]; // Minimum widths for each column (added GPU column)
    let total_min_width: usize = min_widths.iter().sum::<usize>() + 8; // 8 spaces between columns

    let (pid_w, user_w, name_w, cpu_w, mem_w, gpu_w, gpu_mem_w, state_w, command_w) =
        if width > total_min_width {
            let extra_space = width - total_min_width;
            // Distribute extra space mainly to name and command columns
            let name_extra = extra_space / 3;
            let command_extra = extra_space - name_extra;

            (
                min_widths[0],                 // PID: 6
                min_widths[1],                 // USER: 12
                min_widths[2] + name_extra,    // NAME: 8 + extra
                min_widths[3],                 // CPU%: 6
                min_widths[4],                 // MEM%: 8
                min_widths[5],                 // GPU: 4
                min_widths[6],                 // GPU MEM: 8
                min_widths[7],                 // STATE: 8
                min_widths[8] + command_extra, // COMMAND: 10 + extra
            )
        } else {
            // Use minimum widths if terminal is too narrow
            (
                min_widths[0],
                min_widths[1],
                min_widths[2],
                min_widths[3],
                min_widths[4],
                min_widths[5],
                min_widths[6],
                min_widths[7],
                min_widths[8],
            )
        };

    // Print header
    let header_format = format!(
        "{:<pid_w$} {:<user_w$} {:<name_w$} {:<cpu_w$} {:<mem_w$} {:<gpu_w$} {:<gpu_mem_w$} {:<state_w$} {:<command_w$}",
        "PID", "USER", "NAME", "CPU%", "MEM%", "GPU", "GPU MEM", "STATE", "COMMAND"
    );
    print_colored_text(stdout, &header_format, Color::White, None, None);
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
            let name = truncate_to_width(&process.process_name, name_w);
            let cpu_percent = format!("{:.1}", process.cpu_percent);
            let mem_percent = format!("{:.1}", process.memory_percent);

            // Format GPU memory usage
            let gpu_mem = if process.used_memory > 0 {
                let gpu_mem_mb = process.used_memory as f64 / (1024.0 * 1024.0);
                if gpu_mem_mb >= 1024.0 {
                    format!("{:.1}GB", gpu_mem_mb / 1024.0)
                } else {
                    format!("{gpu_mem_mb:.0}MB")
                }
            } else {
                "-".to_string()
            };

            let state = truncate_to_width(&process.state, state_w);
            let command = truncate_to_width(&process.command, command_w);

            // GPU indicator
            let gpu_indicator = if process.uses_gpu { "Yes" } else { "" };

            // Format the complete row
            let row_format = format!(
                "{:<pid_w$} {:<user_w$} {:<name_w$} {:<cpu_w$} {:<mem_w$} {:<gpu_w$} {:<gpu_mem_w$} {:<state_w$} {:<command_w$}",
                truncate_to_width(&pid, pid_w),
                user,
                name,
                truncate_to_width(&cpu_percent, cpu_w),
                truncate_to_width(&mem_percent, mem_w),
                truncate_to_width(gpu_indicator, gpu_w),
                truncate_to_width(&gpu_mem, gpu_mem_w),
                state,
                command
            );

            // Print with selection highlight
            if is_selected {
                print_colored_text(stdout, &row_format, Color::Black, Some(Color::White), None);
            } else {
                // Color code based on resource usage
                let text_color = if process.cpu_percent > 80.0 || process.memory_percent > 80.0 {
                    Color::Red
                } else if process.cpu_percent > 50.0 || process.memory_percent > 50.0 {
                    Color::Yellow
                } else if process.uses_gpu {
                    Color::Green // Has GPU usage
                } else {
                    Color::White
                };
                print_colored_text(stdout, &row_format, text_color, None, None);
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
