use std::io::Write;

use crossterm::{queue, style::Color, style::Print};

use crate::device::{CpuInfo, GpuInfo, MemoryInfo};
use crate::storage::info::StorageInfo;
use crate::ui::text::{print_colored_text, truncate_to_width};
use crate::ui::widgets::draw_bar;

pub fn print_gpu_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &GpuInfo,
    width: usize,
    device_name_scroll_offset: usize,
    hostname_scroll_offset: usize,
) {
    // Format device name with scrolling if needed
    let device_name = if info.name.len() > 15 {
        let scroll_len = info.name.len() + 3;
        let start_pos = device_name_scroll_offset % scroll_len;
        let extended_name = format!("{}   {}", info.name, info.name);
        let visible_name = extended_name
            .chars()
            .skip(start_pos)
            .take(15)
            .collect::<String>();
        visible_name
    } else {
        info.name.clone()
    };

    // Format hostname with scrolling if needed
    let hostname_display = if info.hostname.len() > 9 {
        let scroll_len = info.hostname.len() + 3;
        let start_pos = hostname_scroll_offset % scroll_len;
        let extended_hostname = format!("{}   {}", info.hostname, info.hostname);
        let visible_hostname = extended_hostname
            .chars()
            .skip(start_pos)
            .take(9)
            .collect::<String>();
        visible_hostname
    } else {
        info.hostname.clone()
    };

    // Calculate values
    let memory_gb = info.used_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_memory_gb = info.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let memory_percent = if info.total_memory > 0 {
        (info.used_memory as f64 / info.total_memory as f64) * 100.0
    } else {
        0.0
    };

    // Print info line: GPU <name> @ <hostname> Util:4.0% Mem:25.2/128GB Temp:0°C Pwr:0.0W
    print_colored_text(stdout, "GPU ", Color::Cyan, None, None);
    print_colored_text(stdout, &device_name, Color::White, None, None);
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &hostname_display, Color::White, None, None);
    print_colored_text(stdout, " Util:", Color::Yellow, None, None);
    print_colored_text(
        stdout,
        &format!("{:>5.1}%", info.utilization),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Mem:", Color::Blue, None, None);
    print_colored_text(
        stdout,
        &format!("{:>11}", format!("{memory_gb:.1}/{total_memory_gb:.0}GB")),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Temp:", Color::Magenta, None, None);
    print_colored_text(
        stdout,
        &format!("{:>4}°C", info.temperature),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Pwr:", Color::Red, None, None);

    // Check if power_limit_max is available and display as current/max
    let power_display = if let Some(power_max_str) = info.detail.get("power_limit_max") {
        if let Ok(power_max) = power_max_str.parse::<f64>() {
            format!("{:.0}/{:.0}W", info.power_consumption, power_max)
        } else {
            format!("{:.0}W", info.power_consumption)
        }
    } else {
        format!("{:.0}W", info.power_consumption)
    };

    // Dynamically adjust width based on content, with minimum of 8 chars
    let display_width = power_display.len().max(8);
    print_colored_text(
        stdout,
        &format!("{power_display:>display_width$}"),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side and 2 space separation
    let available_width = width.saturating_sub(10); // 5 padding each side
    let is_apple_silicon = info.name.contains("Apple") || info.name.contains("Metal");
    let num_gauges = if is_apple_silicon { 3 } else { 2 }; // Util, Mem, (ANE for Apple Silicon only)
    let gauge_width = (available_width - (num_gauges - 1) * 2) / num_gauges; // 2 spaces between gauges

    // Print gauges on one line with proper spacing
    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    // Util gauge
    draw_bar(
        stdout,
        "Util",
        info.utilization,
        100.0,
        gauge_width,
        Some(format!("{:.1}%", info.utilization)),
    );
    print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

    // Memory gauge
    draw_bar(
        stdout,
        "Mem",
        memory_percent,
        100.0,
        gauge_width,
        Some(format!("{memory_gb:.1}GB")),
    );

    // ANE gauge only for Apple Silicon
    if is_apple_silicon {
        print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator
        draw_bar(
            stdout,
            "ANE",
            info.ane_utilization,
            100.0,
            gauge_width,
            Some(format!("{:.1}%", info.ane_utilization)),
        );
    }

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char right padding
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_cpu_info<W: Write>(stdout: &mut W, _index: usize, info: &CpuInfo, width: usize) {
    // Print CPU info line
    print_colored_text(stdout, "CPU ", Color::Cyan, None, None);
    print_colored_text(
        stdout,
        &truncate_to_width(&info.cpu_model, 25),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &info.hostname, Color::White, None, None);
    print_colored_text(stdout, " Arch:", Color::Blue, None, None);
    print_colored_text(stdout, &info.architecture, Color::White, None, None);
    print_colored_text(stdout, " Sockets:", Color::Yellow, None, None);
    print_colored_text(
        stdout,
        &format!("{:>2}", info.socket_count),
        Color::White,
        None,
        None,
    );
    // Show P-Core/E-Core counts for Apple Silicon, regular core count for others
    if let Some(apple_info) = &info.apple_silicon_info {
        print_colored_text(stdout, " P-Cores:", Color::Green, None, None);
        print_colored_text(
            stdout,
            &format!("{:>2}", apple_info.p_core_count),
            Color::White,
            None,
            None,
        );
        print_colored_text(stdout, " E-Cores:", Color::Green, None, None);
        print_colored_text(
            stdout,
            &format!("{:>2}", apple_info.e_core_count),
            Color::White,
            None,
            None,
        );
    } else {
        print_colored_text(stdout, " Cores:", Color::Green, None, None);
        print_colored_text(
            stdout,
            &format!("{:>2}", info.total_cores),
            Color::White,
            None,
            None,
        );
    }

    let freq_ghz = info.max_frequency_mhz as f64 / 1000.0;
    print_colored_text(stdout, " Freq:", Color::Magenta, None, None);
    print_colored_text(
        stdout,
        &format!("{freq_ghz:>6.1}GHz"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Cache:", Color::Red, None, None);
    print_colored_text(
        stdout,
        &format!("{:>5}MB", info.cache_size_mb),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side and 2 space separation
    let available_width = width.saturating_sub(10); // 5 padding each side

    if let Some(apple_info) = &info.apple_silicon_info {
        // Apple Silicon: Two gauges for P-Core and E-Core
        let gauge_width = (available_width - 2) / 2; // 2 spaces between gauges

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

        // P-Core gauge
        draw_bar(
            stdout,
            "P-CPU",
            apple_info.p_core_utilization,
            100.0,
            gauge_width,
            None,
        );
        print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

        // E-Core gauge
        draw_bar(
            stdout,
            "E-CPU",
            apple_info.e_core_utilization,
            100.0,
            gauge_width,
            None,
        );

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char right padding
    } else {
        // Other CPUs: Single CPU utilization gauge
        let gauge_width = available_width;

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

        // CPU gauge
        draw_bar(stdout, "CPU", info.utilization, 100.0, gauge_width, None);

        print_colored_text(stdout, "     ", Color::White, None, None); // 5 char right padding
    }

    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_memory_info<W: Write>(stdout: &mut W, _index: usize, info: &MemoryInfo, width: usize) {
    // Convert bytes to GB for display
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = info.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    // Print Memory info line
    print_colored_text(stdout, "Memory @ ", Color::Cyan, None, None);
    print_colored_text(stdout, &info.hostname, Color::White, None, None);
    print_colored_text(stdout, " Total:", Color::Blue, None, None);
    print_colored_text(
        stdout,
        &format!("{total_gb:>6.0}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Used:", Color::Yellow, None, None);
    print_colored_text(
        stdout,
        &format!("{used_gb:>6.1}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Avail:", Color::Green, None, None);
    print_colored_text(
        stdout,
        &format!("{available_gb:>6.1}GB"),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Util:", Color::Red, None, None);
    print_colored_text(
        stdout,
        &format!("{:>5.1}%", info.utilization),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side
    let available_width = width.saturating_sub(10); // 5 padding each side
    let num_gauges = if info.buffers_bytes > 0 || info.cached_bytes > 0 {
        2
    } else {
        1
    };

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    if num_gauges == 2 {
        // Used + Cache gauges
        let gauge_width = (available_width - 2) / 2; // 2 spaces between gauges

        // Used gauge
        draw_bar(
            stdout,
            "Used",
            info.utilization,
            100.0,
            gauge_width,
            Some(format!("{used_gb:.1}GB")),
        );
        print_colored_text(stdout, "  ", Color::White, None, None); // 2 space separator

        // Cache gauge
        let cache_gb = (info.buffers_bytes + info.cached_bytes) as f64 / (1024.0 * 1024.0 * 1024.0);
        let cache_percent =
            ((info.buffers_bytes + info.cached_bytes) as f64 / info.total_bytes as f64) * 100.0;
        draw_bar(
            stdout,
            "Cache",
            cache_percent,
            100.0,
            gauge_width,
            Some(format!("{cache_gb:.1}GB")),
        );
    } else {
        // Just Used gauge
        draw_bar(
            stdout,
            "Used",
            info.utilization,
            100.0,
            available_width,
            Some(format!("{used_gb:.1}GB")),
        );
    }

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char right padding
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_storage_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &StorageInfo,
    width: usize,
) {
    // Convert bytes to appropriate units
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = total_gb - available_gb;

    // Calculate usage percentage
    let usage_percent = if total_gb > 0.0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };

    // Format size with appropriate units
    let format_size = |gb: f64| -> String {
        if gb >= 1024.0 {
            format!("{:.1}TB", gb / 1024.0)
        } else {
            format!("{gb:.0}GB")
        }
    };

    // Print Disk info line
    print_colored_text(stdout, "Disk ", Color::Cyan, None, None);
    print_colored_text(
        stdout,
        &truncate_to_width(&info.mount_point, 15),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " @ ", Color::DarkGreen, None, None);
    print_colored_text(stdout, &info.hostname, Color::White, None, None);
    print_colored_text(stdout, " Mount:", Color::Blue, None, None);
    print_colored_text(stdout, &info.mount_point, Color::White, None, None);
    print_colored_text(stdout, " #:", Color::Yellow, None, None);
    print_colored_text(
        stdout,
        &format!("{:>2}", info.index),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Total:", Color::Green, None, None);
    print_colored_text(
        stdout,
        &format!("{:>8}", format_size(total_gb)),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Used:", Color::Red, None, None);
    print_colored_text(
        stdout,
        &format!("{:>8}", format_size(used_gb)),
        Color::White,
        None,
        None,
    );
    print_colored_text(stdout, " Util:", Color::Magenta, None, None);
    print_colored_text(
        stdout,
        &format!("{usage_percent:>5.1}%"),
        Color::White,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    // Calculate gauge widths with 5 char padding on each side
    let available_width = width.saturating_sub(10); // 5 padding each side

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char left padding

    // Just Used gauge (matching the other lists format)
    draw_bar(
        stdout,
        "Used",
        usage_percent,
        100.0,
        available_width,
        Some(format_size(used_gb)),
    );

    print_colored_text(stdout, "     ", Color::White, None, None); // 5 char right padding
    queue!(stdout, Print("\r\n")).unwrap();
}
