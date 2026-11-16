// Arama dizini: sayfaların kaynağı derleme sırasında metin olarak alınır (`?raw`), satırlar lib/search.ts'de
// ayıklanır. Çalışma anında sayfalar çizilmez; dizin ilk aramada bir kez kurulur.

import { buildIndex, type Entry } from '../lib/search';
import { THEMES } from '../lib/theme';
import { PAGES, SOURCES } from './pages';

const raw = import.meta.glob('./pages/*.svelte', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

let cached: Entry[] | null = null;

export function index(): Entry[] {
  if (!cached) {
    const sources = Object.fromEntries(PAGES.map(p => [p.id, raw[`./pages/${SOURCES[p.id]}.svelte`] ?? '']));
    // Tema adları kodla üretildiği için kaynaktan okunamaz; elle eklenir.
    const themes = THEMES.map(t => ({ page: 'gorunum', group: 'Tema', title: t.name, hint: t.note }));
    cached = buildIndex(PAGES, sources, themes);
  }
  return cached;
}
