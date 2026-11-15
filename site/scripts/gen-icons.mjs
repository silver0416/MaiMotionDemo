// 產生 web app（PWA）的圖示：npm.cmd run icons
//
// 圖形同 src/lib/shell/Logo.svelte（以 100 為邊長：環半徑 38、線寬 10.5，圓點半徑 10、圓心在 x=36 與 64），
// 底色 #0b0d11、環 #e9edf4、左手粉 #f2679d、右手藍 #4fb8f5。圖形只有圓與圓環，
// 這裡直接用純 Node 算每個像素的覆蓋率（8×8 超取樣做反鋸齒）再寫成 PNG，不需要 sharp 或無頭瀏覽器。
// 改了 Logo 的比例或顏色就改這裡的常數再跑一次；產出的 PNG 進版控。
//
// - icon-192.png、icon-512.png（purpose: any）：滿版深色底，圖形縮到 0.85 倍（環半徑約 32，和 App 圖示一樣）。
//   圓角由系統處理。
// - icon-maskable-192.png、icon-maskable-512.png（purpose: maskable）：滿版底色，圖形縮到 0.7 倍，
//   最外緣（環的外緣，半徑 (38 + 5.25) × 0.7 ≈ 30.3）落在中心直徑 80%（半徑 40）的安全圓內。
// - apple-touch-icon.png：180×180、不透明（iOS 會自己裁圓角；透明的角會被填成黑色）。

import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

const STATIC = fileURLToPath(new URL('../static/', import.meta.url));

const COLORS = {
  bg: [0x0b, 0x0d, 0x11],
  ring: [0xe9, 0xed, 0xf4],
  left: [0xf2, 0x67, 0x9d],
  right: [0x4f, 0xb8, 0xf5],
};

/** Logo.svelte 的圖形（以 100 為邊長的座標）。 */
const LOGO = {
  ring: { cx: 50, cy: 50, r: 38, width: 10.5 },
  dots: [
    { cx: 36, cy: 50, r: 10, color: COLORS.left },
    { cx: 64, cy: 50, r: 10, color: COLORS.right },
  ],
};

const ICONS = [
  { file: 'icon-192.png', size: 192, scale: 0.85 },
  { file: 'icon-512.png', size: 512, scale: 0.85 },
  { file: 'icon-maskable-192.png', size: 192, scale: 0.7 },
  { file: 'icon-maskable-512.png', size: 512, scale: 0.7 },
  { file: 'apple-touch-icon.png', size: 180, scale: 0.85 },
];

const SAMPLES = 8;

/** 某一點（100 單位座標）的顏色：後畫的蓋前面（底 → 環 → 圓點）。 */
function colorAt(x, y) {
  for (const dot of LOGO.dots) {
    if ((x - dot.cx) ** 2 + (y - dot.cy) ** 2 <= dot.r ** 2) return dot.color;
  }
  const { cx, cy, r, width } = LOGO.ring;
  const d = Math.hypot(x - cx, y - cy);
  if (Math.abs(d - r) <= width / 2) return COLORS.ring;
  return COLORS.bg;
}

function render(size, scale) {
  const rgb = Buffer.alloc(size * size * 3);
  const unit = 100 / size;
  for (let py = 0; py < size; py += 1) {
    for (let px = 0; px < size; px += 1) {
      let r = 0;
      let g = 0;
      let b = 0;
      for (let sy = 0; sy < SAMPLES; sy += 1) {
        for (let sx = 0; sx < SAMPLES; sx += 1) {
          // 像素內的取樣點 → 100 單位座標 → 以中心縮放回 Logo 的座標。
          const x = (px + (sx + 0.5) / SAMPLES) * unit;
          const y = (py + (sy + 0.5) / SAMPLES) * unit;
          const c = colorAt(50 + (x - 50) / scale, 50 + (y - 50) / scale);
          r += c[0];
          g += c[1];
          b += c[2];
        }
      }
      const n = SAMPLES * SAMPLES;
      const at = (py * size + px) * 3;
      rgb[at] = Math.round(r / n);
      rgb[at + 1] = Math.round(g / n);
      rgb[at + 2] = Math.round(b / n);
    }
  }
  return rgb;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

/** 8-bit RGB（不透明）PNG。 */
function encodePng(size, rgb) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(size, 0);
  header.writeUInt32BE(size, 4);
  header[8] = 8; // bit depth
  header[9] = 2; // color type: RGB
  header[10] = 0;
  header[11] = 0;
  header[12] = 0;
  const stride = size * 3;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y += 1) {
    raw[y * (stride + 1)] = 0; // filter: none
    rgb.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

for (const icon of ICONS) {
  const png = encodePng(icon.size, render(icon.size, icon.scale));
  writeFileSync(STATIC + icon.file, png);
  console.log(`[icons] ${icon.file}  ${icon.size}×${icon.size}  ${(png.length / 1024).toFixed(1)} kB`);
}
