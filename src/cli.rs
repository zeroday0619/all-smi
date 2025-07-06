use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run in API mode, exposing metrics in Prometheus format.
    Api(ApiArgs),
    /// Run in view mode, displaying a TUI. (default)
    View(ViewArgs),
}

#[derive(Parser)]
pub struct ApiArgs {
    /// The port to listen on for the API server.
    #[arg(short, long, default_value_t = 9090)]
    pub port: u16,
    /// The interval in seconds at which to update the GPU information.
    #[arg(short, long, default_value_t = 3)]
    pub interval: u64,
    /// Include the process list in the API output.
    #[arg(long)]
    pub processes: bool,
}

#[derive(Parser, Clone)]
pub struct ViewArgs {
    /// A list of host addresses to connect to for remote monitoring.
    #[arg(long, num_args = 1..)]
    pub hosts: Option<Vec<String>>,
    /// A file containing a list of host addresses to connect to for remote monitoring.
    #[arg(long)]
    pub hostfile: Option<String>,
    /// The interval in seconds at which to update the GPU information. If not specified, uses adaptive interval based on node count.
    #[arg(short, long)]
    pub interval: Option<u64>,
}
