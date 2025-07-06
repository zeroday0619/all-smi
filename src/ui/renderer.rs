use std::io::Write;
use std::collections::HashMap;

use crossterm::{
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::app_state::AppState;
use crate::gpu::{GpuInfo, ProcessInfo};
use crate::storage::info::StorageInfo;

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
    let label_width = label.len();
    let available_bar_width = width.saturating_sub(label_width + 4); // 4 for ": [" and "] "

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
    print_colored_text(stdout, label, Color::White, None, None);
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
    // Calculate summary stats
    let gpu_count = state.gpu_info.len();
    let total_gpus = gpu_count;
    let avg_utilization = if gpu_count > 0 {
        state.gpu_info.iter().map(|gpu| gpu.utilization).sum::<f64>() / gpu_count as f64
    } else {
        0.0
    };

    let total_memory: u64 = state.gpu_info.iter().map(|gpu| gpu.total_memory).sum();
    let used_memory: u64 = state.gpu_info.iter().map(|gpu| gpu.used_memory).sum();
    let memory_utilization = if total_memory > 0 {
        (used_memory as f64 / total_memory as f64) * 100.0
    } else {
        0.0
    };

    let avg_temperature = if gpu_count > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.temperature as f64)
            .sum::<f64>()
            / gpu_count as f64
    } else {
        0.0
    };

    let avg_power = if gpu_count > 0 {
        state
            .gpu_info
            .iter()
            .map(|gpu| gpu.power_consumption)
            .sum::<f64>()
            / gpu_count as f64
    } else {
        0.0
    };

    // Display overview
    queue!(
        stdout,
        Print(format!(
            "GPUs: {} | Avg Util: {:.1}% | Memory: {:.1}% | Avg Temp: {:.0}°C | Avg Power: {:.1}W\r\n",
            total_gpus, avg_utilization, memory_utilization, avg_temperature, avg_power
        ))
    )
    .unwrap();
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

pub fn draw_utilization_history<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    let box_width = (cols as usize).min(80);
    
    if state.utilization_history.is_empty() {
        return;
    }

    // Calculate averages for display
    let avg_util = state.utilization_history.iter().sum::<f64>() / state.utilization_history.len() as f64;
    let avg_mem = state.memory_history.iter().sum::<f64>() / state.memory_history.len() as f64;
    let avg_temp = state.temperature_history.iter().sum::<f64>() / state.temperature_history.len() as f64;

    // Print header
    print_colored_text(stdout, "Cluster Overview", Color::Cyan, None, None);
    queue!(stdout, Print("\r\n")).unwrap();

    // Split layout: left half for node view, right half for history gauges
    let left_width = box_width / 2;
    let right_width = box_width - left_width;
    let history_width = right_width.saturating_sub(15); // Leave space for labels

    // Print node view and history gauges side by side
    print_node_view_and_history(stdout, state, left_width, right_width, history_width, avg_util, avg_mem, avg_temp);
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
        let node_gpus: Vec<_> = state.gpu_info.iter().filter(|gpu| &gpu.hostname == *node).collect();
        if !node_gpus.is_empty() {
            let node_util = node_gpus.iter().map(|gpu| gpu.utilization).sum::<f64>() / node_gpus.len() as f64;
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
            print_node_view_row(stdout, &nodes, &node_utils, state.current_tab, left_width, row, nodes_per_row);
        } else {
            // Print empty space for this row
            print_colored_text(stdout, &" ".repeat(left_width), Color::White, None, None);
        }
        
        // Print corresponding history line
        match row {
            0 => {
                print_colored_text(stdout, "GPU Util: ", Color::Yellow, None, None);
                print_history_bar_with_value(stdout, &state.utilization_history, history_width, 100.0, format!("{:.1}%", avg_util));
            },
            1 => {
                print_colored_text(stdout, "Memory:   ", Color::Yellow, None, None);
                print_history_bar_with_value(stdout, &state.memory_history, history_width, 100.0, format!("{:.1}%", avg_mem));
            },
            2 => {
                print_colored_text(stdout, "Temp:     ", Color::Yellow, None, None);
                print_history_bar_with_value(stdout, &state.temperature_history, history_width, 100.0, format!("{:.0}°C", avg_temp));
            },
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
    for (i, node) in nodes.iter().enumerate().skip(start_index).take(end_index - start_index) {
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
        print_colored_text(stdout, &" ".repeat(remaining_space), Color::White, None, None);
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


pub fn draw_tabs<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    // Print tabs
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Add "All" tab with special formatting
    if state.current_tab == 0 {
        labels.push(("All".to_string(), Color::Black));
        labels.push((" ".to_string(), Color::White));
    } else {
        labels.push(("All".to_string(), Color::White));
        labels.push((" ".to_string(), Color::White));
    }

    // Calculate available width for tabs
    let mut available_width = cols.saturating_sub(5); // Reserve space for "All" and some padding

    // Skip tabs that are before the scroll offset
    let visible_tabs: Vec<_> = state
        .tabs
        .iter()
        .enumerate()
        .skip(1) // Skip "All" tab
        .skip(state.tab_scroll_offset)
        .collect();

    for (i, tab) in visible_tabs {
        let tab_width = tab.len() as u16 + 2; // Tab name + 2 spaces padding
        if available_width < tab_width {
            break; // No more space
        }

        if state.current_tab == i {
            labels.push((format!(" {} ", tab), Color::Black));
        } else {
            labels.push((format!(" {} ", tab), Color::White));
        }

        available_width -= tab_width;
    }

    // Render tabs
    queue!(stdout, Print("Tabs: ")).unwrap();
    for (text, color) in labels {
        if color == Color::Black {
            print_colored_text(stdout, &text, Color::White, Some(Color::White), None);
        } else {
            print_colored_text(stdout, &text, color, None, None);
        }
    }

    queue!(stdout, Print("\r\n")).unwrap();

    // Print separator
    let separator = "─".repeat(cols as usize);
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();
}

pub fn print_gpu_info<W: Write>(
    stdout: &mut W,
    index: usize,
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
    let num_bars = if info.ane_utilization > 0.0 { 3 } else { 2 };
    let individual_bar_width = (bar_width - (num_bars * 2)) / num_bars; // Account for spacing
    
    // GPU Utilization bar
    draw_bar(
        stdout,
        "GPU",
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

    // ANE utilization bar for Apple Silicon GPUs
    if info.ane_utilization > 0.0 {
        queue!(stdout, Print("  ")).unwrap();
        draw_bar(
            stdout,
            "ANE",
            info.ane_utilization,
            100.0,
            individual_bar_width,
            None,
        );
    }

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
    add_label(&mut labels, "DISK ", info.mount_point.clone(), Color::Cyan, 15);
    add_label(&mut labels, " Host:", info.hostname.clone(), Color::Yellow, 12);

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
        "USAGE",
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
    
    // Print header
    print_colored_text(
        stdout,
        &format!(
            "{:<6} {:<20} {:<10} {:<15}",
            "PID", "Name", "GPU", "Memory"
        ),
        Color::Cyan,
        None,
        None,
    );
    queue!(stdout, Print("\r\n")).unwrap();

    let visible_rows = half_rows.saturating_sub(3) as usize; // Subtract header rows
    let end_index = (start_index + visible_rows).min(processes.len());

    for (i, process) in processes.iter().enumerate().skip(start_index).take(visible_rows) {
        let bg_color = if i == selected_index {
            Some(Color::DarkBlue)
        } else {
            None
        };

        let memory_mb = process.used_memory as f64 / (1024.0 * 1024.0);
        
        print_colored_text(
            stdout,
            &format!(
                "{:<6} {:<20} {:<10} {:<15}",
                process.pid,
                if process.process_name.len() > 20 {
                    format!("{}...", &process.process_name[..17])
                } else {
                    process.process_name.clone()
                },
                process.device_id,
                format!("{:.1}MB", memory_mb)
            ),
            Color::White,
            bg_color,
            None,
        );
        queue!(stdout, Print("\r\n")).unwrap();
    }
}

pub fn print_function_keys<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    // Move to bottom of screen
    queue!(stdout, crossterm::cursor::MoveTo(0, rows - 1)).unwrap();

    let function_keys = "F1:Help F10:Exit ←→:Tabs ↑↓:Scroll PgUp/PgDn:Page p:PID m:Memory";
    let truncated_keys = if function_keys.len() > cols as usize {
        &function_keys[..cols as usize]
    } else {
        function_keys
    };

    print_colored_text(stdout, truncated_keys, Color::White, Some(Color::DarkBlue), None);

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

pub fn print_help_popup<W: Write>(stdout: &mut W, cols: u16, rows: u16) {
    // Use nearly full screen with small margin
    let popup_width = cols.saturating_sub(4) as usize;
    let popup_height = rows.saturating_sub(2) as usize;
    let start_x = 2;
    let start_y = 1;

    // Clear the screen area
    for y in 0..popup_height {
        queue!(stdout, crossterm::cursor::MoveTo(start_x, start_y + y as u16)).unwrap();
        print_colored_text(stdout, &" ".repeat(popup_width), Color::White, Some(Color::Black), None);
    }

    // Draw window frame
    draw_window_frame(stdout, start_x, start_y, popup_width, popup_height);

    // Content areas
    let content_width = popup_width.saturating_sub(4);
    let content_x = start_x + 2;
    let mut current_y = start_y + 2;

    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    // TOP SECTION: Title
    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    let title_lines = vec![
        " █████╗ ██╗     ██╗          ███████╗███╗   ███╗██╗",
        "██╔══██╗██║     ██║          ██╔════╝████╗ ████║██║",
        "███████║██║     ██║    █████╗███████╗██╔████╔██║██║",
        "██╔══██║██║     ██║    ╚════╝╚════██║██║╚██╔╝██║██║",
        "██║  ██║███████╗███████╗     ███████║██║ ╚═╝ ██║██║",
        "╚═╝  ╚═╝╚══════╝╚══════╝     ╚══════╝╚═╝     ╚═╝╚═╝",
        "",
        "              GPU Monitoring and Management Tool",
    ];

    for line in &title_lines {
        queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
        let centered_line = center_text(line, content_width);
        print_colored_text(stdout, &centered_line, Color::Green, Some(Color::Black), None);
        current_y += 1;
    }

    current_y += 1;

    // Draw separator
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    print_colored_text(stdout, &"─".repeat(content_width), Color::DarkGrey, Some(Color::Black), None);
    current_y += 2;

    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    // MIDDLE SECTION: Cheat Sheet
    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    let cheat_sheet_title = "KEYBOARD SHORTCUTS & NAVIGATION";
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    let centered_title = center_text(&cheat_sheet_title, content_width);
    print_colored_text(stdout, &centered_title, Color::Yellow, Some(Color::Black), None);
    current_y += 2;

    // Two-column layout for shortcuts
    let col1_width = content_width / 2;
    let col2_x = content_x + col1_width as u16;

    let shortcuts_left = vec![
        ("Navigation", "", true),
        ("  ← →", "Switch between tabs", false),
        ("  ↑ ↓", "Scroll up/down", false),
        ("  PgUp/PgDn", "Page up/down", false),
        ("", "", false),
        ("Display Control", "", true),
        ("  F1 / h", "Toggle this help", false),
        ("  F10 / q", "Exit application", false),
        ("  ESC", "Close help or exit", false),
    ];

    let shortcuts_right = vec![
        ("Process Control", "", true),
        ("  p", "Sort processes by PID", false),
        ("  m", "Sort processes by Memory", false),
        ("", "", false),
        ("Tab Information", "", true),
        ("  All", "Show all GPUs across hosts", false),
        ("  [Host]", "Show GPUs from specific host", false),
        ("", "", false),
        ("Color Legend", "", true),
        ("  Green", "Normal usage (< 60%)", false),
        ("  Yellow", "Medium usage (60-80%)", false),
        ("  Red", "High usage (> 80%)", false),
    ];

    let max_rows = shortcuts_left.len().max(shortcuts_right.len());
    
    for i in 0..max_rows {
        // Left column
        if i < shortcuts_left.len() {
            let (key, desc, is_header) = &shortcuts_left[i];
            queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y + i as u16)).unwrap();
            if key.is_empty() {
                print_colored_text(stdout, "", Color::White, Some(Color::Black), None);
            } else if *is_header {
                // This is a section header
                print_colored_text(stdout, key, Color::Green, Some(Color::Black), None);
            } else {
                print_colored_text(stdout, key, Color::White, Some(Color::Black), None);
                print_colored_text(stdout, " : ", Color::Grey, Some(Color::Black), None);
                print_colored_text(stdout, desc, Color::White, Some(Color::Black), None);
            }
        }

        // Right column
        if i < shortcuts_right.len() {
            let (key, desc, is_header) = &shortcuts_right[i];
            queue!(stdout, crossterm::cursor::MoveTo(col2_x, current_y + i as u16)).unwrap();
            if key.is_empty() {
                print_colored_text(stdout, "", Color::White, Some(Color::Black), None);
            } else if *is_header {
                // This is a section header
                print_colored_text(stdout, key, Color::Green, Some(Color::Black), None);
            } else {
                print_colored_text(stdout, key, Color::White, Some(Color::Black), None);
                print_colored_text(stdout, " : ", Color::Grey, Some(Color::Black), None);
                print_colored_text(stdout, desc, Color::White, Some(Color::Black), None);
            }
        }
    }

    current_y += max_rows as u16 + 2;

    // Draw separator
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    print_colored_text(stdout, &"─".repeat(content_width), Color::DarkGrey, Some(Color::Black), None);
    current_y += 2;

    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    // BOTTOM SECTION: Terminal Options
    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    let terminal_title = "TERMINAL USAGE OPTIONS";
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    let centered_terminal_title = center_text(&terminal_title, content_width);
    print_colored_text(stdout, &centered_terminal_title, Color::Yellow, Some(Color::Black), None);
    current_y += 2;

    let usage_info = vec![
        ("Local Mode:", "", true),
        ("  sudo ./all-smi view", "Monitor local GPUs (requires sudo on macOS)", false),
        ("", "", false),
        ("Remote Mode:", "", true),
        ("  ./all-smi view --hosts http://node1:9090 http://node2:9090", "Monitor remote hosts", false),
        ("  ./all-smi view --hostfile hosts.csv", "Monitor hosts from CSV file", false),
        ("", "", false),
        ("API Mode:", "", true),
        ("  ./all-smi api --port 9090", "Run as Prometheus metrics server", false),
        ("", "", false),
        ("Build Commands:", "", true),
        ("  cargo build --release", "Build the application", false),
        ("  cargo run --bin all-smi -- view", "Run with cargo (development)", false),
    ];

    for (command, desc, is_header) in &usage_info {
        queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
        if command.is_empty() {
            print_colored_text(stdout, "", Color::White, Some(Color::Black), None);
        } else if *is_header {
            // This is a section header
            print_colored_text(stdout, command, Color::Cyan, Some(Color::Black), None);
        } else {
            print_colored_text(stdout, command, Color::White, Some(Color::Black), None);
            if !desc.is_empty() {
                print_colored_text(stdout, " # ", Color::Grey, Some(Color::Black), None);
                print_colored_text(stdout, desc, Color::DarkGreen, Some(Color::Black), None);
            }
        }
        current_y += 1;
    }

    // Bottom instruction
    let bottom_msg = "Press F1, h, or ESC to close this help";
    let bottom_y = start_y + popup_height as u16 - 3;
    queue!(stdout, crossterm::cursor::MoveTo(content_x, bottom_y)).unwrap();
    let centered_bottom = center_text(&bottom_msg, content_width);
    print_colored_text(stdout, &centered_bottom, Color::Magenta, Some(Color::Black), None);
    
    // Add function keys at the very bottom for full-screen consistency
    print_function_keys(stdout, cols, rows);
}

fn draw_window_frame<W: Write>(stdout: &mut W, x: u16, y: u16, width: usize, height: usize) {
    // Top border
    queue!(stdout, crossterm::cursor::MoveTo(x, y)).unwrap();
    print_colored_text(stdout, "╔", Color::White, Some(Color::Black), None);
    print_colored_text(stdout, &"═".repeat(width - 2), Color::White, Some(Color::Black), None);
    print_colored_text(stdout, "╗", Color::White, Some(Color::Black), None);

    // Side borders
    for i in 1..height - 1 {
        queue!(stdout, crossterm::cursor::MoveTo(x, y + i as u16)).unwrap();
        print_colored_text(stdout, "║", Color::White, Some(Color::Black), None);
        queue!(stdout, crossterm::cursor::MoveTo(x + width as u16 - 1, y + i as u16)).unwrap();
        print_colored_text(stdout, "║", Color::White, Some(Color::Black), None);
    }

    // Bottom border
    queue!(stdout, crossterm::cursor::MoveTo(x, y + height as u16 - 1)).unwrap();
    print_colored_text(stdout, "╚", Color::White, Some(Color::Black), None);
    print_colored_text(stdout, &"═".repeat(width - 2), Color::White, Some(Color::Black), None);
    print_colored_text(stdout, "╝", Color::White, Some(Color::Black), None);
}

fn center_text(text: &str, width: usize) -> String {
    let text_len = text.len();
    if text_len >= width {
        text.to_string()
    } else {
        let padding = (width - text_len) / 2;
        format!("{}{}", " ".repeat(padding), text)
    }
}
