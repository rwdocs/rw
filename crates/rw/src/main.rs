//! RW CLI - Documentation engine.
//!
//! Provides commands for:
//! - `serve`: Start the documentation server
//! - `backstage publish`: Publish documentation bundles to S3 for Backstage
//! - `confluence update`: Update Confluence pages from markdown
//! - `confluence generate-tokens`: Generate OAuth access tokens
//! - `comment`: Read and write comments directly against the local `SQLite` store

mod commands;
mod error;
mod output;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use commands::{BackstageCommand, CommentCommand, ConfluenceCommand, ServeArgs};
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
    /// Backstage documentation publishing.
    #[command(subcommand)]
    Backstage(BackstageCommand),
    /// Confluence publishing commands.
    #[command(subcommand)]
    Confluence(ConfluenceCommand),
    /// Read and write inline comments on project docs (for scripts and LLM agents).
    #[command(subcommand)]
    Comment(CommentCommand),
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
        Commands::Backstage(cmd) => cmd.execute(),
        Commands::Confluence(cmd) => cmd.execute(),
        Commands::Comment(cmd) => cmd.execute(),
    };

    if let Err(err) = result {
        output.error(&format!("Error: {err}"));
        std::process::exit(err.exit_code());
    }
}
