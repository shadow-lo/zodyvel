// Kaynaklar (Mail, GitHub…): Rust `sources` modülüyle aynı tipler ve saf kararlar. Tauri'ye bağlı değil,
// vitest ile sınanır. Canlı durum `sources.svelte.ts`'te.
//
// Güvenlik: gizli bilgi (uygulama şifresi, token) arayüzde yalnız kurulum formunda geçer ve doğrudan
// `source_connect` ile Rust'a gider; hiçbir yerde saklanmaz, olaylarda dönmez.

export type SourceId = 'mail' | 'github';
export type SourceStatus = 'off' | 'needs_setup' | 'connecting' | 'ok' | 'error';
export type ItemAction = 'open' | 'mark_read' | 'rerun' | 'fix_with_claude' | 'preview' | 'open_repo';
export type ItemTag = 'ci_failed' | 'review' | 'notif';

export const SOURCE_IDS: SourceId[] = ['mail', 'github'];

/** Tek satır (Rust `SourceItem`). */
export interface SourceItem {
  /** Kaynağın kendi kimliği (IMAP UID, bildirim thread id…). */
  id: string;
  /** Gönderen / depo#numara. */
  title: string;
  /** Konu / PR başlığı / iş adı. */
  summary: string;
  timeMs: number;
  unread: boolean;
  tag: ItemTag | null;
  url: string | null;
  actions: ItemAction[];
  /** Mail: iletinin geldiği Gmail hesabı. */
  account?: string | null;
}

/** Mail'de bağlı tek Gmail hesabı (Rust `AccountLine`). */
export interface AccountLine {
  /** Windows'taki kayıt yuvası (0 tabanlı); "Değiştir" ve "Kaldır" bununla. */
  slot: number;
  address: string;
  status: 'connecting' | 'ok' | 'error';
  error: string | null;
  badge: number;
  /** Mail sekmesi: Gmail kategorileri (Birincil, Güncellemeler, Tanıtımlar). Gmail süzgeci yoksa boş. */
  cats?: MailCat[];
  /** Son başarılı çekiş (ms). */
  syncedMs?: number;
}

export type MailCatKey = 'primary' | 'updates' | 'promotions';

/** Bir kategorinin okunmamış sayısı ve en yeni en çok üç okunmamış iletisi (Rust `MailCat`). */
export interface MailCat {
  key: MailCatKey;
  unread: number;
  items: SourceItem[];
}

/** En çok bu kadar Gmail hesabı (Rust `model::MAIL_SLOTS`). */
export const MAIL_SLOTS = 5;

/** GitHub'da son push edilen depo (Rust `RepoLine`; "Depolarım" sekmesi, rozete girmez). */
export interface RepoLine {
  /** "sahip/depo" */
  slug: string;
  url: string;
  /** Son push anı. */
  timeMs: number;
  private: boolean;
  branch: string | null;
  ci: 'ok' | 'failed' | 'pending' | 'none';
}

/** Bir kaynağın tüm durumu (Rust `SourceState`, `sources://update`). */
export interface SourceState {
  id: SourceId;
  status: SourceStatus;
  /** Türkçe, kullanıcıya gösterilir. */
  error: string | null;
  account: string | null;
  /** Okunmamış / bekleyen. */
  badge: number;
  /** 0 sakin … 3 acil. */
  urgency: number;
  /** En çok 20, yalnız bellekte. */
  items: SourceItem[];
  /** Yalnız GitHub: son push edilen depolar (en çok 10). */
  repos?: RepoLine[];
  /** Yalnız Mail: bağlı hesaplar (yuva sırasıyla). */
  accounts?: AccountLine[];
}

export interface SourcesSnapshot {
  /** Tepsi → Kaynakları duraklat. */
  paused: boolean;
  /** Windows oturumu kilitli. */
  locked: boolean;
  sources: SourceState[];
}

/** Kurulum alanları (Rust `model::FIELDS`). */
export type SourceFields = { mail: { address: string; appPassword: string }; github: { token: string } };

export const SOURCE_NAMES: Record<SourceId, string> = { mail: 'Mail', github: 'GitHub' };

export function idleState(id: SourceId, status: SourceStatus = 'off'): SourceState {
  return { id, status, error: null, account: null, badge: 0, urgency: 0, items: [] };
}

// ─── Kaynaklar ızgarası ────────────────────────────────────────────────────────────────────────────

/** Kutucuk türü: Zody rengi ve adı buna göre (Ayarlar › Kaynaklar da kullanır). */
export type TileKind = 'claude' | 'codex' | SourceId;
/** Bir ajan oturumunun kutucuğu: "s:<oturum id>". */
export type SessionTileId = `s:${string}`;
/** Izgaradaki kutucuk: Mail, GitHub, her ajan oturumu ayrı ya da sığmayan oturumlar için "+N" ('more'). */
export type TileId = SourceId | SessionTileId | 'more';
/** Sol kartta durabilecek her şey; 'zody': hiçbiri öne çıkmıyor, ana Zody "her şey yolunda" der. */
export type Featured = SourceId | SessionTileId | 'zody';

/** Izgara en çok bu kadar kutucuk. Mail ve GitHub hep en başta; oturumlar onların ardından. */
export const MAX_TILES = 6;
/** Kaynak Zody'sinin çerçeve rengi (ajanlar `agentColor` ile aynı; md/faz1-tasarim-kararlari.md §2.1). */
export const TILE_COLORS: Record<TileKind, string> = { claude: '#f0977a', codex: '#5b9cff', mail: '#ea5a47', github: '#9aa3b5' };
export const TILE_NAMES: Record<TileKind, string> = { claude: 'Claude', codex: 'Codex', mail: 'Mail', github: 'GitHub' };

export function sessionTile(id: string): SessionTileId {
  return `s:${id}`;
}

/** Oturum kutucuğunun oturum kimliği; başka kutucukta null. */
export function tileSession(id: string): string | null {
  return id.startsWith('s:') ? id.slice(2) : null;
}

/** Ajanın ızgara için gereken kısmı (AgentSession'dan). */
export interface AgentLite {
  id: string;
  tool: string;
  project: string;
  status: 'running' | 'waiting_input' | 'waiting_approval' | 'done' | 'error';
}

export interface HubInput {
  /** Ajan oturumları, adadaki sırayla (AI modülü kapalıysa boş). */
  agents: AgentLite[];
  /** Adadan cevaplanabilir onay/soru bekleyen oturumların kimliği. */
  waiting: string[];
  sources: SourceState[];
  /** Açık kaynaklar (Özellikler'e göre; çağıran hesaplar). */
  enabled: SourceId[];
  /** Kullanıcının seçili oturumu; birden çok oturum çalışıyorsa o öne çıkar. */
  selected: string | null;
  /** Kullanıcının ızgarada dokunduğu kutucuk ve zamanı. */
  pick: { id: TileId; at: number } | null;
  /** Son acil olay (yeni onay, yeni kırık CI, yeni mail) zamanı. */
  urgentAt: number;
  now: number;
}

/** Kullanıcının seçimi bu kadar korunur (ya da yeni acil olaya kadar). */
export const PICK_HOLD_MS = 2 * 60_000;

/** Soldaki büyük kartta ne durur. Sıra: onay/soru bekleyen oturum › çalışan oturum (seçili olan önce) ›
 *  urgency 3 kaynak (kırık CI) › okunmamış/urgency 2 kaynak (mail) › ana Zody.
 *  Kullanıcı ızgarada bir kutucuğa dokunduysa o seçim 2 dk ya da yeni acil olaya kadar korunur. */
export function featured(input: HubInput): Featured {
  const { pick, agents } = input;
  if (pick && pick.id !== 'more' && input.now - pick.at < PICK_HOLD_MS && input.urgentAt <= pick.at) {
    const session = tileSession(pick.id);
    if (session ? agents.some(a => a.id === session) : input.enabled.includes(pick.id as SourceId)) return pick.id as Featured;
  }
  const waiting = agents.find(a => input.waiting.includes(a.id));
  if (waiting) return sessionTile(waiting.id);
  const running = agents.filter(a => a.status === 'running');
  const run = running.find(a => a.id === input.selected) ?? running[0];
  if (run) return sessionTile(run.id);
  const live = input.sources.filter(s => s.status === 'ok' && input.enabled.includes(s.id));
  for (const level of [3, 2]) {
    const hit = live.filter(s => s.urgency >= level && (level === 3 || s.badge > 0)).sort((a, b) => b.urgency - a.urgency || b.badge - a.badge)[0];
    if (hit) return hit.id;
  }
  return 'zody';
}

export type TileTone = 'calm' | 'busy' | 'attention' | 'alert' | 'off';

export interface Tile {
  id: TileId;
  kind: TileKind;
  label: string;
  /** Sayı rozeti (0: yok). */
  count: number;
  /** Rozet yoksa kısa durum ("çalışıyor", "CI ✕", "onay"). */
  note: string;
  tone: TileTone;
  /** Oturum kutucuğunda araç adı ve proje (ipucu için). */
  hint?: string;
}

export interface TileInput {
  agents: AgentLite[];
  waiting: string[];
  sources: SourceState[];
  enabled: SourceId[];
  /** GitHub kutucuğuna eklenen sayı: bilgisayarda commitlenmemiş değişikliği olan depolar (Yerel Git). */
  localDirty?: number;
}

const SOURCE_NOTE: Record<SourceStatus, string> = { off: 'kapalı', needs_setup: 'bağla', connecting: 'bağlanıyor', ok: '', error: 'hata' };

function sourceTile(id: SourceId, sources: SourceState[], local = 0): Tile {
  const label = TILE_NAMES[id];
  const s = sources.find(x => x.id === id) ?? idleState(id);
  // Yerel değişiklikler token olmadan da sayılır (Yerel Git GitHub bağlantısına muhtaç değil).
  if (s.status !== 'ok' && local > 0 && s.status !== 'error') return { id, kind: id, label, count: local, note: '', tone: 'attention' };
  if (s.status !== 'ok') return { id, kind: id, label, count: 0, note: SOURCE_NOTE[s.status], tone: s.status === 'error' ? 'alert' : 'off' };
  if (s.urgency >= 3) return { id, kind: id, label, count: 0, note: s.items.some(i => i.tag === 'ci_failed') ? 'CI ✕' : 'acil', tone: 'alert' };
  const count = s.badge + local;
  return { id, kind: id, label, count, note: '', tone: count ? 'attention' : 'calm' };
}

function agentTile(a: AgentLite, label: string, waiting: boolean): Tile {
  const kind: TileKind = a.tool === 'codex' ? 'codex' : 'claude';
  const base = { id: sessionTile(a.id), kind, label, count: 0, hint: `${TILE_NAMES[kind]} · ${a.project}` };
  if (waiting || a.status === 'waiting_approval') return { ...base, note: 'onay', tone: 'attention' };
  if (a.status === 'running') return { ...base, note: 'çalışıyor', tone: 'busy' };
  if (a.status === 'error') return { ...base, note: 'hata', tone: 'alert' };
  if (a.status === 'waiting_input') return { ...base, note: 'bekliyor', tone: 'calm' };
  return { ...base, note: 'bitti', tone: 'calm' };
}

/** Izgaranın kutucukları, en çok 6: önce açık kaynaklar (Mail, GitHub; yerleri hiç değişmez), sonra her
 *  oturum ayrı kutucuk, adadaki sırasıyla. Sığmayan oturumlar son kutucukta "+N". Aynı projede iki oturum
 *  varsa ikincisi "proje 2". */
export function tiles(input: TileInput): Tile[] {
  const out = SOURCE_IDS.filter(id => input.enabled.includes(id)).map(id => sourceTile(id, input.sources, id === 'github' ? (input.localDirty ?? 0) : 0));
  const room = MAX_TILES - out.length;
  const seen = new Map<string, number>();
  const sessions = input.agents.map(a => {
    const n = (seen.get(a.project) ?? 0) + 1;
    seen.set(a.project, n);
    return agentTile(a, n > 1 ? `${a.project} ${n}` : a.project, input.waiting.includes(a.id));
  });
  if (sessions.length <= room) return [...out, ...sessions];
  const shown = sessions.slice(0, room - 1);
  const rest = sessions.slice(room - 1);
  const wait = rest.some(t => t.tone === 'attention');
  out.push(...shown, { id: 'more', kind: 'claude', label: `+${rest.length} oturum`, count: 0, note: wait ? 'onay' : '', tone: wait ? 'attention' : 'calm' });
  return out;
}

/** Satırın kısa zamanı: "şimdi", "9 dk", "1 sa", "dün", "3 g". */
export function ago(timeMs: number, now: number): string {
  const min = Math.max(0, Math.floor((now - timeMs) / 60_000));
  if (min < 1) return 'şimdi';
  if (min < 60) return `${min} dk`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} sa`;
  const d = Math.floor(h / 24);
  return d === 1 ? 'dün' : `${d} g`;
}

/** Satırda hangi hesaptan geldiği: yalnız birden çok hesap bağlıyken, adresin "@" öncesi ("is", "kisisel").
 *  Aynı ön ada sahip iki hesap varsa (ad@gmail.com, ad@okul.edu) tam adres. */
export function accountLabel(state: SourceState, item: SourceItem): string | null {
  const accounts = state.accounts ?? [];
  if (accounts.length < 2 || !item.account) return null;
  const local = (address: string) => (address.split('@')[0] ?? address).toLowerCase();
  const short = item.account.split('@')[0] ?? item.account;
  const clash = accounts.filter(a => local(a.address) === short.toLowerCase()).length > 1;
  return clash ? item.account : short;
}

/** Bağlı ama sorunlu hesaplar (diğerleri çalışırken de görünsün diye). */
export function brokenAccounts(state: SourceState): AccountLine[] {
  return state.status === 'ok' ? (state.accounts ?? []).filter(a => a.status === 'error') : [];
}

export const TAG_LABELS: Record<ItemTag, string> = { ci_failed: 'CI ✕', review: 'İnceleme', notif: 'Bildirim' };

// --- Mail sekmesi (Faz 5b) -------------------------------------------------------------------------------

export const CAT_NAMES: Record<MailCatKey, string> = { primary: 'Birincil', updates: 'Güncellemeler', promotions: 'Tanıtımlar' };
/** Birincil'de bu kadar ya da daha çok okunmamış varsa kart vurgu rengine döner ("acil"). */
export const HOT_PRIMARY = 10;

/** Hesabın kartları: Gmail kategorileri varsa onlar (sırasıyla), yoksa tek "Gelen kutusu" kartı (satırlarıyla). */
export interface MailCard {
  key: MailCatKey | 'inbox';
  title: string;
  unread: number;
  /** Kartta gösterilen en yeni üç ileti. */
  items: SourceItem[];
  /** Kategori listesindeki iletilerin hepsi (Rust `CAT_ITEMS`, en çok 15). */
  all: SourceItem[];
}

/** Kart başına ileti sayısı. */
export const CARD_ITEMS = 3;

export function mailCards(state: SourceState, account: AccountLine | null): MailCard[] {
  if (!account) return [];
  if (account.cats?.length)
    return account.cats.map(c => ({ key: c.key, title: CAT_NAMES[c.key] ?? c.key, unread: c.unread, items: c.items.slice(0, CARD_ITEMS), all: c.items }));
  const all = state.items.filter(i => !i.account || i.account === account.address);
  return [{ key: 'inbox', title: 'Gelen kutusu', unread: account.badge, items: all.slice(0, CARD_ITEMS), all }];
}

/** Bir iletinin hangi hesapta ve hangi kartta olduğu ("yuva:uid"; mini karttan Mail sekmesine geçişte). */
export function findMail(state: SourceState, id: string): { slot: number; card: MailCard['key']; item: SourceItem } | null {
  for (const a of state.accounts ?? []) {
    for (const c of mailCards(state, a)) {
      const item = c.all.find(i => i.id === id);
      if (item) return { slot: a.slot, card: c.key, item };
    }
  }
  return null;
}

/** "99+" sınırı. */
export const countText = (n: number) => (n > 99 ? '99+' : String(n));

/** Hesabın kısa adı: "kisisel" ve yanında "@gmail". */
export function splitAddress(address: string): [string, string] {
  const at = address.indexOf('@');
  return at < 0 ? [address, ''] : [address.slice(0, at), address.slice(at).replace(/\.com$/, '')];
}

/** "şimdi yenilendi", "1 dk önce yenilendi". */
export function syncedText(ms: number | undefined, now: number): string {
  if (!ms) return '';
  const a = ago(ms, now);
  return a === 'şimdi' ? 'şimdi yenilendi' : `${a} önce yenilendi`;
}
