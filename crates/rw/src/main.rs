//! RW CLI - Documentation engine.
//!
//! Provides commands for:
//! - `serve`: Start the documentation server
//! - `backstage publish`: Publish documentation bundles to S3 for Backstage
//! - `confluence render`: Render markdown into a Confluence-publishable bundle
//! - `comment`: Read and write comments directly against the local `SQLite` store
//! - `update`: Update rw to the latest release

mod commands;
mod error;
mod output;

use std::io::{self, IsTerminal};

use clap::{Parser, Subcommand};
use rw_error::error_chain;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;

use commands::{BackstageCommand, CommentCommand, ConfluenceCommand, ServeArgs, UpdateArgs};
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
    /// Update rw to the latest release.
    Update(UpdateArgs),
}

fn main() {
    let cli = Cli::parse();
    let output = Output::new();

    let verbose = matches!(&cli.command, Commands::Serve(args) if args.verbose);

    let filter = log_filter(std::env::var("RUST_LOG").ok().as_deref(), verbose);
    // Stdout carries command output, such as `confluence render --out -` XHTML.
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(io::stderr().is_terminal())
        .with_writer(io::stderr)
        .init();

    let result = match cli.command {
        Commands::Serve(args) => {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(args.execute(VERSION))
        }
        Commands::Backstage(cmd) => cmd.execute(),
        Commands::Confluence(cmd) => cmd.execute(),
        Commands::Comment(cmd) => cmd.execute(),
        Commands::Update(args) => args.execute(VERSION),
    };

    if let Err(err) = result {
        output.error(&format!("Error: {}", error_chain(&err)));
        std::process::exit(err.exit_code());
    }
}

/// `RUST_LOG` wins; without it, `-v` logs info and plain runs log warnings.
fn log_filter(rust_log: Option<&str>, verbose: bool) -> EnvFilter {
    let default = if verbose {
        LevelFilter::INFO
    } else {
        LevelFilter::WARN
    };
    EnvFilter::builder()
        .with_default_directive(default.into())
        .parse_lossy(rust_log.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_filter_defaults_to_warn_and_info_when_verbose() {
        assert_eq!(log_filter(None, false).to_string(), "warn");
        assert_eq!(log_filter(None, true).to_string(), "info");
    }

    #[test]
    fn log_filter_takes_rust_log_over_verbose() {
        assert_eq!(log_filter(Some("rw=debug"), true).to_string(), "rw=debug");
    }
}
