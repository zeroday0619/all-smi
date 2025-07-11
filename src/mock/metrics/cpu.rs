//! CPU metrics structures and utilities

use rand::{rng, Rng};

#[derive(Clone)]
pub struct CpuMetrics {
    pub model: String,
    pub utilization: f32,
    pub socket_count: u32,
    pub core_count: u32,
    pub thread_count: u32,
    pub frequency_mhz: u32,
    pub temperature_celsius: Option<u32>,
    pub power_consumption_watts: Option<f32>,
    // Per-socket utilization for multi-socket systems
    pub socket_utilizations: Vec<f32>,
    // Apple Silicon specific fields
    pub p_core_count: Option<u32>,
    pub e_core_count: Option<u32>,
    pub gpu_core_count: Option<u32>,
    pub p_core_utilization: Option<f32>,
    pub e_core_utilization: Option<f32>,
}

impl CpuMetrics {
    /// Update CPU metrics with realistic variations
    pub fn update(&mut self) {
        let mut rng = rng();

        // Update CPU utilization
        let cpu_utilization_delta = rng.random_range(-3.0..3.0);
        self.utilization = (self.utilization + cpu_utilization_delta).clamp(0.0, 100.0);

        // Update per-socket utilizations
        for socket_util in &mut self.socket_utilizations {
            let socket_delta = rng.random_range(-3.0..3.0);
            *socket_util = (*socket_util + socket_delta).clamp(0.0, 100.0);
        }

        // Update CPU temperature if available
        if let Some(ref mut temp) = self.temperature_celsius {
            let temp_delta = rng.random_range(-2..3);
            *temp = (*temp as i32 + temp_delta).clamp(35, 85) as u32;
        }

        // Update CPU power consumption if available
        if let Some(ref mut power) = self.power_consumption_watts {
            let power_delta = rng.random_range(-10.0..10.0);
            *power = (*power + power_delta).clamp(10.0, 500.0);
        }

        // Update Apple Silicon specific metrics
        if let (Some(ref mut p_util), Some(ref mut e_util)) =
            (&mut self.p_core_utilization, &mut self.e_core_utilization)
        {
            let p_delta = rng.random_range(-4.0..4.0);
            let e_delta = rng.random_range(-2.0..2.0);
            *p_util = (*p_util + p_delta).clamp(0.0, 100.0);
            *e_util = (*e_util + e_delta).clamp(0.0, 100.0);
        }
    }
}
