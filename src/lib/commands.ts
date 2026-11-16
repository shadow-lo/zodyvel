// Zody'nin kural tabanlı Türkçe komut ayrıştırıcısı (plan 3.2). Saf: metni bir niyete çevirir, hiçbir şey
// çalıştırmaz. Niyeti eylem kaydına (actions.ts) götüren ve cevabı yazan chat.svelte.ts'dir.
// Model yok: anlaşılmayan cümle 'unknown' döner; ileride sohbet katmanı onu modele verebilir.

import { isoDate } from './today';

export type MusicOp = 'pause' | 'play' | 'next' | 'prev';
export type TimerOp = 'pause' | 'start' | 'reset';
export type AskTopic = 'agents' | 'approvals' | 'today' | 'tomorrow' | 'music' | 'battery' | 'time' | 'date' | 'timer' | 'todos';
export type TalkTopic = 'hello' | 'thanks' | 'help' | 'who' | 'approve' | 'mood';
export type SettingsPage = 'wifi' | 'bluetooth' | 'sound' | 'nightlight' | 'notifications';

export type Intent =
  | { kind: 'music'; op: MusicOp }
  | { kind: 'volume'; op: 'up' | 'down' | 'mute' | 'unmute' | 'set'; level?: number }
  | { kind: 'timer'; minutes: number }
  | { kind: 'timerOp'; op: TimerOp }
  /** "durdur", "devam et": müzik mi zamanlayıcı mı, duruma bakıp sohbet katmanı seçer. */
  | { kind: 'pause' }
  | { kind: 'resume' }
  | { kind: 'focus'; on: boolean }
  | { kind: 'todo'; text: string }
  | { kind: 'event'; title: string; date: string; time: string | null }
  | { kind: 'dark'; on: boolean }
  | { kind: 'radio'; radio: 'wifi' | 'bluetooth'; on: boolean }
  | { kind: 'open'; target: 'taskManager' | 'settings' | 'dataFolder' }
  | { kind: 'page'; page: SettingsPage }
  | { kind: 'ask'; topic: AskTopic }
  | { kind: 'talk'; topic: TalkTopic }
  | { kind: 'unknown' };

// JavaScript'in \b ve \w'si yalnız ASCII bilir ("şarkı"daki ş sözcük harfi sayılmaz). Kurallar okunaklı
// kalsın diye \b/\w ile yazılır, burada Unicode harf sınıflarına çevrilir (bir kez, önbellekli).
const L = '[\\p{L}\\p{N}]';
const B = `(?:(?<=${L})(?!${L})|(?<!${L})(?=${L}))`;
const cache = new Map<string, RegExp>();
function U(re: RegExp): RegExp {
  const key = `${re.source}/${re.flags}`;
  let out = cache.get(key);
  if (!out) {
    out = new RegExp(re.source.replace(/\\b/g, B).replace(/\\w/g, L), re.flags.includes('u') ? re.flags : `${re.flags}u`);
    cache.set(key, out);
  }
  return out;
}
const has = (t: string, re: RegExp) => U(re).test(t);
const find = (t: string, re: RegExp) => t.match(U(re));

const UNITS: Record<string, number> = { sıfır: 0, bir: 1, iki: 2, üç: 3, dört: 4, beş: 5, altı: 6, yedi: 7, sekiz: 8, dokuz: 9 };
const TENS: Record<string, number> = { on: 10, yirmi: 20, otuz: 30, kırk: 40, elli: 50, altmış: 60, yetmiş: 70, seksen: 80, doksan: 90 };

/** Küçük harf (Türkçe kuralıyla), noktalama boşluğa; saatlerdeki ':' ve '.' ile kesme işareti korunur. */
export function normalize(text: string): string {
  return text
    .toLocaleLowerCase('tr')
    .replace(/[’`´]/g, "'")
    .replace(/[!?;,"“”()[\]{}…]/g, ' ')
    .replace(/(\d)\.(\d)/g, '$1:$2')
    .replace(/\.(?!\d)/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/** "yirmi beş" → "25", "yarım saat" → "30 dakika", "bir buçuk saat" → "90 dakika". Diğer sözcükler aynen. */
export function numberWords(text: string): string {
  let t = text
    .replace(U(/\byarım saat\w*/g), '30 dakika')
    .replace(U(/\bçeyrek saat\w*/g), '15 dakika')
    .replace(U(/\b(\d+|bir|iki|üç) buçuk saat\w*/g), (_, n: string) => `${(Number(n) || UNITS[n] || 0) * 60 + 30} dakika`);
  const words = t.split(' ');
  const out: string[] = [];
  for (let i = 0; i < words.length; i++) {
    const w = words[i]!;
    const tens = TENS[w];
    const unit = UNITS[w];
    if (tens !== undefined) {
      const next = UNITS[words[i + 1] ?? ''];
      if (next !== undefined && next > 0) {
        out.push(String(tens + next));
        i++;
      } else out.push(String(tens));
    } else if (unit !== undefined && w !== 'bir') {
      out.push(String(unit));
    } else if (w === 'bir' && has(words[i + 1] ?? '', /^(dakika|dk|saat|sa)\b/)) {
      // "bir dakika", "bir saat" süre; "bir şarkı" değil.
      out.push('1');
    } else out.push(w);
  }
  t = out.join(' ');
  return t;
}

/** Süre (dakika): "25 dakika", "25 dk", "1 saat", "90 dakikalık". */
export function minutesIn(text: string): number | null {
  const hour = find(text, /(\d+)\s*(?:saat|sa)\b/);
  const min = find(text, /(\d+)\s*(?:dakika|dk|dak)\w*/);
  if (!hour && !min) return null;
  const total = (hour ? Number(hour[1]) * 60 : 0) + (min ? Number(min[1]) : 0);
  return total > 0 ? total : null;
}


const OFF = /\b(kapat|kapa|durdur|devre dışı|kes)\w*/;
const ON = /\b(aç|açar mısın|etkinleştir|başlat|aktif)\w*/;

const WEEKDAYS = ['pazar', 'pazartesi', 'salı', 'çarşamba', 'perşembe', 'cuma', 'cumartesi'];
const MONTHS = ['ocak', 'şubat', 'mart', 'nisan', 'mayıs', 'haziran', 'temmuz', 'ağustos', 'eylül', 'ekim', 'kasım', 'aralık'];

interface Found<T> {
  value: T;
  /** Metinden çıkarılacak parçalar (başlık için). */
  spans: string[];
}

/** Tarih: bugün, yarın, öbür gün, hafta günü (bugün o günse gelecek hafta), "25 ekim", "25/10". */
export function findDate(text: string, now: Date): Found<string> | null {
  const day = (offset: number) => isoDate(new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset));
  const rel: [RegExp, number][] = [
    [/\b(öbür gün|öbürgün|yarından sonra)\w*/, 2],
    [/\byarın\w*/, 1],
    [/\bbugün\w*/, 0],
  ];
  for (const [re, offset] of rel) {
    const m = find(text, re);
    if (m) return { value: day(offset), spans: [m[0]] };
  }
  const named = find(text, new RegExp(`\\b(\\d{1,2}) (${MONTHS.join('|')})\\w*`));
  if (named) {
    const d = Number(named[1]);
    const mon = MONTHS.indexOf(named[2]!);
    let year = now.getFullYear();
    const candidate = new Date(year, mon, d);
    if (candidate < new Date(now.getFullYear(), now.getMonth(), now.getDate())) year++;
    if (d >= 1 && d <= 31) return { value: isoDate(new Date(year, mon, d)), spans: [named[0]] };
  }
  const slash = find(text, /\b(\d{1,2})\/(\d{1,2})\b/);
  if (slash) {
    const d = Number(slash[1]);
    const mon = Number(slash[2]) - 1;
    if (d >= 1 && d <= 31 && mon >= 0 && mon < 12) {
      let year = now.getFullYear();
      if (new Date(year, mon, d) < new Date(now.getFullYear(), now.getMonth(), now.getDate())) year++;
      return { value: isoDate(new Date(year, mon, d)), spans: [slash[0]] };
    }
  }
  // Uzun adlar önce: "pazartesi" "pazar"dan, "cumartesi" "cuma"dan önce denenir.
  const order = [...WEEKDAYS.keys()].sort((a, b) => WEEKDAYS[b]!.length - WEEKDAYS[a]!.length);
  for (const i of order) {
    const m = find(text, new RegExp(`\\b(?:bu |gelecek |haftaya )?${WEEKDAYS[i]}\\w*`));
    if (m) {
      const diff = (i - now.getDay() + 7) % 7 || 7;
      return { value: day(diff), spans: [m[0]] };
    }
  }
  return null;
}

const pad = (n: number) => String(n).padStart(2, '0');

/**
 * Saat: "10:30", "saat 10", "10'da", "akşam 7", "öğlen". Gün bölümü söylenmeden 1–7 arası saat öğleden
 * sonra sayılır ("3'te toplantı" → 15:00); "sabah" söylenirse sabah.
 */
export function findTime(text: string): Found<string> | null {
  if (has(text, /\böğle(n|de|yin)?\b/) && !has(text.replace(U(/\böğle\w*/), ''), /\d/)) {
    const m = find(text, /\böğle\w*/)!;
    return { value: '12:00', spans: [m[0]] };
  }
  const re = /\b(?:(sabah|öğleden sonra|öğle|akşam|gece)\s+)?(?:saat\s+)?(\d{1,2})(?::(\d{2}))?(?:'?(?:da|de|ta|te))?\b/g;
  for (const m of text.matchAll(U(re))) {
    const part = m[1];
    const hasSaat = has(m[0], /saat/);
    const hasMin = m[3] !== undefined;
    const hasSuffix = has(m[0], /'|\b(da|de|ta|te)$/);
    if (!part && !hasSaat && !hasMin && !hasSuffix) continue;
    // "25 dakika" gibi süreler saat değildir.
    const after = text.slice((m.index ?? 0) + m[0].length);
    if (has(after, /^\s*(dakika|dk|saat|sa)\b/) && !hasSaat) continue;
    let h = Number(m[2]);
    const min = hasMin ? Number(m[3]) : 0;
    if (h > 23 || min > 59) continue;
    if (part === 'akşam' || part === 'öğleden sonra') h = h < 12 ? h + 12 : h;
    else if (part === 'gece') h = h >= 6 && h < 12 ? h + 12 : h === 12 ? 0 : h;
    else if (!part && h >= 1 && h <= 7) h += 12;
    return { value: `${pad(h)}:${pad(min)}`, spans: [m[0]] };
  }
  return null;
}

const cap = (s: string) => (s ? s[0]!.toLocaleUpperCase('tr') + s.slice(1) : s);

function strip(text: string, spans: string[], words: RegExp): string {
  let t = ` ${text} `;
  for (const s of spans) t = t.replace(s, ' ');
  return t.replace(U(words), ' ').replace(/\s+/g, ' ').trim();
}

const EVENT_FILLER =
  /\b(takvim\w*|ekle\w*|koy\w*|kaydet\w*|yaz\w*|oluştur\w*|ayarla\w*|için|saat|at|bir etkinlik|etkinlik\w*|lütfen|zody|bana|ve)\b/g;
const TODO_FILLER = /\b(yapılacak\w*|liste\w*|görev listesi\w*|ekle\w*|yaz\w*|diye|lütfen|zody|bana)\b/g;

function parseEvent(t: string, now: Date): Intent | null {
  const calendar = has(t, /\btakvim\w*/);
  const verb = has(t, /\b(ekle|koy|kaydet|yaz|oluştur|ayarla)\w*\b/);
  const date = findDate(t, now);
  const time = findTime(t);
  if (!calendar && !(verb && (date || time))) return null;
  if (!date && !time) return null;
  const title = strip(t, [...(date?.spans ?? []), ...(time?.spans ?? [])], EVENT_FILLER);
  return { kind: 'event', title: cap(title) || 'Etkinlik', date: date?.value ?? isoDate(now), time: time?.value ?? null };
}

function parseTodo(t: string): Intent | null {
  if (!has(t, /\b(yapılacak\w*|listeye|listeme|görev listesi\w*)\b/) || !has(t, /\b(ekle|yaz)\w*/)) return null;
  const text = strip(t, [], TODO_FILLER);
  return text ? { kind: 'todo', text: cap(text) } : null;
}

/** Metni niyete çevirir. `now` tarih hesapları için (testlerde sabit). */
export function parse(raw: string, now = new Date()): Intent {
  const t = numberWords(normalize(raw));
  if (!t) return { kind: 'unknown' };

  // Onay sözle verilmez: "evet", "onayla" karttaki düğmeye yönlendirilir.
  if (has(t, /^(evet|onayla\w*|onaylıyorum|tamam onayla|izin ver\w*|kabul( et)?)$/)) return { kind: 'talk', topic: 'approve' };

  // Sohbet
  if (has(t, /^(merhaba|selam\w*|hey|hey zody|zody|günaydın|iyi akşamlar|naber|nbr|sa|slm)( zody)?$/)) return { kind: 'talk', topic: 'hello' };
  if (has(t, /\b(nasılsın|ne haber|keyfin nasıl)\b/)) return { kind: 'talk', topic: 'mood' };
  if (has(t, /\b(teşekkür\w*|sağ ?ol\w*|eyvallah|süpersin|harika|eline sağlık|tşk)\b/)) return { kind: 'talk', topic: 'thanks' };
  if (has(t, /\b(yardım|ne(ler)? yapabil\w*|komutlar\w*|ne biliyorsun)\b/)) return { kind: 'talk', topic: 'help' };
  if (has(t, /\b(sen kimsin|adın ne|kimsin)\b/)) return { kind: 'talk', topic: 'who' };

  // Ayar sayfaları ("bluetooth ayarlarını aç") radyodan önce.
  if (has(t, /\b(ayarlar\w*|ayarı\w*|ayarın\w*|ayar sayfa\w*)/)) {
    if (has(t, /\b(wi-?fi|kablosuz|ağ)\w*/)) return { kind: 'page', page: 'wifi' };
    if (has(t, /\bbluetooth\w*/)) return { kind: 'page', page: 'bluetooth' };
    if (has(t, /\b(ses|hoparlör)\w*/)) return { kind: 'page', page: 'sound' };
    if (has(t, /\bgece ışığ\w*/)) return { kind: 'page', page: 'nightlight' };
    if (has(t, /\bbildirim\w*/)) return { kind: 'page', page: 'notifications' };
    if (has(t, ON) || has(t, /\b(zodyvel|uygulama)\w*/) || has(t, /^ayarlar\w*$/)) return { kind: 'open', target: 'settings' };
  }
  if (has(t, /\bgörev yönetici\w*/)) return { kind: 'open', target: 'taskManager' };
  if (has(t, /\bveri klasör\w*/)) return { kind: 'open', target: 'dataFolder' };

  // Sorular (eylemden önce: "zamanlayıcıda ne kadar kaldı" kurmak değildir).
  if (has(t, /\bsaat kaç\b/)) return { kind: 'ask', topic: 'time' };
  if (has(t, /\b(günlerden ne|bugün ayın kaçı|tarih ne|hangi gündeyiz|bugün ne günü)\b/)) return { kind: 'ask', topic: 'date' };
  if (has(t, /\b(ne kadar|kaç dakika|kaç dk) kaldı\b/)) return { kind: 'ask', topic: 'timer' };
  if (has(t, /\b(pil|şarj|batarya)\w*/) && !has(t, ON) && !has(t, OFF)) return { kind: 'ask', topic: 'battery' };
  if (has(t, /\b(ne çalıyor|hangi şarkı|bu (ne|hangi) şarkı|çalan ne|şarkı ne)\b/)) return { kind: 'ask', topic: 'music' };
  if (has(t, /\b(onay|izin)\w* (bekle\w*|var mı)|\bkaç onay\b/)) return { kind: 'ask', topic: 'approvals' };
  if (has(t, /\b(claude|codex|ajan\w*|oturum\w*)\b/) && has(t, /\b(ne yapıyor|ne durumda|nasıl gidiyor|bitti mi|çalışıyor mu|kim|kaç|durum\w*|neler)\b/))
    return { kind: 'ask', topic: 'agents' };
  if (has(t, /\byapılacak\w*/) && has(t, /\b(ne|neler|kaç|göster|listele|var mı)\b/) && !has(t, /\bekle\w*/)) return { kind: 'ask', topic: 'todos' };
  if (has(t, /\b(yarın|yarınki)\b/) && has(t, /\b(ne var|neler var|program\w*|takvim\w*)\b/) && !has(t, /\bekle\w*/)) return { kind: 'ask', topic: 'tomorrow' };
  if (has(t, /\b(bugün ne var|bugün neler var|program\w*|takvim\w* (ne|neler)|ajanda\w*|sıradaki (ne|etkinlik))\b/) && !has(t, /\bekle\w*/))
    return { kind: 'ask', topic: 'today' };

  // Odak (Pomodoro), zamanlayıcıdan önce.
  if (has(t, /\b(odak\w*|pomodoro\w*)\b/)) return { kind: 'focus', on: !has(t, /\b(bitir|durdur|kapat|kes|son ver|iptal)\w*/) };

  // Zamanlayıcı
  const timerWord = has(t, /\b(zamanlayıcı\w*|sayaç\w*|sayacı|timer|geri sayım\w*|alarm\w*)\b/);
  const minutes = minutesIn(t);
  if (timerWord || (minutes && has(t, /\b(kur|başlat|ayarla|say)\w*/) && !has(t, /\btakvim\w*/))) {
    if (has(t, /\b(sıfırla|iptal|kapat|sil)\w*/)) return { kind: 'timerOp', op: 'reset' };
    if (has(t, /\b(durdur|duraklat|beklet)\w*/)) return { kind: 'timerOp', op: 'pause' };
    if (minutes) return { kind: 'timer', minutes };
    if (has(t, /\b(başlat|devam)\w*/)) return { kind: 'timerOp', op: 'start' };
    if (timerWord) return { kind: 'ask', topic: 'timer' };
  }

  // Takvim ve yapılacaklar
  const todo = parseTodo(t);
  if (todo) return todo;
  const event = parseEvent(t, now);
  if (event) return event;

  // Tema
  if (has(t, /\b(koyu|karanlık|gece) (tema|mod)\w*/)) return { kind: 'dark', on: !has(t, OFF) };
  if (has(t, /\b(açık|aydınlık) (tema|mod)\w*/)) return { kind: 'dark', on: false };

  // Wi-Fi / Bluetooth (onaylı)
  const radio = has(t, /\bbluetooth\w*/) ? 'bluetooth' : has(t, /\b(wi-?fi|wifi|kablosuz)\w*/) ? 'wifi' : null;
  if (radio && (has(t, OFF) || has(t, ON))) return { kind: 'radio', radio, on: !has(t, OFF) };

  // Müzik
  const musicWord = has(t, /\b(müzi\w*|şarkı\w*|parça\w*|spotify)\b/);
  if (has(t, /\b(sonraki|sıradaki|atla|geç|ileri)\w*/) && (musicWord || t.split(' ').length <= 3)) return { kind: 'music', op: 'next' };
  if (has(t, /\b(önceki|geri al|başa (al|sar)|bir önceki)\w*/) && (musicWord || t.split(' ').length <= 3)) return { kind: 'music', op: 'prev' };
  if (musicWord) {
    if (has(t, /\b(durdur|duraklat|kapat|kes|sustur|beklet)\w*/)) return { kind: 'music', op: 'pause' };
    if (has(t, /\b(oynat|çal|devam|başlat|aç)\w*/)) return { kind: 'music', op: 'play' };
  }

  // Ses düzeyi ("müzik" geçmiyorsa)
  if (has(t, /\b(ses\w*|volume)\b/) || has(t, /\bsessiz\w*/)) {
    const pct = find(t, /(?:%|yüzde\s*)(\d{1,3})|(\d{1,3})\s*'?(?:yap|olsun|ayarla|e|a|ye|ya)\b/);
    const level = pct ? Number(pct[1] ?? pct[2]) : null;
    if (level !== null && level <= 100) return { kind: 'volume', op: 'set', level };
    if (has(t, /\bsessiz\w*/) || has(t, /\b(kapat|sustur|kes)\w*/)) return { kind: 'volume', op: has(t, /\b(sessizden çık|sesi geri)\b/) ? 'unmute' : 'mute' };
    if (has(t, /\b(kıs|azalt|düşür|alçalt)\w*/)) return { kind: 'volume', op: 'down' };
    if (has(t, /\b(yükselt|artır|arttır|aç|fazla)\w*/)) return { kind: 'volume', op: 'up' };
  }

  // Tek sözcüklük durdur/devam: müzik mi zamanlayıcı mı sohbet katmanı karar verir.
  if (has(t, /^(durdur|duraklat|dur|bekle|sus)$/)) return { kind: 'pause' };
  if (has(t, /^(devam( et)?|oynat|başlat|çal)$/)) return { kind: 'resume' };

  return { kind: 'unknown' };
}

/** Boş sohbette ve "anlamadım"da gösterilen örnekler (dokununca yazılır). */
export const EXAMPLES = ['25 dakika zamanlayıcı', 'Müziği durdur', 'Claude ne yapıyor?', "Yarın 10'da toplantı ekle", 'Sesi kıs', 'Bugün ne var?'];
