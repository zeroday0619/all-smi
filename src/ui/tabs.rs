use crossterm::{
    queue,
    style::{Color, Print},
};
use std::io::Write;

use crate::app_state::AppState;
use crate::ui::renderer::print_colored_text;

pub fn draw_tabs<W: Write>(stdout: &mut W, state: &AppState, cols: u16) {
    // Print tabs
    let mut labels: Vec<(String, Color)> = Vec::new();

    // Calculate available width for tabs
    // Reserve space for "Tabs: " prefix (6 chars) plus some padding
    let mut available_width = cols.saturating_sub(8);

    // Skip tabs that are before the scroll offset
    let visible_tabs: Vec<_> = state
        .tabs
        .iter()
        .enumerate()
        .skip(state.tab_scroll_offset)
        .collect();

    for (i, tab) in visible_tabs {
        let tab_width = tab.len() as u16 + 2; // Tab name + 2 spaces padding
        if available_width < tab_width {
            break; // No more space
        }

        if state.current_tab == i {
            labels.push((format!(" {tab} "), Color::Black));
        } else {
            labels.push((format!(" {tab} "), Color::White));
        }

        available_width -= tab_width;
    }

    // Render tabs
    render_tab_labels(stdout, labels);
    render_tab_separator(stdout, cols);
}

fn render_tab_labels<W: Write>(stdout: &mut W, labels: Vec<(String, Color)>) {
    queue!(stdout, Print("Tabs: ")).unwrap();
    for (text, color) in labels {
        if color == Color::Black {
            // Selected tab: white text on blue background for good visibility
            print_colored_text(stdout, &text, Color::White, Some(Color::Blue), None);
        } else {
            print_colored_text(stdout, &text, color, None, None);
        }
    }
    queue!(stdout, Print("\r\n")).unwrap();
}

fn render_tab_separator<W: Write>(stdout: &mut W, cols: u16) {
    // Print separator
    let separator = "─".repeat(cols as usize);
    print_colored_text(stdout, &separator, Color::DarkGrey, None, None);
    queue!(stdout, Print("\r\n")).unwrap();
}

#[allow(dead_code)]
pub fn calculate_tab_visibility(state: &AppState, cols: u16) -> TabVisibility {
    let mut available_width = cols.saturating_sub(8);
    let mut last_visible_tab = state.tab_scroll_offset;

    for (i, tab) in state.tabs.iter().enumerate().skip(state.tab_scroll_offset) {
        let tab_width = tab.len() as u16 + 2;
        if available_width < tab_width {
            break;
        }
        available_width -= tab_width;
        last_visible_tab = i;
    }

    TabVisibility {
        first_visible: state.tab_scroll_offset,
        last_visible: last_visible_tab,
        has_more_left: state.tab_scroll_offset > 0,
        has_more_right: last_visible_tab < state.tabs.len() - 1,
    }
}

#[allow(dead_code)]
pub struct TabVisibility {
    pub first_visible: usize,
    pub last_visible: usize,
    pub has_more_left: bool,
    pub has_more_right: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    fn create_test_state() -> AppState {
        AppState {
            gpu_info: Vec::new(),
            cpu_info: Vec::new(),
            memory_info: Vec::new(),
            process_info: Vec::new(),
            selected_process_index: 0,
            start_index: 0,
            sort_criteria: crate::app_state::SortCriteria::Default,
            loading: false,
            tabs: vec![
                "All".to_string(),
                "host1".to_string(),
                "host2".to_string(),
                "host3".to_string(),
            ],
            current_tab: 0,
            gpu_scroll_offset: 0,
            storage_scroll_offset: 0,
            tab_scroll_offset: 0,
            device_name_scroll_offsets: HashMap::new(),
            hostname_scroll_offsets: HashMap::new(),
            frame_counter: 0,
            storage_info: Vec::new(),
            show_help: false,
            utilization_history: VecDeque::new(),
            memory_history: VecDeque::new(),
            temperature_history: VecDeque::new(),
            notifications: crate::ui::notification::NotificationManager::new(),
            nvml_notification_shown: false,
        }
    }

    #[test]
    fn test_tab_visibility_calculation() {
        let state = create_test_state();
        let visibility = calculate_tab_visibility(&state, 80);

        assert_eq!(visibility.first_visible, 0);
        assert!(!visibility.has_more_left);
        assert!(!visibility.has_more_right || state.tabs.len() > 4);
    }

    #[test]
    fn test_tab_visibility_with_scroll() {
        let mut state = create_test_state();
        state.tab_scroll_offset = 1;
        let visibility = calculate_tab_visibility(&state, 80);

        assert_eq!(visibility.first_visible, 1);
        assert!(visibility.has_more_left);
    }
}
