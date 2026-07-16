// build.rs for mdat-view.
//
// Goal: keep `cargo check` / `cargo test` fast and Node-free, while ensuring
// `include_dir!("web/dist")` in `assets.rs` always finds *something* to embed.
//
// Strategy:
//   - Emit `cargo:rerun-if-changed=web/src` so Cargo only re-runs build.rs when
//     frontend sources change. `cargo check` / `cargo test` do NOT touch
//     `web/src`, so they skip build.rs entirely after the first run.
//   - If `web/dist/index.html` is missing: write a tiny placeholder
//     `index.html` so `include_dir!` compiles. We do NOT run `bun`/`npm`
//     from build.rs — a real bundle must be built explicitly via
//     `cd web && bun run build` (or `npm run build`). This keeps Rust builds
//     dependency-free and the placeholder path matches the "build the
//     frontend first" requirement from the spec.
//   - If `web/dist/index.html` already exists (someone ran the frontend
//     build), we leave it alone — build.rs never clobbers a real bundle.
//
// Why not run `bun`/`npm` here? The spec requires `cargo check`/`cargo test`
// to never require Node. Running a package manager from build.rs would make
// every clean checkout depend on a working Node toolchain, which is exactly
// what the spec forbids. The gated approach (placeholder + explicit build)
// satisfies: release builds produce a real binary (after `bun run build`),
// dev/check/test builds produce a working binary with a placeholder page.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let web_dir = manifest_dir.join("web");
    let dist_dir = web_dir.join("dist");
    let dist_index = dist_dir.join("index.html");

    // Only re-run build.rs when frontend sources change. This is what keeps
    // `cargo check` / `cargo test` from re-triggering build.rs (and thus
    // touching the filesystem) on every invocation.
    println!("cargo:rerun-if-changed=web/src");
    // Also re-run if the dist itself changes (e.g. after a `bun run build`).
    println!("cargo:rerun-if-changed=web/dist");

    if !dist_index.exists() {
        if let Err(e) = std::fs::create_dir_all(&dist_dir) {
            // Don't fail the build over a missing placeholder — the embedded
            // path is behind a cfg and the runtime placeholder covers it.
            eprintln!("mdat-view: could not create web/dist: {e}");
            return;
        }
        let placeholder = "<!doctype html>\n\
<html lang=\"en\">\n\
<head><meta charset=\"UTF-8\"><title>mdat view — build the frontend first</title></head>\n\
<body>\n\
<h1>mdat view</h1>\n\
<p>The frontend bundle has not been built yet.</p>\n\
<p>Run <code>cd crates/mdat-view/web &amp;&amp; bun install &amp;&amp; bun run build</code>\n\
(or <code>npm install &amp;&amp; npm run build</code>), then rebuild the binary.</p>\n\
<p>Until then, the API at <code>/&lt;token&gt;/dataset</code> and\n\
<code>/&lt;token&gt;/plane</code> still works.</p>\n\
</body>\n\
</html>\n";
        if let Err(e) = std::fs::write(&dist_index, placeholder) {
            eprintln!("mdat-view: could not write placeholder web/dist/index.html: {e}");
        }
    }
}