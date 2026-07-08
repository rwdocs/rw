//! `rw update` command implementation.
//!
//! Self-updates the `rw` binary via the `axoupdater` library, using the install
//! receipt written by the cargo-dist shell/PowerShell installer.

use axoupdater::{AxoUpdater, AxoupdateError, UpdateRequest};
use clap::Args;

use crate::error::CliError;
use crate::output::Output;

/// Arguments for `rw update`.
#[derive(Args)]
pub(crate) struct UpdateArgs {
    /// Check whether an update is available without installing it.
    #[arg(long)]
    pub check: bool,

    /// Install a specific version (e.g. 0.1.28) instead of the latest.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,

    /// Include pre-releases when resolving the latest version.
    #[arg(long)]
    pub prerelease: bool,
}

impl UpdateArgs {
    /// Translate the flags into an axoupdater target. An explicit `--version`
    /// wins over `--prerelease`; otherwise `--prerelease` widens "latest".
    fn update_request(&self) -> UpdateRequest {
        if let Some(version) = &self.version {
            UpdateRequest::SpecificVersion(version.clone())
        } else if self.prerelease {
            UpdateRequest::LatestMaybePrerelease
        } else {
            UpdateRequest::Latest
        }
    }

    /// Run the update (or, with `--check`, just report availability).
    pub(crate) fn execute(self, current_version: &str) -> Result<(), CliError> {
        let output = Output::new();
        let report_up_to_date =
            || output.success(&format!("rw is already up to date ({current_version})"));

        let mut updater = AxoUpdater::new_for("rw");
        // No install receipt means this install isn't eligible for self-update
        // (Homebrew, npm, `cargo install`, source builds) — surface the friendly
        // guidance. Any *other* receipt error (corrupt receipt, no home dir) is a
        // genuine failure and is propagated as-is rather than masked.
        match updater.load_receipt() {
            Ok(_) => {}
            Err(AxoupdateError::NoReceipt { .. }) => return Err(CliError::CantSelfUpdate),
            Err(err) => return Err(err.into()),
        }
        updater.configure_version_specifier(self.update_request());

        output.info("Checking for updates…");

        if self.check {
            if updater.is_update_needed_sync()? {
                // Re-run the same command minus `--check` so any
                // `--version`/`--prerelease` the user passed still applies.
                output.info("An update is available. Re-run without `--check` to install it.");
            } else {
                report_up_to_date();
            }
            return Ok(());
        }

        match updater.run_sync()? {
            Some(result) => output.success(&format!("Updated rw to {}", result.new_version)),
            None => report_up_to_date(),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// A minimal `Parser` wrapper so `UpdateArgs` (which derives `Args`, not
    /// `Parser`) can be exercised via `try_parse_from`.
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: UpdateArgs,
    }

    fn parse(argv: &[&str]) -> UpdateArgs {
        TestCli::try_parse_from(argv).expect("args parse").args
    }

    #[test]
    fn defaults_to_latest() {
        let args = parse(&["rw"]);
        assert!(!args.check);
        assert!(!args.prerelease);
        assert_eq!(args.version, None);
        assert!(matches!(args.update_request(), UpdateRequest::Latest));
    }

    #[test]
    fn check_flag_parses() {
        assert!(parse(&["rw", "--check"]).check);
    }

    #[test]
    fn prerelease_maps_to_latest_maybe_prerelease() {
        let args = parse(&["rw", "--prerelease"]);
        assert!(matches!(
            args.update_request(),
            UpdateRequest::LatestMaybePrerelease
        ));
    }

    #[test]
    fn version_maps_to_specific_version() {
        let args = parse(&["rw", "--version", "0.1.28"]);
        assert_eq!(args.version.as_deref(), Some("0.1.28"));
        assert!(matches!(
            args.update_request(),
            UpdateRequest::SpecificVersion(v) if v == "0.1.28"
        ));
    }

    #[test]
    fn version_wins_over_prerelease() {
        let args = parse(&["rw", "--version", "0.1.28", "--prerelease"]);
        assert!(matches!(
            args.update_request(),
            UpdateRequest::SpecificVersion(v) if v == "0.1.28"
        ));
    }

    #[test]
    fn check_is_orthogonal_to_version_target() {
        // `--check` must not conflict with a version specifier: it reports
        // availability of the *targeted* version, so both flags apply together.
        let args = parse(&["rw", "--check", "--version", "0.1.28"]);
        assert!(args.check);
        assert!(matches!(
            args.update_request(),
            UpdateRequest::SpecificVersion(v) if v == "0.1.28"
        ));
    }
}
