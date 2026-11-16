import { afterEach, describe, expect, it, vi } from 'vitest';
import { applyCoverTone, coverTone } from '../src/lib/cover';
import { defaultFeatures } from '../src/lib/types';

/** 8×8 RGBA; `fill` her piksel için [r, g, b, a]. */
const pixels = (fill: (i: number) => [number, number, number, number]) => Uint8ClampedArray.from({ length: 256 }, (_, k) => fill(k >> 2)[k & 3]!);

function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255).map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * bl!;
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

const hue = (hex: string) => {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16));
  return r! > g! && r! > b! ? 'red' : g! > b! ? 'green' : 'blue';
};

describe('kapaktan renk', () => {
  it('baskın rengi bulur; doygun pikseller koyu ve gri alanlardan ağır basar', () => {
    // Çoğu siyah-gri, dörtte biri kırmızı bir kapak: ton kırmızı.
    const tone = coverTone(pixels(i => (i % 4 === 0 ? [220, 40, 50, 255] : i % 2 ? [8, 8, 8, 255] : [120, 120, 125, 255])));
    expect(tone && hue(tone.color)).toBe('red');
    expect(hue(coverTone(pixels(() => [30, 90, 200, 255]))!.color)).toBe('blue');
    // Mavi zeminde sarı güneş: iki rengin ortalaması (yeşilimsi gri) değil, baskın olan mavi.
    expect(hue(coverTone(pixels(i => (i % 5 === 0 ? [242, 200, 75, 255] : [29, 111, 216, 255])))!.color)).toBe('blue');
  });

  it('renksiz, saydam ya da tümüyle koyu kapak kartı boyamaz', () => {
    expect(coverTone(pixels(() => [128, 128, 128, 255]))).toBeNull();
    expect(coverTone(pixels(() => [255, 0, 0, 0]))).toBeNull();
    expect(coverTone(pixels(() => [5, 5, 5, 255]))).toBeNull();
    expect(coverTone(pixels(() => [250, 250, 250, 255]))).toBeNull();
  });

  it('ton koyu adada okunur, düğme yazısı üstünde rahat okunur (≥ 7)', () => {
    for (const rgb of [[220, 40, 50], [30, 90, 200], [240, 220, 30], [40, 160, 70], [120, 20, 120]] as const) {
      const tone = coverTone(pixels(() => [...rgb, 255]))!;
      expect(contrast(tone.color, tone.ink), rgb.join()).toBeGreaterThanOrEqual(7);
      expect(contrast(tone.color, '#14161d'), rgb.join()).toBeGreaterThanOrEqual(4.5);
    }
  });
});

describe('kapaktan renk açılıp kapanır', () => {
  const style = { setProperty: vi.fn(), removeProperty: vi.fn() };
  const image = vi.fn();
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('varsayılan açık; kapalıyken kapak okunmaz ve renk silinir', async () => {
    expect(defaultFeatures().coverTint).toBe(true);
    vi.stubGlobal('document', { documentElement: { style } });
    vi.stubGlobal('Image', image);
    await applyCoverTone('data:image/png;base64,AAAA', false);
    await applyCoverTone(null, true);
    expect(image).not.toHaveBeenCalled();
    expect(style.setProperty).not.toHaveBeenCalled();
    expect(style.removeProperty).toHaveBeenCalledWith('--cover');
  });
});
