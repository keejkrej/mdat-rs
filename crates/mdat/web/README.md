# mdat-view web

Solid + Vite + Tailwind v4 frontend for `mdat view`. Single-page app that
extracts the token from the URL path, fetches `/<token>/dataset`, and renders
the dataset name + dims + channel list. Scaffold for the viewer (#4); the
canvas/compositing comes in #5.

## Stack

- [Solid](https://www.solidjs.com/) (`solid-js`)
- [Vite 6](https://vite.dev/) (`vite` + `vite-plugin-solid`)
- [Tailwind v4](https://tailwindcss.com/) (`tailwindcss` + `@tailwindcss/vite`)

Versions mirror the house Lisca stack (see `/home/jack/workspace/lisca`).

## Dev workflow

Two processes: the axum backend (8787) and the Vite dev server (5173).

```sh
# 1. Start the backend (boots axum on 127.0.0.1:8787, prints the token URL):
cargo run -p mdat-view -- <fixture> --no-open
#   -> mdat view serving <name> at http://127.0.0.1:8787/<token>/

# 2. Start the frontend (Vite on http://localhost:5173):
cd crates/mdat-view/web
bun install        # or: npm install
bun dev            # or: npm run dev

# 3. Open http://localhost:5173/<token>/ in a browser.
#    The path segment IS the token; the app extracts it and fetches
#    /<token>/dataset (proxied to axum).
```

The Vite proxy forwards `/<token>/dataset` and `/<token>/plane` to
`http://127.0.0.1:8787`; the token is just a path segment that passes
through. A custom middleware rewrites `/<token>/` to the SPA root so the
Solid app boots at the token-gated URL.

## Build

```sh
bun run build      # or: npm run build
# -> dist/index.html + dist/assets/*.{js,css}
```

The release binary embeds `web/dist` via `include_dir` (see
`../build.rs` and `../src/assets.rs`). `cargo check` / `cargo test` never
require Node: `build.rs` writes a placeholder `dist/index.html` if the
bundle is missing, so `include_dir!` always finds a directory and the
binary boots with a "build the frontend first" placeholder page.