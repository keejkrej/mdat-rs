import type { Dataset, Plane, PlaneAddr } from "./types";

/**
 * Extract the token from the page URL. The app is served at
 * `http://host:port/<token>/`, so the first path segment is the token.
 */
export function tokenFromPath(): string | null {
  const path = window.location.pathname;
  const seg = path.split("/").filter(Boolean)[0];
  if (!seg) return null;
  if (seg === "dataset" || seg === "plane" || seg === "assets") return null;
  return seg;
}

export async function fetchDataset(token: string): Promise<Dataset> {
  const res = await fetch(`/${token}/dataset`, {
    headers: { Accept: "application/json" },
  });
  if (!res.ok) {
    throw new Error(`dataset fetch failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as Dataset;
}

/**
 * Fetch a single u16 plane. Returns the `Uint16Array` and the address it
 * resolved to (defaults applied by the server). Throws `PlaneFetchError` on
 * non-200 with a structured message; throws a plain `Error` (fatal) on a
 * network failure.
 */
export class PlaneFetchError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly frame: PlaneAddr | null,
  ) {
    super(message);
    this.name = "PlaneFetchError";
  }
}

export async function fetchPlane(
  token: string,
  addr: PlaneAddr,
): Promise<Plane> {
  const url = `/${token}/plane?p=${addr.p}&t=${addr.t}&c=${addr.c}&z=${addr.z}`;
  let res: Response;
  try {
    res = await fetch(url);
  } catch {
    throw new Error("network error fetching plane");
  }
  if (!res.ok) {
    let msg = `plane fetch failed: ${res.status} ${res.statusText}`;
    let frame: PlaneAddr | null = null;
    try {
      const body = await res.json();
      if (typeof body?.error === "string") msg = body.error;
      if (body?.frame) frame = body.frame as PlaneAddr;
    } catch {
      /* ignore JSON parse error */
    }
    throw new PlaneFetchError(msg, res.status, frame);
  }
  const buf = await res.arrayBuffer();
  return new Uint16Array(buf);
}