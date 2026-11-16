// Kaynakların canlı durumu (Rust `sources` modülünden) ve komutları. Mail/GitHub görünümleri buradan okur.
// Tarayıcı önizlemesinde örnek veri (demo.ts).

import { demoSources } from './demo';
import type { GithubTab } from './github';
import { call, callOrThrow, inTauri, on } from './ipc';
import {
  featured,
  idleState,
  SOURCE_IDS,
  tiles,
  type Featured,
  type ItemAction,
  type SourceFields,
  type SourceId,
  type SourceState,
  type SourcesSnapshot,
  type Tile,
  type TileId,
} from './sources';
import { localDirty } from './localgit.svelte';
import { data, type IslandContext } from './store.svelte';

export const hub = $state({
  list: (inTauri ? SOURCE_IDS.map(id => idleState(id)) : demoSources()) as SourceState[],
  paused: false,
  locked: false,
  /** Kullanıcının ızgarada son dokunduğu kutucuk (2 dk ya da yeni acil olaya kadar korunur). */
  pick: null as { id: TileId; at: number } | null,
  /** Son acil olay zamanı (yeni okunmamış satır, yeni onay); `pick`'i geçersiz kılar. */
  urgentAt: 0,
  /** Yeni olay geldi: kaynağın Zody'si tek seferlik sıçrar (anahtar değişince). */
  bump: null as { id: SourceId; key: number } | null,
  /** Büyük adada açık kaynak ayrıntısı (Mail/GitHub); null: AI ayrıntısı. */
  detail: null as SourceId | null,
  /** GitHub ayrıntısı bu sekmeyle açılsın (mini karttaki "Depolarım ›"); null: en acil sekme. */
  githubTab: null as GithubTab | null,
  /** Mail sekmesi bu iletiyle açılsın (mini karttaki satır; "yuva:uid"). Sekme okuyunca sıfırlar. */
  mailFocus: null as string | null,
});

export function sourceState(id: SourceId): SourceState {
  return hub.list.find(s => s.id === id) ?? idleState(id);
}

function apply(state: SourceState): void {
  const i = hub.list.findIndex(s => s.id === state.id);
  if (i >= 0) hub.list[i] = state;
  else hub.list.push(state);
}

export async function loadSources(): Promise<void> {
  const snap = await call<SourcesSnapshot>('sources_snapshot');
  if (!snap) return;
  hub.paused = snap.paused;
  hub.locked = snap.locked;
  for (const s of snap.sources) apply(s);
}

export function watchSources(): () => void {
  // Tarayıcı önizlemesi: adres #kaynaklar ile Kaynaklar düzeni (Yeni görünüm, Mail ve GitHub açık).
  if (!inTauri && typeof location !== 'undefined' && location.hash.includes('kaynaklar')) {
    data.settings.appearance.look = 'new';
    Object.assign(data.settings.features, { sources: true, sourceMail: true, sourceGithub: true, localGit: true });
  }
  void loadSources();
  const offs = [
    on<SourceState>('sources://update', apply),
    on<boolean>('sources://paused', p => (hub.paused = p)),
    on<SourceId>('sources://new', id => {
      hub.urgentAt = Date.now();
      hub.bump = { id, key: Date.now() };
    }),
  ];
  return () => offs.forEach(off => off());
}

/** Izgarada kutucuğa dokunma. Aynı kutucuğa ikinci dokunuş `true` döner (çağıran ayrıntıyı açar). */
export function pickTile(id: TileId, shown: string): boolean {
  const again = shown === id;
  hub.pick = { id, at: Date.now() };
  return again;
}

/** Satır eylemi; hata metni ya da null. Tarayıcı önizlemesinde örnek veride uygulanır. */
export async function sourceAction(id: SourceId, item: string, action: Exclude<ItemAction, 'preview'>): Promise<string | null> {
  if (!inTauri) {
    if (action === 'mark_read') {
      const s = sourceState(id);
      apply({ ...s, items: s.items.map(i => (i.id === item ? { ...i, unread: false } : i)), badge: Math.max(0, s.badge - 1) });
    }
    return null;
  }
  try {
    await callOrThrow('source_action', { id, itemId: item, action });
    return null;
  } catch (e) {
    return String(e);
  }
}

/** İletinin düz metin önizlemesi (okundu yapmaz). */
export async function sourcePreview(id: SourceId, item: string): Promise<{ text: string } | { error: string }> {
  if (!inTauri) return { text: 'Merhaba, konuştuğumuz gibi teklifin son hâlini ekte gönderiyorum. Cuma 14:00 uygun mu?' };
  try {
    return { text: await callOrThrow<string>('source_preview', { id, itemId: item }) };
  } catch (e) {
    return { error: String(e) };
  }
}

export async function sourceRefresh(id: SourceId): Promise<void> {
  await call('source_refresh', { id });
}

/** Kurulum: bilgiler Windows Kimlik Bilgisi Yöneticisi'ne gider; sonuç `sources://update` ile gelir
 *  ("connecting" → "ok"/"error"). Hata metni ya da null. Mail'de `slot`: değiştirilen hesap; yoksa yeni
 *  hesap eklenir (en çok `MAIL_SLOTS`). */
export async function sourceConnect<K extends SourceId>(id: K, fields: SourceFields[K], slot?: number): Promise<string | null> {
  if (!inTauri) {
    if (id !== 'mail') {
      apply({ ...sourceState(id), status: 'ok', account: 'Demo' });
      return null;
    }
    const address = (fields as SourceFields['mail']).address;
    const accounts = (sourceState('mail').accounts ?? []).filter(a => a.slot !== slot);
    if (accounts.some(a => a.address.toLowerCase() === address.toLowerCase())) return 'Bu Gmail hesabı zaten bağlı.';
    const free = slot ?? [0, 1, 2, 3, 4].find(n => !accounts.some(a => a.slot === n));
    if (free === undefined) return 'En çok 5 Gmail hesabı bağlanabilir.';
    accounts.push({ slot: free, address, status: 'ok', error: null, badge: 0 });
    accounts.sort((a, b) => a.slot - b.slot);
    apply({ ...sourceState('mail'), status: 'ok', accounts, account: accounts.length === 1 ? address : `${accounts.length} hesap` });
    return null;
  }
  try {
    await callOrThrow('source_connect', { id, fields, slot: slot ?? null });
    return null;
  } catch (e) {
    return String(e);
  }
}

/** Bağlantıyı kaldır: kaynak durur, bilgiler Windows'tan da silinir. Mail'de `slot` verilirse yalnız o hesap. */
export async function sourceDisconnect(id: SourceId, slot?: number): Promise<string | null> {
  if (!inTauri) {
    const rest = slot === undefined ? [] : (sourceState(id).accounts ?? []).filter(a => a.slot !== slot);
    if (!rest.length) apply(idleState(id, 'needs_setup'));
    else {
      const s = sourceState(id);
      const items = s.items.filter(i => rest.some(a => a.address === i.account));
      apply({ ...s, accounts: rest, items, badge: items.filter(i => i.unread).length, account: rest.length === 1 ? (rest[0]?.address ?? null) : `${rest.length} hesap` });
    }
    return null;
  }
  try {
    await callOrThrow('source_disconnect', { id, slot: slot ?? null });
    return null;
  } catch (e) {
    return String(e);
  }
}

/** Kaynaklar düzeni açık mı: Yeni görünüm + Özellikler → Kaynaklar. Eski görünümde AI sekmesi aynen kalır. */
export function hubOn(): boolean {
  return data.settings.appearance.look === 'new' && data.settings.features.sources;
}

export interface HubView {
  /** Sol kartta duran. */
  shown: Featured;
  tiles: Tile[];
  /** AI modülü açık ama çalışan oturum yok (ızgarada kısa not). */
  noSessions: boolean;
}

/** Izgara ve sol kart; `ctx` adanın o anki bağlamı, `now` dakikada bir ilerleyen saat. Müzik ve Bugün
 *  ızgarada yok, kendi sekmeleri var. */
export function hubView(ctx: IslandContext, now: number): HubView {
  const f = data.settings.features;
  const enabled: SourceId[] = [];
  if (f.sources && f.sourceMail) enabled.push('mail');
  if (f.sources && f.sourceGithub) enabled.push('github');
  const agentsOn = data.settings.modules.agents;
  const agents = agentsOn ? ctx.agents.map(a => ({ id: a.id, tool: a.tool, project: a.project, status: a.status })) : [];
  const waiting = ctx.approvals.filter(r => r.actionable).map(r => r.sessionId);
  return {
    shown: featured({ agents, waiting, sources: hub.list, enabled, selected: data.selected, pick: hub.pick, urgentAt: hub.urgentAt, now }),
    tiles: tiles({ agents, waiting, sources: hub.list, enabled, localDirty: localDirty() }),
    noSessions: agentsOn && agents.length === 0,
  };
}
