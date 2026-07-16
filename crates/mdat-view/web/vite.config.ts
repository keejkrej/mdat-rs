import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// mdat-view frontend.
//
// The URL scheme is `/<token>/...` where the token is a 22-char base64url
// string generated at server startup. The Vite dev server (5173) proxies API
// calls (`/<token>/dataset`, `/<token>/plane`) to the axum backend on 8787.
// The token is an arbitrary path segment — Vite just passes it through.
//
// Dev workflow:
//   1. Start the backend:  `cargo run -p mdat-view -- <fixture> --no-open`
//      (boots axum on 127.0.0.1:8787, prints `http://127.0.0.1:8787/<token>/`)
//   2. Start the frontend: `cd web && bun dev`
//      (Vite on http://localhost:5173)
//   3. Open `http://localhost:5173/<token>/` in a browser — the path segment
//      IS the token; the app extracts it and fetches `/<token>/dataset`.
//
// SPA fallback: any single-segment path like `/<token>/` serves index.html
// (the Solid app). API paths (`/<token>/dataset`, `/<token>/plane`) proxy
// to axum and are never handled by the SPA fallback.

const PLACEHOLDER_TOKEN_SEGMENTS = new Set(["dataset", "plane", "assets"]);
const API_SEGMENT_NAMES = new Set(["dataset", "plane"]);

function isTokenSegment(seg: string): boolean {
  // A token is 22 chars of base64url (A-Za-z0-9-_). We also accept any
  // non-API, non-empty segment that isn't a known reserved name, so dev
  // works regardless of the exact token shape.
  if (!seg || API_SEGMENT_NAMES.has(seg) || PLACEHOLDER_TOKEN_SEGMENTS.has(seg)) {
    return false;
  }
  return /^[A-Za-z0-9_-]{8,}$/.test(seg) || seg.length >= 16;
}

export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [
    solid(),
    tailwindcss(),
    {
      name: "mdat-view-spa-fallback",
      configureServer(server) {
        // Serve index.html for `/<token>/` so the Solid app boots at the
        // token-gated URL in dev. Vite's built-in SPA fallback only handles
        // non-`/` paths that look like files; `/<token>/` is a directory-ish
        // path we want to map to the app shell.
        server.middlewares.use((req, res, next) => {
          const url = req.url ?? "";
          const path = url.split("?")[0];
          // Only handle GET/HEAD for the token root, not the API.
          if (req.method !== "GET" && req.method !== "HEAD") return next();
          // Split into segments, drop leading empty + query.
          const segments = path.split("/").filter(Boolean);
          if (segments.length === 1 && isTokenSegment(segments[0])) {
            // Rewrite to root so Vite serves index.html with the app.
            req.url = "/index.html";
          }
          next();
        });
      },
    },
  ],
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    proxy: {
      // The token is a path segment that passes through unchanged.
      // Match any `/<seg>/dataset` or `/<seg>/plane` where <seg> looks like
      // a token, forwarding to axum on 8787.
      "^/[^/]+/dataset$": {
        target: "http://127.0.0.1:8787",
        changeOrigin: true,
      },
      "^/[^/]+/plane$": {
        target: "http://127.0.0.1:8787",
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});