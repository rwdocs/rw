fn main() {
    // Only build frontend when embed-assets feature is enabled
    #[cfg(feature = "embed-assets")]
    {
        use std::path::Path;
        use std::process::Command;

        let frontend_dir = Path::new("../../frontend");

        // Install dependencies if node_modules doesn't exist
        if !frontend_dir.join("node_modules").exists() {
            let install = Command::new("npm")
                .current_dir(frontend_dir)
                .arg("ci")
                .output()
                .expect("failed to run npm ci");

            if !install.status.success() {
                panic!(
                    "failed to install frontend dependencies:\n{}",
                    std::str::from_utf8(&install.stderr).unwrap()
                );
            }
        }

        // Build frontend assets
        let output = Command::new("npm")
            .current_dir(frontend_dir)
            .arg("run")
            .arg("build")
            .output()
            .expect("failed to build the frontend");

        if !output.status.success() {
            panic!(
                "failed to build frontend:\n{}",
                std::str::from_utf8(&output.stderr).unwrap()
            );
        }

        // Rebuild if frontend files or build script changes
        println!("cargo:rerun-if-changed=../../frontend");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
