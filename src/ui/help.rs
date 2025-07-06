use std::io::Write;
use crossterm::{
    queue,
    style::Color,
};

use crate::app_state::AppState;
use crate::ui::renderer::{print_colored_text, print_function_keys};

pub fn print_help_popup<W: Write>(
    stdout: &mut W, 
    cols: u16, 
    rows: u16, 
    state: &AppState, 
    is_remote: bool
) {
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
    current_y = render_title_section(stdout, content_x, current_y, content_width);

    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    // MIDDLE SECTION: Cheat Sheet
    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    current_y = render_shortcuts_section(stdout, content_x, current_y, content_width, is_remote);

    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    // BOTTOM SECTION: Terminal Options
    // ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    current_y = render_usage_section(stdout, content_x, current_y, content_width);

    // Current sort status
    current_y += 1;
    let sort_status = get_current_sort_status(&state.sort_criteria);
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    let centered_sort = center_text(&sort_status, content_width);
    print_colored_text(stdout, &centered_sort, Color::Yellow, Some(Color::Black), None);

    // Bottom instruction
    let bottom_msg = "Press 1, h, or ESC to close this help";
    let bottom_y = start_y + popup_height as u16 - 3;
    queue!(stdout, crossterm::cursor::MoveTo(content_x, bottom_y)).unwrap();
    let centered_bottom = center_text(&bottom_msg, content_width);
    print_colored_text(stdout, &centered_bottom, Color::Magenta, Some(Color::Black), None);
    
    // Add function keys at the very bottom for full-screen consistency
    print_function_keys(stdout, cols, rows, state, is_remote);
}

fn render_title_section<W: Write>(
    stdout: &mut W,
    content_x: u16,
    mut current_y: u16,
    content_width: usize,
) -> u16 {
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
    current_y + 2
}

fn render_shortcuts_section<W: Write>(
    stdout: &mut W,
    content_x: u16,
    mut current_y: u16,
    content_width: usize,
    is_remote: bool,
) -> u16 {
    let cheat_sheet_title = "KEYBOARD SHORTCUTS & NAVIGATION";
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    let centered_title = center_text(&cheat_sheet_title, content_width);
    print_colored_text(stdout, &centered_title, Color::Yellow, Some(Color::Black), None);
    current_y += 2;

    // Two-column layout for shortcuts
    let col1_width = content_width / 2;
    let col2_x = content_x + col1_width as u16;

    let shortcuts_left = get_navigation_shortcuts();
    let shortcuts_right = get_mode_specific_shortcuts(is_remote);

    let max_rows = shortcuts_left.len().max(shortcuts_right.len());
    
    for i in 0..max_rows {
        render_shortcut_row(stdout, &shortcuts_left, &shortcuts_right, i, content_x, col2_x, current_y + i as u16);
    }

    current_y += max_rows as u16 + 2;

    // Draw separator
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    print_colored_text(stdout, &"─".repeat(content_width), Color::DarkGrey, Some(Color::Black), None);
    current_y + 2
}

fn render_usage_section<W: Write>(
    stdout: &mut W,
    content_x: u16,
    mut current_y: u16,
    content_width: usize,
) -> u16 {
    let terminal_title = "TERMINAL USAGE OPTIONS";
    queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
    let centered_terminal_title = center_text(&terminal_title, content_width);
    print_colored_text(stdout, &centered_terminal_title, Color::Yellow, Some(Color::Black), None);
    current_y += 2;

    let usage_info = get_usage_information();

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

    current_y
}

fn get_navigation_shortcuts() -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("Navigation", "", true),
        ("  ← →", "Switch between tabs", false),
        ("  ↑ ↓", "Scroll up/down", false),
        ("  PgUp/PgDn", "Page up/down", false),
        ("", "", false),
        ("Display Control", "", true),
        ("  1 / h", "Toggle this help", false),
        ("  q", "Exit application", false),
        ("  ESC", "Close help or exit", false),
    ]
}

fn get_mode_specific_shortcuts(is_remote: bool) -> Vec<(&'static str, &'static str, bool)> {
    if is_remote {
        vec![
            ("GPU Sorting", "", true),
            ("  d", "Sort GPUs by Default (Host+Index)", false),
            ("  u", "Sort GPUs by Utilization", false),
            ("  g", "Sort GPUs by Memory usage", false),
            ("", "", false),
            ("Tab Information", "", true),
            ("  All", "Show all GPUs across hosts", false),
            ("  [Host]", "Show GPUs from specific host", false),
            ("", "", false),
            ("Color Legend", "", true),
            ("  Green", "Normal usage (≤ 60%)", false),
            ("  Yellow", "Medium usage (60-80%)", false),
            ("  Red", "High usage (> 80%)", false),
        ]
    } else {
        vec![
            ("Process Sorting (Local)", "", true),
            ("  p", "Sort processes by PID", false),
            ("  m", "Sort processes by Memory", false),
            ("", "", false),
            ("GPU Sorting", "", true),
            ("  d", "Sort GPUs by Default (Host+Index)", false),
            ("  u", "Sort GPUs by Utilization", false),
            ("  g", "Sort GPUs by Memory usage", false),
            ("", "", false),
            ("Color Legend", "", true),
            ("  Green", "Normal usage (≤ 60%)", false),
            ("  Yellow", "Medium usage (60-80%)", false),
            ("  Red", "High usage (> 80%)", false),
        ]
    }
}

fn get_usage_information() -> Vec<(&'static str, &'static str, bool)> {
    vec![
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
    ]
}

fn get_current_sort_status(sort_criteria: &crate::app_state::SortCriteria) -> String {
    match sort_criteria {
        crate::app_state::SortCriteria::Default => "Current Sort: Default (Hostname then Index)".to_string(),
        crate::app_state::SortCriteria::Pid => "Current Sort: Process PID".to_string(),
        crate::app_state::SortCriteria::Memory => "Current Sort: Process Memory".to_string(),
        crate::app_state::SortCriteria::Utilization => "Current Sort: GPU Utilization".to_string(),
        crate::app_state::SortCriteria::GpuMemory => "Current Sort: GPU Memory".to_string(),
        crate::app_state::SortCriteria::Power => "Current Sort: Power Consumption".to_string(),
        crate::app_state::SortCriteria::Temperature => "Current Sort: Temperature".to_string(),
    }
}

fn render_shortcut_row<W: Write>(
    stdout: &mut W,
    shortcuts_left: &[(&'static str, &'static str, bool)],
    shortcuts_right: &[(&'static str, &'static str, bool)],
    row_index: usize,
    content_x: u16,
    col2_x: u16,
    current_y: u16,
) {
    // Left column
    if row_index < shortcuts_left.len() {
        let (key, desc, is_header) = &shortcuts_left[row_index];
        queue!(stdout, crossterm::cursor::MoveTo(content_x, current_y)).unwrap();
        render_shortcut_item(stdout, key, desc, *is_header);
    }

    // Right column
    if row_index < shortcuts_right.len() {
        let (key, desc, is_header) = &shortcuts_right[row_index];
        queue!(stdout, crossterm::cursor::MoveTo(col2_x, current_y)).unwrap();
        render_shortcut_item(stdout, key, desc, *is_header);
    }
}

fn render_shortcut_item<W: Write>(
    stdout: &mut W,
    key: &str,
    desc: &str,
    is_header: bool,
) {
    if key.is_empty() {
        print_colored_text(stdout, "", Color::White, Some(Color::Black), None);
    } else if is_header {
        // This is a section header
        print_colored_text(stdout, key, Color::Green, Some(Color::Black), None);
    } else {
        print_colored_text(stdout, key, Color::White, Some(Color::Black), None);
        print_colored_text(stdout, " : ", Color::Grey, Some(Color::Black), None);
        print_colored_text(stdout, desc, Color::White, Some(Color::Black), None);
    }
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