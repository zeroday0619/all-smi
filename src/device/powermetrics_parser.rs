use std::process::Command;

/// Enhanced powermetrics parser for Apple Silicon metrics
/// Based on asitop's approach of parsing active residency data
#[derive(Debug, Default, Clone)]
pub struct PowerMetricsData {
    // CPU metrics
    pub e_cluster_active_residency: f64,
    pub p_cluster_active_residency: f64,
    pub e_cluster_frequency: u32,
    pub p_cluster_frequency: u32,
    pub cpu_power_mw: f64,

    // Per-core metrics
    pub core_active_residencies: Vec<f64>,
    pub core_frequencies: Vec<u32>,
    pub core_cluster_types: Vec<CoreType>, // E or P core

    // GPU metrics
    pub gpu_active_residency: f64,
    pub gpu_frequency: u32,
    pub gpu_power_mw: f64,

    // ANE metrics
    pub ane_power_mw: f64,

    // Combined metrics
    pub combined_power_mw: f64,

    // Thermal
    pub thermal_pressure: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoreType {
    Efficiency,
    Performance,
}

impl PowerMetricsData {
    /// Get CPU utilization as a percentage (0-100)
    /// Uses weighted average of cluster utilization
    pub fn cpu_utilization(&self) -> f64 {
        // Weight P-cores more heavily as they handle more intensive tasks
        self.e_cluster_active_residency * 0.3 + self.p_cluster_active_residency * 0.7
    }

    /// Get GPU utilization as a percentage (0-100)
    pub fn gpu_utilization(&self) -> f64 {
        self.gpu_active_residency
    }
}

/// Run powermetrics and parse the output
#[allow(dead_code)]
pub fn get_powermetrics_data() -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
    let output = Command::new("sudo")
        .args([
            "powermetrics",
            "--samplers",
            "cpu_power,gpu_power,ane_power,thermal",
            "-n",
            "1",
            "-i",
            "1000",
        ])
        .output()?;

    if !output.status.success() {
        return Err("powermetrics command failed".into());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_powermetrics_output(&output_str)
}

/// Parse powermetrics text output
pub fn parse_powermetrics_output(
    output: &str,
) -> Result<PowerMetricsData, Box<dyn std::error::Error>> {
    let mut data = PowerMetricsData::default();
    let mut in_e_cluster = false;
    let mut _in_p_cluster = false;

    for line in output.lines() {
        let line = line.trim();

        // E-Cluster metrics
        if line.starts_with("E-Cluster HW active frequency:") {
            in_e_cluster = true;
            _in_p_cluster = false;
            data.e_cluster_frequency = parse_frequency(line)?;
        } else if line.starts_with("E-Cluster HW active residency:") {
            data.e_cluster_active_residency = parse_residency(line)?;
        }
        // P-Cluster metrics
        else if line.starts_with("P-Cluster HW active frequency:") {
            _in_p_cluster = true;
            in_e_cluster = false;
            data.p_cluster_frequency = parse_frequency(line)?;
        } else if line.starts_with("P-Cluster HW active residency:") {
            data.p_cluster_active_residency = parse_residency(line)?;
        }
        // Per-core metrics
        else if line.starts_with("CPU") && line.contains("frequency:") {
            let freq = parse_frequency(line)?;
            data.core_frequencies.push(freq);

            // Determine core type based on current cluster
            let core_type = if in_e_cluster {
                CoreType::Efficiency
            } else {
                CoreType::Performance
            };
            data.core_cluster_types.push(core_type);
        } else if line.starts_with("CPU") && line.contains("active residency:") {
            let residency = parse_residency(line)?;
            data.core_active_residencies.push(residency);
        }
        // GPU metrics
        else if line.starts_with("GPU HW active frequency:") {
            data.gpu_frequency = parse_frequency(line)?;
        } else if line.starts_with("GPU HW active residency:") {
            data.gpu_active_residency = parse_residency(line)?;
        }
        // Power metrics
        else if line.starts_with("CPU Power:") && !line.contains("GPU") {
            data.cpu_power_mw = parse_power_mw(line)?;
        } else if line.starts_with("GPU Power:") && !line.contains("CPU") {
            data.gpu_power_mw = parse_power_mw(line)?;
        } else if line.starts_with("ANE Power:") {
            data.ane_power_mw = parse_power_mw(line)?;
        } else if line.contains("Combined Power (CPU + GPU + ANE):") {
            data.combined_power_mw = parse_power_mw(line)?;
        }
        // Thermal
        else if line.contains("Thermal pressure") {
            if let Some(pressure_str) = line.split(':').nth(1) {
                data.thermal_pressure = pressure_str.trim().parse::<u32>().ok();
            }
        }
    }

    Ok(data)
}

/// Parse frequency from a line like "E-Cluster HW active frequency: 1187 MHz"
fn parse_frequency(line: &str) -> Result<u32, Box<dyn std::error::Error>> {
    if let Some(freq_str) = line.split(':').nth(1) {
        if let Some(freq) = freq_str.split_whitespace().next() {
            return Ok(freq.parse::<u32>()?);
        }
    }
    Err("Failed to parse frequency".into())
}

/// Parse residency from a line like "E-Cluster HW active residency:  64.29%"
fn parse_residency(line: &str) -> Result<f64, Box<dyn std::error::Error>> {
    if let Some(residency_str) = line.split(':').nth(1) {
        if let Some(percent_str) = residency_str.split_whitespace().next() {
            let percent = percent_str.trim_end_matches('%').parse::<f64>()?;
            return Ok(percent);
        }
    }
    Err("Failed to parse residency".into())
}

/// Parse power from a line like "CPU Power: 475 mW"
fn parse_power_mw(line: &str) -> Result<f64, Box<dyn std::error::Error>> {
    if let Some(power_str) = line.split(':').nth(1) {
        if let Some(mw_str) = power_str.split_whitespace().next() {
            return Ok(mw_str.parse::<f64>()?);
        }
    }
    Err("Failed to parse power".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_powermetrics_output() {
        let sample_output = r#"
E-Cluster HW active frequency: 1187 MHz
E-Cluster HW active residency:  64.29% (600 MHz:   0% 912 MHz:  48% 1284 MHz: 1.6% 1752 MHz: 4.2% 2004 MHz: 3.2% 2256 MHz: 1.1% 2424 MHz: 5.7%)
E-Cluster idle residency:  35.71%
CPU 0 frequency: 1514 MHz
CPU 0 active residency:  27.07%
CPU 1 frequency: 1413 MHz
CPU 1 active residency:  23.67%

P-Cluster HW active frequency: 1172 MHz
P-Cluster HW active residency:  60.00%
P-Cluster idle residency:  40.00%
CPU 4 frequency: 1643 MHz
CPU 4 active residency:  24.71%

CPU Power: 475 mW
GPU Power: 47 mW
ANE Power: 0 mW
Combined Power (CPU + GPU + ANE): 522 mW

GPU HW active frequency: 444 MHz
GPU HW active residency:   9.85%
"#;

        let data = parse_powermetrics_output(sample_output).unwrap();

        assert_eq!(data.e_cluster_frequency, 1187);
        assert_eq!(data.e_cluster_active_residency, 64.29);
        assert_eq!(data.p_cluster_frequency, 1172);
        assert_eq!(data.p_cluster_active_residency, 60.0);
        assert_eq!(data.cpu_power_mw, 475.0);
        assert_eq!(data.gpu_power_mw, 47.0);
        assert_eq!(data.gpu_frequency, 444);
        assert_eq!(data.gpu_active_residency, 9.85);
    }

    #[test]
    fn test_parse_powermetrics_with_cores() {
        let test_output = r#"
E-Cluster HW active frequency: 1020 MHz
E-Cluster HW active residency: 25.5%
CPU 0 frequency: 1020 MHz
CPU 0 active residency: 12.5%
CPU 1 frequency: 1020 MHz
CPU 1 active residency: 13.0%

P-Cluster HW active frequency: 3000 MHz
P-Cluster HW active residency: 75.5%
CPU 4 frequency: 3000 MHz
CPU 4 active residency: 50.0%
CPU 5 frequency: 3000 MHz
CPU 5 active residency: 25.5%

GPU HW active frequency: 1200 MHz
GPU HW active residency: 45.5%

CPU Power: 1500 mW
GPU Power: 2500 mW
ANE Power: 100 mW
Combined Power (CPU + GPU + ANE): 4100 mW
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        // Check cluster data
        assert_eq!(data.e_cluster_frequency, 1020);
        assert_eq!(data.e_cluster_active_residency, 25.5);
        assert_eq!(data.p_cluster_frequency, 3000);
        assert_eq!(data.p_cluster_active_residency, 75.5);

        // Check GPU data
        assert_eq!(data.gpu_frequency, 1200);
        assert_eq!(data.gpu_active_residency, 45.5);

        // Check power data
        assert_eq!(data.cpu_power_mw, 1500.0);
        assert_eq!(data.gpu_power_mw, 2500.0);
        assert_eq!(data.ane_power_mw, 100.0);
        assert_eq!(data.combined_power_mw, 4100.0);

        // Check core data
        assert_eq!(data.core_frequencies.len(), 4);
        assert_eq!(data.core_active_residencies.len(), 4);
        assert_eq!(data.core_cluster_types.len(), 4);

        // E-cores
        assert_eq!(data.core_frequencies[0], 1020);
        assert_eq!(data.core_frequencies[1], 1020);
        assert_eq!(data.core_cluster_types[0], CoreType::Efficiency);
        assert_eq!(data.core_cluster_types[1], CoreType::Efficiency);

        // P-cores
        assert_eq!(data.core_frequencies[2], 3000);
        assert_eq!(data.core_frequencies[3], 3000);
        assert_eq!(data.core_cluster_types[2], CoreType::Performance);
        assert_eq!(data.core_cluster_types[3], CoreType::Performance);
    }

    #[test]
    fn test_parse_powermetrics_with_missing_fields() {
        // Test with minimal output
        let test_output = r#"
GPU HW active frequency: 1000 MHz
GPU HW active residency: 20.0%
"#;

        let data = parse_powermetrics_output(test_output).unwrap();

        assert_eq!(data.gpu_active_residency, 20.0);
        assert_eq!(data.gpu_frequency, 1000);
        assert_eq!(data.e_cluster_active_residency, 0.0);
        assert_eq!(data.p_cluster_active_residency, 0.0);
        assert_eq!(data.core_frequencies.len(), 0);
    }

    #[test]
    fn test_parse_invalid_output() {
        let invalid_output = "not a valid powermetrics output";
        let result = parse_powermetrics_output(invalid_output);
        // Should return default values, not error
        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.gpu_active_residency, 0.0);
        assert_eq!(data.e_cluster_active_residency, 0.0);
    }

    #[test]
    fn test_coretype_equality() {
        assert_eq!(CoreType::Efficiency, CoreType::Efficiency);
        assert_eq!(CoreType::Performance, CoreType::Performance);
        assert_ne!(CoreType::Efficiency, CoreType::Performance);
    }

    #[test]
    fn test_powermetrics_data_utilization_methods() {
        let mut data = PowerMetricsData::default();
        data.e_cluster_active_residency = 30.0;
        data.p_cluster_active_residency = 70.0;
        data.gpu_active_residency = 50.0;

        // Test CPU utilization (weighted average)
        let cpu_util = data.cpu_utilization();
        assert_eq!(cpu_util, 30.0 * 0.3 + 70.0 * 0.7); // 9 + 49 = 58

        // Test GPU utilization
        let gpu_util = data.gpu_utilization();
        assert_eq!(gpu_util, 50.0);
    }
}

// Note: The get_powermetrics_data function is already defined above at line 55
// It returns Result<PowerMetricsData, Box<dyn std::error::Error>>
