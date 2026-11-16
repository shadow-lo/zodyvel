// GitHub kaynağının görünüm kararları (MiniGithub, DetailGithub, GithubSetup): satır sırası, sekmeler, başlık
// rozeti ve token denetimi. Saf; vitest ile sınanır.

import type { ItemAction, SourceItem, SourceState } from './sources';

/** Satır önceliği: kırık CI › inceleme › bildirim › (yeşile dönen CI). */
const RANK: Record<string, number> = { ci_failed: 0, review: 1, notif: 2 };

export function ordered(items: SourceItem[]): SourceItem[] {
  return [...items].sort((a, b) => (RANK[a.tag ?? ''] ?? 3) - (RANK[b.tag ?? ''] ?? 3) || b.timeMs - a.timeMs);
}

export type GithubTab = 'repos' | 'ci' | 'review' | 'notif';

export const TABS: { id: GithubTab; label: string }[] = [
  { id: 'repos', label: 'Depolarım' },
  { id: 'ci', label: 'PR ve CI' },
  { id: 'review', label: 'İncelemem gerekenler' },
  { id: 'notif', label: 'Bildirimler' },
];

/** Büyük adanın sekmeleri: kırık ve az önce düzelen CI satırları "PR ve CI"de. "Depolarım"ın satırları
 *  kaynak satırı değil (yerel depolar ve son push edilenler), burada boş. */
export function inTab(items: SourceItem[], tab: GithubTab): SourceItem[] {
  if (tab === 'repos') return [];
  const want = (i: SourceItem) => (tab === 'ci' ? i.tag === 'ci_failed' || i.tag === null : i.tag === tab);
  return ordered(items.filter(want));
}

/** Açılışta seçilecek sekme: kırık CI › commitlenmemiş değişiklik (`dirty` depo) › inceleme › bildirim;
 *  hiçbiri yoksa "Depolarım" (boş bir "kırık CI yok" ekranı yerine depolar). */
export function firstTab(items: SourceItem[], dirty = 0): GithubTab {
  const top = ordered(items)[0];
  if (top?.tag === 'ci_failed') return 'ci';
  if (dirty > 0) return 'repos';
  return top?.tag === 'review' ? 'review' : top?.tag === 'notif' ? 'notif' : 'repos';
}

/** Başlıktaki rozet ("CI kırık", "2 inceleme", "3 bildirim") ya da null. */
export function headline(state: SourceState): { text: string; tone: 'red' | 'amber' | 'calm' } | null {
  if (state.status !== 'ok') return null;
  const count = (tag: string) => state.items.filter(i => i.tag === tag).length;
  const ci = count('ci_failed');
  if (ci) return { text: ci > 1 ? `${ci} CI kırık` : 'CI kırık', tone: 'red' };
  const rv = count('review');
  if (rv) return { text: `${rv} inceleme`, tone: 'amber' };
  const nt = count('notif');
  if (nt) return { text: `${nt} bildirim`, tone: 'calm' };
  return null;
}

/** Satır altı düğmeleri (Row'un `actions`'ı); onay isteyenler `confirm`. */
export interface RowButton {
  action: Exclude<ItemAction, 'preview' | 'open_repo'>;
  label: string;
  confirm: string | null;
}

const BUTTONS: Record<RowButton['action'], { label: string; confirm: string | null }> = {
  open: { label: 'Aç', confirm: null },
  mark_read: { label: 'Okundu', confirm: null },
  rerun: { label: 'Yeniden çalıştır', confirm: 'Kırılan işler yeniden çalışsın mı?' },
  fix_with_claude: { label: "Claude'a düzelttir", confirm: 'Claude Code bu depoda CI hatasını düzeltmeye başlasın mı?' },
};

export function buttons(item: SourceItem): RowButton[] {
  return item.actions
    .filter((a): a is RowButton['action'] => a in BUTTONS)
    .map(action => {
      const b = BUTTONS[action];
      return { action, confirm: b.confirm, label: action === 'open' && item.tag === 'ci_failed' ? 'Günlüğü aç' : b.label };
    });
}

/** Kurulumda yapıştırılan tokenın türüne göre uyarı (Rust `api::token_problem` ile aynı kural). */
export function tokenProblem(token: string): string | null {
  const t = token.trim();
  if (!t) return null;
  if (t.startsWith('github_pat_')) return 'Bildirimler bu tür tokenla çalışmıyor, klasik token gerekli (ghp_ ile başlar).';
  if (/^gh[suor]_/.test(t)) return 'Bu bir uygulama tokenı; klasik kişisel token gerekli (ghp_ ile başlar).';
  if (/\s/.test(t)) return 'Token geçersiz görünüyor.';
  return null;
}

/** Kapsamları önceden işaretli klasik token sayfası. */
export const NEW_TOKEN_URL = 'https://github.com/settings/tokens/new?scopes=repo,notifications&description=Zodyvel';
