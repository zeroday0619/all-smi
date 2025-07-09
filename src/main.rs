mod api;
mod app_state;
mod cli;
mod common;
mod device;
mod metrics;
mod network;
mod storage;
mod ui;
mod utils;
mod view;

use api::run_api_mode;
use clap::Parser;
use cli::{Cli, Commands};
use tokio::signal;
use utils::{ensure_sudo_permissions, ensure_sudo_permissions_with_fallback};

#[cfg(target_os = "macos")]
use device::is_apple_silicon;
#[cfg(target_os = "macos")]
use device::powermetrics_manager::{
    initialize_powermetrics_manager, shutdown_powermetrics_manager,
};

#[tokio::main]
async fn main() {
    // Set up panic handler for cleanup
    #[cfg(target_os = "macos")]
    setup_panic_handler();

    let cli = Cli::parse();

    // Set up signal handler for clean shutdown
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        #[cfg(target_os = "macos")]
        shutdown_powermetrics_manager();
        std::process::exit(0);
    });

    // Initialize PowerMetricsManager for macOS if we're running locally
    #[cfg(target_os = "macos")]
    let _needs_manager = match &cli.command {
        Some(Commands::Api(_)) => true,
        Some(Commands::View(args)) => args.hosts.is_none() && args.hostfile.is_none(),
        None => true,
    };

    match cli.command {
        Some(Commands::Api(args)) => {
            ensure_sudo_permissions();

            // Initialize PowerMetricsManager after getting sudo
            #[cfg(target_os = "macos")]
            if is_apple_silicon() {
                if let Err(e) = initialize_powermetrics_manager() {
                    eprintln!("Warning: Failed to initialize PowerMetricsManager: {}", e);
                }
            }

            run_api_mode(&args).await;
        }
        Some(Commands::View(args)) => {
            if args.hosts.is_none() && args.hostfile.is_none() {
                ensure_sudo_permissions();

                // Initialize PowerMetricsManager after getting sudo
                #[cfg(target_os = "macos")]
                if is_apple_silicon() {
                    if let Err(e) = initialize_powermetrics_manager() {
                        eprintln!("Warning: Failed to initialize PowerMetricsManager: {}", e);
                    }
                }
            }
            view::run_view_mode(&args).await;
        }
        None => {
            let has_sudo = ensure_sudo_permissions_with_fallback();
            if has_sudo {
                // Initialize PowerMetricsManager after getting sudo
                #[cfg(target_os = "macos")]
                if is_apple_silicon() {
                    if let Err(e) = initialize_powermetrics_manager() {
                        eprintln!("Warning: Failed to initialize PowerMetricsManager: {}", e);
                    }
                }

                view::run_view_mode(&cli::ViewArgs {
                    hosts: None,
                    hostfile: None,
                    interval: None,
                })
                .await;
            }
            // If user declined sudo and chose remote monitoring,
            // they were given instructions and the function exits
        }
    }

    // Cleanup PowerMetricsManager on exit
    #[cfg(target_os = "macos")]
    shutdown_powermetrics_manager();
}

// Set up a panic handler to ensure cleanup
#[cfg(target_os = "macos")]
fn setup_panic_handler() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Clean up PowerMetricsManager before panicking
        device::powermetrics_manager::shutdown_powermetrics_manager();
        default_panic(panic_info);
    }));
}
