fn main() {
    #[cfg(feature = "embed")]
    {
        use std::path::Path;

        let dist = Path::new("../../packages/viewer/dist");
        assert!(
            dist.exists(),
            "packages/viewer/dist not found — run `make build` first"
        );

        // Rebuild if frontend files or build script changes
        println!("cargo:rerun-if-changed=../../packages/viewer/dist");
        println!("cargo:rerun-if-changed=build.rs");
    }
}
