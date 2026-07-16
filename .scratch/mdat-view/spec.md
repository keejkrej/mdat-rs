# Spec — `mdat view` (v1)

## Problem Statement

Microscopy users of `mdat` today produce multi-dimensional datasets (ND2, CZI, or the `mdat convert` TIFF-series layout under `out/PosN/`) but have no quick way to *look* at them on their local machine. Inspecting a run means either firing up a heavyweight commercial viewer, dropping files into Fiji/ImageJ by hand, or scripting one-off plots. Users need a single command — `mdat view <path>` — that boots a local server, opens a browser tab, and lets them scrub through positions, timepoints, Z-slices, and channels with per-channel color/contrast, pixel inspection, and metadata — without leaving the terminal workflow they already use for `mdat convert`.

## Solution

`mdat view` is a self-contained, single-process HTTP server (Rust + axum) backed by the existing `ReaderAdapter`/`ReaderSession` seam, serving plane pixels and dataset metadata as a small JSON+binary API to a Solid + Vite + Tailwind browser client rendered on an HTML5 canvas. The browser is the only client. A single 16-byte random token gates every URL (`/<token>/...`) uniformly on both Tailscale-wide (`0.0.0.0`) and localhost binds, so the server can be exposed over Tailscale to a laptop without extra auth. The frontend composites all enabled channels into one RGBA buffer via additive blend, tinted by each channel's metadata color, and supports Z/T scrubbing with a small JS plane cache and prefetch ring, per-channel visibility/color/contrast, keyboard navigation, and an idle-timeout-driven shutdown so users don't leave orphan servers running.

## User Stories

1. As a microscopy user, I want to run `mdat view <path>` and have a browser tab open to my dataset, so that I can start looking at data with one command.
2. As a microscopy user, I want to point `mdat view` at an ND2 file, so that I can view raw instrument output without converting first.
3. As a microscopy user, I want to point `mdat view` at a CZI file, so that I can view raw instrument output without converting first.
4. As a microscopy user, I want to point `mdat view` at an `out/` directory produced by `mdat convert`, so that I can browse a multi-position converted dataset.
5. As a microscopy user, I want to point `mdat view` at a single `PosN/` directory, so that I can view one position's TIFF series.
6. As a microscopy user, I want to point `mdat view` at a single `.tif`, so that I can quickly look at one frame.
7. As a user on a machine with Tailscale, I want the server to bind to `0.0.0.0` automatically so a colleague on Tailscale can open the URL from their laptop.
8. As a user on a machine without Tailscale, I want the server to bind to `127.0.0.1` automatically, so nothing is exposed to the LAN.
9. As a user who forgot to close the viewer, I want the server to shut itself down after 30 minutes of inactivity, so I don't leave orphan processes.
10. As a user who scripts `mdat view`, I want `--idle-timeout <min>` to override the 30-minute default, so I can set longer or shorter lifetimes in automation.
11. As a user who scripts `mdat view`, I want `--no-open`, so the server boots without trying to open a browser (for headless/remote use).
12. As a user, I want the actual bound URL printed to stdout, so I can copy-paste it or pipe it elsewhere.
13. As a user, I want Ctrl-C to shut the server down gracefully, so in-flight plane requests finish and the port is released cleanly.
14. As a user, I want every request gated by a token I don't have to manage, so that someone who can reach my port but doesn't have the URL can't see my data.
15. As a user, I want a wrong-token request to return a 404, so the gated path's existence isn't leaked.
16. As a viewer user, I want all enabled channels composited into one image, so I can see colocalization the way I think about it.
17. As a viewer user, I want each channel tinted by its metadata color by default, so I don't have to assign colors manually to get a sensible image.
18. As a viewer user, I want to toggle each channel's visibility, so I can isolate channels.
19. As a viewer user, I want to change a channel's color, so I can override the metadata default.
20. As a viewer user, I want to set per-channel contrast min/max, so I can bring out weak signal.
21. As a viewer user, I want a single-channel (grayscale) view to just work, so I don't have to think about "compositing" when there's only one channel.
22. As a viewer user, I want to auto-contrast the current frame across all enabled channels, so weak signal becomes visible without manual sliders.
23. As a viewer user, I want auto-contrast computed client-side from the pixels I already fetched, so it's instant and needs no extra server round-trip.
24. As a viewer user, I want to scrub the time axis with Left/Right, so I can watch a time-lapse.
25. As a viewer user, I want to scrub the Z axis with Up/Down (Up=Z-1, Down=Z+1), so I can move through a Z-stack.
26. As a viewer user, I want keys 1-9 to toggle channels, so I can toggle visibility without reaching for the mouse.
27. As a viewer user, I want `a` to auto-contrast, `f` to fit-to-window, and `+`/`-` to zoom, so common actions are one keypress.
28. As a viewer user, I want zoom centered on the cursor when hovering, else centered, so I can zoom into a region of interest.
29. As a viewer user, I want adjacent T (±1) and Z (±1) planes for enabled channels prefetched, so scrubbing feels responsive.
30. As a viewer user, I want a small in-memory plane cache so revisiting a recent plane is instant.
31. As a viewer user, I want a position dropdown, so I can switch between positions in a multi-position dataset.
32. As a viewer user, I want selecting a new position to reset T=0 and Z=0 but keep my channel visibility/color/contrast, so my display setup survives a position switch.
33. As a viewer user, I want the plane cache cleared on a position switch, so I never show a stale plane from the previous position.
34. As a viewer user, I want a pixel inspector, so I can read the raw u16 value(s) at a pixel across enabled channels.
35. As a viewer user, I want a metadata panel, so I can see dataset dims, channel names, and the normalized (and raw, when available) metadata.
36. As a viewer user, I want `t_real` exposed from `time_map.csv`, so I can see wall-clock times alongside the canonical time index.
37. As a viewer user, I want out-of-range plane indices to return a clear 400 error, so the frontend can show a clean message rather than a crash.
38. As a viewer user, I want a read failure to return a 500 with `{error, frame}` JSON, so the frontend can surface which frame failed.
39. As a viewer user, I want a fetch failure to show "the server may have exited — check the terminal" with a Reload button that re-fetches `/dataset`, so a dead server is obvious and recoverable.
40. As a viewer user, I want other errors shown as a centered overlay with the message, so problems are visible without leaving the viewer.
41. As a developer building `mdat view`, I want the frontend to be a single embedded static bundle in the release binary, so I can ship one file.
42. As a developer iterating on `mdat view`, I want the Vite dev server (5173) to proxy API calls to axum (8787) with HMR, so I can change frontend and Rust without restarting everything.
43. As a developer running `cargo check` or `cargo test`, I want the build to *not* require Node, so plain Rust workflows stay fast and dependency-free.
44. As a developer who hasn't built the frontend, I want the server to serve a "build the frontend first" placeholder instead of crashing, so the binary still runs.
45. As a developer, I want `build.rs` to run the frontend build only when `web/dist/index.html` is missing or stale, so incremental Rust builds stay fast.

## Implementation Decisions

### Invocation & CLI
- `mdat view <path>` is a new subcommand of the `mdat` CLI (structural choice of *where* the code lives — new `crates/mdat-view` workspace member vs. modules under `crates/mdat` — is left to the ticket breakdown).
- `<path>` accepts: ND2 file, CZI file, `out/` root (multi-pos), `PosN/` dir (single-pos), or single `.tif`.
- Flags: `--idle-timeout <min>` (default 30), `--no-open`. Bind is auto: Tailscale-first `0.0.0.0`, else `127.0.0.1`.

### Backend
- Rust + axum. One `ReaderSession` held for the dataset's lifetime behind the server state.
- Reuses `nd2-rs`, `czi-rs`, and the existing `ReaderAdapter` / `ReaderSession` seam (`ImageInfo { n_pos, n_time, n_chan, n_z }`, `width`, `height`, `read_frame(p,t,c,z) -> Result<Vec<u16>>`).
- A new TIFF-series `ReaderAdapter` is registered alongside ND2/CZI.
- `resolve_reader_adapter` gains a **directory dispatch branch *before* the suffix check**: if the path is a directory matching the TIFF-series layout (or contains `PosN/` children), the TIFF-series adapter handles it. The existing suffix-keyed ND2/CZI branches remain for file inputs.
- `ensure_input_exists` is relaxed for directory inputs (accepts `is_dir()` in addition to `is_file()`).

### TIFF-series Adapter
- Accepts `out/` root, `PosN/` dir, or a single `.tif`.
- Dims reconstructed by globbing the canonical `mdat convert` filename pattern `img_channel{c:03}_position{p:03}_time{t:09}_z{z:03}.tif` and parsing the three numeric fields; `n_pos`/`n_time`/`n_chan`/`n_z` = max observed index + 1 per axis.
- `time_map.csv` (cols `t,t_real`) under `PosN/` is read to expose `t_real` in the normalized metadata. The filename's `t_new` field is the canonical time address passed to `read_frame`.
- `read_frame` lazily opens and caches the matching `.tif` per call (cache scoped to the session).
- **Strict mdat-filename match only.** Zero matches → `MdatError::UnsupportedFormat` naming the expected pattern. Generic OME-TIFF, ACDC layout, and arbitrary TIFF folders are explicitly *not* supported in v1.

### Data API
- `GET /<token>/dataset` → one load-time round-trip returning:
  ```
  { name: String, info:{n_pos,n_time,n_chan,n_z}, width, height,
    channels:[{index,name,color}],
    metadata:{normalized, raw?, raw_format?} }
  ```
  `name` is the dataset's display name (derived from the input path's file/dir name). `channels[].color` and `channels[].name` are seeded from the normalized metadata (per-channel `color`, `name`/`fluor`).
- `GET /<token>/plane?p=&t=&c=&z=` → `application/octet-stream`, `width*height*2` bytes, u16 little-endian. Defaults: `p=t=z=0`, `c=0`. No flat sequence index; the 4D address maps 1:1 to `ReaderSession::read_frame`.
- Out-of-range indices → `400` JSON `{error}`. Read failures → `500` JSON `{error, frame}`.
- **No server-side histogram endpoint.** Auto-contrast is a client-side percentile pass over the already-fetched `Uint16Array`.

### Frontend / Render
- Solid + Vite + Tailwind v4; HTML5 canvas; single-plane fetch (no tiling).
- **Composite color overlay:** all enabled channels composited into one RGBA buffer via additive blend; each channel tinted by its per-channel color (seeded from metadata, editable in UI). Grayscale is the degenerate "exactly one channel enabled" case.
- Per-channel UI state: visibility toggle, user-assigned color, contrast min/max.
- Plane cache in JS keyed by `(p,t,c,z)`, LRU size 256.
- Stack reference (house style): Lisca — Solid + Vite + Tailwind v4, Kobalte, phosphor-icons-solid, @tanstack/solid-router, vite-plugin-solid.

### Lifecycle
- Single-file, single-process. No multi-file server, no PID file, no port reuse.
- Binds a free port (`*:0`); prints the actual URL to stdout.
- **Idle timeout counts `/plane` + `/dataset` activity only** (default 30 min, `--idle-timeout`). On expiry, prints "shutting down (idle)" and exits.
- Ctrl-C → graceful shutdown via axum `with_graceful_shutdown`.
- No websocket, no tab-close detection.
- Browser open is best-effort and non-fatal: try `xdg-open` / `open` / `cmd /c start`; skip if the binary is missing or `--no-open` is set.

### Auth
- Token **always on**, uniform `/<token>/...` path scheme on both Tailscale and localhost binds.
- 16 random bytes, base64url-encoded (22 chars), generated once at startup, kept in process memory only, never logged to stdout (the stdout URL includes it; that's the only disclosure).
- Wrong token → **404** (no 401; does not leak the gated path).
- No rate-limiting, no bind-logging in v1. The token is the entire auth model.

### Bundling / Dev Loop
- **Release:** Vite builds `web/dist/`; embedded into the binary via `include_dir`. Single static binary. **Implementation decision:** `build.rs` never invokes `bun`/`npm` — it only writes a placeholder `index.html` when `web/dist` is missing. The developer runs `bun run build` manually before `cargo build --release` to embed the real bundle. This keeps Rust builds Node-free (the spec's hard constraint) at the cost of a two-step release workflow.
- **Dev:** Vite dev server (5173) with `server.proxy` forwarding `/<token>/dataset` and `/<token>/plane` to axum (8787); HMR against the live Rust API.
- `cargo check` / `cargo test` never require Node. Absent bundle → server serves a "build the frontend first" placeholder instead of failing.

### UX Details
- **Scrubbing:** debounce-on-change ~50ms + prefetch ring of adjacent T (±1) and Z (±1) planes for currently enabled channels.
- **Position nav:** single dropdown. Selecting a position resets T=0, Z=0; **keeps** channel visibility/contrast/color; **clears** the JS plane cache.
- **Keyboard (v1):** Left/Right = T∓1; Up/Down = Z∓1 (Up=Z−1, Down=Z+1); 1-9 = toggle channel N visibility; `a` = auto-contrast current frame across all enabled channels; `f` = fit-to-window; `+`/`-` = zoom (centered on cursor if hovering, else image center).
- **No `/shutdown` endpoint in v1** — Ctrl-C or idle timeout only.
- **Errors:** centered overlay with the message. On network/fetch failure: "the server may have exited — check the terminal" hint + a Reload button (re-fetches `/dataset`); no "open another file". Server returns `400 {error}` for out-of-range indices, `500 {error,frame}` for read failures.

## Testing Decisions

### What makes a good test here
- Test **external behavior** through the highest available seam, not implementation details. The highest seam for the backend is the HTTP API (`/<token>/dataset`, `/<token>/plane`) exercised against a real (small) `ReaderSession`; the highest seam for the TIFF-series adapter is its `ReaderAdapter` trait surface (`inspect`, `inspect_metadata`, `open` → `read_frame`) against a fixture tree of `mdat convert`-named TIFFs.
- Avoid asserting on internal caches, glob internals, or axum wiring that isn't observable through responses.
- Frontend is exercised via the webapp-testing skill (Playwright) against the dev-server or the embedded bundle for smoke-level interactions (load dataset, toggle channel, scrub, auto-contrast). Pixel-perfect canvas assertions are *not* required; behavioral assertions (URLs fetched, overlay shown on error, cache cleared on position switch) are.

### Modules to test
- **TIFF-series adapter**: glob/parsing correctness, dim reconstruction (`n_*` = max index + 1), `time_map.csv` parsing → `t_real` in metadata, zero-match → `UnsupportedFormat` with the expected pattern named, `read_frame` returns the right bytes for a given `(p,t,c,z)`, lazy open/cache behavior observable via repeated reads of the same plane.
- **Directory dispatch in `resolve_reader_adapter`**: an `out/` root and a `PosN/` dir resolve to the TIFF-series adapter; a single `.tif` resolves to the TIFF-series adapter; ND2/CZI file suffixes still resolve to their adapters; an unknown directory / unknown suffix → `UnsupportedFormat`. `ensure_input_exists` accepts directories.
- **HTTP API**: `/dataset` shape matches the contract (including `channels[].color`/`name` seeded from metadata); `/plane` returns `width*height*2` bytes of little-endian u16 for valid `(p,t,c,z)`; defaults applied when query params omitted; 400 on out-of-range; 500 on a forced read failure; wrong token → 404 (not 401); correct token → 200.
- **Idle timeout**: server exits after the configured idle window when only idle (no `/plane`+`/dataset` traffic); a request inside the window keeps it alive (observable via process lifecycle in a test harness).
- **Token**: generated once, 22 base64url chars, present in every served URL, never disclosed except via the stdout URL line.
- **Frontend (smoke, Playwright)**: dataset loads, channel toggle changes the fetched plane set, position switch clears cache (observable by network panel or a second load of the same plane re-fetching), auto-contrast key runs, error overlay appears on a forced 500.

### Prior art in the codebase
- Existing `ReaderAdapter` / `ReaderSession` trait tests and `mdat convert` output tests (where the `img_channel{c:03}_position{p:03}_time{t_new:09}_z{z:03}.tif` filename contract is established) are the reference for the TIFF-series adapter's fixture layout. Reuse the same fixture-generation helper if one exists; otherwise mirror its naming exactly.
- `resolve_reader_adapter` / `ensure_input_exists` already have call sites to mirror in tests.

## Out of Scope

- **3D rendering / volume view** — v1 is 2D single-plane only.
- **Export** (PNG/TIFF/snapshot from the viewer).
- **Deconvolution** or any in-plane processing beyond color/contrast compositing.
- **Tiling** or pyramidal / multi-resolution serving — single-plane fetch only.
- **Server-side histograms** — auto-contrast is purely client-side.
- **Multi-file server** — one `mdat view` invocation serves exactly one dataset.
- **Generic OME-TIFF, ACDC layout, or arbitrary TIFF folder support** — v1 strictly matches the `mdat convert` filename pattern; anything else is `UnsupportedFormat`.
- **`/shutdown` endpoint** — shutdown is via Ctrl-C or idle timeout only.
- **Rate-limiting / bind-logging** — not in v1; the token is the entire auth surface.
- **Websocket / tab-close detection** — the server cannot detect a closed tab; idle timeout covers lifecycle.
- **"Open another file" from the UI** — the viewer is bound to the one dataset it was launched with.

## Further Notes

- **Tailscale detection** determines bind address: if a Tailscale interface is present, bind `0.0.0.0`; otherwise `127.0.0.1`. The chosen bind should be cheap to detect and not require root.
- **Plane wire format** is fixed: little-endian u16, row-major, `width*height*2` bytes, no header. The frontend reads it directly into a `Uint16Array` via `Response.arrayBuffer()`. Changing this is a coordinated frontend+backend change.
- **`t_new` vs `t`**: the filename's `t_new` field (zero-padded to 9 digits in `mdat convert` output) is the canonical time address for `read_frame`. `time_map.csv`'s `t` column indexes the same space; **`t_real` in the CSV is actually the original integer time index (`t_orig`), not a wall-clock time** — the adapter exposes it as `t_orig` in normalized metadata (integer, not float). The spec's earlier "wall-clock" framing was wrong; `mdat convert` writes an integer index, and the viewer surfaces it as such.
- **Per-channel color seeding** comes from the normalized metadata's per-channel `color` field (already produced by ND2/CZI adapters and absent for the TIFF-series adapter, which should seed a default palette when metadata has no color).
- **Structural choice deferred to tickets**: whether the new code lives as a `view` subcommand + modules inside `crates/mdat`, or as a new `crates/mdat-view` workspace member that depends on `crates/mdat`'s input seam. Both are viable; the ticket breakdown should pick one and keep the `ReaderAdapter`/`ReaderSession` contract unchanged.
- **Frontend stack alignment**: when tickets scaffold the frontend, match the house Lisca stack (Solid + Vite + Tailwind v4, Kobalte, phosphor-icons-solid, @tanstack/solid-router, vite-plugin-solid) so component patterns and iconography stay consistent.
- **Idle-timeout clock**: "activity" is defined as a completed `/plane` or `/dataset` request. A long-running but idle server still counts as idle. The clock should reset on each such request, not on connection open.