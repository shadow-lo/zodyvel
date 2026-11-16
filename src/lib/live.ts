// Canlı çentik (Faz 4, md/yeni-rota-plani.md §4 "Bildirimler", eskiz design/eskiz-paket2.html §8–9).
// Ada kapalıyken çentik, o an süren en önemli etkinliği tek satırda gösterir: solda küçük durum simgesi,
// ortada yazı, sağda süre; alt kenarda durum renginde ince çizgi. Öncelik: olay (onay › soru › etkinlik ›
// Bitiş › hatırlatma › bitti/hata › kısa not; sırası notice.ts noticeFor'da) › Zodymetre › mod › AI
// çalışıyor › müzik. Biri geniş hapta (olay › müzik › Zodymetre › mod › AI), diğerleri solunda ayrı küçük
// baloncuk. Faz 5a: en çok üç adlı Zodymetre (her biri ayrı; en yakın biten önde) ve açık mod.
// "Yalnız olunca" kipinde (notch.live false) süren etkinlikler gösterilmez, yalnız olaylar.
// Saf fonksiyonlar: zamanlama ve çizim Island.svelte ve parts/Notch.svelte'de.

import { clockText, meterLeft } from './clock-cards';
import { toolName } from './format';
import type { Notice } from './notice';
import { stepVerb } from './steps';
import type { Tab } from './tabs';
import { focusLabel, minutesText, modeLeft } from './today';
import type { AgentSession, MeterState, ModeDef, ModeIcon, ModeRun, NotchPrefs, NowPlaying, ZodyMood } from './types';

/** Çentikte süren bir etkinlik. */
export type LiveKind = 'timer' | 'mode' | 'working' | 'music';

export interface LiveItem {
  kind: LiveKind;
  /** Değişince geliş animasyonu yeniden oynar (yalnız tür ya da oturum değişince, saniyede bir değil). */
  key: string;
  title: string;
  /** Başlığın yanındaki soluk yazı (dosya adı, sanatçı). */
  detail: string;
  /** Tek satırlık çentik hapındaki kısa yazı (Faz 7.3): "Claude · Düzenliyor", kırpılmış şarkı adı.
   * Ayrıntı (dosya, sanatçı) yalnız fareyle açılan ara boyda. Yoksa başlık + ayrıntı gösterilir. */
  short?: string;
  /** Dosya adı/komut gibi eşaralıklı yazılacak ayrıntı. */
  mono: boolean;
  /** Sağdaki süre ("24:58", "4 dk"); yoksa boş. */
  time: string;
  /** Ara boydaki üst satır ("Gezgin · Claude Code", "Zodymetre"). */
  over: string;
  tab: Tab;
  mood: ZodyMood;
  /** Zodymetre halkası: kalan oran (0–1). */
  ratio?: number;
  /** Zodymetre/mod çalışıyor (duraklat düğmesi) ya da duraklatılmış (başlat). */
  running?: boolean;
  /** Zodymetre: hangisi (ara boy düğmeleri bu kimlikle). */
  meterId?: number;
  /** Zodymetre/mod rengi (boş: vurgu). */
  color?: string;
  /** Mod simgesi ve süresiz mi (süresizde +5 dk yok). */
  icon?: ModeIcon;
  endless?: boolean;
  /** AI: oturum (tıklayınca bu oturum seçilir) ve araç (Zody çerçevesi). */
  session?: string;
  tool?: string;
  /** AI ara boyu: son adımlar (eskiden yeniye, en çok 5), sürmekte olan son sırada. */
  steps?: ('done' | 'now')[];
  /** AI ara boyu: adım sayacı ("4. aşama"). */
  stepNo?: number;
}

/** Çentiğin bütün hâli: sağda geniş hap (olay ya da süren etkinlik), solunda ayrı küçük baloncuklar. */
export interface NotchView {
  /** Geniş haptaki olay (onay, Bitiş, bitti…); yoksa null. */
  notice: Notice | null;
  /** Olay yoksa geniş haptaki süren etkinlik. */
  lead: LiveItem | null;
  /** Geniş hapın solundaki ayrı baloncuklar (yalnız simge; basınca kendi sekmesi), en çok MAX_BUBBLES. */
  bubbles: LiveItem[];
  /** "Her zaman açık ada" açık ve hiçbir şey yok: küçük adada yalnız Zody'nin çerçevesiz gözleri. */
  idle: boolean;
}

/** Çentikte fareyle gelinebilecek yer: süren etkinlik, olay ya da boştaki Zody. */
export type NotchTarget = LiveItem | 'notice' | 'idle';
export const targetKey = (t: NotchTarget) => (typeof t === 'string' ? t : t.key);

export const MAX_BUBBLES = 3;

export interface LiveInput {
  notice: Notice | null;
  prefs: NotchPrefs;
  agents: AgentSession[];
  meters: MeterState[];
  /** Açık mod ve tanımı. */
  mode: { run: ModeRun; def: ModeDef | null } | null;
  music: NowPlaying | null;
  /** Saniyelik saat (yalnız Zodymetre çalışırken saniyede bir ilerletilir). */
  now: number;
  /** Açık mini pencerenin hapı (Faz 9): müzik duraklasa da çentikte kalır. */
  hold?: string | null;
}

/** Zodymetre: çalışıyor ya da duraklatılmış ama bitmemiş. Dolmuş Zodymetre Bitiş bildirimidir. */
export function meterItem(meter: MeterState, now: number): LiveItem | null {
  const running = meter.endsAt !== null;
  const { left, ratio } = meterLeft(meter, now);
  // Hiç başlatılmamış (tam süre, duruyor) ya da dolmuş Zodymetre süren bir etkinlik değildir.
  if (left <= 0 || (!running && meter.leftMs >= meter.minutes * 60_000)) return null;
  return {
    kind: 'timer',
    key: `timer:${meter.id}`,
    title: meter.name || 'Zodymetre',
    detail: running ? '' : 'duraklatıldı',
    mono: false,
    time: clockText(left),
    over: 'Zodymetre',
    tab: 'today',
    mood: running ? 'working' : 'idle',
    ratio,
    running,
    meterId: meter.id,
    color: meter.color,
  };
}

/** Süren Zodymetreler, en yakın biten önde (duraklatılmışlar sonda). */
export function meterItems(meters: MeterState[], now: number): LiveItem[] {
  const ends = (m: MeterState) => (m.endsAt ?? Number.MAX_SAFE_INTEGER - m.leftMs);
  return [...meters].sort((a, b) => ends(a) - ends(b)).map(m => meterItem(m, now)).filter((x): x is LiveItem => x !== null);
}

/** Açık mod (Faz 5a): modun renginde; süreliyse kalan, süresizse geçen süre. */
export function modeItem(run: ModeRun, def: ModeDef | null, now: number): LiveItem {
  const left = modeLeft(run, now);
  const elapsed = Math.max(0, Math.floor((now - run.startedAt) / 60_000));
  const name = def?.name ?? 'Mod';
  return {
    kind: 'mode',
    key: `mode:${run.id}`,
    title: name,
    detail: run.paused ? 'duraklatıldı' : run.focus ? focusLabel(run.focus) : 'modu',
    mono: false,
    time: left !== null ? clockText(left) : minutesText(elapsed < 1 ? 1 : elapsed),
    over: left !== null ? `Mod · ${minutesText(run.minutes)}` : 'Mod · süresiz',
    tab: 'modes',
    mood: run.paused ? 'idle' : 'working',
    ...(left !== null && { ratio: Math.min(1, left / Math.max(1, run.minutes * 60_000)) }),
    running: !run.paused,
    color: def?.color ?? '',
    icon: def?.icon ?? 'star',
    endless: left === null,
  };
}

/** Çalışan AI oturumu: en son hareket eden. Örnek oturumlar sayılmaz. */
export function workingItem(agents: AgentSession[], now: number): LiveItem | null {
  const running = agents.filter(a => a.status === 'running' && !a.demo);
  if (!running.length) return null;
  const a = running.reduce((m, x) => (x.lastSeen > m.lastSeen ? x : m));
  const steps = a.steps.filter(s => !s.sub);
  const current = [...steps].reverse().find(s => s.state === 'now') ?? steps.at(-1) ?? null;
  const started = a.turnStartedAt ?? a.statusSince;
  const minutes = Math.max(0, Math.floor((now - started) / 60_000));
  // Ara boydaki aşama çubuğu: bu turdaki adımlar (prompt'tan sonra), son beşi.
  const turn = steps.filter(s => s.at >= started && s.kind !== 'prompt');
  return {
    kind: 'working',
    key: `working:${a.id}`,
    title: current ? stepVerb(current) : 'Çalışıyor',
    detail: current?.target ? baseName(current.target) : '',
    short: `${toolName(a.tool)} · ${current ? stepVerb(current) : 'Çalışıyor'}`,
    mono: Boolean(current?.target) && current?.kind !== 'search' && current?.kind !== 'web',
    time: minutes < 1 ? 'şimdi' : `${minutes} dk`,
    over: `${a.project} · ${toolName(a.tool)}${running.length > 1 ? ` · +${running.length - 1} oturum` : ''}`,
    tab: 'ai',
    mood: 'working',
    session: a.id,
    tool: a.tool,
    steps: turn.slice(-5).map(s => (s.state === 'now' ? 'now' : 'done')),
    stepNo: turn.length,
  };
}

/** Çalan müzik (duraklatılmışsa çentikte yer kaplamaz). `hold`: Mini Oynatıcı açıkken duraklatılan şarkı da
 * kalır, yoksa "Duraklat"a basınca pencere elden kaçardı (Faz 9). */
export function musicItem(music: NowPlaying | null, hold = false): LiveItem | null {
  if (!music?.title || (!music.playing && !hold)) return null;
  return {
    kind: 'music',
    key: 'music',
    title: music.title,
    detail: music.artist,
    short: clip(music.title, SHORT_TITLE),
    mono: false,
    time: '',
    over: music.app || 'Müzik',
    tab: 'music',
    mood: 'music',
    running: music.playing,
  };
}

/** Çentik hapında şarkı adının en uzun hâli (karakter; sanatçı yalnız fareyle). */
export const SHORT_TITLE = 26;

/** Yazıyı en çok `max` karaktere indirir ("…" dahil). Karakter karakter sayılır (Türkçe harfler bölünmez). */
export function clip(text: string, max: number): string {
  const chars = Array.from(text.trim());
  return chars.length <= max ? chars.join('') : `${chars.slice(0, max - 1).join('').trimEnd()}…`;
}

/** Yol ya da komuttan kısa ad: "src/lib/auth.ts" → "auth.ts"; komutlar olduğu gibi. */
export function baseName(target: string): string {
  if (/\s/.test(target.trim())) return target.trim();
  const parts = target.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? target;
}

/** Çentiğin hâli (karar 9 Kasım): geniş hapta olay, yoksa müzik, yoksa öndeki süren etkinlik; diğerleri
 * solda ayrı baloncuk (ör. AI çalışırken müzik çalıyorsa solda AI baloncuğu, sağda müzik). */
export function notchView({ notice, prefs, agents, meters, mode, music, now, hold = null }: LiveInput): NotchView {
  const live = prefs.live
    ? [
        ...(prefs.timer ? meterItems(meters, now) : []),
        prefs.mode && mode ? modeItem(mode.run, mode.def, now) : null,
        prefs.ai ? workingItem(agents, now) : null,
      ].filter((x): x is LiveItem => x !== null)
    : [];
  const song = prefs.live && prefs.music ? musicItem(music, hold === 'music') : null;
  if (notice) return { notice, lead: null, bubbles: [...live, ...(song ? [song] : [])].slice(0, MAX_BUBBLES), idle: false };
  if (song) return { notice: null, lead: song, bubbles: live.slice(0, MAX_BUBBLES), idle: false };
  const [first, ...rest] = live;
  return { notice: null, lead: first ?? null, bubbles: rest.slice(0, MAX_BUBBLES), idle: !first && prefs.live };
}

/** Çentikte bir şey gösteriliyor mu (siyah şerit değil). */
export const notchShown = (v: NotchView) => v.notice !== null || v.lead !== null || v.idle;

/** Olay çentikte bu kadar "taze" kalır (renkli hareketli çerçeve + yazı). Fare gelmezse sonra söner:
 * onay/soru küçük Zody gözlerine döner (istek bitene kadar), bitti/hata ve kısa notlar kaybolur. */
export const NOTICE_HOLD_MS = 9000;

/** Tazeliği geçmiş olay çentikte kalır mı: yalnız karar bekleyenler (onay, soru, etkinlik) ve Bitiş. */
export const noticeStays = (n: Notice) => Boolean(n.finish) || n.mood === 'attention';

/** Ara boy (fare gelince genişleme) olur mu: süren etkinlik ya da bitti/hata gibi bilgi olayı. Onay ve soru
 * doğrudan açılır (karar adada verilir); Bitiş zaten büyük; boştaki Zody adayı açar. */
export function peekable(t: NotchTarget, notice: Notice | null): boolean {
  if (t === 'idle') return false;
  if (t === 'notice') return notice !== null && !notice.finish && notice.mood !== 'attention';
  return true;
}

/** Çentik boyları. Yükseklik sabit; genişlik içeriğe göre ölçülür ve bu aralıkta tutulur. */
export const NOTCH_H = 36;
export const NOTCH_MIN_W = 36;
/** Boştaki Zody hapı: siyah alan gözlerle aynı boyda (karar 9 Kasım 22:59). */
export const IDLE_H = 20;
/** Tek hapın en geniş hâli (AI çalışıyor, müzik; Faz 7.3). */
export const PILL_MAX_W = 300;
/** Baloncuklar dahil en geniş çentik. */
export const NOTCH_MAX_W = 700;
/** Ara boy (fare gelince; genişlik baloncuklarla birlikte ölçülür) ve Bitiş bildirimi. */
export const PEEK_H = 92;
export const FINISH_SIZE = { w: 460, h: 116 };
/** Ses göstergesi kapsülü (Faz 10): simge + 132 px çubuk + yüzde. */
export const OSD_W = 224;

/** Mini pencereler (Faz 9; eskiz-paket3 ekran 1–3). Fare bir hapa gelince ('mini' kipinde) o hap bu boya
 * büyür; ayarda kapalı olan hap ve mod ara boyda kalır. */
export type MiniKind = 'ai' | 'music' | 'timer';
export const MINI_SIZE: Record<MiniKind, { w: number; h: number }> = {
  ai: { w: 540, h: 140 },
  music: { w: 420, h: 112 },
  timer: { w: 420, h: 128 },
};
/** Birden çok Zodymetre varken Mini Kronometre'nin üstündeki sekme satırı. */
export const MINI_TABS_H = 30;
/** Fare çentik + mini pencere alanından çıkınca bu kadar bekleyip hap hâline döner (kısa kaçışta titremesin). */
export const MINI_CLOSE_MS = 350;
/** Güvenlik: fare hareketsiz/pencere dışında bu kadar kaldıysa açık mini pencere kapanır (ms). */
export const MINI_IDLE_MS = 8000;

/** Fareyle gelinen yer mini pencere açar mı, açarsa hangisi. Bitti/hata bildirimi o oturumun Mini AI'ı olur;
 * onay ve soru her zaman doğrudan açılır (bugünkü kural), Bitiş ve boştaki Zody de mini değildir. */
export function miniFor(t: NotchTarget, prefs: NotchPrefs, notice: Notice | null): MiniKind | null {
  if (prefs.hover !== 'mini' || t === 'idle') return null;
  if (t === 'notice') {
    const done = notice !== null && !notice.finish && (notice.mood === 'done' || notice.mood === 'error') && Boolean(notice.open?.session);
    return done && prefs.miniAi ? 'ai' : null;
  }
  if (t.kind === 'working') return prefs.miniAi ? 'ai' : null;
  if (t.kind === 'music') return prefs.miniMusic ? 'music' : null;
  if (t.kind === 'timer') return prefs.miniTimer ? 'timer' : null;
  return null;
}

/** Mini pencerenin boyu: genişlik içerikten (baloncuklarla) ölçülür, en az pencerenin kendi genişliği. */
export function miniHeight(kind: MiniKind, timers: number): number {
  return MINI_SIZE[kind].h + (kind === 'timer' && timers > 1 ? MINI_TABS_H : 0);
}

export function notchWidth(measured: number): number {
  return Math.round(Math.min(NOTCH_MAX_W, Math.max(NOTCH_MIN_W, measured)));
}
