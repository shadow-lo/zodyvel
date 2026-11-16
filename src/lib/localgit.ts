// Yerel Git (GitHub › "Depolarım"): Rust `localgit` modülüyle aynı tipler ve saf kararlar (sıra, varsayılan
// seçim, onay metni, harfler). Tauri'ye bağlı değil, vitest ile sınanır. Canlı durum `localgit.svelte.ts`'te.
//
// Güvenlik: commit ve push yalnız onay kartından; hassas görünen dosyalar (.env, anahtar, şifre) varsayılan
// olarak seçilmez ve seçilirse ayrıca onay ister. Dosya içeriği hiç okunmaz.

import type { RepoLine } from './sources';

export type ChangeKind = 'added' | 'modified' | 'deleted' | 'renamed' | 'untracked' | 'conflict';

export interface FileChange {
  /** Depo köküne göre, `/` ile. */
  path: string;
  orig: string | null;
  kind: ChangeKind;
  sensitive: boolean;
  /** 50 MB üstü. */
  big: boolean;
}

export interface RepoView {
  path: string;
  name: string;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
  /** Listede gösterilmeyen değişiklik sayısı. */
  more: number;
  /** GitHub "sahip/depo" (küçük harf). */
  slug: string | null;
  error: string | null;
}

export interface Overview {
  git: boolean;
  repos: RepoView[];
}

export interface CommitResult {
  sha: string;
  pushed: boolean;
  pushError: string | null;
}

/** Dosya satırındaki harf ve açıklaması. */
export const KIND_LETTER: Record<ChangeKind, { letter: string; label: string }> = {
  added: { letter: 'Y', label: 'Yeni' },
  untracked: { letter: 'Y', label: 'Yeni' },
  modified: { letter: 'D', label: 'Değişti' },
  deleted: { letter: 'S', label: 'Silindi' },
  renamed: { letter: 'A', label: 'Adı değişti' },
  conflict: { letter: 'Ç', label: 'Çakışma' },
};

export function changes(r: RepoView): number {
  return r.files.length + r.more;
}

/** Değişikliği olan depo sayısı (GitHub kutucuğunun rozetine eklenir). */
export function dirtyCount(o: Overview | null): number {
  return o ? o.repos.filter(r => changes(r) > 0).length : 0;
}

/** Sıra: değişikliği olanlar (çoktan aza), gönderilmemiş commiti olanlar, hata verenler, sonra temizler (ada göre). */
export function sortRepos(repos: RepoView[]): RepoView[] {
  const rank = (r: RepoView) => (changes(r) > 0 ? 0 : r.ahead > 0 ? 1 : r.error ? 2 : 3);
  return [...repos].sort((a, b) => rank(a) - rank(b) || changes(b) - changes(a) || a.name.localeCompare(b.name, 'tr'));
}

/** Satırın kısa durumu: "5 değişiklik · ↑2 gönderilmedi", "temiz". */
export function repoNote(r: RepoView): string {
  if (r.error) return r.error;
  const parts: string[] = [];
  const n = changes(r);
  if (n) parts.push(`${n} değişiklik`);
  if (r.ahead) parts.push(`↑${r.ahead} gönderilmedi`);
  if (r.behind) parts.push(`↓${r.behind} GitHub'da yeni`);
  if (!r.upstream && r.branch && n === 0 && r.ahead === 0) parts.push('GitHub’da yok');
  return parts.length ? parts.join(' · ') : 'temiz';
}

/** Açılışta seçili dosyalar: hassas görünenler ve çakışmalar hariç hepsi. */
export function defaultPicked(r: RepoView): string[] {
  return r.files.filter(f => !f.sensitive && f.kind !== 'conflict').map(f => f.path);
}

/** Depo GitHub'da herkese açık mı (son push edilen depolar listesinden; bilinmiyorsa null). */
export function visibility(r: RepoView, remote: RepoLine[] | undefined): 'public' | 'private' | null {
  if (!r.slug) return null;
  const hit = remote?.find(x => x.slug.toLowerCase() === r.slug);
  return hit ? (hit.private ? 'private' : 'public') : null;
}

export interface Ask {
  kind: 'commit' | 'push' | 'discard' | 'pull';
  /** Kartın sorusu. */
  text: string;
  /** Kırmızı uyarılar (hassas dosya, büyük dosya, herkese açık depo). */
  warnings: string[];
}

/** Commit / push onay kartı. `push`: committen sonra gönderilsin mi. */
export function askFor(r: RepoView, picked: string[], push: boolean, remote: RepoLine[] | undefined): Ask {
  const branch = r.branch ?? 'ayrık HEAD';
  const vis = visibility(r, remote);
  const where = vis === 'public' ? 'herkese açık' : vis === 'private' ? 'gizli' : null;
  const warnings: string[] = [];
  const chosen = r.files.filter(f => picked.includes(f.path));
  const secret = chosen.filter(f => f.sensitive);
  if (secret.length) warnings.push(`Hassas görünen dosya: ${secret.map(f => f.path).join(', ')}`);
  const big = chosen.filter(f => f.big);
  if (big.length) warnings.push(`50 MB üstü: ${big.map(f => f.path).join(', ')} (GitHub 100 MB üstünü reddeder)`);
  if (push && vis === 'public') warnings.push('Bu depo herkese açık: gönderilen her şeyi herkes görebilir.');
  if (picked.length === 0) {
    const n = r.ahead;
    return {
      kind: 'push',
      text: `${r.name} · ${branch}: ${n} commit GitHub’a${where ? ` (${where} depo)` : ''} gönderilsin mi?`,
      warnings: vis === 'public' ? ['Bu depo herkese açık: gönderilen her şeyi herkes görebilir.'] : [],
    };
  }
  const files = `${picked.length} dosya`;
  return {
    kind: 'commit',
    text: push
      ? `${r.name} · ${branch}: ${files} commitlenip GitHub’a${where ? ` (${where} depo)` : ''} gönderilsin mi?`
      : `${r.name} · ${branch}: ${files} commitlensin mi?`,
    warnings,
  };
}

/** Geri al (discard) onayı: seçilen dosyaların şimdiki hâli Geri Dönüşüm Kutusu'na, izlenenler son commit'e döner. */
export function askDiscard(r: RepoView, picked: string[]): Ask {
  const chosen = r.files.filter(f => picked.includes(f.path));
  const fresh = chosen.filter(f => f.kind === 'untracked' || f.kind === 'added').map(f => f.path);
  const warnings = ['Değişiklikler kaybolur; dosyaların şimdiki hâli Geri Dönüşüm Kutusu’na taşınır, gerekirse oradan geri getirirsin.'];
  if (fresh.length) warnings.push(`Yeni dosyalar diskten kalkar: ${fresh.slice(0, 3).join(', ')}${fresh.length > 3 ? ` ve ${fresh.length - 3} dosya daha` : ''}`);
  return { kind: 'discard', text: `${r.name}: ${chosen.length} dosyadaki değişiklik geri alınsın mı?`, warnings };
}

/** Pull onayı: yalnız ileri sarma (birleştirme yok, commitlenmemiş dosyanın üzerine yazılmaz). */
export function askPull(r: RepoView): Ask {
  const warnings = r.ahead > 0 ? [`Sende de gönderilmemiş ${r.ahead} commit var; dallar ayrıştıysa Pull durur, birleştirmeyi GitHub Desktop’ta yaparsın.`] : [];
  return { kind: 'pull', text: `${r.name} · ${r.branch ?? ''}: GitHub’daki ${r.behind} yeni commit alınsın mı?`, warnings };
}

/** Pull düğmesi: dal yayınlanmış ve GitHub'da yeni commit var (Fetch'ten sonra görünür). */
export function canPull(r: RepoView): boolean {
  return !!r.upstream && !!r.branch && r.behind > 0;
}

/** Fetch düğmesi: deponun GitHub'da bir uzak dalı var. */
export function canFetch(r: RepoView): boolean {
  return !!r.upstream;
}

/** Push düğmesi gösterilsin mi: dal GitHub'a yayınlanmış ve gönderilmemiş commit var. */
export function canPush(r: RepoView): boolean {
  return !!r.upstream && !!r.branch && r.ahead > 0;
}

/** Commit + Push düğmesi: dal yayınlanmış olmalı (yoksa yalnız Commit). */
export function canCommitPush(r: RepoView): boolean {
  return !!r.upstream && !!r.branch;
}

/** Depolarım sekmesinin GitHub listesi: bilgisayarda klonlu olanlar işaretli, en yeni push önde. */
export function remoteRows(remote: RepoLine[] | undefined, local: RepoView[]): (RepoLine & { cloned: boolean })[] {
  const slugs = new Set(local.map(r => r.slug).filter(Boolean));
  return [...(remote ?? [])].sort((a, b) => b.timeMs - a.timeMs).map(r => ({ ...r, cloned: slugs.has(r.slug.toLowerCase()) }));
}

/** "GitHub Desktop'tan al": bulunan depo yollarından eklenecek klasörler. Aynı üst klasörde iki ya da daha çok
 *  depo varsa üst klasör (o klasördeki yeni depolar da görünür), yoksa deponun kendisi. Zaten eklenmiş klasörün
 *  altında kalanlar atlanır. Windows yolları (`\`) büyük/küçük harf duyarsız karşılaştırılır. */
export function foldersFrom(paths: string[], existing: string[]): string[] {
  const norm = (p: string) => p.replace(/[\\/]+$/, '').toLowerCase();
  const parent = (p: string) => p.replace(/[\\/]+$/, '').replace(/[\\/][^\\/]+$/, '');
  const under = (p: string, dir: string) => norm(p) === norm(dir) || norm(p).startsWith(norm(dir) + '\\') || norm(p).startsWith(norm(dir) + '/');
  const fresh = paths.filter(p => !existing.some(e => under(p, e)));
  const byParent = new Map<string, string[]>();
  for (const p of fresh) {
    const key = norm(parent(p));
    byParent.set(key, [...(byParent.get(key) ?? []), p]);
  }
  const out: string[] = [];
  for (const group of byParent.values()) {
    const add = group.length > 1 ? [parent(group[0] ?? '')] : group;
    for (const a of add) if (!out.some(o => norm(o) === norm(a))) out.push(a);
  }
  return out;
}
