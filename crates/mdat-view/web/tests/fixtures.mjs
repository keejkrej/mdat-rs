import { mkdirSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

export const W = 8;
export const H = 6;

function makeFrame(seed) {
  const out = new Uint16Array(W * H);
  for (let i = 0; i < W * H; i++) out[i] = (seed + i) % 4096;
  return out;
}

function writeGray16Tiff(path, pixels, width, height) {
  const stripBytes = width * height * 2;
  const numEntries = 10;
  const dataOffset = 8 + 2 + numEntries * 12 + 4;
  const total = dataOffset + stripBytes;
  const buf = Buffer.alloc(total);
  buf.writeUInt8(0x49, 0);
  buf.writeUInt8(0x49, 1);
  buf.writeUInt16LE(42, 2);
  buf.writeUInt32LE(8, 4);
  const entries = [
    [256, 3, 1, width],
    [257, 3, 1, height],
    [258, 3, 1, 16],
    [259, 3, 1, 1],
    [262, 3, 1, 1],
    [273, 4, 1, dataOffset],
    [277, 3, 1, 1],
    [278, 3, 1, height],
    [279, 4, 1, stripBytes],
    [284, 3, 1, 1],
  ];
  buf.writeUInt16LE(numEntries, 8);
  let p = 10;
  for (const [tag, type, count, value] of entries) {
    buf.writeUInt16LE(tag, p);
    buf.writeUInt16LE(type, p + 2);
    buf.writeUInt32LE(count, p + 4);
    if (type === 3) buf.writeUInt16LE(value, p + 8);
    else buf.writeUInt32LE(value, p + 8);
    p += 12;
  }
  buf.writeUInt32LE(0, p);
  for (let i = 0; i < pixels.length; i++) buf.writeUInt16LE(pixels[i], dataOffset + i * 2);
  writeFileSync(path, buf);
}

export function writeMdatTree(root, nPos, nTime, nChan, nZ) {
  if (existsSync(root)) rmSync(root, { recursive: true, force: true });
  mkdirSync(root, { recursive: true });
  for (let p = 0; p < nPos; p++) {
    const posDir = join(root, `Pos${p}`);
    mkdirSync(posDir, { recursive: true });
    let timeMap = "t,t_real\n";
    for (let t = 0; t < nTime; t++) timeMap += `${t},${t}\n`;
    writeFileSync(join(posDir, "time_map.csv"), timeMap);
    for (let t = 0; t < nTime; t++) {
      for (let c = 0; c < nChan; c++) {
        for (let z = 0; z < nZ; z++) {
          const seed = (p * 100 + t * 10 + c * 3 + z) * 7;
          const frame = makeFrame(seed);
          const name = `img_channel${String(c).padStart(3, "0")}_position${String(p).padStart(3, "0")}_time${String(t).padStart(9, "0")}_z${String(z).padStart(3, "0")}.tif`;
          writeGray16Tiff(join(posDir, name), frame, W, H);
        }
      }
    }
  }
  return root;
}