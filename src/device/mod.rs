pub mod apple_silicon;
pub mod nvidia;
pub mod nvidia_jetson;

// Re-export NVML status function for UI
pub use nvidia::get_nvml_status_message;

// CPU reader modules
pub mod cpu_linux;
pub mod cpu_macos;

// Memory reader modules
pub mod memory_linux;
pub mod memory_macos;

// Powermetrics parser for Apple Silicon
pub mod powermetrics_manager;
pub mod powermetrics_parser;

// Refactored modules
pub mod platform_detection;
pub mod process_list;
pub mod process_utils;
pub mod reader_factory;
pub mod traits;
pub mod types;

// Re-export commonly used items
pub use platform_detection::*;
pub use reader_factory::*;
pub use traits::*;
pub use types::*;
