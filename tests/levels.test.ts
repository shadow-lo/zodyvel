import { describe, expect, it } from 'vitest';
import { insetFor, LARGEST, MINI, morphArea, NOTCH, OVERSHOOT, radiusFor, sizeFor, WINDOW } from '../src/island/levels';
import { TABS } from '../src/lib/tabs';

describe('ada geometrisi', () => {
  it('kırpma payı adayı pencerenin üst ortasına koyar', () => {
    expect(insetFor(NOTCH)).toEqual({ x: 310, y: 454 });
    expect(insetFor(WINDOW)).toEqual({ x: 0, y: 0 });
  });

  it('büyürken bölge yayın taşma payını da kapsar, pencereyi aşmaz', () => {
    expect(morphArea(NOTCH, MINI)).toEqual({ w: MINI.w + OVERSHOOT.w, h: MINI.h + OVERSHOOT.h });
    const detail = sizeFor(2, 'ai');
    const area = morphArea(MINI, detail);
    expect(area.w).toBeLessThanOrEqual(WINDOW.w);
    expect(area.h).toBeLessThanOrEqual(WINDOW.h);
  });

  it('küçülürken bölge eski alandır, taşma payı yok', () => {
    expect(morphArea(MINI, NOTCH)).toEqual(MINI);
  });

  it('K11: yalnız AI minili; sabit sekmeler her seviyede kendi boyunda', () => {
    expect(sizeFor(1, 'ai')).toEqual(MINI);
    expect(sizeFor(2, 'pc')).toEqual({ w: 720, h: 244 });
    expect(sizeFor(1, 'pc')).toEqual(sizeFor(2, 'pc'));
  });

  it('kayıttaki her boy pencereye sığar (taşma payını morphArea pencereyle sınırlar)', () => {
    for (const t of TABS) {
      expect(t.size.w, t.id).toBeLessThanOrEqual(WINDOW.w);
      expect(t.size.h, t.id).toBeLessThanOrEqual(WINDOW.h);
    }
    expect(LARGEST.h).toBeLessThanOrEqual(WINDOW.h);
  });

  it('köşe yarıçapı boya göre (Rust apply_region ile aynı kural)', () => {
    expect(radiusFor(NOTCH)).toBe(3);
    expect(radiusFor({ w: 300, h: 36 })).toBe(18);
    expect(radiusFor(MINI)).toBe(22);
    expect(radiusFor(sizeFor(2, 'ai'))).toBe(22);
  });

  it('seviye 0: çentik boyu verilirse o, yoksa ince şerit', () => {
    expect(sizeFor(0, 'ai')).toEqual(NOTCH);
    expect(sizeFor(0, 'ai', { w: 320, h: 36 })).toEqual({ w: 320, h: 36 });
  });
});
