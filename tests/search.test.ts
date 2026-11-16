import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { buildIndex, fold, rowsOf, search } from '../src/lib/search';
import { THEMES } from '../src/lib/theme';
import { GROUPS, PAGES, SOURCES } from '../src/settings/pages';

const page = (file: string) => readFileSync(new URL(`../src/settings/pages/${file}.svelte`, import.meta.url), 'utf8');
const sources = Object.fromEntries(PAGES.map(p => [p.id, page(SOURCES[p.id])]));
const themes = THEMES.map(t => ({ page: 'gorunum', group: 'Tema', title: t.name, hint: t.note }));
const index = buildIndex(PAGES, sources, themes);

describe('arama: Türkçe katlama', () => {
  it('büyük/küçük harf, ı/i ve aksanlar', () => {
    expect(fold('İĞNE ŞEKER ÇÖP ÜZÜM IŞIK')).toBe('igne seker cop uzum isik');
    expect(fold('  ışık  Ada ')).toBe('isik ada');
    expect(fold('Gün batımı')).toBe('gun batimi');
  });

  it('klavyede aksansız yazılan sorgu bulur', () => {
    expect(search(index, 'gorunum')[0]?.title).toBe('Görünüm');
    expect(search(index, 'ozellikler')[0]?.title).toBe('Özellikler');
    expect(search(index, 'ISIK').length).toBe(search(index, 'ışık').length);
    expect(search(index, 'gun batimi')[0]?.title).toBe('Gün batımı');
  });
});

describe('arama dizini sayfa kaynağından üretilir', () => {
  it('her sayfanın dosyası eşlemede var ve pages/ klasöründe okunur', () => {
    for (const p of PAGES) expect(sources[p.id]?.length, p.id).toBeGreaterThan(100);
    expect(Object.keys(SOURCES).sort()).toEqual(PAGES.map(p => p.id).sort());
  });

  it('Yeni görünümün menü grupları her sayfayı bir kez, menü sırasıyla içerir', () => {
    expect(GROUPS.flatMap(g => g.pages)).toEqual(PAGES.map(p => p.id));
  });

  it('Özellikler satırları grup başlığıyla gelir', () => {
    const shelf = index.find(e => e.title === 'Dosya rafı');
    expect(shelf?.page).toBe('ozellikler');
    expect(shelf?.group).toBe('Dosyalar');
    expect(index.some(e => e.title === 'Plan onayı adada' && e.group === 'Claude')).toBe(true);
  });

  it('her sayfanın kendisi ve her <b> başlıklı satırı dizinde', () => {
    for (const p of PAGES) {
      expect(index.some(e => e.kind === 'page' && e.page === p.id), p.id).toBe(true);
      for (const row of rowsOf(sources[p.id]!)) {
        expect(index.some(e => e.page === p.id && e.title === row.title), `${p.id}: ${row.title}`).toBe(true);
      }
    }
  });

  it('yeni eklenen bir satır kendiliğinden bulunur (dizin elle güncellenmez)', () => {
    const extra = `${sources.ozellikler}\n<section class="group"><h3>Yeni grup</h3><label class="row"><span class="tx"><b>Parlak şerit</b><small>Deneme açıklaması</small></span></label></section>`;
    const next = buildIndex(PAGES, { ...sources, ozellikler: extra });
    const hit = search(next, 'parlak serit')[0];
    expect(hit?.title).toBe('Parlak şerit');
    expect(hit?.group).toBe('Yeni grup');
    expect(hit?.page).toBe('ozellikler');
  });

  it('kodla üretilen {…} başlıklar atlanır, açıklamadaki ifadeler temizlenir', () => {
    const rows = rowsOf('<h3>G</h3><div class="tx"><b>{t.name}</b><small>x</small></div><div class="tx"><b>Gerçek</b><small>{a} bir <i>şey</i></small></div>');
    expect(rows).toEqual([{ group: 'G', title: 'Gerçek', hint: 'bir şey' }]);
  });

  it('IslandPage gibi satır sonuna bölünmüş etiketler okunur', () => {
    const open = index.find(e => e.title === 'Açılış');
    expect(open?.page).toBe('ada');
    expect(open?.hint).toContain('Üzerine gelince');
  });
});

describe('arama sıralaması', () => {
  it('ad eşleşmesi açıklama eşleşmesinden önce gelir; sayfa satırdan önce', () => {
    const hits = search(index, 'ada');
    expect(hits[0]?.kind).toBe('page');
    expect(hits[0]?.title).toBe('Ada');
  });

  it('boş sorgu sonuç vermez; çok sözcük hepsini ister', () => {
    expect(search(index, '  ')).toEqual([]);
    expect(search(index, 'dosya rafi').some(e => e.title === 'Dosya rafı')).toBe(true);
    expect(search(index, 'dosya zzzzqq')).toEqual([]);
  });

  it('en çok 8 sonuç', () => {
    expect(search(index, 'a').length).toBeLessThanOrEqual(8);
  });

  it('tema adı ve açıklaması Görünüm sayfasına götürür', () => {
    const hit = search(index, 'okyanus')[0];
    expect(hit?.page).toBe('gorunum');
    expect(hit?.group).toBe('Tema');
  });
});
