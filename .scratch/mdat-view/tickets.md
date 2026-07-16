# Tickets: `mdat view` v1

A local single-process HTTP server + Solid browser viewer for microscopy datasets (ND2, CZI, `mdat convert` TIFF-series), behind a token-gated API. Reference spec: `.scratch/mdat-view/spec.md`.

Work the **frontier**: any ticket whose blockers are all done. Tickets #1 and #2 are done (reviewed + orchestrator fixes applied). #2a is the next unblocked ticket (independent, unblocks #3 and #4). After #2a lands, #3 and #4 can go out together (the API/token/URL shape is fixed by the spec, so #4 only needs #1's flags + bind shape, not #3's working endpoints).

**Structural choice (committed, per spec's deferred decision):** new `crates/mdat-view` workspace member (binary `mdat-view`, plus a `mdat_view` lib target). It depends on `crates/mdat`'s input seam (`ReaderAdapter` / `ReaderSession` / `resolve_reader_adapter` / `ensure_input_exists`) via the `mdat` crate's public API. The `mdat` CLI gains a thin `view <path>` subcommand that delegates to `mdat_view::run(args)` — no logic duplication, the server/frontend code stays in `mdat-view`, the `ReaderAdapter`/`ReaderSession` contract is untouched.

**Resolution of the cyclic-dep contradiction (reviewer finding):** `mdat → mdat-view` (for the subcommand) and `mdat-view → mdat` (for the input seam, needed by #3) is a cycle Cargo forbids. The cleanest fix is **extracting the input seam into a new `crates/mdat-input` crate** that both `mdat` and `mdat-view` depend on — see ticket #2a. #3 and #4 are blocked on #2a.

---

## Scaffold `mdat-view` crate + `view` subcommand + CLI flags + bind detection  ✓ DONE

**Blocked by:** None — can start immediately.

- [ ] New workspace member `crates/mdat-view` with a `mdat_view` lib target exposing `pub async fn run(args: ViewArgs) -> Result<()>` and a `mdat-view` binary that calls it. Workspace `Cargo.toml` updated; `mdat` (and the workspace) still build.
- [ ] `ViewArgs` struct: `path: PathBuf`, `idle_timeout: Option<u64>` (minutes), `no_open: bool`. Clap derive, mirroring `crates/mdat`'s existing subcommand style.
- [ ] `mdat view <path>` subcommand added to `crates/mdat`'s clap CLI; it constructs `ViewArgs` and calls `mdat_view::run(args)`. `--idle-timeout <min>` (default 30) and `--no-open` flags wired through clap. `--idle-timeout` parses minutes as a positive integer; invalid → clap error.
- [ ] Bind-address detection helper: cheap, non-root check for a Tailscale interface (parse `ip addr` / `ifconfig` output, or walk `std::net::TcpStream`-free sysinfo — pick the cheapest cross-platform approach). Returns `0.0.0.0` if Tailscale present, else `127.0.0.1`. Unit-tested with stubbed interface lists (no real Tailscale required).
- [ ] Free-port selection helper: bind a `TcpListener` to `bind_addr:0`, retrieve the assigned port, drop the listener, return `(bind_addr, port)`. (Used here just to print the candidate URL; real bind happens in #3.)
- [ ] `run()` stub: resolves bind address, picks a free port, prints `mdat view serving <dataset-name> at http://<bind>:<port>/<token>/` *would* serve (with a placeholder token note) and exits 0 with a TODO marker — no axum, no listener held, no browser open. The point is the CLI → detection → stdout path is observable.
- [ ] Token generator helper exists (16 random bytes → 22-char base64url) and is unit-tested for length/charset, even if not yet used by the stub. (Keeps #3 from re-plumbing.)
- [ ] **Smoke test:** `cargo run -p mdat-view -- <some path>` prints a URL line containing `http://` and the detected bind address; `mdat view --help` lists `--idle-timeout` and `--no-open`; `mdat view <path> --idle-timeout 5` parses without error. `cargo check -p mdat` and `cargo check -p mdat-view` both pass with no Node installed.

## TIFF-series `ReaderAdapter` + directory dispatch  ✓ DONE

**Blocked by:** None — can start immediately (independent of the CLI scaffold).

- [ ] New TIFF-series `ReaderAdapter` impl registered in `resolve_reader_adapter` alongside ND2/CZI. Implements `inspect`, `inspect_metadata`, `open` → `ReaderSession` with `ImageInfo { n_pos, n_time, n_chan, n_z }`, `width`, `height`, `read_frame(p,t,c,z) -> Result<Vec<u16>>` — matching the existing trait surface exactly, no contract changes.
- [ ] Directory dispatch branch in `resolve_reader_adapter` runs **before** the suffix check: if the path is a directory matching the TIFF-series layout (an `out/` root containing `PosN/` children, or a `PosN/` dir itself), the TIFF-series adapter handles it. A single `.tif` resolves to the TIFF-series adapter via suffix. ND2/CZI file suffixes still resolve to their adapters. Unknown dir or unknown suffix → `MdatError::UnsupportedFormat`.
- [ ] Filename glob parses the canonical `mdat convert` pattern `img_channel{c:03}_position{p:03}_time{t:09}_z{z:03}.tif` (zero-padded `t` to 9 digits). Dims = `max_observed_index + 1` per axis. Zero matches → `UnsupportedFormat` naming the expected pattern (not a generic glob error). Generic OME-TIFF / ACDC / arbitrary TIFF folders are *not* matched.
- [ ] `time_map.csv` (cols `t,t_real`) under `PosN/` is parsed; `t_real` is exposed in the normalized metadata. The filename's `t_new` field is the canonical time address passed to `read_frame`; `time_map.csv`'s `t` column indexes the same space.
- [ ] `read_frame` lazily opens and caches the matching `.tif` per call (cache scoped to the session). Repeated reads of the same `(p,t,c,z)` reuse the cached decode — observable via fixture tests, not asserted on internal cache fields.
- [ ] `ensure_input_exists` relaxed to accept `is_dir()` in addition to `is_file()`; existing file-input call sites unaffected.
- [ ] Per-channel color seeding: TIFF-series adapter seeds a default palette when normalized metadata has no per-channel `color` (ND2/CZI already supply colors).
- [ ] **Smoke test:** fixture tree of `mdat convert`-named TIFFs (reuse the existing `mdat convert` fixture helper if present, else mirror its exact filenames) resolves, `inspect` reports the expected `n_pos/n_time/n_chan/n_z` and `width/height`, `read_frame` returns `width*height` u16s for a known `(p,t,c,z)`; an empty/foreign directory returns `UnsupportedFormat` naming the expected pattern; an ND2 and a CZI fixture still resolve to their adapters; `ensure_input_exists` accepts a directory path. All via `cargo test` against the trait surface, no HTTP.

## Extract `mdat-input` crate (break the mdat ↔ mdat-view cycle)  ✓ DONE

**What to build:** A new `crates/mdat-input` workspace member holds the input seam (`ReaderAdapter`, `ReaderSession`, `ImageInfo`, `MetadataPayload`, `resolve_reader_adapter`, `ensure_input_exists`, the ND2/CZI/TIFF-series adapters, and the error types they need) so that both `mdat` and `mdat-view` depend on `mdat-input` instead of each other. After this, `mdat-view → mdat-input` and `mdat → mdat-input`; `mdat → mdat-view` stays (for the `view` subcommand) but `mdat-view` no longer depends on `mdat`, so no cycle. The `ReaderAdapter`/`ReaderSession` contract is unchanged — this is a pure move + re-export.

**Blocked by:** None — unblocked now (after #1 and #2 landed; this is a refactor of their output).

- [ ] New workspace member `crates/mdat-input` with a lib target. Move into it: `input/types.rs` (ReaderAdapter/ReaderSession/ImageInfo/MetadataPayload), `input/nd2.rs`, `input/czi.rs`, `input/tiff_series.rs`, `input/registry.rs` (resolve_reader_adapter, ensure_input_exists, location_suffix), and the error variants those modules use (`MdatError`, or a scoped `MdatInputError`). The `tiff`/`roxmltree`/`regex`/`serde_json` deps move with them; `nd2-rs`/`czi-rs` deps move to `mdat-input`.
- [ ] `crates/mdat` depends on `mdat-input` and re-exports the public seam (`pub use mdat_input::{ReaderAdapter, ReaderSession, ...}`) so existing `mdat` code (convert, metadata, main.rs) keeps compiling with no logic changes. `mdat`'s `convert`/`metadata`/`io`/`selection`/`slices`/`output` stay in `mdat`.
- [ ] `crates/mdat-view` depends on `mdat-input` (NOT on `mdat`). `mdat-view` uses `mdat_input::resolve_reader_adapter` etc. directly. The `mdat → mdat-view` edge stays for the subcommand dispatch only.
- [ ] `MdatError` strategy: either move the whole enum to `mdat-input` (and `mdat` re-exports), or split into `MdatInputError` (input-seam variants) in `mdat-input` and keep `MdatError` in `mdat` for the rest. Pick the lower-churn option: move the whole enum to `mdat-input` and re-export from `mdat`. Document the choice in the crate's lib.rs doc-comment.
- [ ] All existing tests pass unchanged (29 in `mdat`, 10 in `mdat-view` after the #1/#2 fixes). The TIFF-series tests move with the module into `mdat-input` and pass there.
- [ ] **Smoke test:** `cargo check -p mdat -p mdat-view -p mdat-input` clean; `cargo test -p mdat-input` green (the moved tests); `cargo test -p mdat` green (convert/metadata/output tests); `cargo test -p mdat-view` green; `cargo run -p mdat -- view --help` still works; `cargo run -p mdat-view -- /tmp` still prints the URL line. No Node required.

## axum server: token-gated API + lifecycle + idle timeout  ✓ DONE

**Blocked by:** Extract `mdat-input` crate (needs the input seam in `mdat-input` so `mdat-view` can depend on it without a cycle); Scaffold `mdat-view` crate + `view` subcommand + CLI flags + bind detection (needs the crate, `ViewArgs`, bind detection, free-port, and token helpers).

- [ ] `run()` holds one `ReaderSession` for the dataset's lifetime in axum `State`; resolves the adapter via `resolve_reader_adapter` (including the new TIFF-series dispatch from #2 once that lands — works against ND2/CZI immediately, TIFF once #2 is done; this ticket's server code is agnostic to adapter kind).
- [ ] `GET /<token>/dataset` returns the contract: `{ info:{n_pos,n_time,n_chan,n_z}, width, height, channels:[{index,name,color}], metadata:{normalized, raw?, raw_format?} }`. `channels[].color`/`name` seeded from normalized metadata.
- [ ] `GET /<token>/plane?p=&t=&c=&z=` returns `application/octet-stream`, `width*height*2` bytes, little-endian u16, row-major, no header. Defaults `p=t=z=0, c=0`. 4D address maps 1:1 to `read_frame`.
- [ ] Out-of-range indices → `400` JSON `{error}`. `read_frame` failure → `500` JSON `{error, frame}`. Wrong token → `404` (not 401, no gated-path leak). Correct token → 200 on both endpoints.
- [ ] Token: 16 random bytes, base64url (22 chars), generated once at startup, in process memory only, never logged except via the stdout URL line (which includes it). No other disclosure.
- [ ] Idle-timeout activity clock: resets on each *completed* `/plane` or `/dataset` request; default 30 min, `--idle-timeout <min>` overrides. On expiry prints `shutting down (idle)` and exits. Connection open does not count as activity.
- [ ] Ctrl-C → graceful shutdown via `axum::serve(...).with_graceful_shutdown(...)`; in-flight plane requests finish; port released cleanly.
- [ ] Binds `bind_addr:0`, retrieves the real port, prints `mdat view serving <name> at http://<bind>:<port>/<token>/` to stdout.
- [ ] Browser open: best-effort, non-fatal. Try `xdg-open` / `open` / `cmd /c start`; skip if binary missing or `--no-open`. Failure to open does not fail the server.
- [ ] **Smoke test:** `mdat view <fixture ND2>` prints a URL; `curl <url>/dataset` returns the contract JSON; `curl <url>/plane` returns `width*height*2` bytes; wrong token → 404; out-of-range → 400; a forced read failure (point at a deliberately broken fixture) → 500 with `{error,frame}`; the process exits on `--idle-timeout 0` style short window when idle; Ctrl-C exits cleanly. No Node required to run or test.

## Frontend scaffold: Solid + Vite + Tailwind, proxy, gated build, embedding  ✓ DONE

**Blocked by:** Extract `mdat-input` crate (so the frontend build wiring can live in `mdat-view` without pulling `mdat`); Scaffold `mdat-view` crate + `view` subcommand + CLI flags + bind detection (needs the crate location, the URL/token path shape, and the server's `/<token>/...` contract to proxy against). Can parallelize with the axum server ticket once #2a + #1 are done — the API contract is fixed in the spec, so this ticket builds against the documented shape, not a live #3.

- [ ] Solid + Vite + Tailwind v4 app at `crates/mdat-view/web/`. House stack from Lisca: Solid, Vite, Tailwind v4, Kobalte, phosphor-icons-solid, @tanstack/solid-router, vite-plugin-solid.
- [ ] Vite dev server (5173) with `server.proxy` forwarding `/<token>/dataset` and `/<token>/plane` to axum (8787); HMR against the live Rust API. Document the dev workflow (run axum + `bun dev`).
- [ ] `build.rs` runs the frontend build (`bun` preferred, else `npm`) **only when** `web/dist/index.html` is missing or stale. Gated so `cargo check` / `cargo test` never require Node. Verify with Node uninstalled: `cargo check -p mdat-view` succeeds.
- [ ] Frontend bundle embedded into the binary via `include_dir` (or equivalent). Release binary serves the static bundle at `/` (non-token root) and the API at `/<token>/...`.
- [ ] Missing bundle (Node never run, no `web/dist`) → server serves a "build the frontend first" placeholder instead of crashing. Binary still boots and serves the API.
- [ ] App shell: mounts, fetches `/dataset` (token resolved from the page URL/path), renders dataset name + `info` dims + channel list in a panel. No canvas, no plane fetch — just proves the fetch + render + embedding loop.
- [ ] **Smoke test:** `cargo run -p mdat-view -- <fixture>` (with a built bundle) opens a page that shows the dataset name and dims from `/dataset`; with no bundle built, the server serves the placeholder and `curl /` returns the placeholder HTML; `cargo check` passes with no Node installed.

## Canvas renderer + composite overlay + plane cache + prefetch + UI controls  ✓ DONE

**Blocked by:** axum server: token-gated API + lifecycle + idle timeout (needs live `/plane` + `/dataset` to fetch from); Frontend scaffold: Solid + Vite + Tailwind, proxy, gated build, embedding (needs the app shell, proxy, and embedding to extend).

- [ ] Canvas + composite overlay: fetch `/<token>/plane?p=&t=&c=&z=` → `Uint16Array` via `Response.arrayBuffer()`; composite all enabled channels into one RGBA buffer via additive blend, each channel tinted by its per-channel color. Grayscale is the degenerate single-enabled-channel case.
- [ ] JS plane cache keyed by `(p,t,c,z)`, LRU size 256. Revisiting a cached plane is instant (no re-fetch).
- [ ] Prefetch ring: on plane change, prefetch adjacent T (±1) and Z (±1) for currently enabled channels. Debounce-on-change ~50ms before triggering fetches.
- [ ] Per-channel UI: visibility toggle, user-assigned color, contrast min/max — applied in the composite. Single-channel grayscale view just works without manual setup.
- [ ] Auto-contrast: `a` key runs client-side percentile pass over the already-fetched `Uint16Array`(s) for the current frame across all enabled channels; sets per-channel contrast min/max. No server round-trip.
- [ ] Pixel inspector: read raw u16 value(s) at a pixel across enabled channels.
- [ ] Keyboard: Left/Right = T∓1; Up/Down = Z∓1 (Up=Z−1, Down=Z+1); 1-9 = toggle channel N visibility; `a` = auto-contrast; `f` = fit-to-window; `+`/`-` = zoom (centered on cursor if hovering, else image center).
- [ ] Position dropdown: switch positions in a multi-position dataset. Selecting a position resets T=0, Z=0, **keeps** channel visibility/contrast/color, **clears** the JS plane cache.
- [ ] Error handling: out-of-range (server 400) and read-failure (server 500) shown as centered overlay with the message; network/fetch failure → "the server may have exited — check the terminal" + Reload button that re-fetches `/dataset`.
- [ ] Metadata panel: dataset dims, channel names, normalized (and raw when available) metadata, `t_real` from `time_map.csv` for TIFF-series.
- [ ] **Smoke test (Playwright via webapp-testing skill, dev mode):** dataset loads and renders the composite; toggling a channel changes which planes are fetched; auto-contrast key changes contrast (observable via rendered output or per-channel state); position switch clears the cache (observable: re-fetch of a previously-cached plane happens); a forced 500 shows the error overlay; pixel inspector returns a u16 value. Pixel-perfect canvas assertions not required — behavioral assertions only.

## End-to-end smoke + integration test  ✓ DONE

**Blocked by:** axum server: token-gated API + lifecycle + idle timeout; Canvas renderer + composite overlay + plane cache + prefetch + UI controls (needs the full stack to exercise).

- [ ] Playwright smoke (via webapp-testing skill) against the embedded bundle (or dev server) on a small fixture: `mdat view <fixture>` boots, browser opens, dataset loads, one channel toggled, one scrub step (T±1), one auto-contrast, one position switch (when multi-pos), Ctrl-C shuts down cleanly. Behavioral assertions only.
- [ ] Run the smoke against all three input kinds from the spec: a small ND2, a small CZI, and a small `mdat convert` TIFF-series tree (reuse the #2 fixture helper).
- [ ] Backend integration test: `mdat view <fixture>` boots the server in-process (or via `mdtest`-style subprocess), exercises `/dataset` + `/plane` + wrong-token + out-of-range + read-failure across all three adapter kinds through the real HTTP API, asserts the contract responses. No browser.
- [ ] Lifecycle assertions in the integration test: idle-timeout exits the process within the configured window when idle; a request inside the window keeps it alive; Ctrl-C path is graceful (port released, exit 0).
- [ ] **Smoke test:** the integration test + the Playwright smoke both pass in `cargo test` (backend) and the webapp-testing harness (frontend) on the CI-equivalent environment. A green run here is the definition of "the tracer bullet landed."