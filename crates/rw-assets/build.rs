fn main() {
    #[cfg(feature = "embed")]
    {
        use std::path::Path;

        let dist = Path::new("../../frontend/dist");
        assert!(
            dist.exists(),
            "frontend/dist not found — run `make build` first"
        );

        // Rebuild if frontend files or build script changes
        println!("cargo:rerun-if-changed=../../frontend/dist");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
