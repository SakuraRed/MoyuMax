// MoyuMax 正式图标程序化生成（可复现，无外部素材依赖）。
// 用法:node scripts/generate-icon.mjs [--check]
// 产物:apps/desktop/src-tauri/icons/{icon.ico,icon.png,32x32.png,64x64.png,128x128.png,128x128@2x.png}
// 设计:深底圆角方块 + 品牌绿 M 字形,与界面变量(--surface #25252a / --accent #7cc46c)一致。
import { deflateSync } from "node:zlib";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconsDir = join(repoRoot, "apps", "desktop", "src-tauri", "icons");
const checkOnly = process.argv.includes("--check");

const SURFACE = [0x25, 0x25, 0x2a, 0xff];
const BORDER = [0x3a, 0x3a, 0x41, 0xff];
const ACCENT = [0x7c, 0xc4, 0x6c, 0xff];
const TRANSPARENT = [0, 0, 0, 0];

function renderIcon(size) {
  const scale = size / 256;
  const radius = 56 * scale;
  const inset = 4 * scale;
  // M 字形几何（256 坐标系）：双竖干 + 双斜杠,线宽 26。
  const stroke = 26 * scale;
  const stemLeft = [64 * scale, 62 * scale, 90 * scale, 194 * scale];
  const stemRight = [166 * scale, 62 * scale, 192 * scale, 194 * scale];
  const diagLeft = { from: [77 * scale, 62 * scale], to: [128 * scale, 132 * scale] };
  const diagRight = { from: [179 * scale, 62 * scale], to: [128 * scale, 132 * scale] };

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
  return pixels;

  function pixelColor(px, py) {
    // 圆角方块:点到矩形最近角域的距离不超过半径即在内。
    const inRounded = (rx, ry, insetBy) => {
      const left = inset + insetBy;
      const right = size - inset - insetBy;
      if (rx < left || rx >= right || ry < left || ry >= right) return false;
      const cx = Math.max(left + radius, Math.min(rx, right - radius));
      const cy = Math.max(left + radius, Math.min(ry, right - radius));
      const dx = rx - cx;
      const dy = ry - cy;
      return dx * dx + dy * dy <= radius * radius;
    };
    if (!inRounded(px, py, 0)) return TRANSPARENT;
    const borderInner = !inRounded(px, py, 2 * scale);
    const onStem =
      (px >= stemLeft[0] && px < stemLeft[2] && py >= stemLeft[1] && py < stemLeft[3]) ||
      (px >= stemRight[0] && px < stemRight[2] && py >= stemRight[1] && py < stemRight[3]);
    const onDiag =
      distanceToSegment(px, py, diagLeft.from, diagLeft.to) <= stroke / 2 ||
      distanceToSegment(px, py, diagRight.from, diagRight.to) <= stroke / 2;
    if (onStem || onDiag) return ACCENT;
    return borderInner ? SURFACE : BORDER;
  }
}

function distanceToSegment(px, py, [x1, y1], [x2, y2]) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const lengthSquared = dx * dx + dy * dy;
  const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lengthSquared));
  const cx = x1 + t * dx;
  const cy = y1 + t * dy;
  return Math.hypot(px - cx, py - cy);
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
