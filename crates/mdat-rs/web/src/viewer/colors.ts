import type { PlaneAddr, Plane } from "../types";

/** Parse a `#rrggbb` or `#rgb` color string into `[r, g, b]` in 0..255. */
export function parseColor(hex: string): [number, number, number] {
  let h = hex.trim();
  if (h.startsWith("#")) h = h.slice(1);
  if (h.length === 3) {
    h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
  }
  if (h.length !== 6) return [255, 255, 255];
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  if ([r, g, b].some((n) => Number.isNaN(n))) return [255, 255, 255];
  return [r, g, b];
}

/** Format an `[r,g,b]` triple as a `#rrggbb` string. */
export function formatColor(rgb: [number, number, number]): string {
  const toHex = (n: number) =>
    Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${toHex(rgb[0])}${toHex(rgb[1])}${toHex(rgb[2])}`;
}

export function planeKey(a: PlaneAddr): string {
  return `${a.p},${a.t},${a.c},${a.z}`;
}

/**
 * LRU plane cache keyed by `p,t,c,z`. Map preserves insertion order, so the
 * oldest entry is the first to evict. Cap is 256 planes.
 */
export class PlaneCache {
  private map = new Map<string, Plane>();
  readonly cap: number;

  constructor(cap = 256) {
    this.cap = cap;
  }

  get(addr: PlaneAddr): Plane | undefined {
    const k = planeKey(addr);
    const v = this.map.get(k) as Plane | undefined;
    if (v === undefined) return undefined;
    // Refresh insertion order (most-recently-used at the end).
    this.map.delete(k);
    this.map.set(k, v);
    return v;
  }

  set(addr: PlaneAddr, plane: Plane): void {
    const k = planeKey(addr);
    if (this.map.has(k)) {
      this.map.delete(k);
    }
    this.map.set(k, plane);
    while (this.map.size > this.cap) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }

  has(addr: PlaneAddr): boolean {
    return this.map.has(planeKey(addr));
  }

  clear(): void {
    this.map.clear();
  }

  get size(): number {
    return this.map.size;
  }
}