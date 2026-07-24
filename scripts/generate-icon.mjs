// MoyuMax 正式图标程序化生成（可复现，无外部素材依赖）。
// 用法:node scripts/generate-icon.mjs [--check]
// 产物:apps/desktop/src-tauri/icons/{icon.ico,icon.png,32x32.png,64x64.png,128x128.png,128x128@2x.png}
// 设计:深底圆角方块 + 等距像素立方体(草方块三色),顶面深绿像素 M 负形;
// 4x 超采样抗锯齿,小尺寸(16/32)边缘仍然干净。
import { deflateSync } from "node:zlib";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconsDir = join(repoRoot, "apps", "desktop", "src-tauri", "icons");
const checkOnly = process.argv.includes("--check");

const SURFACE = [0x1e, 0x20, 0x1c, 0xff];
const BORDER = [0x33, 0x35, 0x2e, 0xff];
const CUBE_TOP = [0x8f, 0xd4, 0x7c, 0xff];
const CUBE_LEFT = [0x5a, 0x9a, 0x4c, 0xff];
const CUBE_RIGHT = [0x3f, 0x73, 0x36, 0xff];
const CUBE_GLYPH = [0x2c, 0x52, 0x28, 0xff];
const TRANSPARENT = [0, 0, 0, 0];

// 像素 M(5x5 网格,顶面负形)。
const GLYPH_M = new Set([
  "0,0", "4,0",
  "0,1", "1,1", "3,1", "4,1",
  "0,2", "2,2", "4,2",
  "0,3", "4,3",
  "0,4", "4,4",
]);

// 立方体几何(256 坐标系,等距投影;侧面高度 72)。
const TOP = { cx: 128, cy: 94, hw: 72, hh: 42 };

function renderIcon(targetSize) {
  const SS = 4; // 超采样倍数
  const size = targetSize * SS;
  const scale = size / 256;
  const radius = 52 * scale;
  const inset = 6 * scale;

  const pixels = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const color = pixelColor(x + 0.5, y + 0.5);
      const offset = (y * size + x) * 4;
      pixels[offset] = color[0];
      pixels[offset + 1] = color[1];
      pixels[offset + 2] = color[2];
      pixels[offset + 3] = color[3];
    }
  }
  if (SS === 1) return pixels;
  return downsample(pixels, size, SS);

  function pixelColor(px, py) {
    if (!inRounded(px, py, 0)) return TRANSPARENT;
    const borderInner = !inRounded(px, py, 2.5 * scale);
    const cube = cubeColor(px / scale, py / scale);
    if (cube) return cube;
    return borderInner ? SURFACE : BORDER;
  }

  function inRounded(rx, ry, insetBy) {
    const left = inset + insetBy;
    const right = size - inset - insetBy;
    if (rx < left || rx >= right || ry < left || ry >= right) return false;
    const cx = Math.max(left + radius, Math.min(rx, right - radius));
    const cy = Math.max(left + radius, Math.min(ry, right - radius));
    const dx = rx - cx;
    const dy = ry - cy;
    return dx * dx + dy * dy <= radius * radius;
  }

  // 在 256 坐标系判定立方体各面;顶面内再判像素 M。
  // 顶面:|u|+|v|<=1(u=(x-128)/72,v=(y-94)/42);
  // 侧面:au=|u|<=1 且 y∈(94+42*(1-au), 166+42*(1-au)],u<0 为左面。
  function cubeColor(x, y) {
    const u = (x - TOP.cx) / TOP.hw;
    const v = (y - TOP.cy) / TOP.hh;
    if (Math.abs(u) + Math.abs(v) <= 1) {
      const gu = Math.floor(((u + 0.5) / 1.0) * 5);
      const gv = Math.floor(((v + 0.5) / 1.0) * 5);
      if (gu >= 0 && gu < 5 && gv >= 0 && gv < 5 && GLYPH_M.has(`${gu},${gv}`)) {
        return CUBE_GLYPH;
      }
      return CUBE_TOP;
    }
    const au = Math.abs(u);
    if (au <= 1) {
      const yTop = TOP.cy + TOP.hh * (1 - au);
      if (y > yTop && y <= yTop + 72) {
        return u < 0 ? CUBE_LEFT : CUBE_RIGHT;
      }
    }
    return null;
  }
}

function downsample(pixels, size, factor) {
  const target = size / factor;
  const out = Buffer.alloc(target * target * 4);
  for (let y = 0; y < target; y += 1) {
    for (let x = 0; x < target; x += 1) {
      const acc = [0, 0, 0, 0];
      for (let dy = 0; dy < factor; dy += 1) {
        for (let dx = 0; dx < factor; dx += 1) {
          const offset = ((y * factor + dy) * size + (x * factor + dx)) * 4;
          acc[0] += pixels[offset];
          acc[1] += pixels[offset + 1];
          acc[2] += pixels[offset + 2];
          acc[3] += pixels[offset + 3];
        }
      }
      const count = factor * factor;
      const offset = (y * target + x) * 4;
      out[offset] = Math.round(acc[0] / count);
      out[offset + 1] = Math.round(acc[1] / count);
      out[offset + 2] = Math.round(acc[2] / count);
      out[offset + 3] = Math.round(acc[3] / count);
    }
  }
  return out;
}

function crc32(buffer) {
  let table = crc32.table;
  if (!table) {
    table = crc32.table = new Int32Array(256);
    for (let n = 0; n < 256; n += 1) {
      let c = n;
      for (let k = 0; k < 8; k += 1) {
        c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      }
      table[n] = c;
    }
  }
  let crc = -1;
  for (const byte of buffer) {
    crc = crc32.table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ -1) >>> 0;
}

function pngChunk(type, data) {
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  chunk.write(type, 4, "ascii");
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([Buffer.from(type, "ascii"), data])), 8 + data.length);
  return chunk;
}

function encodePng(size, rgba) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const stride = size * 4 + 1;
  const scanlines = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y += 1) {
    scanlines[y * stride] = 0;
    rgba.copy(scanlines, y * stride + 1, y * size * 4, (y + 1) * size * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(scanlines, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function encodeIco(pngs) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(pngs.length, 4);
  const entries = [];
  const payloads = [];
  let offset = 6 + pngs.length * 16;
  for (const { size, png } of pngs) {
    const entry = Buffer.alloc(16);
    entry[0] = size >= 256 ? 0 : size;
    entry[1] = size >= 256 ? 0 : size;
    entry[2] = 0;
    entry[3] = 0;
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(png.length, 8);
    entry.writeUInt32LE(offset, 12);
    entries.push(entry);
    payloads.push(png);
    offset += png.length;
  }
  return Buffer.concat([header, ...entries, ...payloads]);
}

const SIZES = [16, 32, 48, 64, 128, 256];

function generate() {
  mkdirSync(iconsDir, { recursive: true });
  const pngs = SIZES.map((size) => ({ size, png: encodePng(size, renderIcon(size)) }));
  writeFileSync(join(iconsDir, "icon.ico"), encodeIco(pngs));
  const named = new Map([
    ["32x32.png", 32],
    ["64x64.png", 64],
    ["128x128.png", 128],
    ["128x128@2x.png", 256],
    ["icon.png", 256],
    ["tray-icon.png", 64],
  ]);
  for (const [name, size] of named) {
    writeFileSync(join(iconsDir, name), pngs.find((entry) => entry.size === size).png);
  }
  console.log(`已生成 ${iconsDir} 的 icon.ico 与 ${named.size} 个 PNG`);
}

function check() {
  const icoPath = join(iconsDir, "icon.ico");
  if (!existsSync(icoPath)) {
    console.error("缺少 icon.ico,请先运行 node scripts/generate-icon.mjs");
    process.exit(2);
  }
  const ico = readFileSync(icoPath);
  if (ico.readUInt16LE(0) !== 0 || ico.readUInt16LE(2) !== 1) {
    console.error("icon.ico 头部无效");
    process.exit(1);
  }
  const count = ico.readUInt16LE(4);
  if (count !== SIZES.length) {
    console.error(`icon.ico 应包含 ${SIZES.length} 个尺寸,实际 ${count}`);
    process.exit(1);
  }
  for (const name of ["32x32.png", "64x64.png", "128x128.png", "128x128@2x.png", "icon.png", "tray-icon.png"]) {
    const path = join(iconsDir, name);
    if (!existsSync(path)) {
      console.error(`缺少 ${name}`);
      process.exit(1);
    }
    const png = readFileSync(path);
    if (png.readUInt32BE(0) !== 0x89504e47) {
      console.error(`${name} 不是有效 PNG`);
      process.exit(1);
    }
  }
  console.log(`icon.ico 含 ${count} 个尺寸,全部 PNG 有效`);
}

if (checkOnly) {
  check();
} else {
  generate();
  check();
}
