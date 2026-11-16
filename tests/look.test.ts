import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// Tek görünüm (K2): Kara cam jetonları base.css'in ilk :root bloğunda. Eski "Klasik" bloğu kalktı.
const css = readFileSync(new URL('../src/styles/base.css', import.meta.url), 'utf8');

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`${selector} yok`);
  const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start));
  return Object.fromEntries([...body.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(m => [m[1], m[2]!.trim()]));
}

const root = block(':root');

/** WCAG göreli parlaklık karşıtlığı (#rrggbb). */
function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255).map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r! + 0.7152 * g! + 0.0722 * bl!;
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

/** Siyah ada üstünde yarı saydam beyaz cam yüzeyin gerçek rengi ("rgb(255 255 255 / 0.075)"). */
function onBlack(glass: string): string {
  const alpha = parseFloat(glass.split('/')[1]!);
  const v = Math.round(255 * alpha)
    .toString(16)
    .padStart(2, '0');
  return `#${v}${v}${v}`;
}

describe('görünüm jetonları', () => {
  it('eski görünüm bloğu yok', () => {
    expect(css).not.toContain('data-look');
  });

  it('Kara: ada siyah, kartlar yarı saydam cam', () => {
    expect(root['--island']).toBe('#000');
    for (const k of ['--surface', '--surface-2', '--surface-3']) expect(root[k]).toMatch(/^rgb\(255 255 255 \/ 0\.\d+\)$/);
  });

  it('en küçük yazı 10,5 px, ölçek 6 basamağı aşmıyor', () => {
    const sizes = Object.entries(root)
      .filter(([k]) => k.startsWith('--fs-'))
      .map(([, v]) => parseFloat(v));
    expect(Math.min(...sizes)).toBeGreaterThanOrEqual(10.5);
    expect(new Set(sizes).size).toBeLessThanOrEqual(6);
  });

  it('ikincil yazılar her zeminde okunur (AA, ≥ 4,5)', () => {
    for (const text of ['--text-2', '--text-3']) {
      for (const bg of ['--surface', '--surface-2', '--surface-3']) {
        expect(contrast(root[text]!, onBlack(root[bg]!)), `${text} / ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
      expect(contrast(root[text]!, '#000000'), `${text} / ada`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('simge çubuğu düğmesi rahat tıklanır ve çubuğa (36 px) sığar', () => {
    expect(parseFloat(root['--ico-h']!)).toBeGreaterThanOrEqual(30);
    expect(parseFloat(root['--ico-h']!)).toBeLessThanOrEqual(36);
  });
});
