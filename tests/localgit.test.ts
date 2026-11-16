import { describe, expect, it } from 'vitest';
import { askDiscard, askFor, askPull, canCommitPush, canFetch, canPull, canPush, defaultPicked, dirtyCount, foldersFrom, remoteRows, repoNote, sortRepos, visibility, type FileChange, type RepoView } from '../src/lib/localgit';
import { tiles, idleState, type RepoLine, type SourceState } from '../src/lib/sources';

const file = (path: string, kind: FileChange['kind'] = 'modified', extra: Partial<FileChange> = {}): FileChange => ({ path, orig: null, kind, sensitive: false, big: false, ...extra });
const repo = (name: string, extra: Partial<RepoView> = {}): RepoView => ({
  path: `C:\\work\\${name}`,
  name,
  branch: 'main',
  upstream: 'origin/main',
  ahead: 0,
  behind: 0,
  files: [],
  more: 0,
  slug: `ornek/${name.toLowerCase()}`,
  error: null,
  ...extra,
});
const line = (slug: string, priv: boolean, timeMs = 0): RepoLine => ({ slug, url: `https://github.com/${slug}`, timeMs, private: priv, branch: 'main', ci: 'none' });

describe('yerel git', () => {
  it('sıra: değişiklikli (çoktan aza), gönderilmemiş, hatalı, temiz', () => {
    const list = [repo('Temiz'), repo('Hata', { error: 'x' }), repo('Bir', { files: [file('a')] }), repo('Ileri', { ahead: 2 }), repo('Uc', { files: [file('a'), file('b')], more: 1 })];
    expect(sortRepos(list).map(r => r.name)).toEqual(['Uc', 'Bir', 'Ileri', 'Hata', 'Temiz']);
    expect(dirtyCount({ git: true, repos: list })).toBe(2);
    expect(dirtyCount(null)).toBe(0);
  });

  it('kısa durum', () => {
    expect(repoNote(repo('a', { files: [file('x')], more: 4, ahead: 2, behind: 1 }))).toBe("5 değişiklik · ↑2 gönderilmedi · ↓1 GitHub'da yeni");
    expect(repoNote(repo('a'))).toBe('temiz');
    expect(repoNote(repo('a', { upstream: null }))).toBe('GitHub’da yok');
    expect(repoNote(repo('a', { error: 'Git zamanında bitmedi' }))).toBe('Git zamanında bitmedi');
  });

  it('hassas dosya ve çakışma varsayılan seçilmez', () => {
    const r = repo('a', { files: [file('src/a.ts'), file('.env', 'untracked', { sensitive: true }), file('x.txt', 'conflict')] });
    expect(defaultPicked(r)).toEqual(['src/a.ts']);
  });

  it('onay kartı: dal, dosya sayısı, gizlilik ve uyarılar', () => {
    const r = repo('Zodyvel', { files: [file('a.ts'), file('.env', 'untracked', { sensitive: true }), file('big.bin', 'untracked', { big: true })], ahead: 1 });
    const pub = [line('ornek/zodyvel', false)];
    const commit = askFor(r, ['a.ts'], false, pub);
    expect(commit).toEqual({ kind: 'commit', text: 'Zodyvel · main: 1 dosya commitlensin mi?', warnings: [] });
    const all = askFor(r, ['a.ts', '.env', 'big.bin'], true, pub);
    expect(all.text).toBe('Zodyvel · main: 3 dosya commitlenip GitHub’a (herkese açık depo) gönderilsin mi?');
    expect(all.warnings).toHaveLength(3);
    expect(all.warnings[0]).toContain('.env');
    const push = askFor(r, [], true, [line('ornek/zodyvel', true)]);
    expect(push).toEqual({ kind: 'push', text: 'Zodyvel · main: 1 commit GitHub’a (gizli depo) gönderilsin mi?', warnings: [] });
    expect(askFor(r, [], true, undefined).text).toBe('Zodyvel · main: 1 commit GitHub’a gönderilsin mi?');
    expect(visibility(repo('b', { slug: null }), pub)).toBeNull();
  });

  it('push düğmeleri yalnız yayınlanmış dalda', () => {
    expect(canPush(repo('a', { ahead: 1 }))).toBe(true);
    expect(canPush(repo('a', { ahead: 0 }))).toBe(false);
    expect(canPush(repo('a', { ahead: 1, upstream: null }))).toBe(false);
    expect(canCommitPush(repo('a', { branch: null }))).toBe(false);
  });

  it('GitHub listesi: en yeni önde, klonlular işaretli', () => {
    const rows = remoteRows([line('Ornek/Api', true, 1), line('ornek/zodyvel', false, 5)], [repo('Zodyvel')]);
    expect(rows.map(r => [r.slug, r.cloned])).toEqual([
      ['ornek/zodyvel', true],
      ['Ornek/Api', false],
    ]);
  });

  it('GitHub Desktop klasörleri: aynı üstte iki depo üst klasör olur, eklenmiş klasörün altı atlanır', () => {
    const found = ['C:\\work\\Api', 'C:\\work\\Psrc', 'D:\\Oyun\\Barinak', 'C:\\Users\\a\\Kod\\x', 'E:\\Eski\\y'];
    expect(foldersFrom(found, ['c:\\users\\a\\kod'])).toEqual(['C:\\work', 'D:\\Oyun\\Barinak', 'E:\\Eski\\y']);
    expect(foldersFrom(['C:\\work\\Api'], ['C:\\work\\'])).toEqual([]);
  });

  it('GitHub kutucuğu yerel değişiklikleri sayar; token olmadan da', () => {
    const gh = (s: Partial<SourceState>): SourceState => ({ ...idleState('github', 'ok'), ...s });
    const tile = (s: SourceState, localDirty: number) => tiles({ agents: [], waiting: [], sources: [s], enabled: ['github'], localDirty })[0];
    expect(tile(gh({ badge: 2 }), 3)).toMatchObject({ count: 5, tone: 'attention' });
    expect(tile(idleState('github', 'needs_setup'), 2)).toMatchObject({ count: 2, tone: 'attention', note: '' });
    expect(tile(idleState('github', 'needs_setup'), 0)).toMatchObject({ count: 0, note: 'bağla' });
    expect(tile(gh({ status: 'error' }), 2)).toMatchObject({ tone: 'alert' });
  });

  it('geri al ve pull onayı', () => {
    const r = repo('Oyun', { files: [file('a.cs'), file('yeni.cs', 'untracked'), file('b.cs')], behind: 3 });
    const d = askDiscard(r, ['a.cs', 'yeni.cs']);
    expect(d.kind).toBe('discard');
    expect(d.text).toBe('Oyun: 2 dosyadaki değişiklik geri alınsın mı?');
    expect(d.warnings[0]).toContain('Geri Dönüşüm Kutusu');
    expect(d.warnings[1]).toBe('Yeni dosyalar diskten kalkar: yeni.cs');
    expect(askDiscard(r, ['a.cs']).warnings).toHaveLength(1);
    expect(askPull(r).text).toBe('Oyun · main: GitHub’daki 3 yeni commit alınsın mı?');
    expect(askPull(r).warnings).toEqual([]);
    expect(askPull(repo('x', { ahead: 1, behind: 1 })).warnings[0]).toContain('GitHub Desktop');
    expect(canPull(r)).toBe(true);
    expect(canPull(repo('x'))).toBe(false);
    expect(canPull(repo('x', { behind: 1, upstream: null }))).toBe(false);
    expect(canFetch(repo('x'))).toBe(true);
    expect(canFetch(repo('x', { upstream: null }))).toBe(false);
  });
});
