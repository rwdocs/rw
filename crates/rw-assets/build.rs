fn main() {
    // Only build frontend when embed feature is enabled
    #[cfg(feature = "embed")]
    {
        use std::path::Path;
        use std::process::Command;

        fn run_npm(args: &[&str], cwd: &Path) -> std::io::Result<std::process::Output> {
            #[cfg(target_os = "windows")]
            {
                Command::new("cmd")
                    .args(["/C", "npm"])
                    .args(args)
                    .current_dir(cwd)
                    .output()
            }
            #[cfg(not(target_os = "windows"))]
            {
                Command::new("npm").args(args).current_dir(cwd).output()
            }
        }

        let frontend_dir = Path::new("../../frontend");

        // Install dependencies if node_modules doesn't exist
        if !frontend_dir.join("node_modules").exists() {
            let install = run_npm(&["ci"], frontend_dir).expect("failed to run npm ci");
            assert!(
                install.status.success(),
                "failed to install frontend dependencies:\n{}",
                std::str::from_utf8(&install.stderr).unwrap()
            );
        }

        // Build frontend assets
        let output =
            run_npm(&["run", "build"], frontend_dir).expect("failed to build the frontend");
        assert!(
            output.status.success(),
            "failed to build frontend:\n{}",
            std::str::from_utf8(&output.stderr).unwrap()
        );

        // Rebuild if frontend files or build script changes
        println!("cargo:rerun-if-changed=../../frontend");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
