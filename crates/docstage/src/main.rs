//! Docstage CLI - Documentation engine for Backstage.
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

/// Docstage - Where documentation takes the stage.
#[derive(Parser)]
#[command(name = "docstage", version, about)]
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
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let output = Output::new();

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
