// Çentik bildirimi: ada kapalıyken önemli bir olay olursa ince şerit, durumu anlatan Zody ve kısa bir
// yazıyla küçük bir habere dönüşür ("Onay bekleniyor"). Üzerine gelince ya da tıklayınca ilgili bölüm
// açılır. Onay/soru istek bitene kadar durur; bir ajanın işi bitince ya da hata verince birkaç saniyelik
// kısa bildirim çıkar. Saf fonksiyonlar: hangi olayın gösterileceğini seçer; çizim parts/Notch.svelte'de.
// Zamanlayıcı dolunca "Zamanlayıcı doldu", takvimdeki bir etkinliğin saati gelince etkinliğin adı (sesleri
// Rust çalar); ikisi de Bugün bölümü görülene kadar durur.
// Sustur kısayoluna basılınca "Sesler kapalı/açık" kısa notu (en düşük öncelik).
// Akıllı uyarılar (Ayarlar → Özellikler, her biri ayrı anahtar): "Bitti" bildiriminde tur süresi ve
// değişen dosya sayısı; uzun bekleyen onay ya da yanıt için bir kez hatırlatma. Pil notları da kısa not
// olarak gösterilir (şarja takıldı, pilden çalışıyor, pil azaldı).

import { duration, toolName } from './format';
import type { Tab } from './island-machine';
import { focusStepText, minutesText } from './today';
import type { AgentSession, AgentStatus, ApprovalRequest, Battery, EventDue, FocusStep, MeterDone, ModeDone, ModeIcon, ZodyMood } from './types';

export interface Notice {
  /** Değişince geliş animasyonu yeniden oynar. */
  key: string;
  mood: ZodyMood;
  title: string;
  detail: string;
  /** Üzerine gelince/tıklayınca açılacak bölüm. */
  tab: Tab;
  /** Aynı anda bekleyen diğer istekler. */
  more: number;
  /** Tıklayınca ayrıntıda açılacak oturum ve sekmesi ("Bitti · 4 dosya" → o oturumun Değişenler'i). */
  open?: { session: string; section: 'changed' | null };
  /** Bitiş bildirimi (Faz 4–5a): Zodymetre doldu ya da mod bitti; büyük çizilir, düğmeleri vardır. */
  finish?: Finish;
}

/** Bitiş: Zodymetre (+5 dk / Yeniden / Tamam; adı ve bugünkü turu) ya da mod (kapattığı uygulamaları
 * yeniden aç / 5 dk mola / Tamam), rengi kaynağından. */
export type Finish =
  | { kind: 'meter'; minutes: number; meterId: number | null; name: string; color: string; rounds: number; totalMs: number }
  | { kind: 'mode'; minutes: number; modeId: string; name: string; color: string; icon: ModeIcon; reopen: string[] };

/** Bitiş düğmeleri: Zodymetre 'add' (+5 dk) / 'again'; mod 'reopen' / 'break' (5 dk mola); 'ok' kapat. */
export type FinishAction = 'add' | 'again' | 'reopen' | 'break' | 'ok';

/** Kısa süreli olay: bir ajan işini bitirdi ya da hata verdi. */
export interface AgentEvent {
  key: string;
  /** Oturum kimliği (bildirime tıklayınca bu oturum açılır). */
  session: string;
  kind: 'done' | 'error';
  tool: AgentSession['tool'];
  project: string;
  /** Turun süresi (ms); bilinmiyorsa null. */
  turnMs: number | null;
  /** Turda değişen farklı dosya sayısı. */
  files: number;
}

/** Bekleme hatırlatması: oturum uzun süredir kullanıcının yanıtını bekliyor (onay isteği dışında). */
export interface Reminder {
  key: string;
  tool: AgentSession['tool'];
  project: string;
  minutes: number;
}

/** Kısa not (ör. sustur kısayolu); hazır bir bildirim gibi gösterilir. */
export type Toast = Omit<Notice, 'more'>;

/** Kısa bildirimin ekranda kalma süresi. */
/** Bitti/hata olayının en uzun kalışı; çentikte asıl süreyi live.ts NOTICE_HOLD_MS belirler (fareyle bakılırsa uzar). */
export const FLASH_MS = 30_000;

const MINUTE = 60_000;

export interface NoticeInput {
  approvals: ApprovalRequest[];
  /** Son kısa olay (FLASH_MS sonra kaldırılır). */
  flash?: AgentEvent | null;
  /** Az önce dolan Zodymetre (`meter`) ya da açık modda biten odak aşaması (`focus`). */
  timerDone?: { key: number; minutes: number; meter?: MeterDone; focus?: FocusStep } | null;
  /** Az önce biten mod (Mod Bitişi). */
  modeDone?: (ModeDone & { key: number }) | null;
  /** Saati gelen etkinlik. */
  eventDue?: EventDue | null;
  /** Kısa not (birkaç saniye sonra kaldırılır). */
  toast?: Toast | null;
  /** "Bitti" bildiriminde tur süresi ve dosya sayısı (Özellikler → Bitti özeti). */
  doneSummary?: boolean;
  /** Bu kadar dakikadır bekleyen onayın başlığı "N dk'dır onay bekliyor" olur; null: kapalı. */
  lateAfterMin?: number | null;
  /** Hesap anı (bekleme süresi için). */
  now?: number;
  /** Uzun süredir yanıt bekleyen oturumun hatırlatması (birkaç saniye). */
  reminder?: Reminder | null;
}

/**
 * Durumu bitti/hata'ya yeni geçen oturumlar. Önceki durumu bilinmeyen oturum (ilk yükleme, yeni
 * oturum) olay üretmez: Zodyvel açılınca eski bitmiş oturumlar için bildirim yağmaz.
 */
/** Durdurulan turun adım yazısı (Rust State::stopped ve uzaktan oturumda kullanıcı kesince). */
export const STOPPED = 'Durduruldu';

export function agentEvents(previous: ReadonlyMap<string, AgentStatus>, agents: AgentSession[], now: number): AgentEvent[] {
  return agents
    .filter(a => {
      const before = previous.get(a.id);
      // Kullanıcının durdurduğu tur (mini AI › Durdur, "Durduruldu") bitti bildirimi ve sesi çıkarmaz.
      if (a.step === STOPPED) return false;
      return before !== undefined && before !== a.status && (a.status === 'done' || a.status === 'error');
    })
    .map(a => ({
      key: `${a.id}:${a.status}:${now}`,
      session: a.id,
      kind: a.status as AgentEvent['kind'],
      tool: a.tool,
      project: a.project,
      turnMs: a.turnStartedAt !== null && a.turnEndedAt !== null && a.turnEndedAt >= a.turnStartedAt ? a.turnEndedAt - a.turnStartedAt : null,
      files: a.turnFiles,
    }));
}

/**
 * Hatırlatılacak oturum: `minutes` dakikadır yanıtını bekleyen ilk oturum (onay isteği olanlar
 * noticeFor'da başlıkla hatırlatılır). Her bekleme bir kez hatırlatılır: `reminded` anahtarları oturum +
 * beklemenin başladığı an. Örnek oturumlar hatırlatılmaz.
 */
export function dueReminder(agents: AgentSession[], now: number, minutes: number, reminded: ReadonlySet<string>): Reminder | null {
  for (const a of agents) {
    if (a.status !== 'waiting_input' || a.demo) continue;
    const waited = now - a.statusSince;
    const key = `${a.id}:${a.statusSince}`;
    if (waited < minutes * MINUTE || reminded.has(key)) continue;
    return { key, tool: a.tool, project: a.project, minutes: Math.floor(waited / MINUTE) };
  }
  return null;
}

/** Pil notu: şarja takıldı ya da çıkarıldı, pil %20 / %10'un altına indi. İlk okuma (önceki yok) not üretmez. */
export function batteryNote(before: Battery | null, after: Battery | null, now: number): Toast | null {
  if (!before || !after) return null;
  const key = `battery:${now}`;
  const left = after.minutesLeft !== null ? ` · yaklaşık ${duration(after.minutesLeft * MINUTE)}` : '';
  if (!before.plugged && after.plugged) return { key, mood: 'done', title: 'Şarj oluyor', detail: `Pil %${after.percent}`, tab: 'pc' };
  if (before.plugged && !after.plugged) return { key, mood: 'idle', title: 'Pilden çalışıyor', detail: `Pil %${after.percent}${left}`, tab: 'pc' };
  if (after.plugged) return null;
  const limits = [
    [10, 'error', 'Pil çok azaldı'],
    [20, 'attention', 'Pil azaldı'],
  ] as const;
  for (const [limit, mood, title] of limits) {
    if (after.percent <= limit && before.percent > limit) return { key, mood, title, detail: `%${after.percent} · şarja tak${left}`, tab: 'pc' };
  }
  return null;
}

/**
 * Güncelleme notu: kayıtlı önceki sürüm varsa ve şimdiki sürümden farklıysa "0.x.y'ye güncellendi" kısa notu.
 * Önceki sürüm boşsa (yeni kurulum ya da bu özellikten önceki ayar dosyası) not yok; yalnız sürüm kaydedilir.
 */
export function updateNote(previous: string, current: string, now: number): Toast | null {
  if (!previous || !current || previous === current) return null;
  return { key: `updated:${current}:${now}`, mood: 'done', title: `${current}'ye güncellendi`, detail: `Önceki sürüm ${previous}`, tab: 'pc' };
}

/** Bitti/hata bildiriminin yazısı; özet açıksa tur süresi ve değişen dosya sayısı da yazılır. */
function flashText(flash: AgentEvent, summary: boolean): { title: string; detail: string } {
  const base = `${toolName(flash.tool)} · ${flash.project}`;
  const title = flash.kind === 'done' ? 'Bitti' : 'Hata verdi';
  if (!summary) return { title, detail: base };
  return {
    title: flash.turnMs !== null ? `${title} · ${duration(flash.turnMs)}` : title,
    detail: flash.files > 0 ? `${base} · ${flash.files} dosya` : base,
  };
}

/** Gösterilecek bildirim; yoksa null (çentik siyah şerit kalır). Öncelik: bekleyen istek > gelen etkinlik
 * > Bitiş kartı (Zodymetre/mod) > kısa olay (bitti/hata) > odak aşaması notu > bekleme hatırlatması > kısa
 * not; istekler arasında adadan karar verilebilen öne çıkar. "Bitti" düşük öncelikli notların arkasında
 * beklemez, onları iter (Faz 7.2). */
export function noticeFor({
  approvals,
  flash = null,
  timerDone = null,
  modeDone = null,
  eventDue = null,
  toast = null,
  doneSummary = false,
  lateAfterMin = null,
  now = Date.now(),
  reminder = null,
}: NoticeInput): Notice | null {
  const first = approvals.find(r => r.actionable) ?? approvals[0];
  if (!first && eventDue) {
    return {
      key: `event:${eventDue.key}`,
      mood: 'attention',
      title: eventDue.title,
      // Ajanda planı (Faz 5b): "5 dk önce" haberinde başlama saati; eski etkinlikte tam saat.
      detail: eventDue.lead ? `${eventDue.time}'da başlıyor · ${eventDue.lead} dk kaldı` : `Plan · ${eventDue.time}`,
      tab: 'agenda',
      more: eventDue.more,
    };
  }
  if (!first && timerDone && !timerDone.focus) {
    const m = timerDone.meter;
    const name = m?.name || 'Zodymetre';
    return {
      key: `timer:${timerDone.key}`,
      mood: 'done',
      title: `${name} bitti · ${timerDone.minutes} dk`,
      detail: m && m.rounds > 1 ? `Bugün ${m.rounds}. tur · toplam ${minutesText(Math.round(m.totalMs / 60_000))}` : 'Süre doldu',
      tab: 'today',
      more: 0,
      finish: { kind: 'meter', minutes: timerDone.minutes, meterId: m?.id ?? null, name, color: m?.color ?? '', rounds: m?.rounds ?? 1, totalMs: m?.totalMs ?? timerDone.minutes * 60_000 },
    };
  }
  if (!first && modeDone) {
    const apps = modeDone.reopen;
    return {
      key: `mode:${modeDone.key}`,
      mood: 'done',
      title: `${modeDone.name} modu bitti · ${minutesText(Math.max(1, modeDone.minutes))}`,
      detail: apps.length ? `${apps.join(', ')} kapatılmıştı, yeniden açılsın mı?` : 'Bitti, iyi iş.',
      tab: 'modes',
      more: 0,
      finish: { kind: 'mode', minutes: modeDone.minutes, modeId: modeDone.id, name: modeDone.name, color: modeDone.color, icon: modeDone.icon, reopen: apps },
    };
  }
  if (!first && flash) {
    // Yazıda dosya sayısı görünüyorsa tıklayınca o oturumun Değişenler sekmesi açılır.
    const section = doneSummary && flash.files > 0 ? 'changed' : null;
    return { key: flash.key, mood: flash.kind, ...flashText(flash, doneSummary), tab: 'ai', more: 0, open: { session: flash.session, section } };
  }
  // Modun içinde odak aşaması bitti: sıradaki kendiliğinden başladı, kısa not yeter (söner).
  if (!first && timerDone?.focus) {
    return { key: `timer:${timerDone.key}`, mood: 'done', ...focusStepText(timerDone.focus), tab: 'modes', more: 0 };
  }
  if (!first && reminder) {
    return {
      key: `remind:${reminder.key}`,
      mood: 'attention',
      title: `${reminder.minutes} dk'dır yanıtını bekliyor`,
      detail: `${toolName(reminder.tool)} · ${reminder.project}`,
      tab: 'ai',
      more: 0,
    };
  }
  if (!first) return toast ? { ...toast, more: 0 } : null;
  // Uzun bekleyen istek: başlık bekleme süresini söyler. Eşik aşılınca anahtar değişir ve geliş animasyonu
  // bir kez daha oynar (hatırlatma); sonraki dakikalarda yalnız sayı değişir.
  const waited = Math.floor((now - first.createdAt) / MINUTE);
  const late = lateAfterMin !== null && waited >= lateAfterMin;
  const title = late
    ? `${waited} dk'dır ${first.kind === 'question' ? 'cevap' : 'onay'} bekliyor`
    : !first.actionable
      ? 'Araçta onay bekliyor'
      : first.kind === 'question'
        ? 'Soru soruyor'
        : first.kind === 'plan'
          ? 'Planını sundu'
          : 'Onay bekleniyor';
  return {
    key: late ? `${first.id}:late` : first.id,
    mood: 'attention',
    title,
    detail: `${toolName(first.tool)} · ${first.project}`,
    tab: 'ai',
    more: approvals.length - 1,
  };
}
