// Adanın tek veri kaynağı. Uygulamada her şey Rust çekirdeğinden gelir: AI ajanları (hook köprüsü),
// müzik (Windows medya oturumları), sistem yükü (performans sayaçları). Örnek veri yalnız tarayıcı önizlemesinde.

import {
  demoAgents,
  demoApproval,
  demoBattery,
  demoClosedSessions,
  demoControls,
  demoMusic,
  demoRecentProjects,
  demoRequestDiff,
  demoShelf,
  demoStartTask,
  demoStepContent,
  demoSystem,
  demoToday,
  demoTodayApply,
} from './demo';
import { shelfVisible } from './shelf';
import { call, callOrThrow, inTauri, on } from './ipc';
import { visibleTabs, type Tab } from './tabs';
import { demoUsage, usageSpans, type UsageSpans, type UsageSummary } from './usage';
import type { Attachment } from './write';
import { liveAgent, moodFor, SLEEP_MS } from './mood';
import { musicStarted } from './recent';
import { finishMeter, FOCUS_MIN, isoDate } from './today';
import {
  defaultSettings,
  type AgentSession,
  type AgentStep,
  type AgentsSnapshot,
  type ApprovalRequest,
  type AudioOutput,
  type Battery,
  type ClosedSession,
  type ControlsState,
  type EventDue,
  type FocusStep,
  type MeterDone,
  type MeterState,
  type ModeDef,
  type ModeDone,
  type ModeRun,
  type MusicUpdate,
  type NowPlaying,
  type OutputSwitch,
  type RecentProject,
  type PlanLimits,
  type SessionContext,
  type Settings,
  type ShelfData,
  type ShelfDrag,
  type ShelfItem,
  type ShelfSent,
  type StepContent,
  type SystemLoad,
  type TaskFailed,
  type TaskPermission,
  type TodayData,
  type TodayOp,
  type ZodyMood,
} from './types';

export const data = $state({
  agents: (inTauri ? [] : demoAgents()) as AgentSession[],
  approvals: [] as ApprovalRequest[],
  music: (inTauri ? null : demoMusic()) as NowPlaying | null,
  /** Geçerli şarkının kapağı (data: adresi). */
  artUrl: null as string | null,
  system: (inTauri ? null : demoSystem()) as SystemLoad | null,
  today: (inTauri ? null : demoToday()) as TodayData | null,
  /** Az önce dolan zamanlayıcı (çentik bildirimi); Bugün bölümü görülünce temizlenir. */
  timerDone: null as TimerDone | null,
  /** Biten mod (Mod Bitişi); Tamam ya da Modlar görülünce kalkar. */
  modeDone: null as (ModeDone & { key: number }) | null,
  /** Saati gelen etkinlik (çentik bildirimi); Bugün bölümü görülünce temizlenir. */
  eventDue: null as EventDue | null,
  /** "En son hareketli bölümde aç" için son etkinlik anları (Unix ms): müziğin başlaması / şarkı
   *  değişmesi, zamanlayıcının dolması ya da etkinliğin saati. Bildirim kapansa da kalır. */
  activity: { music: 0, today: 0 },
  settings: structuredClone(defaultSettings) as Settings,
  /** Ajan verisi örnek mi (yalnız tarayıcı önizlemesi). */
  sampleAgents: !inTauri,
  /** Müzik ve sistem yükü örnek mi (yalnız tarayıcı önizlemesi). */
  sampleMedia: !inTauri,
  lastEvents: { claude: null, codex: null } as AgentsSnapshot['lastEvents'],
  /** Klavye kısayollarının hedefi: adada görünen, karar verilebilir onay isteği. */
  keyTarget: null as string | null,
  /** Kısayolla gelen karar; ilgili onay kartı alıp uygular. */
  keyDecision: null as { id: string; decision: 'allow' | 'deny' } | null,
  /** Birden çok istek beklerken öne alınan (‹ › ile seçilen) istek; null: ilk gelen. */
  approvalFirst: null as string | null,
  /** Ayrıntı açılınca not kutusu odaklansın (mini "Sonra…"). */
  noteFocus: false,
  /** "Sonra ⌄" ile katlanan onay/soru kartları (istek kimlikleri); istek kalkınca listeden düşer. */
  folded: [] as string[],
  /** Yazma sahnesi açılırken seçilecek hedef (oturum kimliği ya da yeni görev); null: seçili oturum. */
  writeTarget: null as string | 'new' | null,
  /** Raftan "Claude'a sor": yazma sahnesi açılırken eklenecek dosyalar; sahne alınca boşaltır. */
  writeAttach: [] as Attachment[],
  /** Arka planda başlatılan görev hatayla bitti; yazma sahnesi ve çentik gösterir. */
  taskFailed: null as TaskFailed | null,
  /** Ayrıntı açılınca bu sekme seçilsin ("Bitti · 4 dosya" bildirimi → Değişenler). */
  detailTab: null as 'changed' | null,
  /** Mini AI ve ayrıntıda seçili oturum (null: en önemlisi). */
  selected: null as string | null,
  /** Uzun süredir hiçbir şey olmadı; Zody uyuyor. */
  asleep: false,
  /** Dakikada bir ilerler: sessiz kalan oturumların görünen durumu yeniden hesaplanır. */
  minute: Date.now(),
  /** Pil (yalnız pili olan bilgisayarda ve pil özelliklerinden biri açıkken dolu). */
  battery: (inTauri ? null : demoBattery()) as Battery | null,
  /** Claude kullanım özeti (yalnız AI paneli açıkken ve özellik açıksa okunur). */
  usage: null as { summary: UsageSummary; spans: UsageSpans } | null,
  /** Ayrıntıda kullanım paneli açık (mini görünümdeki haptan açılır). */
  usageOpen: false,
  /** Dosya rafı (Özellikler → Dosya rafı). */
  shelf: (inTauri ? { items: [], nextId: 0 } : demoShelf()) as ShelfData,
  /** Adaya şu an dosya sürükleniyor (kaç dosya); bırakılınca ya da çıkınca null. */
  shelfDrag: null as number | null,
  /** Raftan dışarı sürükleme sürüyor: ada kapanmaz. */
  shelfOut: false,
  /** Dosya sürüklenirken imlecin ada penceresindeki yeri (arayüz pikseli); raf altındaki oturum adını parlatır. */
  shelfOver: null as { x: number; y: number } | null,
  /** Son raftan AI oturumuna gönderim (kısa not); `at` notun ne zaman çıktığı. */
  shelfSent: null as (ShelfSent & { at: number }) | null,
  /** Bir oturuma bırakıldıktan sonra kısa süre raf bölümü (ve gönderim notu) görünür kalır: raf boşsa bölüm kaybolup notu yutmasın. */
  shelfHold: false,
});

let sleepTimer: ReturnType<typeof setTimeout> | undefined;
/** Ada açıldı ya da bir durum değişti: Zody uyanır, uyku sayacı baştan başlar (tek zamanlayıcı, yoklama yok). */
export function noteActivity(): void {
  data.asleep = false;
  clearTimeout(sleepTimer);
  sleepTimer = setTimeout(() => (data.asleep = true), SLEEP_MS);
}

export async function loadSettings(): Promise<void> {
  const saved = await call<Settings>('get_settings');
  if (saved) data.settings = saved;
}

export function watchSettings(): () => void {
  return on<Settings>('settings://changed', next => (data.settings = next));
}

export async function saveSettings(next: Settings): Promise<void> {
  const saved = await call<Settings>('update_settings', { next });
  data.settings = saved ?? next;
}

function applySnapshot(snapshot: AgentsSnapshot) {
  data.agents = snapshot.agents;
  data.approvals = snapshot.approvals;
  if (data.folded.length) data.folded = data.folded.filter(id => snapshot.approvals.some(r => r.id === id));
  data.lastEvents = snapshot.lastEvents;
}

/** Rust'taki ajan durumunu yükler ve değişiklikleri izler. */
export function watchAgents(): () => void {
  // Olay gelmese de "çalışıyor"un bayatlayıp bayatlamadığı dakikada bir yeniden hesaplanır.
  const tick = setInterval(() => (data.minute = Date.now()), 60_000);
  if (!inTauri) return () => clearInterval(tick);
  void call<AgentsSnapshot>('agents_snapshot').then(s => s && applySnapshot(s));
  const stop = on<AgentsSnapshot>('agents://snapshot', applySnapshot);
  return () => {
    clearInterval(tick);
    stop();
  };
}

export function applyMusic(update: MusicUpdate) {
  const previous = data.music?.artId ?? null;
  if (musicStarted(data.music, update.now)) data.activity.music = Date.now();
  data.music = update.now;
  const artId = update.now?.artId ?? null;
  if (update.art) data.artUrl = update.art;
  else if (artId === null || artId !== previous) data.artUrl = null;
}

/** Çalan müziği yükler ve Windows medya olaylarını izler. */
export function watchMusic(): () => void {
  if (!inTauri) return () => {};
  void call<MusicUpdate>('music_snapshot').then(u => u && applyMusic(u));
  return on<MusicUpdate>('music://now', applyMusic);
}

/** Dolan Zodymetre (`meter`) ya da açık modda biten odak aşaması (`focus`). */
export interface TimerDone {
  key: number;
  minutes: number;
  meter?: MeterDone;
  focus?: FocusStep;
}

export function timerFinished(minutes: number, extra: { meter?: MeterDone; focus?: FocusStep } = {}) {
  data.timerDone = { key: Date.now(), minutes, ...extra };
  data.activity.today = data.timerDone.key;
}

export function modeFinished(done: ModeDone) {
  data.modeDone = { ...done, key: Date.now() };
}

function eventReached(event: Omit<EventDue, 'key'>) {
  data.eventDue = { ...event, key: Date.now() };
  data.activity.today = data.eventDue.key;
}

/** Bugün verisi (takvim, yapılacaklar, zamanlayıcı) ve zamanlayıcının dolması. */
export function watchToday(): () => void {
  if (!inTauri) return () => {};
  void call<TodayData>('today_snapshot').then(t => t && (data.today = t));
  const stops = [
    on<TodayData>('today://changed', t => (data.today = t)),
    on<MeterDone>('today://timer-done', meter => timerFinished(meter.minutes, { meter })),
    on<FocusStep>('today://focus', step => timerFinished(FOCUS_MIN[step.ended], { focus: step })),
    on<ModeDone>('today://mode-done', modeFinished),
    on<Omit<EventDue, 'key'>>('today://event-due', eventReached),
  ];
  return () => stops.forEach(stop => stop());
}

/** Dosya rafı ve adaya dosya sürükleme. `ondrag`: Island adayı açar/kapatır. Özellik kapalıyken Rust olay göndermez. */
let holdTimer: ReturnType<typeof setTimeout> | undefined;

export function watchShelf(ondrag: (drag: ShelfDrag) => void): () => void {
  if (!inTauri) return () => {};
  void call<ShelfData>('shelf_snapshot').then(s => s && (data.shelf = s));
  const stops = [
    on<ShelfData>('shelf://changed', s => (data.shelf = s)),
    on<ShelfDrag>('shelf://drag', drag => {
      data.shelfDrag = drag.kind === 'enter' ? drag.count : null;
      if (drag.kind !== 'enter') data.shelfOver = null;
      if (drag.kind === 'send') {
        data.shelfHold = true;
        clearTimeout(holdTimer);
        holdTimer = setTimeout(() => (data.shelfHold = false), 4500);
      }
      ondrag(drag);
    }),
    on<{ x: number; y: number }>('shelf://over', point => {
      if (data.shelfDrag !== null) data.shelfOver = point;
    }),
    on<ShelfSent>('shelf://sent', sent => (data.shelfSent = { ...sent, at: Date.now() })),
  ];
  return () => stops.forEach(stop => stop());
}

/** Bir öğeyi ya da (`id` yoksa) hepsini raftan kaldırır; dosyalar yerinde kalır. */
export async function shelfRemove(id?: number): Promise<void> {
  if (!inTauri) {
    data.shelf.items = id === undefined ? [] : data.shelf.items.filter(i => i.id !== id);
    return;
  }
  const next = await call<ShelfData>('shelf_remove', { id: id ?? null });
  if (next) data.shelf = next;
}

/** Fare tuşu basılıyken çağrılır: dosyayı Windows sürüklemesine verir, bırakılınca döner. */
export async function shelfDragOut(items: ShelfItem[]): Promise<void> {
  if (!inTauri || data.shelfOut) return;
  data.shelfOut = true;
  try {
    await call<boolean>('shelf_drag_out', { ids: items.map(i => i.id) });
  } finally {
    data.shelfOut = false;
  }
}

/** Sürüklenen dosyalar bırakılınca gidecek AI oturumu (imleç adının üstünde); null: rafa. */
export function shelfTarget(session: string | null): void {
  if (inTauri) void call('shelf_target', { session });
}

/** Raftaki kartlar bir AI oturumunun adına bırakıldı: göreve eklenir ya da yol panoya, pencere öne. */
export async function shelfSend(session: AgentSession, items: ShelfItem[]): Promise<void> {
  if (!inTauri) {
    const how = session.attach ? 'attached' : 'copied';
    data.shelfSent = { session: session.id, how, focused: !!session.app, count: items.length, name: items[0]?.name ?? '', at: Date.now() };
    return;
  }
  try {
    const sent = await callOrThrow<ShelfSent>('shelf_send', { session: session.id, ids: items.map(i => i.id) });
    data.shelfSent = { ...sent, at: Date.now() };
  } catch (error) {
    const message = typeof error === 'string' ? error : 'Gönderilemedi.';
    data.shelfSent = { session: session.id, how: 'copied', focused: false, count: 0, name: items[0]?.name ?? '', error: message, at: Date.now() };
  }
}

/** Dosyayı Gezgin'de gösterir. */
export function shelfReveal(item: ShelfItem): void {
  void call('shelf_reveal', { id: item.id });
}

// Tarayıcı önizlemesinde Zodymetreleri ve modu Rust yerine bu sayaç bitirir (yalnız en yakın bitişe kadar).
let demoTimer: ReturnType<typeof setTimeout> | undefined;
function scheduleDemoTimer() {
  clearTimeout(demoTimer);
  const t = data.today;
  if (!t) return;
  const ends = [...t.meters.map(m => m.endsAt), t.mode?.endsAt ?? null].filter((x): x is number => x !== null);
  if (!ends.length) return;
  demoTimer = setTimeout(() => {
    const today = data.today;
    if (!today) return;
    const now = Date.now();
    today.meters = today.meters.map(m => {
      if (m.endsAt === null || m.endsAt > now) return m;
      const { meter, done } = finishMeter(m, isoDate(new Date(now)));
      timerFinished(done.minutes, { meter: done });
      return meter;
    });
    const run = today.mode;
    if (run?.endsAt != null && run.endsAt <= now) {
      const def = today.modes.find(d => d.id === run.id);
      modeFinished({ id: run.id, name: def?.name ?? 'Mod', icon: def?.icon ?? 'star', color: def?.color ?? '#8b7cff', minutes: run.minutes, reopen: run.closed.map(a => a.name) });
      today.reopen = run.closed;
      today.mode = null;
    }
    scheduleDemoTimer();
  }, Math.max(0, Math.min(...ends) - Date.now()));
}

/** Bir değişiklik; hata varsa metni döner (ör. "Geçersiz tarih ya da saat."). */
export async function todayApply(op: TodayOp): Promise<string | null> {
  if (!inTauri) {
    if (!data.today) return null;
    if (op.op === 'focusStart' && !data.settings.features.focus) return 'Odak modu kapalı (Ayarlar → Özellikler).';
    const run = data.today.mode;
    demoTodayApply(data.today, op, Date.now());
    if (op.op === 'modeStop' && run) {
      const def = data.today.modes.find(d => d.id === run.id);
      modeFinished({ id: run.id, name: def?.name ?? 'Mod', icon: def?.icon ?? 'star', color: def?.color ?? '#8b7cff', minutes: Math.floor((Date.now() - run.startedAt) / 60_000), reopen: run.closed.map(a => a.name) });
    }
    scheduleDemoTimer();
    return null;
  }
  try {
    data.today = await callOrThrow<TodayData>('today_apply', { op });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Kaydedilemedi.';
  }
}

/** Pil durumu: Rust yalnız güç değişince (şarj, yüzde) bildirir; yoklama yok. `onchange` bildirim içindir. */
export function watchPower(onchange?: (before: Battery | null, after: Battery | null) => void): () => void {
  if (!inTauri) return () => {};
  const set = (next: Battery | null) => {
    const before = data.battery;
    data.battery = next;
    onchange?.(before, next);
  };
  void call<Battery | null>('power_status').then(b => (data.battery = b ?? null));
  return on<Battery | null>('power://battery', set);
}

export function watchSystem(): () => void {
  if (!inTauri) return () => {};
  return on<SystemLoad>('system://load', load => (data.system = load));
}

/** 0: gösterilmiyor · 1: CPU/RAM/GPU · 2: + en çok bellek kullanan uygulamalar. Rust yalnız gerekince ölçer. */
export function setSystemWatch(level: 0 | 1 | 2): void {
  void call('system_watch', { level });
}

export interface IslandContext {
  agents: AgentSession[];
  approvals: ApprovalRequest[];
  /** "Sonra ⌄" ile katlanmış, hâlâ cevap bekleyen istekler. */
  folded: ApprovalRequest[];
  music: NowPlaying | null;
  system: SystemLoad | null;
  today: TodayData | null;
  meters: MeterState[];
  modeRun: ModeRun | null;
  modeDef: ModeDef | null;
  timerDone: TimerDone | null;
  modeDone: (ModeDone & { key: number }) | null;
  eventDue: EventDue | null;
  /** Bilgisayar ekranındaki pil (modül ve özellik açıksa, pil varsa). */
  battery: Battery | null;
  /** Dosya rafı (özellik kapalıyken boş). */
  shelf: ShelfItem[];
  working: number;
  waiting: number;
  /** Simge çubuğunda görünen bölümler (kapalı modüllerinki yok). */
  tabs: Tab[];
  /** Üzerine gelince açılacak bölüm: bekleyen onay varsa mini AI; Saat düzeninde Saat; müzik çalıyorsa müzik ekranı; değilse mini AI. */
  hoverTab: Tab;
  mood: ZodyMood;
}

/** Kapalı modüller hiçbir yerde görünmez. */
export function context(): IslandContext {
  const m = data.settings.modules;
  const now = Math.max(data.minute, Date.now());
  const agents = m.agents ? data.agents.map(a => liveAgent(a, now)) : [];
  // Öne alınan istek ilk sırada: hem kart hem klavye kısayolu onu hedefler.
  const first = data.approvals.find(r => r.id === data.approvalFirst);
  const ordered = !m.agents ? [] : first ? [first, ...data.approvals.filter(r => r !== first)] : data.approvals;
  // Katlanan kartlar ("Sonra ⌄") ne kartta ne bildirimde ne kısayolda görünür; simge çubuğunda hap kalır.
  const foldOn = data.settings.features.foldCard && data.folded.length > 0;
  const approvals = foldOn ? ordered.filter(r => !data.folded.includes(r.id)) : ordered;
  const folded = foldOn ? ordered.filter(r => data.folded.includes(r.id)) : [];
  const music = m.music ? data.music : null;
  const system = m.system ? data.system : null;
  // Bugün, Ajanda ve Modlar aynı Rust deposunda; Zodymetreler Bugün'ün, planlar Ajanda'nın, açık mod Modlar'ın.
  const today = m.today || m.modes || m.agenda ? data.today : null;
  const meters = m.today ? (data.today?.meters ?? []) : [];
  const modeRun = m.modes ? (data.today?.mode ?? null) : null;
  const modeDef = modeRun ? (data.today?.modes.find(d => d.id === modeRun.id) ?? null) : null;
  // Odak aşaması modun içinde döner (Modlar modülü); Zodymetre Bugün'ün.
  const timerDone = (data.timerDone?.focus ? m.modes : m.today) ? data.timerDone : null;
  const modeDone = m.modes ? data.modeDone : null;
  const eventDue = m.agenda ? data.eventDue : null;
  const battery = m.system && data.settings.features.battery ? data.battery : null;
  const shelfOn = shelfVisible(data.settings.features.shelf, data.shelf.items.length, data.shelfDrag !== null || data.shelfHold);
  const shelf = data.settings.features.shelf ? data.shelf.items : [];
  const working = agents.filter(a => a.status === 'running').length;
  const waiting = agents.filter(a => a.status === 'waiting_input').length;
  const errors = agents.some(a => a.status === 'error');
  const done = agents.some(a => a.status === 'done');
  // Zody sohbeti modüle bağlı değil: özellik açıksa her zaman en sağda.
  const zody = data.settings.features.zodyChat;
  // Saat düzeni (Görünüm › Düzen): en solda Saat ana ekranı; üzerine gelince o açılır.
  const clock = data.settings.appearance.layout === 'clock';
  // Mail sekmesi (Faz 5b) Mail kaynağına bağlı: kaynak kapalıyken sekme yok, IMAP bağlantısı da yok.
  const f = data.settings.features;
  const mail = data.settings.appearance.look === 'new' && f.sources && f.sourceMail;
  const tabs = visibleTabs({ clock, ai: m.agents, music: m.music, today: m.today, agenda: m.agenda, modes: m.modes, mail, pc: m.system, shelf: shelfOn, zody });
  const preferMusic = music?.playing && data.settings.island.hover !== 'ai';
  const hoverTab: Tab = approvals.length ? 'ai' : clock ? 'clock' : preferMusic ? 'music' : m.agents ? 'ai' : (tabs[0] ?? 'ai');
  const mood = moodFor({
    approvals: approvals.length,
    errors,
    working,
    musicPlaying: Boolean(music?.playing),
    done,
    asleep: data.asleep,
  });
  return { agents, approvals, folded, music, system, today, meters, modeRun, modeDef, timerDone, modeDone, eventDue, battery, shelf, working, waiting, tabs, hoverTab, mood };
}

export function requestDemoApproval(kind: 'permission' | 'question' | 'plan' = 'permission'): void {
  if (inTauri) {
    void call('agents_demo', { kind });
    return;
  }
  const target = data.agents.find(a => a.status !== 'done') ?? data.agents[0];
  if (!target) return;
  target.status = 'waiting_approval';
  target.step = kind === 'question' ? 'Sorularını bekliyor' : kind === 'plan' ? 'Planını onaylaman bekleniyor' : 'Onayını bekliyor';
  data.approvals.push(demoApproval(target, kind));
}

// Kullanım özeti: panel açıkken dakikada bir yenilenir (Rust yalnız yeni satırları okur); kapalıyken okunmaz.
const USAGE_MS = 60_000;
let usageLoading = false;

export async function loadUsage(): Promise<void> {
  if (!data.settings.features.usage) {
    data.usage = null;
    return;
  }
  if (usageLoading) return;
  usageLoading = true;
  try {
    const spans = usageSpans(new Date());
    const summary = inTauri ? await call<UsageSummary | null>('usage_summary', { ...spans }) : demoUsage();
    data.usage = summary ? { summary, spans } : null;
  } finally {
    usageLoading = false;
  }
}

/** Bir bileşen ekrandayken kullanım özetini taze tutar; dönen işlev durdurur. */
export function watchUsage(): () => void {
  void loadUsage();
  const timer = setInterval(() => void loadUsage(), USAGE_MS);
  return () => clearInterval(timer);
}

/** Claude plan limitleri (köprünün yazdığı küçük dosyadan); özellik kapalıyken okunmaz. */
export async function planLimits(): Promise<PlanLimits | null> {
  if (!data.settings.features.planLimit) return null;
  if (!inTauri) {
    const hour = 3_600_000;
    return { fiveHour: { pct: 73, resetsAt: Date.now() + 2 * hour }, sevenDay: { pct: 41, resetsAt: Date.now() + 80 * hour }, at: Date.now() };
  }
  return (await call<PlanLimits | null>('plan_limits')) ?? null;
}

/** Tarayıcı önizlemesinde kapanan örnek isteğin oturumunu ilerletir. */
function settleDemo(id: string, step: string): string | null {
  const request = data.approvals.find(r => r.id === id);
  if (!request) return 'Bu istek artık geçerli değil.';
  data.approvals = data.approvals.filter(r => r.id !== id);
  const session = data.agents.find(a => a.id === request.sessionId);
  if (session) {
    session.status = 'running';
    session.step = step;
  }
  return null;
}

/** Seçenekli soruların cevabı. Rust her sorunun cevaplandığını ve isteğin hâlâ beklediğini denetler. */
export async function answerQuestions(id: string, answers: Record<string, string>): Promise<string | null> {
  if (!inTauri) return settleDemo(id, 'Sorular cevaplandı · devam ediyor');
  try {
    await callOrThrow('agents_answer', { id, answers });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Cevap iletilemedi.';
  }
}

/** "Araçta cevapla": istek aracın kendi penceresine bırakılır. */
export async function passToTool(id: string): Promise<string | null> {
  if (!inTauri) return settleDemo(id, 'Soruyu araçta cevaplaman bekleniyor');
  try {
    await callOrThrow('agents_pass', { id });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'İstek bırakılamadı.';
  }
}

/** Ayrıntı ekranı: bir adımın içeriği (okunan satırlar, fark, komut çıktısı). */
export async function stepContent(session: AgentSession, step: AgentStep): Promise<StepContent | null> {
  if (!inTauri) return demoStepContent(step);
  return (await call<StepContent | null>('agents_step', { session: session.id, step: step.id })) ?? null;
}

/** Onay isteğinin tam farkı (dosya henüz değişmedi). */
export async function requestDiff(id: string): Promise<StepContent | null> {
  if (!inTauri) return demoRequestDiff();
  return (await call<StepContent | null>('agents_request_diff', { id })) ?? null;
}

/** "Terminale git": oturumun çalıştığı pencereyi öne getirir. */
export async function focusSession(session: string): Promise<string | null> {
  if (!inTauri) return null;
  try {
    await callOrThrow('agents_focus', { session });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Pencere öne getirilemedi.';
  }
}

/** Ajana not: `now` bir sonraki araç çağrısında araya girer, değilse tur biterken iletilir. */
export async function sendNote(session: AgentSession, text: string, now: boolean): Promise<string | null> {
  const clean = text.trim();
  if (!clean) return 'Notu yaz.';
  if (!inTauri) {
    const target = data.agents.find(a => a.id === session.id);
    if (!target?.canNote) return 'Bu oturuma not gönderilemiyor.';
    target.notes.push({ id: Date.now(), text: clean, now, at: Date.now() });
    return null;
  }
  try {
    await callOrThrow('agents_note', { session: session.id, text: clean, now });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Not iletilemedi.';
  }
}

/** Boştaki Claude oturumuna adadan görev: tur sonunda bekleyen köprü Claude'u uyandırır. */
export async function sendTask(session: AgentSession, text: string): Promise<string | null> {
  const clean = text.trim();
  if (!clean) return 'Görevi yaz.';
  if (!inTauri) {
    const target = data.agents.find(a => a.id === session.id);
    if (!target?.canTask) return 'Bu oturum adadan görev almıyor.';
    target.canTask = false;
    target.status = 'running';
    target.step = 'Görevini okuyor';
    return null;
  }
  try {
    await callOrThrow('agents_task', { session: session.id, text: clean });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Görev iletilemedi.';
  }
}

/** Sıradaki notu iptal eder (henüz iletilmediyse). */
export async function removeNote(session: AgentSession, id: number): Promise<string | null> {
  if (!inTauri) {
    const target = data.agents.find(a => a.id === session.id);
    if (target) target.notes = target.notes.filter(n => n.id !== id);
    return null;
  }
  try {
    await callOrThrow('agents_note_remove', { session: session.id, id });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Not iptal edilemedi.';
  }
}

/** Oturumun klasörünü Gezgin'de (`folder`) ya da VS Code'da (`code`) açar. */
export async function openFolder(session: string, target: 'folder' | 'code'): Promise<string | null> {
  if (!inTauri) return null;
  try {
    await callOrThrow('agents_open', { session, target });
    return null;
  } catch (error) {
    return typeof error === 'string' ? error : 'Klasör açılamadı.';
  }
}

let editor: Promise<boolean> | null = null;
/** VS Code kurulu mu (bir kez sorulur). */
export function editorPresent(): Promise<boolean> {
  editor ??= inTauri ? call<boolean>('agents_editor_present').then(Boolean) : Promise.resolve(true);
  return editor;
}

/** Claude oturumunun modeli ve bağlam doluluğu (kaydın sonundan; yalnız ayrıntı açıkken). */
export async function sessionContext(session: AgentSession): Promise<SessionContext | null> {
  if (session.tool !== 'claude' || !data.settings.features.agentContext) return null;
  if (!inTauri) return { model: 'claude-opus-5-5', tokens: 92_400, window: 200_000 };
  const id = session.id.replace(/^claude:/, '');
  return (await call<SessionContext | null>('usage_session', { session: id })) ?? null;
}

/**
 * Karar yalnız bu isteğe gider; Rust tarafı tek kullanımlık ve süreli olduğunu denetler.
 * "always": onayla ve aracın önerdiği izin kuralını kaydet (kural Rust'taki istek kaydından gelir).
 * "revise": yalnız planlarda; plan onaylanmaz, `note` Claude'a ne değişmesi gerektiğini söyler.
 */
export async function answerApproval(id: string, decision: 'allow' | 'deny' | 'always' | 'revise', note?: string): Promise<string | null> {
  if (inTauri) {
    try {
      await callOrThrow('agents_decide', { id, decision, note: note ?? null });
      return null;
    } catch (error) {
      return typeof error === 'string' ? error : 'Karar iletilemedi.';
    }
  }
  const request = data.approvals.find(r => r.id === id);
  if (!request) return 'Bu istek artık geçerli değil.';
  if (decision === 'revise' && !note?.trim()) return 'Ne değişmesi gerektiğini yaz.';
  data.approvals = data.approvals.filter(r => r.id !== id);
  const session = data.agents.find(a => a.id === request.sessionId);
  if (session) {
    session.status = 'running';
    session.step =
      request.kind === 'plan'
        ? decision === 'allow' || decision === 'always'
          ? 'Plan onaylandı · uyguluyor'
          : 'Planı yeniden düşünüyor'
        : decision === 'deny'
          ? 'İstek reddedildi'
          : decision === 'always'
            ? 'Her zaman izin verildi · devam ediyor'
            : `Çalıştırıyor · ${request.detail}`;
  }
  return null;
}

export function musicControl(action: 'toggle' | 'next' | 'prev'): void {
  if (!data.music) return;
  if (action === 'toggle') {
    // Anında tepki; gerçek durum Windows'tan gelen olayla doğrulanır.
    const music = data.music;
    const now = Date.now();
    if (music.playing) music.positionMs = Math.min(music.durationMs || Infinity, music.positionMs + (now - music.updatedAt));
    music.updatedAt = now;
    music.playing = !music.playing;
    if (music.playing) data.activity.music = now;
  }
  if (inTauri) void call('music_control', { action });
}

/** Parçanın başından `positionMs`'e gider. Anında tepki; gerçek konum Windows'tan gelen olayla düzelir. */
export function musicSeek(positionMs: number): void {
  const music = data.music;
  if (!music?.canSeek) return;
  const target = Math.round(Math.max(0, Math.min(music.durationMs, positionMs)));
  music.positionMs = target;
  music.updatedAt = Date.now();
  if (inTauri) void call('music_seek', { positionMs: target });
}

// Kontroller: durum yalnız ekran açılınca ve bir ayar değişince okunur (arka planda izlenmez).
const sampleControls = demoControls();

export async function readControls(): Promise<ControlsState | null> {
  if (!inTauri) return structuredClone(sampleControls);
  return (await call<ControlsState>('controls_read')) ?? null;
}

/** Wi-Fi, Bluetooth ya da koyu mod. Başarılıysa güncel durum, değilse hata metni. */
export async function setControl(kind: 'wifi' | 'bluetooth' | 'dark', on: boolean): Promise<ControlsState | string> {
  if (!inTauri) {
    sampleControls[kind] = on;
    return structuredClone(sampleControls);
  }
  try {
    return kind === 'dark' ? await callOrThrow<ControlsState>('controls_dark', { on }) : await callOrThrow<ControlsState>('controls_radio', { kind, on });
  } catch (error) {
    return typeof error === 'string' ? error : 'Ayar değiştirilemedi.';
  }
}

/** Varsayılan çıkış aygıtının ses düzeyi (0–100) ve/veya sessizliği. */
export async function setVolume(change: { level?: number; muted?: boolean }): Promise<void> {
  if (!inTauri) {
    if (sampleControls.volume) Object.assign(sampleControls.volume, change);
    return;
  }
  await call('controls_volume', { level: change.level ?? null, muted: change.muted ?? null });
}

const sampleOutputs: AudioOutput[] = [
  { id: 'demo-speaker', name: 'Realtek(R) Audio', kind: 'Hoparlör', default: true },
  { id: 'demo-headset', name: 'Örnek Kulaklık (BT)', kind: 'Kulaklıklar', default: false },
  { id: 'demo-monitor', name: 'Örnek Monitör', kind: null, default: false },
];

/** Etkin ses çıkışları; null: liste alınamadı (arayüz Windows'un ses ayarlarını açar). */
export async function readOutputs(): Promise<AudioOutput[] | null> {
  if (!inTauri) return structuredClone(sampleOutputs);
  const list = await call<AudioOutput[]>('controls_outputs');
  return list && list.length > 0 ? list : null;
}

/** Varsayılan çıkışı değiştirir. Başarılıysa yeni durum ve liste, değilse hata metni. */
export async function setOutput(id: string): Promise<OutputSwitch | string> {
  if (!inTauri) {
    for (const o of sampleOutputs) o.default = o.id === id;
    if (sampleControls.volume) sampleControls.volume.device = sampleOutputs.find(o => o.default)?.name ?? null;
    return { controls: structuredClone(sampleControls), outputs: structuredClone(sampleOutputs) };
  }
  try {
    return await callOrThrow<OutputSwitch>('controls_output', { id });
  } catch (error) {
    return typeof error === 'string' ? error : 'Ses çıkışı değiştirilemedi.';
  }
}

/** Resmi API'si olmayan ayarlar (gece ışığı, rahatsız etme; liste alınamazsa ses) için Windows'un ayar sayfası. */
export function openWindowsSettings(page: 'wifi' | 'bluetooth' | 'sound' | 'nightlight' | 'notifications'): void {
  void call('controls_open', { page });
}

export function openTaskManager(): void {
  void call('open_task_manager');
}

export function openSettings(): void {
  void call('open_settings');
}

/** Adadaki bir metin kutusuna yazılacak: ada geçici olarak odak alır. Ada kapanınca Rust eski
 * hâline döndürür ve odağı önceki pencereye geri verir. */
export function startTyping(): void {
  void call('island_typing', { on: true });
}

/** "Sonra ⌄": kart cevaplanmadan katlanır; istek beklemeye devam eder. */
export function foldApproval(id: string): void {
  if (!data.folded.includes(id)) data.folded = [...data.folded, id];
  if (data.approvalFirst === id) data.approvalFirst = null;
}

/** Simge çubuğundaki "N onay bekliyor · göster": katlanan kartların hepsi geri gelir. */
export function unfoldApprovals(): void {
  data.folded = [];
}

// Yeni görev ve kapanmış oturuma devam (Rust: src-tauri/src/tasks.rs). Hatalar kısa Türkçe metin olarak döner.
const failText = (error: unknown, fallback: string) => (typeof error === 'string' ? error : fallback);

/** Yeni görev için son kullanılan klasörler (en çok 8). */
export async function listRecentProjects(): Promise<RecentProject[]> {
  if (!inTauri) return demoRecentProjects();
  return (await call<RecentProject[]>('list_recent_projects')) ?? [];
}

/** Windows klasör seçici; vazgeçilirse null. */
export async function pickFolder(): Promise<string | null> {
  if (!inTauri) return null;
  return (await call<string | null>('pick_folder')) ?? null;
}

/** Kapanmış oturumlar, yeniden eskiye (açık olanlar hariç). */
export async function listClosedSessions(limit = 10): Promise<ClosedSession[]> {
  if (!inTauri) return demoClosedSessions().slice(0, limit);
  return (await call<ClosedSession[]>('list_closed_sessions', { limit })) ?? [];
}

/** Claude Code bu klasörde arka planda başlar; oturum hook'la adaya düşer. Başarıda oturum kimliği döner. */
export async function startTask(cwd: string, prompt: string, permission: TaskPermission): Promise<{ sessionId: string; taskNo: number } | string> {
  const clean = prompt.trim();
  if (!clean) return 'Görevi yaz.';
  if (!cwd) return 'Klasör seç.';
  if (!inTauri) return demoStartTask(data.agents, cwd, clean);
  try {
    return await callOrThrow<{ sessionId: string; taskNo: number }>('start_task', { cwd, prompt: clean, permission });
  } catch (error) {
    return failText(error, 'Görev başlatılamadı.');
  }
}

/** Kapanmış oturum kaldığı yerden sürer (`claude --resume`, oturumun klasöründe). */
export async function resumeSession(session: ClosedSession, prompt: string): Promise<string | null> {
  const clean = prompt.trim();
  if (!clean) return 'Ne yapılacağını yaz.';
  if (!inTauri) return null;
  try {
    await callOrThrow('resume_session', { sessionId: session.sessionId, cwd: session.cwd, prompt: clean });
    return null;
  } catch (error) {
    return failText(error, 'Oturum sürdürülemedi.');
  }
}

/** Görev süreci hatayla biterse ("claude bulunamadı", giriş yok…) Rust bildirir. */
export function watchTasks(onfail: (fail: TaskFailed) => void): () => void {
  return on<TaskFailed>('task-failed', fail => {
    data.taskFailed = fail;
    onfail(fail);
  });
}
