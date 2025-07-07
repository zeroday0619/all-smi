mod api;
mod app_state;
mod cli;
mod device;
mod storage;
mod ui;
mod utils;
mod view;

use api::run_api_mode;
use clap::Parser;
use cli::{Cli, Commands};
use utils::{ensure_sudo_permissions, ensure_sudo_permissions_with_fallback};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Api(args)) => {
            ensure_sudo_permissions();
            run_api_mode(&args).await;
        }
        Some(Commands::View(args)) => {
            if args.hosts.is_none() && args.hostfile.is_none() {
                ensure_sudo_permissions();
            }
            view::run_view_mode(&args).await;
        }
        None => {
            let has_sudo = ensure_sudo_permissions_with_fallback();
            if has_sudo {
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
}
