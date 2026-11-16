// Ayarlarda arama. Dizin elle yazılmaz: Ayarlar sayfalarının kaynağındaki grup başlıkları (<h3>) ve ayar
// satırlarının adı/açıklaması (<b>…</b><small>…</small>) okunur; yeni bir satır eklenince kendiliğinden girer.
// Eşleştirme Türkçe harfleri katlar: "ışık" = "ISIK" = "isik", "şeker" = "seker", "İğne" = "igne".

export interface PageInfo {
  id: string;
  name: string;
}

export interface Entry {
  /** page: yalnız sayfa · row: sayfadaki bir ayar. */
  kind: 'page' | 'row';
  page: string;
  pageName: string;
  /** Satırın grup başlığı ("Claude"); sayfa girdisinde boş. */
  group: string;
  title: string;
  hint: string;
  /** Katlanmış arama metinleri. */
  fTitle: string;
  fHint: string;
  fWhere: string;
}

export type Hit = Entry & { score: number };

/** Küçük harfe, aksansız ve ı→i yapar; eşleştirme bununla yapılır. */
export function fold(text: string): string {
  return text
    .toLocaleLowerCase('tr')
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .replace(/ı/g, 'i')
    .replace(/\s+/g, ' ')
    .trim();
}

const clean = (html: string) =>
  html
    .replace(/\{[^{}]*\}/g, ' ')
    .replace(/<[^>]*>/g, ' ')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/\s+/g, ' ')
    .trim();

/** Bir sayfanın kaynağından ayar satırları. `{…}` içeren (kodla üretilen) başlıklar atlanır. */
export function rowsOf(source: string): { group: string; title: string; hint: string }[] {
  const out: { group: string; title: string; hint: string }[] = [];
  let group = '';
  const token = /<h3>([\s\S]*?)<\/h3>|<b>([^<>{}]+)<\/b>\s*(?:<small[^>]*>([\s\S]*?)<\/small\s*>)?/g;
  for (const m of source.matchAll(token)) {
    if (m[1] !== undefined) {
      group = clean(m[1]);
    } else if (m[2] !== undefined) {
      const title = clean(m[2]);
      if (title) out.push({ group, title, hint: clean(m[3] ?? '') });
    }
  }
  return out;
}

function entry(kind: Entry['kind'], page: PageInfo, group: string, title: string, hint: string): Entry {
  return {
    kind,
    page: page.id,
    pageName: page.name,
    group,
    title,
    hint,
    fTitle: fold(title),
    fHint: fold(hint),
    fWhere: fold(`${page.name} ${group}`),
  };
}

/**
 * `sources`: sayfa kimliği → .svelte kaynağı. `extra`: kodla üretilen satırlar (ör. tema adları).
 * Sayfalar sırayla, her sayfanın önce kendisi sonra satırları eklenir.
 */
export function buildIndex(pages: PageInfo[], sources: Record<string, string>, extra: { page: string; group: string; title: string; hint?: string }[] = []): Entry[] {
  const index: Entry[] = [];
  for (const page of pages) {
    index.push(entry('page', page, '', page.name, ''));
    for (const row of rowsOf(sources[page.id] ?? '')) index.push(entry('row', page, row.group, row.title, row.hint));
    for (const e of extra.filter(x => x.page === page.id)) index.push(entry('row', page, e.group, e.title, e.hint ?? ''));
  }
  return index;
}

/** Her sözcük bir yerde geçmeli; ad eşleşmesi açıklama eşleşmesinden önce gelir. Eşleşmeyen: null. */
function score(e: Entry, words: string[]): number | null {
  let total = 0;
  for (const w of words) {
    let s: number;
    if (e.fTitle.startsWith(w)) s = 0;
    else if (e.fTitle.split(' ').some(t => t.startsWith(w))) s = 1;
    else if (e.fTitle.includes(w)) s = 2;
    else if (e.fWhere.includes(w)) s = 3;
    else if (e.fHint.includes(w)) s = 4;
    else return null;
    total += s;
  }
  // Sayfa girdisi aynı puanda satırdan önce.
  return total * 2 + (e.kind === 'page' ? 0 : 1);
}

export function search(index: Entry[], query: string, limit = 8): Hit[] {
  const words = fold(query).split(' ').filter(Boolean);
  if (!words.length) return [];
  const hits: Hit[] = [];
  for (const e of index) {
    const s = score(e, words);
    if (s !== null) hits.push({ ...e, score: s });
  }
  // Array.sort kararlı: puanı eşit olanlar sayfa sırasını korur.
  return hits.sort((a, b) => a.score - b.score).slice(0, limit);
}
