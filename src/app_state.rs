use crate::gpu::{CpuInfo, GpuInfo, ProcessInfo};
use crate::storage::info::StorageInfo;
use std::collections::{HashMap, VecDeque};

#[derive(Clone)]
pub struct AppState {
    pub gpu_info: Vec<GpuInfo>,
    pub cpu_info: Vec<CpuInfo>,
    pub process_info: Vec<ProcessInfo>,
    pub selected_process_index: usize,
    pub start_index: usize,
    pub sort_criteria: SortCriteria,
    pub loading: bool,
    pub tabs: Vec<String>,
    pub current_tab: usize,
    pub gpu_scroll_offset: usize,
    pub storage_scroll_offset: usize,
    pub tab_scroll_offset: usize,
    pub device_name_scroll_offsets: HashMap<String, usize>,
    pub hostname_scroll_offsets: HashMap<String, usize>,
    pub frame_counter: u64,
    pub storage_info: Vec<StorageInfo>,
    pub show_help: bool,
    pub utilization_history: VecDeque<f64>,
    pub memory_history: VecDeque<f64>,
    pub temperature_history: VecDeque<f64>,
}

#[derive(Clone)]
pub enum SortCriteria {
    // Process sorting (local mode only)
    Pid,
    Memory,
    // GPU sorting (both local and remote modes)
    Default,     // Hostname then index (current behavior)
    Utilization, // GPU utilization
    GpuMemory,   // GPU memory usage
    #[allow(dead_code)]
    Power, // Power consumption
    #[allow(dead_code)]
    Temperature, // Temperature
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            gpu_info: Vec::new(),
            cpu_info: Vec::new(),
            process_info: Vec::new(),
            selected_process_index: 0,
            start_index: 0,
            sort_criteria: SortCriteria::Default,
            loading: true,
            tabs: vec![
                "All".to_string(),
                "GPU".to_string(),
                "Storage".to_string(),
                "Process".to_string(),
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
        }
    }
}
