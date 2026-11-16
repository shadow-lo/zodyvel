import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { NOTCH, WINDOW } from '../src/island/levels';
import { DEMO_SCREENS, missingScreen, SIZE } from '../src/lib/screen';

// Ada geometrisinin tek kaynağı: arayüz ölçüleri levels.ts'te, ekran ölçeği ve ada boyutu çarpanı Rust'ta.
// İki tarafın aynı sayıları kullandığı burada denetlenir.
const island = readFileSync(new URL('../src-tauri/src/island.rs', import.meta.url), 'utf8');
const display = readFileSync(new URL('../src-tauri/src/display.rs', import.meta.url), 'utf8');

function rustConst(source: string, name: string): number {
  const m = source.match(new RegExp(`const ${name}: (?:f64|u32) = ([\\d.]+);`));
  if (!m) throw new Error(`${name} yok`);
  return Number(m[1]);
}

describe('ada ölçüleri Rust ile aynı', () => {
  it('pencere ve şerit', () => {
    expect(rustConst(island, 'WINDOW_W')).toBe(WINDOW.w);
    expect(rustConst(island, 'WINDOW_H')).toBe(WINDOW.h);
    expect(rustConst(island, 'NOTCH_W')).toBe(NOTCH.w);
    expect(rustConst(island, 'NOTCH_H')).toBe(NOTCH.h);
  });

  it('ada boyutu aralığı', () => {
    expect(rustConst(display, 'SIZE_MIN')).toBe(SIZE.min);
    expect(rustConst(display, 'SIZE_MAX')).toBe(SIZE.max);
    expect(rustConst(display, 'SIZE_STEP')).toBe(SIZE.step);
    expect((SIZE.max - SIZE.min) % SIZE.step).toBe(0);
  });

  it('ekran kipleri', () => {
    expect(display).toContain('NAMES: [&\'static str; 3] = ["primary", "fixed", "cursor"]');
  });
});

describe('seçilen ekran', () => {
  it('bağlı değilse uyarılır', () => {
    expect(missingScreen(DEMO_SCREENS, '\\\\.\\DISPLAY2')).toBe(false);
    expect(missingScreen(DEMO_SCREENS, '\\\\.\\DISPLAY9')).toBe(true);
    // Henüz seçilmedi ya da liste okunamadı: uyarı yok.
    expect(missingScreen(DEMO_SCREENS, '')).toBe(false);
    expect(missingScreen([], '\\\\.\\DISPLAY9')).toBe(false);
  });
});
