//! Mock server module for all-smi
//!
//! This module provides a high-performance mock server that simulates
//! realistic GPU clusters with multiple nodes, each containing multiple GPUs.

pub mod args;
pub mod constants;
pub mod generator;
pub mod metrics;
pub mod node;
pub mod server;
pub mod template;

pub use args::Args;
pub use server::start_servers;
