// Zody sohbetinin sağlayıcıdan bağımsız modeli. Saf: mesaj türleri, "beyin" arayüzü ve kural beyni.
//
// Akış: kullanıcı yazar (ya da bas-konuş sözü yazıya döner) → beyinler sırayla sorulur, ilk cevap veren
// kazanır → cevaptaki adımlar (eylem kaydındaki adlar) chat.svelte.ts'de requestAction ile çalışır.
// Onay isteyen adım sohbete kart olarak düşer; kartın düğmesine yalnız fare/klavye basar. Ne yazı ne ses
// onay verir. Bugün tek beyin kurallar (commands.ts); ileride bir model beyni zincirin sonuna eklenir ve
// aynı Reply/Step biçimini döndürür (model yalnız önerir, eylem kaydı uygular).

import { EXAMPLES, parse, type AskTopic, type Intent, type MusicOp, type TalkTopic } from './commands';
import { toolName } from './format';
import { plansOn } from './agenda';
import { isoDate, timerLeft, whenLabel } from './today';
import type { AgentSession, ApprovalRequest, Battery, NowPlaying, TodayData, VolumeState } from './types';

export type Tone = 'ok' | 'info' | 'warn' | 'error';
/** Zody'nin sohbetteki hâli: maskotun ruh hâline ve küçük yazıya çevrilir. */
export type ChatMood = 'idle' | 'thinking' | 'listening' | 'happy' | 'confused';

/** Eylem kaydındaki bir eylem (src-tauri/src/actions/spec.rs ACTIONS adları). */
export interface Step {
  action: string;
  args?: Record<string, unknown>;
}

export interface Card {
  /** Eylem merkezindeki bekleyen isteğin numarası. */
  id: number;
  summary: string;
  state: 'waiting' | 'allowed' | 'denied' | 'expired' | 'failed';
  note?: string;
}

export interface ChatMessage {
  id: number;
  from: 'me' | 'zody';
  text: string;
  at: number;
  /** Bas-konuş ile söylendi. */
  via?: 'voice';
  tone?: Tone;
  card?: Card;
  /** Dokununca yazılan öneriler. */
  chips?: string[];
}

/** Beynin gördüğü dünya: adanın zaten bildiği durum (ek okuma yapılmaz). */
export interface World {
  now: Date;
  agents: AgentSession[];
  approvals: ApprovalRequest[];
  music: NowPlaying | null;
  today: TodayData | null;
  battery: Battery | null;
  /** Yalnız ses komutunda okunur (controls.read). */
  volume?: VolumeState | null;
  /** Kapalı modüller: Zody o konuda "kapalı" der. */
  modules: { agents: boolean; music: boolean; today: boolean; system: boolean; modes?: boolean; agenda?: boolean };
}

export interface Turn {
  text: string;
  via: 'text' | 'voice';
  world: World;
}

export interface Reply {
  /** Adımlar başarıyla bitince (ya da adım yoksa hemen) söylenen. */
  text: string;
  tone?: Tone;
  mood?: ChatMood;
  steps?: Step[];
  chips?: string[];
  /** Onay kartının üstündeki cümle (adım onay isterse `text` yerine). */
  confirmText?: string;
}

/** Sohbet sağlayıcısı. `null`: anlamadım, sıradakine sor. */
export interface Brain {
  id: string;
  /** Ayrıntıda Zody'nin altında görünen ad ("Kurallar", ileride "Claude"). */
  label: string;
  reply(turn: Turn): Reply | null | Promise<Reply | null>;
}

/** Beyinleri sırayla dener; hiçbiri anlamazsa Zody'nin "anlamadım"ı. */
export async function think(brains: Brain[], turn: Turn): Promise<Reply> {
  for (const brain of brains) {
    const reply = await brain.reply(turn);
    if (reply) return reply;
  }
  return confused();
}

const pick = <T>(list: T[], seed: number) => list[Math.abs(seed) % list.length]!;

export function confused(): Reply {
  return {
    text: 'Bunu henüz anlamıyorum. Şunlardan birini dener misin?',
    tone: 'info',
    mood: 'confused',
    chips: EXAMPLES.slice(0, 3),
  };
}

const off = (what: string): Reply => ({ text: `${what} kapalı. Ayarlar › Modüller'den açabilirsin.`, tone: 'warn', mood: 'confused' });

const STATUS: Record<AgentSession['status'], string> = {
  running: 'çalışıyor',
  waiting_input: 'seni bekliyor',
  waiting_approval: 'onay bekliyor',
  done: 'bitirdi',
  error: 'hata verdi',
};

function agentsReply(w: World): Reply {
  if (!w.modules.agents) return off('AI modülü');
  const live = w.agents.filter(a => !a.asleep || a.status === 'waiting_approval');
  if (!live.length) return { text: 'Şu an açık bir ajan oturumu yok. Hepsi dinleniyor.', mood: 'idle' };
  const lines = live.slice(0, 3).map(a => {
    const step = a.status === 'running' && a.step ? `: ${a.step}` : '';
    return `${toolName(a.tool)} · ${a.project} ${STATUS[a.status]}${step}`;
  });
  const more = live.length > 3 ? ` (+${live.length - 3})` : '';
  const waiting = w.approvals.length ? ` ${w.approvals.length} onay seni bekliyor.` : '';
  return { text: `${lines.join('. ')}${more}.${waiting}`, tone: w.approvals.length ? 'warn' : undefined, mood: 'idle' };
}

function dayReply(w: World, offset: 0 | 1): Reply {
  if (w.modules.agenda === false || !w.today) return off('Ajanda modülü');
  const date = isoDate(new Date(w.now.getFullYear(), w.now.getMonth(), w.now.getDate() + offset));
  const events = plansOn(w.today.events, date);
  const label = offset ? 'Yarın' : 'Bugün';
  if (!events.length) return { text: `${label} takvimde bir şey yok. Kafan rahat.`, mood: 'happy' };
  const list = events.slice(0, 4).map(e => (e.time ? `${e.time} ${e.title}` : e.title));
  return { text: `${label}: ${list.join(', ')}${events.length > 4 ? ` ve ${events.length - 4} tane daha` : ''}.`, mood: 'idle' };
}

function timerReply(w: World): Reply {
  if (!w.modules.today || !w.today) return off('Bugün modülü');
  // Zodymetreler (Faz 5a): sohbet ilk Zodymetre'yi kullanır.
  const t = w.today.meters[0];
  const left = t ? timerLeft(t, w.now.getTime()) : 0;
  if (!t || (!t.endsAt && (left === 0 || left === t.minutes * 60_000)))
    return { text: 'Kurulu bir zamanlayıcı yok. "25 dakika zamanlayıcı" demen yeter.', chips: ['25 dakika zamanlayıcı'] };
  const min = Math.ceil(left / 60_000);
  return { text: t.endsAt ? `${min} dakika kaldı.` : `Zamanlayıcı duraklatıldı, ${min} dakika kaldı.`, mood: 'idle' };
}

function askReply(topic: AskTopic, w: World): Reply {
  const now = w.now;
  switch (topic) {
    case 'agents':
      return agentsReply(w);
    case 'approvals':
      if (!w.modules.agents) return off('AI modülü');
      return w.approvals.length
        ? { text: `${w.approvals.length} onay bekliyor. AI bölümünde kartı açıp karar verebilirsin.`, tone: 'warn' }
        : { text: 'Bekleyen onay yok.', mood: 'happy' };
    case 'today':
      return dayReply(w, 0);
    case 'tomorrow':
      return dayReply(w, 1);
    case 'todos': {
      if (!w.modules.today || !w.today) return off('Bugün modülü');
      const open = w.today.todos.filter(t => !t.done);
      if (!open.length) return { text: 'Yapılacaklar listen boş.', mood: 'happy' };
      return { text: `${open.length} iş açık: ${open.slice(0, 4).map(t => t.text).join(', ')}${open.length > 4 ? '…' : '.'}` };
    }
    case 'timer':
      return timerReply(w);
    case 'music':
      if (!w.modules.music) return off('Müzik modülü');
      return w.music
        ? { text: `${w.music.playing ? 'Çalıyor' : 'Duraklatıldı'}: ${w.music.title}${w.music.artist ? ` · ${w.music.artist}` : ''}.` }
        : { text: 'Şu an bir şey çalmıyor.' };
    case 'battery':
      if (!w.battery) return { text: 'Pil bilgisi yok (masaüstü bilgisayar ya da Bilgisayar modülü kapalı).' };
      return {
        text: `Pil %${w.battery.percent}${w.battery.charging ? ', şarj oluyor' : w.battery.minutesLeft ? `, yaklaşık ${Math.round(w.battery.minutesLeft / 6) / 10} saat gider` : ''}.`,
        tone: w.battery.percent <= 15 && !w.battery.charging ? 'warn' : undefined,
      };
    case 'time':
      return { text: `Saat ${now.toLocaleTimeString('tr-TR', { hour: '2-digit', minute: '2-digit' })}.` };
    case 'date':
      return { text: `Bugün ${now.toLocaleDateString('tr-TR', { day: 'numeric', month: 'long', weekday: 'long' })}.` };
  }
}

const HELP =
  'Zamanlayıcı, odak, müzik, ses düzeyi, takvim, yapılacaklar, tema ve ajanların durumu. Kısa yaz: "25 dk zamanlayıcı", "müziği durdur", "yarın 10\'da toplantı ekle".';

function talkReply(topic: TalkTopic, seed: number): Reply {
  switch (topic) {
    case 'hello':
      return { text: pick(['Selam! Ne yapalım?', 'Merhaba! Buradayım.', 'Hey! Bir şey mi lazım?'], seed), mood: 'happy', chips: EXAMPLES.slice(0, 3) };
    case 'mood':
      return { text: pick(['Yörüngem sakin, teşekkürler. Sen nasılsın?', 'Halkalarım parlıyor, iyiyim!'], seed), mood: 'happy' };
    case 'thanks':
      return { text: pick(['Ne demek!', 'Rica ederim.', 'Her zaman!'], seed), mood: 'happy' };
    case 'help':
      return { text: HELP, chips: EXAMPLES.slice(0, 4) };
    case 'who':
      return { text: 'Ben Zody, adada yaşayan küçük gezegen. Şimdilik kurallarla anlıyorum; ileride daha çok konuşacağız.', mood: 'happy' };
    case 'approve':
      return { text: 'Onayı sözle vermiyorum, güvenlik için. Karttaki Onayla düğmesine basman gerekiyor.', tone: 'info' };
  }
}

const MUSIC: Record<MusicOp, { action: 'toggle' | 'next' | 'prev'; say: string }> = {
  pause: { action: 'toggle', say: 'Müziği duraklattım.' },
  play: { action: 'toggle', say: 'Müzik devam ediyor.' },
  next: { action: 'next', say: 'Sonraki parçaya geçtim.' },
  prev: { action: 'prev', say: 'Önceki parçaya döndüm.' },
};

function musicReply(op: MusicOp, w: World): Reply {
  if (!w.modules.music) return off('Müzik modülü');
  if (!w.music) return { text: 'Şu an çalan bir şey yok.', mood: 'confused' };
  if (op === 'pause' && !w.music.playing) return { text: 'Müzik zaten duraklatılmış.' };
  if (op === 'play' && w.music.playing) return { text: 'Müzik zaten çalıyor.' };
  const m = MUSIC[op];
  return { text: m.say, mood: 'happy', steps: [{ action: 'music.control', args: { action: m.action } }] };
}

const VOLUME_STEP = 10;

function volumeReply(intent: Extract<Intent, { kind: 'volume' }>, w: World): Reply {
  const v = w.volume;
  if (!v) return { text: 'Ses düzeyini okuyamadım.', tone: 'error', mood: 'confused' };
  const set = (level: number, muted?: boolean): Reply => {
    const clamped = Math.max(0, Math.min(100, Math.round(level)));
    return {
      text: muted === true ? 'Sesi kapattım.' : `Ses %${clamped}.`,
      mood: 'happy',
      steps: [{ action: 'controls.volume', args: { level: clamped, ...(muted !== undefined && { muted }) } }],
    };
  };
  switch (intent.op) {
    case 'set':
      return set(intent.level ?? v.level, false);
    case 'up':
      return set(v.muted ? v.level : v.level + VOLUME_STEP, false);
    case 'down':
      return set(v.level - VOLUME_STEP);
    case 'mute':
      return v.muted ? { text: 'Ses zaten kapalı.' } : { text: 'Sesi kapattım.', mood: 'happy', steps: [{ action: 'controls.volume', args: { muted: true } }] };
    case 'unmute':
      return { text: `Ses açık, %${v.level}.`, mood: 'happy', steps: [{ action: 'controls.volume', args: { muted: false } }] };
  }
}

const PAGES = { wifi: 'Wi-Fi', bluetooth: 'Bluetooth', sound: 'Ses', nightlight: 'Gece ışığı', notifications: 'Bildirim' } as const;

/** Niyeti cevaba çevirir. Kural beyninin kalbi; `seed` cevap çeşitlemesi için (testte sabit). */
export function replyFor(intent: Intent, w: World, seed = 0): Reply | null {
  const todayOff = !w.modules.today ? off('Bugün modülü') : null;
  switch (intent.kind) {
    case 'unknown':
      return null;
    case 'talk':
      return talkReply(intent.topic, seed);
    case 'ask':
      return askReply(intent.topic, w);
    case 'music':
      return musicReply(intent.op, w);
    case 'pause':
      if (w.music?.playing && w.modules.music) return musicReply('pause', w);
      if (w.today?.meters[0]?.endsAt && w.modules.today) return { text: 'Zamanlayıcıyı duraklattım.', steps: [{ action: 'today.timer_pause' }] };
      return { text: 'Durduracak bir şey bulamadım: müzik çalmıyor, zamanlayıcı da kurulu değil.' };
    case 'resume':
      if (w.music && !w.music.playing && w.modules.music) return musicReply('play', w);
      if (w.today?.meters[0] && !w.today.meters[0].endsAt && w.today.meters[0].leftMs > 0 && w.modules.today)
        return { text: 'Zamanlayıcı devam ediyor.', steps: [{ action: 'today.timer_start' }] };
      return { text: 'Devam ettirecek bir şey bulamadım.' };
    case 'volume':
      return volumeReply(intent, w);
    case 'timer':
      if (todayOff) return todayOff;
      if (intent.minutes > 180) return { text: 'Zamanlayıcı en çok 180 dakika olabiliyor.', tone: 'warn' };
      return {
        text: `${intent.minutes} dakikalık zamanlayıcı başladı.`,
        mood: 'happy',
        steps: [{ action: 'today.timer_set', args: { minutes: intent.minutes } }, { action: 'today.timer_start' }],
      };
    case 'timerOp': {
      if (todayOff) return todayOff;
      const map = {
        pause: { action: 'today.timer_pause', text: 'Zamanlayıcıyı duraklattım.' },
        start: { action: 'today.timer_start', text: 'Zamanlayıcı devam ediyor.' },
        reset: { action: 'today.timer_reset', text: 'Zamanlayıcıyı sıfırladım.' },
      } as const;
      const m = map[intent.op];
      return { text: m.text, steps: [{ action: m.action }] };
    }
    case 'focus':
      // Odak döngüsü Faz 5a'dan beri modun içinde döner (Modlar modülü).
      if (!w.modules.modes) return off('Modlar modülü');
      return intent.on
        ? { text: 'Çalışma modu odak döngüsüyle başladı: 25 dakika odak, sonra mola. Kolay gelsin!', mood: 'happy', steps: [{ action: 'today.focus_start' }] }
        : { text: 'Odak döngülü modu bitirdim.', steps: [{ action: 'today.focus_stop' }] };
    case 'todo':
      if (todayOff) return todayOff;
      return { text: `"${intent.text}" yapılacaklara eklendi.`, mood: 'happy', steps: [{ action: 'today.todo_add', args: { text: intent.text } }] };
    case 'event': {
      if (w.modules.agenda === false) return off('Ajanda modülü');
      const label = whenLabel({ id: 0, date: intent.date, time: intent.time, title: intent.title }, w.now);
      return {
        text: `Takvime ekledim: ${label} · ${intent.title}.`,
        confirmText: `${label} · ${intent.title}. Ekleyeyim mi?`,
        mood: 'happy',
        steps: [{ action: 'today.event_add', args: { title: intent.title, date: intent.date, ...(intent.time && { time: intent.time }) } }],
      };
    }
    case 'dark':
      return { text: intent.on ? 'Koyu temaya geçtim.' : 'Açık temaya geçtim.', steps: [{ action: 'controls.dark', args: { on: intent.on } }] };
    case 'radio': {
      const name = intent.radio === 'wifi' ? 'Wi-Fi' : 'Bluetooth';
      return {
        text: `${name} ${intent.on ? 'açıldı' : 'kapandı'}.`,
        confirmText: `${name} ${intent.on ? 'açılsın' : 'kapansın'} mı?`,
        steps: [{ action: 'controls.radio', args: { kind: intent.radio, on: intent.on } }],
      };
    }
    case 'open': {
      const map = {
        taskManager: { action: 'system.task_manager', text: "Görev Yöneticisi'ni açtım." },
        settings: { action: 'settings.open', text: "Zodyvel Ayarları'nı açtım." },
        dataFolder: { action: 'settings.data_folder', text: 'Veri klasörünü açtım.' },
      } as const;
      const m = map[intent.target];
      return { text: m.text, steps: [{ action: m.action }] };
    }
    case 'page':
      return { text: `Windows ${PAGES[intent.page]} ayarlarını açtım.`, steps: [{ action: 'controls.open', args: { page: intent.page } }] };
  }
}

/** Kural beyni: model yok, Türkçe kalıplar. Anlamazsa sıradaki beyne bırakır. */
export const rulesBrain: Brain = {
  id: 'rules',
  label: 'Kurallar',
  reply: turn => replyFor(parse(turn.text, turn.world.now), turn.world, turn.text.length),
};

/** Bu niyet okumadan önce ses düzeyini ister mi (sohbet katmanı controls.read yapar). */
export const needsVolume = (text: string, now = new Date()) => parse(text, now).kind === 'volume';

/** Sohbet geçmişi bu kadar mesajı tutar (yalnız bellekte; uygulama kapanınca silinir). */
export const MAX_MESSAGES = 60;

/** Zody'nin maskot ruh hâli: sohbet hâli → ZodyMood. */
export function mascotMood(mood: ChatMood): 'idle' | 'working' | 'attention' | 'done' {
  return mood === 'thinking' ? 'working' : mood === 'listening' ? 'attention' : mood === 'happy' ? 'done' : 'idle';
}

/** Zody'nin altındaki küçük yazı. */
export function moodLabel(mood: ChatMood): string {
  switch (mood) {
    case 'listening':
      return 'Dinliyorum…';
    case 'thinking':
      return 'Düşünüyorum…';
    case 'confused':
      return 'Hmm…';
    case 'happy':
      return 'Tamamdır!';
    default:
      return 'Bir şey yaz';
  }
}
