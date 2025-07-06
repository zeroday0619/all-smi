use std::collections::HashMap;
use std::io::Write;

use crossterm::{
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app_state::AppState;
use crate::gpu::{CpuInfo, GpuInfo, MemoryInfo, ProcessInfo};
use crate::storage::info::StorageInfo;

// Helper function to format RAM values with appropriate units
fn format_ram_value(gb_value: f64) -> String {
    if gb_value >= 1024.0 {
        format!("{:.2}TB", gb_value / 1024.0)
    } else {
        format!("{:.0}GB", gb_value)
    }
}

pub fn print_colored_text<W: Write>(
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

pub fn draw_bar<W: Write>(
    stdout: &mut W,
    label: &str,
    value: f64,
    max_value: f64,
    width: usize,
    show_text: Option<String>,
) {
    // Format label to exactly 5 characters for consistent alignment
    let formatted_label = if label.len() > 5 {
        // Trim to 5 characters if too long
        label[..5].to_string()
    } else {
        // Pad with spaces if too short
        format!("{:<5}", label)
    };
    let available_bar_width = width.saturating_sub(9); // 9 for "LABEL: [" and "] " (5 + 4)

    // Calculate the filled portion
    let fill_ratio = (value / max_value).min(1.0);
    let filled_width = (available_bar_width as f64 * fill_ratio) as usize;

    // Choose color based on usage
    let color = if fill_ratio > 0.8 {
        Color::Red
    } else if fill_ratio > 0.70 {
        Color::Yellow
    } else if fill_ratio > 0.25 {
        Color::Green
    } else if fill_ratio > 0.05 {
        Color::DarkGreen
    } else {
        Color::DarkGrey
    };

    // Prepare text to display inside the bar
    let display_text = if let Some(text) = show_text {
        text
    } else {
        format!("{:.1}%", fill_ratio * 100.0)
    };

    // Print label
    print_colored_text(stdout, &formatted_label, Color::White, None, None);
    print_colored_text(stdout, ": [", Color::White, None, None);

    // Calculate positioning for right-aligned text
    let text_len = display_text.len();
    let text_pos = if available_bar_width > text_len {
        available_bar_width - text_len
    } else {
        0
    };

    // Print the bar with embedded text using filled vertical lines
    for i in 0..available_bar_width {
        if i >= text_pos && i < text_pos + text_len {
            // Print text character
            let char_index = i - text_pos;
            if let Some(ch) = display_text.chars().nth(char_index) {
                // Always use white for text to ensure readability
                print_colored_text(stdout, &ch.to_string(), Color::White, None, None);
            }
        } else if i < filled_width {
            // Print filled area with shorter vertical lines in load color
            print_colored_text(stdout, "▬", color, None, None);
        } else {
            // Print empty line segments
            print_colored_text(stdout, "─", Color::DarkGrey, None, None);
        }
    }

    print_colored_text(stdout, "]", Color::White, None, None);
}

pub fn draw_system_view<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let box_width = (cols as usize).min(80);

    // Calculate cluster statistics
    let total_nodes = state.tabs.len().saturating_sub(1); // Exclude "All" tab
    let total_gpus = state.gpu_info.len();
    let total_memory_gb = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.total_memory)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);
    let total_power_watts = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.power_consumption)
        .sum::<f64>();

    // Calculate total CPU cores
    let total_cpu_cores = state
        .cpu_info
        .iter()
        .map(|cpu| {
            if let Some(apple_info) = &cpu.apple_silicon_info {
                apple_info.p_core_count + apple_info.e_core_count
            } else {
                cpu.total_cores
            }
        })
        .sum::<u32>();

    // Calculate total system memory
    let total_system_memory_gb = state
        .memory_info
        .iter()
        .map(|memory| memory.total_bytes)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    let used_system_memory_gb = state
        .memory_info
        .iter()
        .map(|memory| memory.used_bytes)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    // Calculate averages
    let avg_utilization = if total_gpus > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.utilization)
            .sum::<f64>()
            / total_gpus as f64
    } else {
        0.0
    };

    let avg_temperature = if total_gpus > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.temperature as f64)
            .sum::<f64>()
            / total_gpus as f64
    } else {
        0.0
    };

    // Calculate temperature standard deviation
    let temp_std_dev = if total_gpus > 1 {
        let temp_variance = state
            .gpu_info
            .iter()
            .map(|gpu| {
                let diff = gpu.temperature as f64 - avg_temperature;
                diff * diff
            })
            .sum::<f64>()
            / (total_gpus - 1) as f64;
        temp_variance.sqrt()
    } else {
        0.0
    };

    let avg_power = if total_gpus > 0 {
        total_power_watts / total_gpus as f64
    } else {
        0.0
    };

    // Calculate used GPU memory in GB
    let used_gpu_memory_gb = state
        .gpu_info
        .iter()
        .map(|gpu| gpu.used_memory)
        .sum::<u64>() as f64
        / (1024.0 * 1024.0 * 1024.0);

    // First row: | Nodes | Total RAM | GPU Cores | Total GPU RAM | Avg. Temp | Total Power |
    print_dashboard_row(
        stdout,
        &[
            ("Nodes", format!("{}", total_nodes), Color::Yellow),
            (
                "Total RAM",
                format_ram_value(total_system_memory_gb),
                Color::Green,
            ),
            ("GPU Cores", format!("{}", total_gpus), Color::Cyan),
            ("Total VRAM", format_ram_value(total_memory_gb), Color::Blue),
            (
                "Avg. Temp",
                format!("{:.0}°C", avg_temperature),
                Color::Magenta,
            ),
            (
                "Total Power",
                format!("{:.1}kW", total_power_watts / 1000.0),
                Color::Red,
            ),
        ],
        box_width,
    );

    // Second row: | CPU Cores | Used RAM | Avg. GPU Util | Used GPU RAM | Temp. Stdev | Avg. Power |
    print_dashboard_row(
        stdout,
        &[
            ("CPU Cores", format!("{}", total_cpu_cores), Color::Cyan),
            (
                "Used RAM",
                format_ram_value(used_system_memory_gb),
                Color::Green,
            ),
            ("GPU Util", format!("{:.1}%", avg_utilization), Color::Blue),
            (
                "Used VRAM",
                format_ram_value(used_gpu_memory_gb),
                Color::Blue,
            ),
            (
                "Temp. Stdev",
                format!("±{:.1}°C", temp_std_dev),
                Color::Magenta,
            ),
            ("Avg. Power", format!("{:.1}W", avg_power), Color::Red),
        ],
        box_width,
    );
}

pub fn draw_dashboard_items<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    // Print separator
    let separator = "─".repeat(cols as usize);
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Node utilization history box
    draw_utilization_history(stdout, state, cols);
    queue!(stdout, Print("\r\n")).unwrap();
}

fn print_dashboard_row<W: Write>(
    stdout: &mut W,
    items: &[(&str, String, Color)],
    _total_width: usize,
) {
    const ITEM_WIDTH: usize = 15; // Fixed width for each dashboard item

    // Print labels row
    print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    for (label, _, color) in items {
        // Truncate label if too long, ensuring it fits in 15 characters minus padding and separator
        let max_label_len = ITEM_WIDTH.saturating_sub(3);
        let truncated_label = if label.len() > max_label_len {
            &label[..max_label_len]
        } else {
            label
        };
        let formatted_label = format!(" {:<width$}", truncated_label, width = max_label_len);
        print_colored_text(stdout, &formatted_label, *color, None, None);
        print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();

    // Print values row
    print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    for (_, value, _) in items {
        // Truncate value if too long, ensuring it fits in 15 characters minus padding and separator
        let max_value_len = ITEM_WIDTH.saturating_sub(3);
        let truncated_value = if value.len() > max_value_len {
            &value[..max_value_len]
        } else {
            value
        };
        let formatted_value = format!(" {:<width$}", truncated_value, width = max_value_len);
        print_colored_text(stdout, &formatted_value, Color::White, None, None);
        print_colored_text(stdout, "│", Color::DarkGrey, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn draw_utilization_history<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let box_width = (cols as usize).min(80);

    if state.utilization_history.is_empty() {
        return;
    }

    // Calculate averages for display
    let avg_util =
        state.utilization_history.iter().sum::<f64>() / state.utilization_history.len() as f64;
    let avg_mem = state.memory_history.iter().sum::<f64>() / state.memory_history.len() as f64;
    let avg_temp =
        state.temperature_history.iter().sum::<f64>() / state.temperature_history.len() as f64;

    // Print header
    print_colored_text(stdout, "Live Statistics", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Split layout: left half for node view, right half for history gauges
    let left_width = box_width / 2;
    let right_width = box_width - left_width;
    let history_width = right_width.saturating_sub(15); // Leave space for labels

    // Print node view and history gauges side by side
    print_node_view_and_history(
        stdout,
        state,
        left_width,
        right_width,
        history_width,
        avg_util,
        avg_mem,
        avg_temp,
    );
}

fn print_node_view_and_history<W: Write>(
    stdout: &mut W,
    state: &AppState,
    left_width: usize,
    _right_width: usize,
    history_width: usize,
    avg_util: f64,
    avg_mem: f64,
    avg_temp: f64,
) {
    // Get nodes (excluding "All" tab)
    let nodes: Vec<&String> = state.tabs.iter().skip(1).collect();

    // Calculate per-node utilization
    let mut node_utils: HashMap<String, f64> = HashMap::new();
    for node in &nodes {
        let node_gpus: Vec<_> = state
            .gpu_info
            .iter()
            .filter(|gpu| &gpu.hostname == *node)
            .collect();
        if !node_gpus.is_empty() {
            let node_util =
                node_gpus.iter().map(|gpu| gpu.utilization).sum::<f64>() / node_gpus.len() as f64;
            node_utils.insert(node.to_string(), node_util);
        }
    }

    // Calculate node grid layout
    let nodes_per_row = left_width.saturating_sub(2).max(1);
    let num_rows = if nodes.is_empty() {
        1
    } else {
        ((nodes.len() - 1) / nodes_per_row) + 1
    };
    let num_rows = num_rows.min(3); // Limit to 3 rows max

    // Print each row of the combined view
    for row in 0..3 {
        if row < num_rows {
            // Print node view for this row
            print_node_view_row(
                stdout,
                &nodes,
                &node_utils,
                state.current_tab,
                left_width,
                row,
                nodes_per_row,
            );
        } else {
            // Print empty space for this row
            print_colored_text(stdout, &" ".repeat(left_width), Color::White, None, None);
        }

        // Print corresponding history line
        match row {
            0 => {
                print_colored_text(stdout, "GPU Util.", Color::Yellow, None, None);
                print_history_bar_with_value(
                    stdout,
                    &state.utilization_history,
                    history_width,
                    100.0,
                    format!("{:.1}%", avg_util),
                );
            }
            1 => {
                print_colored_text(stdout, "GPU Mem. ", Color::Yellow, None, None);
                print_history_bar_with_value(
                    stdout,
                    &state.memory_history,
                    history_width,
                    100.0,
                    format!("{:.1}%", avg_mem),
                );
            }
            2 => {
                print_colored_text(stdout, "Temp     ", Color::Yellow, None, None);
                print_history_bar_with_value(
                    stdout,
                    &state.temperature_history,
                    history_width,
                    100.0,
                    format!("{:.0}°C", avg_temp),
                );
            }
            _ => {}
        }
        queue!(stdout, Print("\r\n")).unwrap();
    }
}

fn print_node_view_row<W: Write>(
    stdout: &mut W,
    nodes: &[&String],
    node_utils: &HashMap<String, f64>,
    current_tab: usize,
    left_width: usize,
    row: usize,
    nodes_per_row: usize,
) {
    let start_index = row * nodes_per_row;
    let end_index = ((row + 1) * nodes_per_row).min(nodes.len());
    let mut node_count = 0;

    // Print nodes for this specific row
    for (i, node) in nodes
        .iter()
        .enumerate()
        .skip(start_index)
        .take(end_index - start_index)
    {
        let utilization = node_utils.get(*node).unwrap_or(&0.0);
        let is_selected = current_tab == i + 1; // +1 because we skip "All" tab

        let (char, color) = get_node_char_and_color(*utilization, is_selected);

        // Print the character with its color
        print_colored_text(stdout, &char.to_string(), color, None, None);
        node_count += 1;
    }

    // Pad the remaining space
    let remaining_space = left_width.saturating_sub(node_count);
    if remaining_space > 0 {
        print_colored_text(
            stdout,
            &" ".repeat(remaining_space),
            Color::White,
            None,
            None,
        );
    }
}

fn get_node_char_and_color(utilization: f64, is_selected: bool) -> (char, Color) {
    let base_char = if utilization > 87.5 {
        '█'
    } else if utilization > 75.0 {
        '▇'
    } else if utilization > 62.5 {
        '▆'
    } else if utilization > 50.0 {
        '▅'
    } else if utilization > 37.5 {
        '▄'
    } else if utilization > 25.0 {
        '▃'
    } else if utilization > 12.5 {
        '▂'
    } else if utilization > 0.0 {
        '▁'
    } else {
        '░'
    };

    let color = if is_selected {
        Color::Cyan
    } else if utilization > 80.0 {
        Color::Red
    } else if utilization > 60.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    (base_char, color)
}

fn print_history_bar_with_value<W: Write>(
    stdout: &mut W,
    history: &std::collections::VecDeque<f64>,
    width: usize,
    max_value: f64,
    value_text: String,
) {
    queue!(stdout, Print("[")).unwrap();

    let data_points = history.len();
    if data_points == 0 {
        // Empty history
        print_colored_text(stdout, &"⠀".repeat(width), Color::DarkGrey, None, None);
    } else {
        // Calculate position for value text (right-aligned)
        let text_len = value_text.len();
        let text_pos = if width > text_len {
            width - text_len
        } else {
            0
        };

        for i in 0..width {
            // Check if we should print the value text character
            if i >= text_pos && i < text_pos + text_len {
                let char_index = i - text_pos;
                if let Some(ch) = value_text.chars().nth(char_index) {
                    print_colored_text(stdout, &ch.to_string(), Color::White, None, None);
                    continue;
                }
            }

            let data_index = if data_points >= width {
                data_points - width + i
            } else {
                if i < width - data_points {
                    print_colored_text(stdout, "⠀", Color::DarkGrey, None, None);
                    continue;
                } else {
                    i - (width - data_points)
                }
            };

            if data_index < history.len() {
                let value = history[data_index];
                let intensity = (value / max_value).min(1.0);

                let (char, color) = if intensity > 0.875 {
                    ("⣿", Color::Red)
                } else if intensity > 0.9 {
                    ("⣶", Color::Red)
                } else if intensity > 0.85 {
                    ("⣴", Color::Yellow)
                } else if intensity > 0.70 {
                    ("⣤", Color::Yellow)
                } else if intensity > 0.625 {
                    ("⣠", Color::Green)
                } else if intensity > 0.50 {
                    ("⣀", Color::Green)
                } else if intensity > 0.25 {
                    ("⡀", Color::DarkGreen)
                } else if intensity > 0.0 {
                    ("⠀", Color::DarkGreen)
                } else {
                    ("⠀", Color::DarkGrey)
                };

                print_colored_text(stdout, char, color, None, None);
            } else {
                print_colored_text(stdout, "⠀", Color::DarkGrey, None, None);
            }
        }
    }

    queue!(stdout, Print("]")).unwrap();
}

pub fn print_gpu_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &GpuInfo,
    width: usize,
    device_name_scroll_offset: usize,
    hostname_scroll_offset: usize,
) {
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Helper function to add labels with fixed width for alignment
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
        value_width: usize,
    ) {
        labels.push((label.to_string(), label_color));
        // Pad or truncate value to ensure consistent width
        let formatted_value = if value.len() > value_width {
            value.chars().take(value_width).collect()
        } else {
            format!("{:<width$}", value, width = value_width)
        };
        labels.push((formatted_value, Color::White));
    }

    // Add GPU index with device name
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

    add_label(&mut labels, "GPU ", device_name, Color::Cyan, 15);

    // Add hostname
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

    add_label(&mut labels, " Host:", hostname_display, Color::Yellow, 12);

    // Add utilization
    let util_color = if info.utilization > 80.0 {
        Color::Red
    } else if info.utilization > 60.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    add_label(
        &mut labels,
        " Util:",
        format!("{:.1}%", info.utilization),
        util_color,
        6,
    );

    // Add memory usage
    let memory_used_gb = info.used_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let memory_total_gb = info.total_memory as f64 / (1024.0 * 1024.0 * 1024.0);
    let memory_percent = if info.total_memory > 0 {
        (info.used_memory as f64 / info.total_memory as f64) * 100.0
    } else {
        0.0
    };

    let mem_color = if memory_percent > 80.0 {
        Color::Red
    } else if memory_percent > 60.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    add_label(
        &mut labels,
        " Mem:",
        format!("{:.1}/{:.1}GB", memory_used_gb, memory_total_gb),
        mem_color,
        12,
    );

    // Add temperature
    let temp_color = if info.temperature > 80 {
        Color::Red
    } else if info.temperature > 70 {
        Color::Yellow
    } else {
        Color::Green
    };
    add_label(
        &mut labels,
        " Temp:",
        format!("{}°C", info.temperature),
        temp_color,
        5,
    );

    // Add power
    let power_color = if info.power_consumption > 200.0 {
        Color::Red
    } else if info.power_consumption > 150.0 {
        Color::Yellow
    } else {
        Color::Green
    };
    add_label(
        &mut labels,
        " Power:",
        format!("{:.1}W", info.power_consumption),
        power_color,
        8,
    );

    // Add frequency
    add_label(
        &mut labels,
        " Freq:",
        format!("{}MHz", info.frequency),
        Color::Magenta,
        8,
    );

    // Add ANE utilization for Apple Silicon GPUs
    if info.ane_utilization > 0.0 {
        let ane_color = if info.ane_utilization > 80.0 {
            Color::Red
        } else if info.ane_utilization > 60.0 {
            Color::Yellow
        } else {
            Color::Green
        };
        add_label(
            &mut labels,
            " ANE:",
            format!("{:.1}%", info.ane_utilization),
            ane_color,
            6,
        );
    }

    // Print all labels in one line
    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();

    // Print progress bars on the same line with embedded text to prevent wrapping
    let bar_width = width.saturating_sub(10);

    queue!(stdout, Print("     ")).unwrap();

    // Calculate bar widths based on available space and number of bars
    // Show ANE gauge for Apple Silicon GPUs (based on UUID or name containing "Apple")
    let is_apple_silicon = info.uuid == "AppleSiliconGPU" || info.name.contains("Apple");
    let (individual_bar_width, last_bar_width) = if is_apple_silicon {
        // 3 bars: ensure same total space usage as 2 bars
        let standard_width = (bar_width - 4) / 3 + 1;
        let two_bar_width = (bar_width - 2) / 2;
        let single_bar_width = bar_width;

        // Calculate total length with 3 standard-width bars
        let total_with_three = 3 * standard_width + 4; // 4 spaces between bars
        let target_length = std::cmp::max(
            2 * two_bar_width + 2, // 2-bar total length
            single_bar_width,      // single bar length
        );

        // If 3-bar total is 1 character longer, reduce last bar by 1
        if total_with_three == target_length + 1 {
            (standard_width, standard_width - 1)
        } else {
            (standard_width, standard_width)
        }
    } else {
        let width = (bar_width - 2) / 2;
        (width, width)
    };

    // GPU Utilization bar
    draw_bar(
        stdout,
        "UTIL",
        info.utilization,
        100.0,
        individual_bar_width,
        None,
    );

    // Memory usage bar
    queue!(stdout, Print("  ")).unwrap();
    draw_bar(
        stdout,
        "MEM",
        memory_percent,
        100.0,
        individual_bar_width,
        None,
    );

    // ANE utilization bar for Apple Silicon GPUs (show even when 0)
    if is_apple_silicon {
        queue!(stdout, Print("  ")).unwrap();
        draw_bar(
            stdout,
            "ANE",
            info.ane_utilization,
            10.0, // ANE scale: 0-10W instead of 0-100%
            last_bar_width,
            Some(format!("{:.1}W", info.ane_utilization)),
        );
    }

    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_cpu_info<W: Write>(stdout: &mut W, _index: usize, info: &CpuInfo, width: usize) {
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Helper function to add labels with fixed width for alignment
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
        value_width: usize,
    ) {
        labels.push((label.to_string(), label_color));
        // Pad or truncate value to ensure consistent width
        let formatted_value = if value.len() > value_width {
            value.chars().take(value_width).collect()
        } else {
            format!("{:<width$}", value, width = value_width)
        };
        labels.push((formatted_value, Color::White));
    }

    // Add CPU model (truncated if too long)
    let cpu_model = if info.cpu_model.len() > 30 {
        format!("{}...", &info.cpu_model[..27])
    } else {
        info.cpu_model.clone()
    };
    add_label(&mut labels, "CPU ", cpu_model, Color::Cyan, 30);

    // Add hostname
    add_label(
        &mut labels,
        " Host:",
        info.hostname.clone(),
        Color::Yellow,
        12,
    );

    // For Apple Silicon, show core counts without utilization
    if let Some(apple_info) = &info.apple_silicon_info {
        // Add core counts
        add_label(
            &mut labels,
            " Cores:",
            format!("{}P+{}E", apple_info.p_core_count, apple_info.e_core_count),
            Color::White,
            8,
        );

        // Add GPU core count
        add_label(
            &mut labels,
            " GPU:",
            format!("{}c", apple_info.gpu_core_count),
            Color::Magenta,
            5,
        );
    } else {
        // Add socket and core counts without utilization
        add_label(
            &mut labels,
            " Sockets:",
            format!("{}", info.socket_count),
            Color::White,
            3,
        );

        add_label(
            &mut labels,
            " Cores:",
            format!("{}", info.total_cores),
            Color::White,
            4,
        );
    }

    // Add frequency
    add_label(
        &mut labels,
        " Freq:",
        format!("{}MHz", info.base_frequency_mhz),
        Color::Green,
        8,
    );

    // Add temperature if available
    if let Some(temp) = info.temperature {
        let temp_color = if temp > 80 {
            Color::Red
        } else if temp > 70 {
            Color::Yellow
        } else {
            Color::Green
        };

        add_label(&mut labels, " Temp:", format!("{}°C", temp), temp_color, 5);
    }

    // Add power consumption if available
    if let Some(power) = info.power_consumption {
        add_label(
            &mut labels,
            " Power:",
            format!("{:.1}W", power),
            Color::Blue,
            7,
        );
    }

    // Print all labels, wrapping as needed
    let mut current_width = 0;
    for (text, color) in labels {
        if current_width + text.len() > width && current_width > 0 {
            queue!(stdout, Print("\r\n")).unwrap();
            current_width = 0;
        }
        print_colored_text(stdout, &text, color, None, None);
        current_width += text.len();
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Now add utilization gauges on the next line (matching GPU format)
    let bar_width = width.saturating_sub(10);
    queue!(stdout, Print("     ")).unwrap();

    if let Some(apple_info) = &info.apple_silicon_info {
        // Calculate bar widths for P-core and E-core gauges (2 bars)
        let individual_bar_width = (bar_width - 2) / 2; // Account for spacing between bars

        // Show P-core utilization gauge
        draw_bar(
            stdout,
            "P-CPU",
            apple_info.p_core_utilization as f64,
            100.0,
            individual_bar_width,
            None,
        );

        queue!(stdout, Print("  ")).unwrap();

        // Show E-core utilization gauge
        draw_bar(
            stdout,
            "E-CPU",
            apple_info.e_core_utilization as f64,
            100.0,
            individual_bar_width,
            None,
        );
    } else {
        // For multi-socket CPUs, show per-socket utilization gauges
        if info.socket_count > 1 {
            let num_sockets = info.per_socket_info.len().min(2);
            let individual_bar_width = (bar_width - (num_sockets * 2 - 2)) / num_sockets; // Account for spacing

            for (i, socket_info) in info.per_socket_info.iter().take(2).enumerate() {
                if i > 0 {
                    queue!(stdout, Print("  ")).unwrap();
                }
                draw_bar(
                    stdout,
                    &format!("CPU{}", i),
                    socket_info.utilization as f64,
                    100.0,
                    individual_bar_width,
                    None,
                );
            }
        } else {
            // Single socket - show overall utilization gauge
            draw_bar(
                stdout,
                "CPU",
                info.utilization as f64,
                100.0,
                bar_width,
                None,
            );
        }
    }

    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_memory_info<W: Write>(stdout: &mut W, _index: usize, info: &MemoryInfo, width: usize) {
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Helper function to add labels with fixed width for alignment
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
        value_width: usize,
    ) {
        labels.push((label.to_string(), label_color));
        // Pad or truncate value to ensure consistent width
        let formatted_value = if value.len() > value_width {
            value.chars().take(value_width).collect()
        } else {
            format!("{:<width$}", value, width = value_width)
        };
        labels.push((formatted_value, Color::White));
    }

    // Add memory type label
    add_label(&mut labels, "RAM ", "Memory".to_string(), Color::Cyan, 6);

    // Add hostname
    add_label(
        &mut labels,
        " Host:",
        info.hostname.clone(),
        Color::Yellow,
        12,
    );

    // Add total memory
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    add_label(
        &mut labels,
        " Total:",
        format!("{:.1}GB", total_gb),
        Color::White,
        8,
    );

    // Add used memory
    let used_gb = info.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    add_label(
        &mut labels,
        " Used:",
        format!("{:.1}GB", used_gb),
        Color::White,
        8,
    );

    // Add available memory
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    add_label(
        &mut labels,
        " Avail:",
        format!("{:.1}GB", available_gb),
        Color::Green,
        8,
    );

    // Add swap if available
    if info.swap_total_bytes > 0 {
        let swap_total_gb = info.swap_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let swap_used_gb = info.swap_used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        add_label(
            &mut labels,
            " Swap:",
            format!("{:.1}/{:.1}GB", swap_used_gb, swap_total_gb),
            Color::DarkYellow,
            12,
        );
    }

    // Print all labels, wrapping as needed
    let mut current_width = 0;
    for (text, color) in labels {
        if current_width + text.len() > width && current_width > 0 {
            queue!(stdout, Print("\r\n")).unwrap();
            current_width = 0;
        }
        print_colored_text(stdout, &text, color, None, None);
        current_width += text.len();
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Add memory utilization gauge on the next line (matching GPU/CPU format)
    let bar_width = width.saturating_sub(10);
    queue!(stdout, Print("     ")).unwrap();

    // Show memory utilization gauge
    draw_bar(
        stdout,
        "RAM",
        info.utilization as f64,
        100.0,
        bar_width,
        None,
    );

    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_storage_info<W: Write>(
    stdout: &mut W,
    _index: usize,
    info: &StorageInfo,
    width: usize,
) {
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Helper function to add labels with fixed width for alignment
    fn add_label(
        labels: &mut Vec<(String, Color)>,
        label: &str,
        value: String,
        label_color: Color,
        value_width: usize,
    ) {
        labels.push((label.to_string(), label_color));
        // Pad or truncate value to ensure consistent width
        let formatted_value = if value.len() > value_width {
            value.chars().take(value_width).collect()
        } else {
            format!("{:<width$}", value, width = value_width)
        };
        labels.push((formatted_value, Color::White));
    }

    // Format storage sizes
    let total_gb = info.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = info.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = total_gb - available_gb;
    let usage_percent = if info.total_bytes > 0 {
        (used_gb / total_gb) * 100.0
    } else {
        0.0
    };

    // First line: Basic disk information
    add_label(
        &mut labels,
        "DISK ",
        info.mount_point.clone(),
        Color::Cyan,
        15,
    );
    add_label(
        &mut labels,
        " Host:",
        info.hostname.clone(),
        Color::Yellow,
        12,
    );

    // Add usage percentage
    let usage_color = if usage_percent > 90.0 {
        Color::Red
    } else if usage_percent > 80.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    add_label(
        &mut labels,
        " Usage:",
        format!("{:.1}%", usage_percent),
        usage_color,
        6,
    );

    add_label(
        &mut labels,
        " Free:",
        format!("{:.1}GB", available_gb),
        Color::Green,
        10,
    );

    // Print all labels in first line
    for (text, color) in labels {
        print_colored_text(stdout, &text, color, None, None);
    }
    queue!(stdout, Print("\r\n")).unwrap();

    // Second line: Usage bar with capacity information embedded
    queue!(stdout, Print("     ")).unwrap(); // Indent to align with GPU bars

    let bar_width = width.saturating_sub(10);
    let capacity_text = format!("{:.1}/{:.1}GB", used_gb, total_gb);

    draw_bar(
        stdout,
        "USED",
        usage_percent,
        100.0,
        bar_width,
        Some(capacity_text),
    );

    queue!(stdout, Print("\r\n")).unwrap();
}

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
    let min_widths = [6, 12, 8, 6, 8, 8, 8, 10]; // Minimum widths for each column
    let total_min_width: usize = min_widths.iter().sum::<usize>() + 7; // 7 spaces between columns

    let (pid_w, user_w, name_w, cpu_w, mem_w, gpu_mem_w, state_w, command_w) =
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
                min_widths[5],                 // GPU MEM: 8
                min_widths[6],                 // STATE: 8
                min_widths[7] + command_extra, // COMMAND: 10 + extra
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
            )
        };

    // Print header with improved spacing and colors
    print_colored_text(
        stdout,
        &format!(
            "{:<width_pid$} {:<width_user$} {:<width_name$} {:<width_cpu$} {:<width_mem$} {:<width_gpu_mem$} {:<width_state$} {:<width_command$}",
            "PID", "USER", "NAME", "CPU%", "MEM%", "GPU MEM", "STATE", "COMMAND",
            width_pid = pid_w,
            width_user = user_w,
            width_name = name_w,
            width_cpu = cpu_w,
            width_mem = mem_w,
            width_gpu_mem = gpu_mem_w,
            width_state = state_w,
            width_command = command_w
        ),
        Color::Cyan,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    let visible_rows = half_rows.saturating_sub(3) as usize; // Subtract header rows

    for (i, process) in processes
        .iter()
        .enumerate()
        .skip(start_index)
        .take(visible_rows)
    {
        let bg_color = if i == selected_index {
            Some(Color::DarkBlue)
        } else {
            None
        };

        // Format memory values
        let gpu_memory_mb = process.used_memory as f64 / (1024.0 * 1024.0);
        let gpu_mem_str = if gpu_memory_mb >= 1024.0 {
            format!("{:.1}GB", gpu_memory_mb / 1024.0)
        } else {
            format!("{:.0}MB", gpu_memory_mb)
        };

        // Format CPU percentage with appropriate color
        let cpu_color = if process.cpu_percent > 80.0 {
            Color::Red
        } else if process.cpu_percent > 50.0 {
            Color::Yellow
        } else {
            Color::White
        };

        // Format memory percentage with appropriate color
        let mem_color = if process.memory_percent > 80.0 {
            Color::Red
        } else if process.memory_percent > 50.0 {
            Color::Yellow
        } else {
            Color::White
        };

        // Truncate strings to fit column widths
        let truncate_string = |s: &str, max_len: usize| -> String {
            if s.len() > max_len {
                if max_len > 3 {
                    format!("{}...", &s[..max_len - 3])
                } else {
                    s.chars().take(max_len).collect()
                }
            } else {
                s.to_string()
            }
        };

        let user_display = truncate_string(&process.user, user_w);
        let name_display = truncate_string(&process.process_name, name_w);
        let command_display = truncate_string(&process.command, command_w);

        // Print PID column
        print_colored_text(
            stdout,
            &format!("{:<width$}", process.pid, width = pid_w),
            Color::White,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print USER column
        print_colored_text(
            stdout,
            &format!("{:<width$}", user_display, width = user_w),
            Color::Green,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print NAME column
        print_colored_text(
            stdout,
            &format!("{:<width$}", name_display, width = name_w),
            Color::White,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print CPU% column
        print_colored_text(
            stdout,
            &format!("{:<width$.1}", process.cpu_percent, width = cpu_w),
            cpu_color,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print MEM% column
        print_colored_text(
            stdout,
            &format!("{:<width$.1}", process.memory_percent, width = mem_w),
            mem_color,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print GPU MEM column
        print_colored_text(
            stdout,
            &format!("{:<width$}", gpu_mem_str, width = gpu_mem_w),
            Color::Magenta,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print STATE column
        let state_color = match process.state.as_str() {
            "R" => Color::Green,  // Running
            "S" => Color::White,  // Sleeping
            "D" => Color::Red,    // Uninterruptible sleep
            "Z" => Color::Red,    // Zombie
            "T" => Color::Yellow, // Stopped
            _ => Color::White,
        };
        print_colored_text(
            stdout,
            &format!("{:<width$}", process.state, width = state_w),
            state_color,
            bg_color,
            None,
        );
        queue!(stdout, Print(" ")).unwrap();

        // Print COMMAND column
        print_colored_text(
            stdout,
            &format!("{:<width$}", command_display, width = command_w),
            Color::Cyan,
            bg_color,
            None,
        );

        queue!(stdout, Print("\r\n")).unwrap();
    }
}

pub fn print_function_keys<W: Write>(
    stdout: &mut W,
    cols: u16,
    rows: u16,
    state: &crate::app_state::AppState,
    is_remote: bool,
) {
    // Move to bottom of screen
    queue!(stdout, crossterm::cursor::MoveTo(0, rows - 1)).unwrap();

    // Get current sorting indicator
    let sort_indicator = match state.sort_criteria {
        crate::app_state::SortCriteria::Default => "Sort:Default",
        crate::app_state::SortCriteria::Pid => "Sort:PID",
        crate::app_state::SortCriteria::Memory => "Sort:Memory",
        crate::app_state::SortCriteria::Utilization => "Sort:Util",
        crate::app_state::SortCriteria::GpuMemory => "Sort:GPU-Mem",
        crate::app_state::SortCriteria::Power => "Sort:Power",
        crate::app_state::SortCriteria::Temperature => "Sort:Temp",
    };

    let function_keys = if is_remote {
        // Remote mode: only GPU sorting
        format!(
            "1:Help q:Exit ←→:Tabs ↑↓:Scroll PgUp/PgDn:Page d:Default u:Util g:GPU-Mem [{}]",
            sort_indicator
        )
    } else {
        // Local mode: both process and GPU sorting
        format!("1:Help q:Exit ←→:Tabs ↑↓:Scroll PgUp/PgDn:Page p:PID m:Memory d:Default u:Util g:GPU-Mem [{}]", sort_indicator)
    };

    let truncated_keys = if function_keys.len() > cols as usize {
        &function_keys[..cols as usize]
    } else {
        &function_keys
    };

    print_colored_text(
        stdout,
        truncated_keys,
        Color::White,
        Some(Color::DarkBlue),
        None,
    );

    // Fill remaining space with background color
    let remaining = cols as usize - truncated_keys.len();
    if remaining > 0 {
        let padding = " ".repeat(remaining);
        print_colored_text(stdout, &padding, Color::White, Some(Color::DarkBlue), None);
    }
}

pub fn print_loading_indicator<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    let message = "Loading GPU information...";
    let x = (cols.saturating_sub(message.len() as u16)) / 2;
    let y = rows / 2;

    queue!(stdout, crossterm::cursor::MoveTo(x, y)).unwrap();
    print_colored_text(stdout, message, Color::Yellow, None, None);
}
