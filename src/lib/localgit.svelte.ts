// Yerel Git'in canlı durumu ve komutları (Rust `localgit`). Yoklama yok: depolar yalnız istenince taranır
// (ada açılışında bir kez, "Depolarım" açılınca, "Yenile"de, commit/push'tan sonra). Tarayıcı önizlemesinde
// örnek veri (demo.ts) ve sahte commit.

import { demoLocalRepos } from './demo';
import { callOrThrow, inTauri } from './ipc';
import { dirtyCount, type CommitResult, type Overview } from './localgit';
import { data } from './store.svelte';

export const git = $state({
  overview: null as Overview | null,
  loading: false,
  error: null as string | null,
});

/** Özellik açık mı: Kaynaklar + GitHub + Yerel Git. */
export function localGitOn(): boolean {
  const f = data.settings.features;
  return f.sources && f.sourceGithub && f.localGit;
}

/** GitHub kutucuğuna eklenecek sayı (özellik kapalıysa 0). */
export function localDirty(): number {
  return localGitOn() ? dirtyCount(git.overview) : 0;
}

/** Depoları tarar. Aynı anda ikinci tarama başlamaz. */
export async function loadGit(): Promise<void> {
  if (git.loading || !localGitOn()) return;
  git.loading = true;
  git.error = null;
  try {
    git.overview = inTauri ? await callOrThrow<Overview>('git_overview') : (git.overview ?? demoLocalRepos());
  } catch (e) {
    git.error = String(e);
  } finally {
    git.loading = false;
  }
}

/** Seçilen dosyaları commitler (`push` ise ardından gönderir), sonra yeniden tarar. Hata metni ya da sonuç. */
export async function gitCommit(path: string, message: string, files: string[], allowSensitive: string[], push: boolean): Promise<CommitResult | { error: string }> {
  if (!inTauri) {
    const repo = git.overview?.repos.find(r => r.path === path);
    if (repo) {
      repo.files = repo.files.filter(f => !files.includes(f.path));
      repo.ahead = push ? 0 : repo.ahead + 1;
    }
    return { sha: 'a1b2c3d', pushed: push, pushError: null };
  }
  try {
    const result = await callOrThrow<CommitResult>('git_commit', { path, message, files, allowSensitive, push });
    await loadGit();
    return result;
  } catch (e) {
    return { error: String(e) };
  }
}

/** Gönderilmemiş commitleri gönderir; hata metni ya da null. */
export async function gitPush(path: string): Promise<string | null> {
  if (!inTauri) {
    const repo = git.overview?.repos.find(r => r.path === path);
    if (repo) repo.ahead = 0;
    return null;
  }
  try {
    await callOrThrow('git_push', { path });
    await loadGit();
    return null;
  } catch (e) {
    await loadGit();
    return String(e);
  }
}

/** Seçilen dosyaların değişikliklerini geri alır (şimdiki hâlleri Geri Dönüşüm Kutusu'na); hata metni ya da null. */
export async function gitDiscard(path: string, files: string[]): Promise<string | null> {
  if (!inTauri) {
    const repo = git.overview?.repos.find(r => r.path === path);
    if (repo) repo.files = repo.files.filter(f => !files.includes(f.path));
    return null;
  }
  return run('git_discard', { path, files });
}

/** GitHub'daki durumu indirir (dosyalar değişmez); hata metni ya da null. */
export async function gitFetch(path: string): Promise<string | null> {
  if (!inTauri) {
    const repo = git.overview?.repos.find(r => r.path === path);
    if (repo && repo.upstream) repo.behind = Math.max(repo.behind, 2);
    return null;
  }
  return run('git_fetch', { path });
}

/** GitHub'daki yeni commitleri alır (yalnız ileri sarma); hata metni ya da null. */
export async function gitPull(path: string): Promise<string | null> {
  if (!inTauri) {
    const repo = git.overview?.repos.find(r => r.path === path);
    if (repo) repo.behind = 0;
    return null;
  }
  return run('git_pull', { path });
}

/** Komutu çalıştırır, sonra (hata olsa da) yeniden tarar. */
async function run(command: string, args: Record<string, unknown>): Promise<string | null> {
  let error: string | null = null;
  try {
    await callOrThrow(command, args);
  } catch (e) {
    error = String(e);
  }
  await loadGit();
  return error;
}

export async function gitOpenFolder(path: string): Promise<string | null> {
  if (!inTauri) return null;
  try {
    await callOrThrow('git_open_folder', { path });
    return null;
  } catch (e) {
    return String(e);
  }
}
