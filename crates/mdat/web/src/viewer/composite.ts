import type { ChannelState, Plane, PlaneAddr } from "../types";
import { parseColor } from "./colors";

/**
 * Composite all enabled channels into a single RGBA `Uint8ClampedArray` of
 * `width*height*4`. Each channel's u16 plane is contrast-stretched to 0..255
 * using its per-channel min/max, tinted by its color, and additively blended
 * into the RGBA buffer.
 *
 * A single enabled channel renders as grayscale (its tint IS its color, which
 * defaults to white for single-channel datasets seeded with no metadata color;
 * the user can override).
 */
export function composite(
  planes: { channel: ChannelState; plane: Plane }[],
  width: number,
  height: number,
): Uint8ClampedArray {
  const n = width * height;
  const rgba = new Uint8ClampedArray(n * 4);
  const single = planes.length === 1;

  for (const { channel, plane } of planes) {
    let tr: number, tg: number, tb: number;
    if (single) {
      tr = tg = tb = 1;
    } else {
      const [r, g, b] = parseColor(channel.color);
      tr = r / 255;
      tg = g / 255;
      tb = b / 255;
    }
    const min = channel.contrastMin;
    const max = channel.contrastMax;
    const range = max > min ? max - min : 1;
    for (let i = 0; i < n; i++) {
      const v = plane[i];
      let norm: number;
      if (v <= min) {
        norm = 0;
      } else if (v >= max) {
        norm = 1;
      } else {
        norm = (v - min) / range;
      }
      rgba[i * 4 + 0] = Math.min(255, rgba[i * 4 + 0] + norm * tr * 255);
      rgba[i * 4 + 1] = Math.min(255, rgba[i * 4 + 1] + norm * tg * 255);
      rgba[i * 4 + 2] = Math.min(255, rgba[i * 4 + 2] + norm * tb * 255);
    }
  }
  if (planes.length > 0) {
    for (let i = 3; i < rgba.length; i += 4) rgba[i] = 255;
  }
  return rgba;
}

/**
 * Compute per-channel contrast min/max via percentile pass over an already-
 * fetched plane. `lowPct`/`highPct` are in 0..1 (defaults ~0.5% / 99.5%).
 * Uses a histogram over the u16 range for speed (65536 buckets, O(N)).
 */
export function percentileContrast(
  plane: Plane,
  lowPct = 0.005,
  highPct = 0.995,
): { min: number; max: number } {
  const n = plane.length;
  if (n === 0) return { min: 0, max: 1 };
  const hist = new Uint32Array(65536);
  let maxVal = 0;
  for (let i = 0; i < n; i++) {
    const v = plane[i];
    hist[v]++;
    if (v > maxVal) maxVal = v;
  }
  if (maxVal === 0) return { min: 0, max: 1 };
  const lowCount = Math.floor(n * lowPct);
  const highCount = Math.floor(n * highPct);
  let min = 0;
  let acc = 0;
  for (let v = 0; v <= maxVal; v++) {
    acc += hist[v];
    if (acc >= lowCount) {
      min = v;
      break;
    }
  }
  acc = 0;
  for (let v = 0; v <= maxVal; v++) {
    acc += hist[v];
    if (acc >= highCount) {
      const max = v;
      if (max <= min) return { min, max: min + 1 };
      return { min, max };
    }
  }
  return { min, max: maxVal || 1 };
}

/** Default contrast when none has been set: min=0, max=65535 (full range). */
export function defaultContrast(): { min: number; max: number } {
  return { min: 0, max: 65535 };
}

/** Read raw u16 values at pixel (x,y) for each channel's plane. */
export function pixelValues(
  planes: { channel: ChannelState; plane: Plane }[],
  width: number,
  x: number,
  y: number,
): { channel: ChannelState; value: number }[] {
  const idx = y * width + x;
  return planes.map(({ channel, plane }) => ({
    channel,
    value: plane[idx] ?? 0,
  }));
}

export type { ChannelState, Plane, PlaneAddr };