// 生成 MoyuMax 开发占位托盘图标(64×64 PNG)。
// 正式品牌图标属首个公开预览发布缺口,本文件仅保证托盘功能可验证。
// 用法:node scripts/generate-tray-icon.mjs
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const SIZE = 64;
const BG = [23, 26, 33, 255]; // 深色圆角底
const ACCENT = [88, 196, 138, 255]; // 主色绿
const RADIUS = 14;

// 5x7 点阵字母 M
const GLYPH_M = [
  "1...1",
  "11.11",
  "11.11",
  "1.1.1",
  "1...1",
  "1...1",
  "1...1",
];
const GLYPH_SCALE = 6;
const GLYPH_W = 5 * GLYPH_SCALE;
const GLYPH_H = 7 * GLYPH_SCALE;
const GLYPH_X = Math.floor((SIZE - GLYPH_W) / 2);
const GLYPH_Y = Math.floor((SIZE - GLYPH_H) / 2);

const pixels = new Uint8Array(SIZE * SIZE * 4);
for (let y = 0; y < SIZE; y += 1) {
  for (let x = 0; x < SIZE; x += 1) {
    const offset = (y * SIZE + x) * 4;
    const inside =
      x >= RADIUS && x < SIZE - RADIUS ||
      y >= RADIUS && y < SIZE - RADIUS ||
      (x - RADIUS) ** 2 + (y - RADIUS) ** 2 <= RADIUS ** 2 ||
      (x - (SIZE - 1 - RADIUS)) ** 2 + (y - RADIUS) ** 2 <= RADIUS ** 2 ||
      (x - RADIUS) ** 2 + (y - (SIZE - 1 - RADIUS)) ** 2 <= RADIUS ** 2 ||
      (x - (SIZE - 1 - RADIUS)) ** 2 + (y - (SIZE - 1 - RADIUS)) ** 2 <= RADIUS ** 2;
    if (!inside) continue;
    pixels.set(BG, offset);
    const gx = Math.floor((x - GLYPH_X) / GLYPH_SCALE);
    const gy = Math.floor((y - GLYPH_Y) / GLYPH_SCALE);
    if (gy >= 0 && gy < 7 && gx >= 0 && gx < 5 && GLYPH_M[gy][gx] === "1") {
      pixels.set(ACCENT, offset);
    }
  }
}

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y += 1) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  Buffer.from(pixels.buffer, y * SIZE * 4, SIZE * 4).copy(raw, y * (SIZE * 4 + 1) + 1);
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const target = join(
  dirname(fileURLToPath(import.meta.url)),
  "../apps/desktop/src-tauri/icons/tray-icon.png",
);
mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, png);
console.log(`written ${target} (${png.length} bytes)`);
