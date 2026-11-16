// Temalar: vurgu rengi (anahtarlar, seçili simge, zamanlayıcı, Zody'nin çerçevesi) ve ada zemini (varsayılan Kara).
// Durum renkleri (onay turuncu, bitti yeşil, hata kırmızı, müzik mercan) her temada aynı kalır.
// Ada ve ayarlar penceresi aynı fonksiyonla boyanır; değişiklik settings://changed ile ikisine de ulaşır.

export interface Theme {
  id: string;
  name: string;
  note: string;
  accent: string;
  accentSoft: string;
  accent2: string;
  onAccent: string;
}

export interface Surface {
  id: string;
  name: string;
  island: string;
  bg: string;
}

export const THEMES: Theme[] = [
  { id: 'gece', name: 'Gece', note: 'mor + turkuaz', accent: '#8b7cff', accentSoft: '#b3a8ff', accent2: '#6fe3cf', onAccent: '#140f33' },
  { id: 'okyanus', name: 'Okyanus', note: 'mavi + deniz yeşili', accent: '#4fa8ff', accentSoft: '#9ccbff', accent2: '#5ee6d0', onAccent: '#04182e' },
  { id: 'gunbatimi', name: 'Gün batımı', note: 'mercan + altın', accent: '#ff8a65', accentSoft: '#ffb9a1', accent2: '#ffc46b', onAccent: '#2b0f08' },
  { id: 'orman', name: 'Orman', note: 'yeşil + limon', accent: '#4fcf86', accentSoft: '#9ee6bb', accent2: '#c4e46b', onAccent: '#052b14' },
  { id: 'gul', name: 'Gül', note: 'pembe + lavanta', accent: '#ff7eb6', accentSoft: '#ffb3d4', accent2: '#b98cff', onAccent: '#2e0a1c' },
  { id: 'grafit', name: 'Grafit', note: 'renksiz, sade', accent: '#c9ceda', accentSoft: '#e3e6ee', accent2: '#8f96a8', onAccent: '#14161d' },
];

export const SURFACES: Surface[] = [
  { id: 'siyah', name: 'Kara', island: '#000', bg: '#0a0a0d' },
  { id: 'grafit', name: 'Grafit', island: '#17191f', bg: '#131519' },
  { id: 'lacivert', name: 'Gece mavisi', island: '#0a1020', bg: '#0c1222' },
];

export const themeById = (id: string): Theme => THEMES.find(t => t.id === id) ?? THEMES[0]!;
export const surfaceById = (id: string): Surface => SURFACES.find(s => s.id === id) ?? SURFACES[0]!;

/** CSS değişkenleri; ada ve ayarlar penceresi `applyTheme` ile kullanır (saf, testli). */
export function themeVars(theme: string, surface: string): Record<string, string> {
  const t = themeById(theme);
  const s = surfaceById(surface);
  return {
    '--accent': t.accent,
    '--accent-soft': t.accentSoft,
    '--accent-2': t.accent2,
    '--on-accent': t.onAccent,
    '--island': s.island,
    '--bg': s.bg,
  };
}

export function applyTheme(theme: string, surface: string): void {
  const root = document.documentElement;
  for (const [name, value] of Object.entries(themeVars(theme, surface))) root.style.setProperty(name, value);
}
