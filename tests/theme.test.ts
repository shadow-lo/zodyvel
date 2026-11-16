import { describe, expect, it } from 'vitest';
import { SURFACES, THEMES, themeVars } from '../src/lib/theme';

describe('tema', () => {
  it('seçilen tema ve zemin CSS değişkenlerine dönüşür', () => {
    expect(themeVars('okyanus', 'lacivert')).toMatchObject({ '--accent': '#4fa8ff', '--accent-2': '#5ee6d0', '--island': '#0a1020' });
  });

  it('bilinmeyen değer varsayılana döner (Gece, Siyah)', () => {
    expect(themeVars('neon', '???')).toMatchObject({ '--accent': THEMES[0]!.accent, '--island': SURFACES[0]!.island });
  });

  it("kimlikler Rust'taki listeyle aynı", () => {
    expect(THEMES.map(t => t.id)).toEqual(['gece', 'okyanus', 'gunbatimi', 'orman', 'gul', 'grafit']);
    expect(SURFACES.map(s => s.id)).toEqual(['siyah', 'grafit', 'lacivert']);
  });
});
