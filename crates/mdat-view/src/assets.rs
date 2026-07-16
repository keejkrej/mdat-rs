//! Static-asset serving for `mdat view`.
//!
//! Two sources, picked at compile time:
//!   - [`AssetSource::Embedded`] - the real frontend bundle produced by
//!     `cd web && bun run build`, embedded into the binary via `include_dir!`.
//!   - [`AssetSource::Placeholder`] - a static "build the frontend first"
//!     HTML page, used when no bundle has been built (so `cargo check` /
//!     `cargo test` never need Node, and a binary without a bundle still
//!     boots and serves the API).
//!
//! The server (#3) mounts [`asset_router`] (a `Router<()>`) at the non-token
//! root `/` so the API at `/<token>/...` and the static bundle at `/`
//! coexist on one router. The asset source is captured in closures, so the
//! returned router has no axum state of its own and merges cleanly into a
//! parent router that carries `ServerState`.

use axum::body::Body;
use axum::extract::Path as AxumPath;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use include_dir::Dir;
use std::sync::Arc;

/// Where the static frontend bundle comes from.
pub enum AssetSource {
    /// Embedded at compile time from `web/dist` (real frontend bundle).
    Embedded(&'static Dir<'static>),
    /// No bundle built yet - serve a "build the frontend first" page.
    Placeholder,
}

/// The placeholder shown when no frontend bundle has been built.
const PLACEHOLDER_HTML: &str = include_str!("placeholder.html.in");

/// A single embedded asset file, returned to the browser.
struct EmbeddedFile {
    mime: &'static str,
    bytes: &'static [u8],
}

/// Map a file extension to a static MIME type. Covers the asset types a
/// Vite/Solid/Tailwind build emits; unknown extensions fall back to octet.
fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "map" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Heuristic: does this path look like a gated API path (`/<seg>/dataset` or
/// `/<seg>/plane`)? These must 404 when the token doesn't match, never serve
/// the SPA shell.
fn is_api_path(clean: &str) -> bool {
    clean.ends_with("/dataset") || clean.ends_with("/plane")
}

impl AssetSource {
    /// Look up a path within the embedded bundle. Returns `None` if the
    /// source is the placeholder or the path isn't in the bundle.
    fn get(&self, req_path: &str) -> Option<EmbeddedFile> {
        match self {
            AssetSource::Embedded(dir) => {
                let clean = req_path.trim_start_matches('/');
                if clean.is_empty() {
                    return dir
                        .get_file("index.html")
                        .map(|f| embed_file("index.html", f.contents()));
                }
                if let Some(f) = dir.get_file(clean) {
                    return Some(embed_file(clean, f.contents()));
                }
                // SPA fallback: a non-asset path that isn't a gated API path
                // serves the app shell (so `/<token>/` boots the app). Gated
                // API paths (`/<seg>/dataset`, `/<seg>/plane`) that didn't
                // match a real token route fall through to a 404 here.
                if !looks_like_asset(clean) && !is_api_path(clean) {
                    return dir
                        .get_file("index.html")
                        .map(|f| embed_file("index.html", f.contents()));
                }
                None
            }
            AssetSource::Placeholder => {
                // Only the root is served from the placeholder; everything
                // else 404s so the API paths stay clean.
                if req_path == "/" || req_path.is_empty() {
                    Some(EmbeddedFile {
                        mime: "text/html; charset=utf-8",
                        bytes: PLACEHOLDER_HTML.as_bytes(),
                    })
                } else {
                    None
                }
            }
        }
    }
}

fn embed_file(path: &str, bytes: &'static [u8]) -> EmbeddedFile {
    let ext = path.rsplit('.').next().unwrap_or("");
    EmbeddedFile {
        mime: mime_for_ext(ext),
        bytes,
    }
}

/// Heuristic: does this path look like a static asset (vs an SPA route)?
fn looks_like_asset(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "mjs" | "css" | "html" | "json" | "svg" | "png" | "jpg" | "jpeg" | "gif"
            | "ico" | "woff" | "woff2" | "ttf" | "eot" | "map" | "webp" | "avif"
    )
}

fn serve(source: &AssetSource, path: &str) -> Response {
    match source.get(path) {
        Some(file) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, file.mime)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(file.bytes))
            .unwrap(),
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            "not found",
        )
            .into_response(),
    }
}

/// Build a stateless router (`Router<()>`) that serves the static bundle at
/// `/` and falls back to the app shell (or placeholder) for unknown non-asset
/// paths. The [`AssetSource`] is captured in closures, so this router merges
/// cleanly into a parent router that carries a different state type.
///
/// This router is designed to be installed as the **fallback** of the main
/// router (see [`crate::asset_fallback`]): registered API routes (e.g.
/// `/<token>/dataset`) take priority; only unmatched paths reach here, where
/// we serve a static asset, the SPA shell, the placeholder, or a 404.
pub fn asset_router(source: AssetSource) -> Router<()> {
    asset_router_with(Arc::new(source))
}

/// Same as [`asset_router`] but takes an already-Arc'd source.
pub fn asset_router_with(source: Arc<AssetSource>) -> Router<()> {
    let root = source.clone();
    let path_src = source.clone();
    Router::new()
        .route(
            "/",
            get(move || async move { serve(&root, "/") }),
        )
        .route(
            "/assets/{*path}",
            get(move |AxumPath(path): AxumPath<String>| async move {
                // The route strips the `/assets/` prefix; restore it so the
                // embedded-dir lookup finds `assets/index-*.js` etc.
                let full = format!("assets/{path}");
                serve(&path_src, &full)
            }),
        )
}

/// A fallback handler that serves a static asset for the requested path, the
/// SPA shell (`index.html`) for non-asset paths, the placeholder, or a 404.
/// Mount this as the router's `fallback` so API routes take priority.
pub fn asset_fallback(source: Arc<AssetSource>) -> axum::routing::MethodRouter<()> {
    let s = source.clone();
    axum::routing::any(move |req: axum::http::Request<Body>| {
        let s = s.clone();
        async move {
            let path = req.uri().path().to_string();
            serve(&s, &path)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_serves_root_only() {
        let s = AssetSource::Placeholder;
        assert!(s.get("/").is_some());
        assert!(s.get("").is_some());
        assert!(s.get("foo.js").is_none());
    }

    #[test]
    fn looks_like_asset_recognizes_static_extensions() {
        assert!(looks_like_asset("app.js"));
        assert!(looks_like_asset("style.css"));
        assert!(looks_like_asset("logo.svg"));
        assert!(!looks_like_asset("dataset"));
        assert!(!looks_like_asset("plane"));
        assert!(!looks_like_asset("abc123def456")); // token-ish
    }
}