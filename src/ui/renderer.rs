// Re-export all the renderer functions from their respective modules
pub use crate::ui::chrome::{print_function_keys, print_loading_indicator};
pub use crate::ui::device_renderers::{
    print_cpu_info, print_gpu_info, print_memory_info, print_storage_info,
};
pub use crate::ui::process_renderer::print_process_info;
