// build.rs for mdat-view.
//
// Ensure `include_dir!("$CARGO_MANIFEST_DIR/web/dist")` has something to embed.
// A committed placeholder `web/dist/index.html` is part of the package so
// crates.io verify never needs to write into the source tree (cargo publish
// forbids build.rs from modifying anything outside OUT_DIR).
//
// Do NOT run bun/npm here — keep `cargo check`/`cargo test` Node-free. A real
// frontend bundle is built explicitly via `cd web && bun run build`.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let dist_index = manifest_dir.join("web/dist/index.html");

    println!("cargo:rerun-if-changed=web/src");
    println!("cargo:rerun-if-changed=web/dist");

    if !dist_index.exists() {
        // Fail loudly in clean checkouts that lack the placeholder; do not
        // write files (cargo publish --verify would reject that).
        panic!(
            "mdat-view: missing {}; commit the placeholder or run the frontend build",
            dist_index.display()
        );
    }
}