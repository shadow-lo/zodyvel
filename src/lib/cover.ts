// Kapaktan renk (Yeni görünüm, "Kapaktan renk" özelliği): kapak değişince bir kez 8×8'e küçültülüp
// baskın renk bulunur ve kökte `--cover` / `--cover-ink` olarak verilir. Sürekli iş yok: aynı kapak için
// yeniden hesaplanmaz; özellik kapalıysa ya da Klasik görünümde hiçbir şey okunmaz ve değişkenler silinir.

/** Kapağın baskın rengi: koyu adada okunur, açık bir ton (#rrggbb) ve üstündeki koyu yazı rengi. */
export interface CoverTone {
  color: string;
  ink: string;
}

const SIDE = 8;

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h / 6, s, l];
}

function hslToHex(h: number, s: number, l: number): string {
  const f = (n: number) => {
    const k = (n + h * 12) % 12;
    const a = s * Math.min(l, 1 - l);
    const c = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(c * 255)
      .toString(16)
      .padStart(2, '0');
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

/**
 * RGBA piksellerden (8×8 = 256 değer) baskın renk: pikseller 12 renk dilimine ayrılır, doygun pikseller
 * ağır basar ve en ağır dilimin ortalaması alınır (mavi + sarı bir kapak yeşilimsi griye dönmez).
 * Neredeyse siyah, beyaz ya da saydam pikseller sayılmaz; kapak gri tonluysa null. Saf, testli.
 */
export function coverTone(pixels: ArrayLike<number>): CoverTone | null {
  const bins = Array.from({ length: 12 }, () => ({ r: 0, g: 0, b: 0, w: 0 }));
  for (let i = 0; i + 3 < pixels.length; i += 4) {
    if ((pixels[i + 3] ?? 0) < 128) continue;
    const pr = (pixels[i] ?? 0) / 255;
    const pg = (pixels[i + 1] ?? 0) / 255;
    const pb = (pixels[i + 2] ?? 0) / 255;
    const max = Math.max(pr, pg, pb);
    const min = Math.min(pr, pg, pb);
    if (max < 0.1 || min > 0.92 || max - min < 0.08) continue;
    const [h] = rgbToHsl(pr, pg, pb);
    const bin = bins[Math.floor(h * 12) % 12]!;
    const w = (max - min) ** 2;
    bin.r += pr * w;
    bin.g += pg * w;
    bin.b += pb * w;
    bin.w += w;
  }
  const top = bins.reduce((a, b) => (b.w > a.w ? b : a));
  if (top.w === 0) return null;
  const [h, s] = rgbToHsl(top.r / top.w, top.g / top.w, top.b / top.w);
  if (s < 0.12) return null;
  // Ada koyu: ton açık ve canlı tutulur, üstündeki yazı aynı tonun çok koyusu (karşıtlık ≥ 7).
  const sat = Math.min(0.85, Math.max(0.55, s));
  return { color: hslToHex(h, sat, 0.7), ink: hslToHex(h, 0.45, 0.07) };
}

let last: { url: string; tone: CoverTone | null } | null = null;

async function measure(url: string): Promise<CoverTone | null> {
  const img = new Image();
  img.src = url;
  await img.decode();
  const canvas = document.createElement('canvas');
  canvas.width = SIDE;
  canvas.height = SIDE;
  const ctx = canvas.getContext('2d', { willReadFrequently: true });
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0, SIDE, SIDE);
  return coverTone(ctx.getImageData(0, 0, SIDE, SIDE).data);
}

let pending = 0;

/** Kapağın tonunu köke yazar (`--cover`, `--cover-ink`); `enabled` değilse ya da kapak yoksa siler. */
export async function applyCoverTone(url: string | null, enabled: boolean): Promise<void> {
  const ticket = ++pending;
  const root = document.documentElement;
  const write = (tone: CoverTone | null) => {
    if (ticket !== pending) return;
    if (tone) {
      root.style.setProperty('--cover', tone.color);
      root.style.setProperty('--cover-ink', tone.ink);
    } else {
      root.style.removeProperty('--cover');
      root.style.removeProperty('--cover-ink');
    }
  };
  if (!enabled || !url) return write(null);
  if (last?.url !== url) {
    let tone: CoverTone | null = null;
    try {
      tone = await measure(url);
    } catch {
      tone = null;
    }
    last = { url, tone };
  }
  write(last.tone);
}
