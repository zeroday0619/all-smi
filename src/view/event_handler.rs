use crossterm::{
    event::KeyEvent,
    event::KeyCode,
    terminal::size,
};

use crate::app_state::{AppState, SortCriteria};
use crate::cli::ViewArgs;

pub async fn handle_key_event(
    key_event: KeyEvent,
    state: &mut AppState,
    args: &ViewArgs,
) -> bool {
    match key_event.code {
        KeyCode::Esc => {
            if state.show_help {
                state.show_help = false;
                false
            } else {
                true // Exit
            }
        }
        KeyCode::Char('q') => true, // Exit
        KeyCode::Char('1') | KeyCode::Char('h') => {
            state.show_help = !state.show_help;
            false
        }
        KeyCode::Left => {
            if !state.show_help {
                handle_left_arrow(state);
            }
            false
        }
        KeyCode::Right => {
            if !state.show_help {
                handle_right_arrow(state);
            }
            false
        }
        _ if !state.loading && !state.show_help => {
            handle_navigation_keys(key_event.code, state, args);
            false
        }
        _ => false,
    }
}

fn handle_left_arrow(state: &mut AppState) {
    if state.current_tab > 0 {
        state.current_tab -= 1;
        if state.current_tab < state.tab_scroll_offset + 1 && state.tab_scroll_offset > 0 {
            state.tab_scroll_offset -= 1;
        }
    }
    state.gpu_scroll_offset = 0;
    state.storage_scroll_offset = 0;
}

fn handle_right_arrow(state: &mut AppState) {
    if state.current_tab < state.tabs.len() - 1 {
        state.current_tab += 1;
        let (cols, _) = size().unwrap();
        let mut available_width = cols.saturating_sub(5);
        let mut last_visible_tab = state.tab_scroll_offset;
        
        for (i, tab) in state
            .tabs
            .iter()
            .enumerate()
            .skip(1)
            .skip(state.tab_scroll_offset)
        {
            let tab_width = tab.len() as u16 + 2;
            if available_width < tab_width {
                break;
            }
            available_width -= tab_width;
            last_visible_tab = i;
        }
        
        if state.current_tab > last_visible_tab {
            state.tab_scroll_offset += 1;
        }
    }
    state.gpu_scroll_offset = 0;
    state.storage_scroll_offset = 0;
}

fn handle_navigation_keys(key_code: KeyCode, state: &mut AppState, args: &ViewArgs) {
    match key_code {
        KeyCode::Up => handle_up_arrow(state, args),
        KeyCode::Down => handle_down_arrow(state, args),
        KeyCode::PageUp => handle_page_up(state, args),
        KeyCode::PageDown => handle_page_down(state, args),
        KeyCode::Char('p') => state.sort_criteria = SortCriteria::Pid,
        KeyCode::Char('m') => state.sort_criteria = SortCriteria::Memory,
        KeyCode::Char('u') => state.sort_criteria = SortCriteria::Utilization,
        KeyCode::Char('g') => state.sort_criteria = SortCriteria::GpuMemory,
        _ => {}
    }
}

fn handle_up_arrow(state: &mut AppState, args: &ViewArgs) {
    let is_remote = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote {
        // Unified scrolling for remote mode
        if state.gpu_scroll_offset > 0 {
            state.gpu_scroll_offset -= 1;
            state.storage_scroll_offset = 0; // Reset storage scroll when in GPU area
        } else if state.storage_scroll_offset > 0 {
            state.storage_scroll_offset -= 1;
        }
    } else {
        // Local mode - process list scrolling
        if state.selected_process_index > 0 {
            state.selected_process_index -= 1;
        }
        if state.selected_process_index < state.start_index {
            state.start_index = state.selected_process_index;
        }
    }
}

fn handle_down_arrow(state: &mut AppState, args: &ViewArgs) {
    let is_remote = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote {
        // Unified scrolling for remote mode
        let gpu_count = if state.current_tab == 0 {
            state.gpu_info.len()
        } else {
            state
                .gpu_info
                .iter()
                .filter(|info| info.hostname == state.tabs[state.current_tab])
                .count()
        };

        let storage_count = if state.current_tab == 0 {
            // No storage on 'All' tab
            0
        } else {
            state
                .storage_info
                .iter()
                .filter(|info| info.hostname == state.tabs[state.current_tab])
                .count()
        };

        if state.gpu_scroll_offset < gpu_count.saturating_sub(1) {
            state.gpu_scroll_offset += 1;
            state.storage_scroll_offset = 0; // Reset storage scroll when in GPU area
        } else if state.storage_scroll_offset < storage_count.saturating_sub(1) {
            state.storage_scroll_offset += 1;
        }
    } else {
        // Local mode - process list scrolling
        if !state.process_info.is_empty()
            && state.selected_process_index < state.process_info.len() - 1
        {
            state.selected_process_index += 1;
        }
        let (_cols, rows) = size().unwrap();
        let half_rows = rows / 2;
        let visible_process_rows = half_rows.saturating_sub(1) as usize;
        if state.selected_process_index >= state.start_index + visible_process_rows {
            state.start_index = state.selected_process_index - visible_process_rows + 1;
        }
    }
}

fn handle_page_up(state: &mut AppState, args: &ViewArgs) {
    let is_remote = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote {
        // Remote mode - page up through GPU list
        let (_cols, rows) = size().unwrap();
        let content_start_row = 19;
        let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize;

        // Calculate storage display space for current tab
        let storage_items_count = if state.current_tab > 0 && !state.storage_info.is_empty() {
            let current_hostname = &state.tabs[state.current_tab];
            state
                .storage_info
                .iter()
                .filter(|info| info.hostname == *current_hostname)
                .count()
        } else {
            0
        };
        let storage_display_rows = if storage_items_count > 0 {
            storage_items_count + 2 // Each storage item takes 1 line (labels + bar on same line)
        } else {
            0
        };

        let gpu_display_rows = available_rows.saturating_sub(storage_display_rows);
        let lines_per_gpu = 2; // Each GPU takes 2 lines (labels + progress bars on same line)
        let max_gpu_items = gpu_display_rows / lines_per_gpu;
        let page_size = max_gpu_items.max(1); // At least 1 item per page

        state.gpu_scroll_offset = state.gpu_scroll_offset.saturating_sub(page_size);
        state.storage_scroll_offset = 0; // Reset storage scroll when paging GPU list
    } else {
        // Local mode - page up through process list
        let (_cols, rows) = size().unwrap();
        let half_rows = rows / 2;
        let page_size = half_rows.saturating_sub(1) as usize;
        state.selected_process_index = state.selected_process_index.saturating_sub(page_size);
        if state.selected_process_index < state.start_index {
            state.start_index = state.selected_process_index;
        }
    }
}

fn handle_page_down(state: &mut AppState, args: &ViewArgs) {
    let is_remote = args.hosts.is_some() || args.hostfile.is_some();
    if is_remote {
        // Remote mode - page down through GPU list
        let (_cols, rows) = size().unwrap();
        let content_start_row = 19;
        let available_rows = rows.saturating_sub(content_start_row).saturating_sub(1) as usize;

        // Calculate storage display space for current tab
        let storage_items_count = if state.current_tab > 0 && !state.storage_info.is_empty() {
            let current_hostname = &state.tabs[state.current_tab];
            state
                .storage_info
                .iter()
                .filter(|info| info.hostname == *current_hostname)
                .count()
        } else {
            0
        };
        let storage_display_rows = if storage_items_count > 0 {
            storage_items_count + 2 // Each storage item takes 1 line (labels + bar on same line)
        } else {
            0
        };

        let gpu_display_rows = available_rows.saturating_sub(storage_display_rows);
        let lines_per_gpu = 2; // Each GPU takes 2 lines (labels + progress bars on same line)
        let max_gpu_items = gpu_display_rows / lines_per_gpu;
        let page_size = max_gpu_items.max(1); // At least 1 item per page

        // Calculate total GPUs for current tab
        let total_gpus = if state.current_tab == 0 {
            state.gpu_info.len()
        } else {
            state
                .gpu_info
                .iter()
                .filter(|info| info.hostname == state.tabs[state.current_tab])
                .count()
        };

        if total_gpus > 0 {
            let max_offset = total_gpus.saturating_sub(max_gpu_items);
            state.gpu_scroll_offset = (state.gpu_scroll_offset + page_size).min(max_offset);
            state.storage_scroll_offset = 0; // Reset storage scroll when paging GPU list
        }
    } else {
        // Local mode - page down through process list
        if !state.process_info.is_empty() {
            let (_cols, rows) = size().unwrap();
            let half_rows = rows / 2;
            let page_size = half_rows.saturating_sub(1) as usize;
            state.selected_process_index =
                (state.selected_process_index + page_size).min(state.process_info.len() - 1);
            let visible_process_rows = half_rows.saturating_sub(1) as usize;
            if state.selected_process_index >= state.start_index + visible_process_rows {
                state.start_index = state.selected_process_index - visible_process_rows + 1;
            }
        }
    }
}
