use std::io::{self, Write};
use std::process::Command;

pub fn get_hostname() -> String {
    let output = Command::new("hostname")
        .output()
        .expect("Failed to execute hostname command");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Check if the current process already has sudo privileges
pub fn has_sudo_privileges() -> bool {
    Command::new("sudo")
        .arg("-n") // Non-interactive mode
        .arg("-v") // Validate sudo timestamp
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[allow(dead_code)] // Used in runner_old.rs (backup file)
pub fn calculate_adaptive_interval(node_count: usize) -> u64 {
    // Adaptive interval based on node count to prevent overwhelming the network
    // For 1-10 nodes: 2 seconds
    // For 11-50 nodes: 3 seconds
    // For 51-100 nodes: 4 seconds
    // For 101-200 nodes: 5 seconds
    // For 201+ nodes: 6 seconds
    match node_count {
        0..=10 => 2,
        11..=50 => 3,
        51..=100 => 4,
        101..=200 => 5,
        _ => 6,
    }
}

pub fn ensure_sudo_permissions() {
    if cfg!(target_os = "macos") {
        request_sudo_with_explanation();
    }
}

pub fn ensure_sudo_permissions_with_fallback() -> bool {
    if cfg!(target_os = "macos") {
        request_sudo_with_explanation_and_fallback()
    } else {
        true
    }
}

pub fn request_sudo_with_explanation() {
    // Check if we already have sudo privileges
    if has_sudo_privileges() {
        println!("✅ Administrator privileges already available.");
        println!("   Starting system monitoring...");
        println!();
        return;
    }

    println!("🔧 all-smi: System Monitoring Interface");
    println!("============================================");
    println!();
    println!("This application monitors GPU, CPU, and memory usage on your system.");
    println!();
    println!("🔒 Administrator privileges are required because:");
    println!("   • Access to hardware metrics requires the 'powermetrics' command");
    println!("   • powermetrics needs elevated privileges to read low-level system data");
    println!("   • This includes GPU utilization, power consumption, and thermal information");
    println!();
    println!("🛡️  Security Information:");
    println!("   • all-smi only reads system metrics - it does not modify your system");
    println!("   • The sudo access is used exclusively for running 'powermetrics'");
    println!("   • No data is transmitted externally without your explicit configuration");
    println!();
    println!("📋 What will be monitored:");
    println!("   • GPU: Utilization, memory usage, temperature, power consumption");
    println!("   • CPU: Core utilization and performance metrics");
    println!("   • Memory: System RAM usage and allocation");
    println!("   • Storage: Disk usage and performance");
    println!();

    // Give user a choice to continue
    print!("Do you want to continue and grant administrator privileges? [y/N]: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read user input");

    let input = input.trim().to_lowercase();
    if input != "y" && input != "yes" {
        println!();
        println!("❌ Administrator privileges declined.");
        println!("   → For remote monitoring only, use: all-smi view --hosts <url1> <url2>");
        println!("   → For local monitoring, administrator privileges are required.");
        println!();
        std::process::exit(0);
    }

    println!();
    println!("🔑 Requesting administrator privileges...");
    println!("   (You may be prompted for your password)");
    println!();

    // Attempt to get sudo privileges
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .expect("Failed to execute sudo command");

    if !status.success() {
        println!("❌ Failed to acquire administrator privileges.");
        println!();
        println!("💡 Troubleshooting:");
        println!("   • Make sure you entered the correct password");
        println!("   • Ensure your user account has administrator privileges");
        println!("   • Try running 'sudo -v' manually to test sudo access");
        println!();
        println!("   For remote monitoring without sudo, use:");
        println!("   → all-smi view --hosts <url1> <url2>");
        println!();
        std::process::exit(1);
    }

    println!("✅ Administrator privileges granted successfully.");
    println!("   Starting system monitoring...");
    println!();
}

pub fn request_sudo_with_explanation_and_fallback() -> bool {
    // Check if we already have sudo privileges
    if has_sudo_privileges() {
        println!("✅ Administrator privileges already available.");
        println!("   Starting local system monitoring...");
        println!();
        return true;
    }

    println!("🔧 all-smi: System Monitoring Interface");
    println!("============================================");
    println!();
    println!("This application monitors GPU, CPU, and memory usage on your system.");
    println!();
    println!("🔒 Administrator privileges are required for local monitoring because:");
    println!("   • Access to hardware metrics requires the 'powermetrics' command");
    println!("   • powermetrics needs elevated privileges to read low-level system data");
    println!("   • This includes GPU utilization, power consumption, and thermal information");
    println!();
    println!("🛡️  Security Information:");
    println!("   • all-smi only reads system metrics - it does not modify your system");
    println!("   • The sudo access is used exclusively for running 'powermetrics'");
    println!("   • No data is transmitted externally without your explicit configuration");
    println!();
    println!("📋 What will be monitored:");
    println!("   • GPU: Utilization, memory usage, temperature, power consumption");
    println!("   • CPU: Core utilization and performance metrics");
    println!("   • Memory: System RAM usage and allocation");
    println!("   • Storage: Disk usage and performance");
    println!();

    // Give user a choice to continue
    print!("Do you want to continue and grant administrator privileges? [y/N]: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read user input");

    let input = input.trim().to_lowercase();
    if input != "y" && input != "yes" {
        println!();
        println!("❌ Administrator privileges declined.");
        println!();
        println!("💡 Alternative: You can still use all-smi for remote monitoring!");
        println!("   Remote monitoring allows you to monitor other systems without sudo.");
        println!();
        print!("Would you like to continue in remote monitoring mode? [y/N]: ");
        io::stdout().flush().unwrap();

        let mut fallback_input = String::new();
        io::stdin()
            .read_line(&mut fallback_input)
            .expect("Failed to read user input");

        let fallback_input = fallback_input.trim().to_lowercase();
        if fallback_input == "y" || fallback_input == "yes" {
            println!();
            println!("📡 Remote monitoring mode selected.");
            println!("   Use the following commands to monitor remote systems:");
            println!("   → all-smi view --hosts http://host1:9090 http://host2:9090");
            println!("   → all-smi view --hostfile hosts.csv");
            println!();
            println!("   Note: Remote systems must be running all-smi in API mode:");
            println!("   → all-smi api --port 9090");
            println!();
            return false; // User chose remote monitoring
        } else {
            println!();
            println!("❌ Exiting all-smi.");
            println!("   To use all-smi later:");
            println!("   → For local monitoring: all-smi view (requires sudo)");
            println!("   → For remote monitoring: all-smi view --hosts <url1> <url2>");
            println!();
            std::process::exit(0);
        }
    }

    println!();
    println!("🔑 Requesting administrator privileges...");
    println!("   (You may be prompted for your password)");
    println!();

    // Attempt to get sudo privileges
    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .expect("Failed to execute sudo command");

    if !status.success() {
        println!("❌ Failed to acquire administrator privileges.");
        println!();
        println!("💡 Troubleshooting:");
        println!("   • Make sure you entered the correct password");
        println!("   • Ensure your user account has administrator privileges");
        println!("   • Try running 'sudo -v' manually to test sudo access");
        println!();
        println!("   For remote monitoring without sudo, use:");
        println!("   → all-smi view --hosts <url1> <url2>");
        println!();
        std::process::exit(1);
    }

    println!("✅ Administrator privileges granted successfully.");
    println!("   Starting local system monitoring...");
    println!();
    true // User granted sudo permissions
}
