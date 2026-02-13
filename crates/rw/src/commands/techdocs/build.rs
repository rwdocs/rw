//! `rw techdocs build` command implementation.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use rw_config::{CliSettings, Config};
use rw_site::PageRendererConfig;
use rw_storage::Storage;
use rw_storage_fs::FsStorage;
use rw_techdocs::{BuildConfig, StaticSiteBuilder};

use crate::error::CliError;
use crate::output::Output;

/// Arguments for the techdocs build command.
#[derive(Args)]
pub(crate) struct BuildArgs {
    /// Output directory for the generated site (default: .rw/techdocs/build/).
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    /// Markdown source directory (overrides config).
    #[arg(short, long)]
    source_dir: Option<PathBuf>,

    /// Kroki server URL for diagram rendering (overrides config).
    #[arg(long)]
    kroki_url: Option<String>,

    /// Site name for techdocs_metadata.json.
    #[arg(long)]
    site_name: Option<String>,

    /// Disable caching.
    #[arg(long)]
    no_cache: bool,

    /// Path to configuration file (default: auto-discover rw.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl BuildArgs {
    pub(crate) fn execute(self) -> Result<(), CliError> {
        let output = Output::new();

        let cli_settings = CliSettings {
            source_dir: self.source_dir.clone(),
            kroki_url: self.kroki_url.clone(),
            cache_enabled: self.no_cache.then_some(false),
            ..CliSettings::default()
        };
        let config = Config::load(self.config.as_deref(), Some(&cli_settings))?;

        let output_dir = self
            .output_dir
            .unwrap_or_else(|| config.docs_resolved.project_dir.join("techdocs/build"));

        output.info(&format!(
            "Source: {}",
            config.docs_resolved.source_dir.display()
        ));
        output.info(&format!("Output: {}", output_dir.display()));

        // Create storage
        let meta_filename = &config.metadata.name;
        let storage: Arc<dyn Storage> = Arc::new(FsStorage::with_meta_filename(
            config.docs_resolved.source_dir.clone(),
            meta_filename,
        ));

        let site_name = self.site_name.unwrap_or_else(|| "Documentation".to_owned());

        let build_config = BuildConfig {
            site_name,
            css_content: None,
        };

        let builder = StaticSiteBuilder::new(storage, build_config).with_renderer_config(
            PageRendererConfig {
                extract_title: true,
                kroki_url: config.diagrams_resolved.kroki_url.clone(),
                include_dirs: config.diagrams_resolved.include_dirs.clone(),
                dpi: config.diagrams_resolved.dpi,
                relative_links: true,
                trailing_slash: true,
            },
        );

        builder.build(&output_dir)?;

        output.success(&format!(
            "Site built successfully to {}",
            output_dir.display()
        ));
        Ok(())
    }
}
