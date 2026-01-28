//! RW CLI - Documentation engine.
//!
//! Provides commands for:
//! - `serve`: Start the documentation server
//! - `confluence update`: Update Confluence pages from markdown
//! - `confluence generate-tokens`: Generate OAuth access tokens

mod commands;
mod error;
mod output;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use commands::{ConfluenceCommand, ServeArgs};
use output::Output;

/// Application version from Cargo.toml.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// RW - Documentation engine.
#[derive(Parser)]
#[command(name = "rw", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the documentation server.
    Serve(ServeArgs),
    /// Confluence publishing commands.
    #[command(subcommand)]
    Confluence(ConfluenceCommand),
}

fn main() {
    let cli = Cli::parse();
    let output = Output::new();

    // Check if verbose flag is set for serve command
    let verbose = matches!(&cli.command, Commands::Serve(args) if args.verbose);

    // Initialize tracing with appropriate log level
    // --verbose enables INFO level, otherwise use RUST_LOG or default to WARN
    let filter = if verbose {
        EnvFilter::new("info")
    } else {
        EnvFilter::from_default_env()
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let result = match cli.command {
        Commands::Serve(args) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(args.execute(VERSION))
        }
        Commands::Confluence(cmd) => cmd.execute(),
    };

    if let Err(err) = result {
        output.error(&format!("Error: {err}"));
        std::process::exit(1);
    }
}
