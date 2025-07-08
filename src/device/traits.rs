use crate::device::{CpuInfo, GpuInfo, MemoryInfo, ProcessInfo};

pub trait GpuReader: Send {
    fn get_gpu_info(&self) -> Vec<GpuInfo>;
    fn get_process_info(&self) -> Vec<ProcessInfo>;
}

pub trait CpuReader: Send {
    fn get_cpu_info(&self) -> Vec<CpuInfo>;
}

pub trait MemoryReader: Send {
    fn get_memory_info(&self) -> Vec<MemoryInfo>;
}