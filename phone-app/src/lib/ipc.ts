import type { AgendaView, AppState, FolderItem, LiveState, PairBegin, Probe, SessionInfo, ApprovalInfo } from './types';

// Tarayıcıda (telefon dışında) görsel deneme için sahte arka uç. Adres çubuğu: ?hw=0 (korumasız), ?off=1 (ağ dışı), ?pair=0 (eşleşmemiş).
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const mock = !inTauri;
const qs = typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams();

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Date.now();
const sec = () => Math.floor(now() / 1000);

let mockState: AppState = qs.get('pair') === '0'
  ? { phase: 'none', device_secure: true, desktop_name: null, fingerprint: null, unlocked: false }
  : { phase: 'paired', device_secure: true, desktop_name: 'Masaüstü-PC', fingerprint: '7F3A · C1E0 · 92B4', unlocked: false };
let mockLiveOn = true;
let mockRemind = true;
let mockBoot = false;
const mockNet = qs.get('off') !== '1';
let mockUntil = 0;
const mockHw = qs.get('hw') === '0' ? false : true;
const t0 = sec();
let mockSessions: SessionInfo[] = [
  { id: 's1', tool: 'claude', name: 'Veritabanı göçü', folder: 'Gezgin', status: 'waiting_approval', started: t0 - 200, step: 'Komut bekliyor: npm test', steps: [{ kind: 'read', label: 'Okudu', target: 'migrate.ts' }, { kind: 'edit', label: 'Düzenledi', target: 'schema.sql' }, { kind: 'bash', label: 'Çalıştıracak', target: 'npm test' }], reply: 'Göç dosyasını yazdım; testleri çalıştırıp bir şeyi bozmadığına bakmak istiyorum.' },
  { id: 's2', tool: 'claude', name: 'Naber', folder: 'Ödev', status: 'running', started: t0 - 42, step: 'Düzenliyor: auth.ts', steps: [{ kind: 'read', label: 'Okudu', target: 'auth.ts' }, { kind: 'search', label: 'Aradı', target: '"refresh"' }, { kind: 'edit', label: 'Düzenliyor', target: 'auth.ts' }], reply: 'Giriş akışındaki token yenileme hatasını buldum; şimdi süresi dolan oturumu yakalayıp yeniden deneyen kısmı düzenliyorum.' },
  { id: 's3', tool: 'codex', name: 'Rapor tablosu', folder: 'Okul', status: 'waiting_input', started: t0 - 190, step: 'Soru soruyor', steps: [{ kind: 'read', label: 'Okudu', target: 'rapor.xlsx' }], reply: null },
  { id: 's4', tool: 'claude', name: 'Sunum notları', folder: 'Okul', status: 'done', started: t0 - 18 * 60, step: 'Bitti · 2 dosya', steps: [{ kind: 'edit', label: 'Düzenledi', target: 'notlar.md' }, { kind: 'edit', label: 'Düzenledi', target: 'sunum.md' }], reply: 'Sunum notlarını iki dosyada toparladım.' },
];
let mockApprovals: ApprovalInfo[] = [
  { id: 'ap1', session: 'Veritabanı göçü', session_id: 's1', kind: 'permission', action: 'Bash: npm test', questions: null },
  { id: 'ap2', session: 'Rapor tablosu', session_id: 's3', kind: 'question', action: null, questions: [
    { q: 'Eski tabloyu ne yapayım?', options: ['Yedekle, sonra sil', 'Olduğu gibi bırak', 'Hemen sil'], multi: false },
    { q: 'Hangi sütunlar kalsın?', options: ['Ad', 'Not', 'Devamsızlık'], multi: true },
  ] },
];

function mockAgenda(): AgendaView {
  const d = (off: number) => { const x = new Date(); x.setDate(x.getDate() + off); return `${x.getFullYear()}-${String(x.getMonth() + 1).padStart(2, '0')}-${String(x.getDate()).padStart(2, '0')}`; };
  const it = (id: number, off: number, time: string | null, title: string) => ({ id, date: d(off), time, title, minutes: 60, lead: 15, notify: !!time });
  return { from: d(0), days: 14, items: [it(1, 0, '10:00', 'Diş hekimi'), it(2, 0, '14:30', 'Proje toplantısı'), it(3, 0, '19:00', 'Spor'), it(4, 1, null, 'Annemin doğum günü'), it(5, 1, '11:00', 'Kahvaltı · Ece'), it(6, 2, '09:00', 'Okul · Veri yapıları'), it(7, 4, '16:00', 'Rapor teslimi')] };
}

/** Sahte eylem: imza penceresini taklit eder, korumasız telefonda kısıtlıları reddeder. */
async function mockAct(kind: 'decide' | 'answer' | 'note' | 'task' | 'new_task' | 'stop', args: Record<string, unknown> = {}): Promise<void> {
  await wait(700);
  if (!mockLiveOn || !mockNet) throw 'Bilgisayara bağlı değilsin.';
  if (!mockHw && (kind === 'note' || kind === 'task' || kind === 'new_task')) throw 'Bu telefon donanım korumalı değil; bu eylem kapalı.';
  const done = (sid: string, status: string) => { mockSessions = mockSessions.map((s) => (s.id === sid ? { ...s, status, started: sec() } : s)); };
  if (kind === 'decide' || kind === 'answer') {
    const ap = mockApprovals.find((a) => a.id === args.id);
    if (!ap) throw 'Bu istek artık yok.';
    mockApprovals = mockApprovals.filter((a) => a !== ap);
    done(ap.session_id, 'running');
  } else if (kind === 'stop') done(String(args.session), 'done');
  else if (kind === 'task') done(String(args.session), 'running');
  else if (kind === 'new_task') mockSessions = [{ id: 's' + sec(), tool: 'claude', name: String(args.prompt).slice(0, 30), folder: 'Zodyvel', status: 'running', started: sec(), step: null, steps: [], reply: null }, ...mockSessions];
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!mock) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  await wait(200);
  const open = mockUntil > now();
  switch (cmd) {
    case 'app_state':
      return structuredClone({ ...mockState, unlocked: open }) as T;
    case 'pair_begin':
      return { code: '482916', fingerprint: '7F3A · C1E0 · 92B4' } as T;
    case 'pair_finish':
      await wait(1200);
      mockState = { ...mockState, phase: 'paired', desktop_name: 'Masaüstü-PC', fingerprint: '7F3A · C1E0 · 92B4' };
      return undefined as T;
    case 'probe':
      return { sessions: mockSessions.length, approvals: mockApprovals.length } as T;
    case 'unlock':
      await wait(500);
      mockUntil = now() + 120_000;
      return undefined as T;
    case 'lock':
      mockUntil = 0;
      return undefined as T;
    case 'unpair':
      mockState = { phase: 'none', device_secure: true, desktop_name: null, fingerprint: null, unlocked: false };
      return undefined as T;
    case 'live_state':
      return {
        enabled: mockLiveOn,
        running: mockLiveOn,
        link: !mockLiveOn ? 'off' : mockNet ? 'connected' : 'retry',
        detail: mockNet ? '' : 'Bilgisayar şu an bağlantıya kapalı.',
        approvals: mockLiveOn && mockNet ? mockApprovals.map((a) => (open ? a : { ...a, action: null, questions: null })) : [],
        sessions: mockLiveOn && mockNet ? mockSessions.map((s) => (open ? s : { ...s, step: null, steps: null, reply: null })) : [],
        unlocked_until: open ? mockUntil : null,
        last_connected: now() - (mockNet ? 0 : 42 * 60_000),
        hw: mockHw,
        hw_level: mockHw ? 'tee' : '',
        reminders: mockRemind,
        reminder_count: mockRemind ? mockAgenda().items.filter((i) => i.notify && i.time).length : null,
        autostart: mockBoot,
      } satisfies LiveState as T;
    case 'live_set_enabled':
      mockLiveOn = !!args?.on;
      return undefined as T;
    case 'live_decide':
      return mockAct('decide', args) as T;
    case 'live_answer':
      return mockAct('answer', args) as T;
    case 'live_note':
      return mockAct('note', args) as T;
    case 'live_task':
      return mockAct('task', args) as T;
    case 'live_new_task':
      return mockAct('new_task', args) as T;
    case 'live_stop':
      return mockAct('stop', args) as T;
    case 'live_folders':
      if (!mockLiveOn || !mockNet) throw 'Bilgisayara bağlı değilsin.';
      return (mockHw ? [{ id: 'f1', name: 'Zodyvel' }, { id: 'f2', name: 'Gezgin' }, { id: 'f3', name: 'Okul' }, { id: 'f4', name: 'Ödev' }] : []) as T;
    case 'live_agenda':
      return mockAgenda() as T;
    case 'reminders_set':
      mockRemind = !!args?.on;
      return undefined as T;
    case 'autostart_set':
      mockBoot = !!args?.on;
      return undefined as T;
    case 'battery_exempt':
      return true as T;
    default:
      return undefined as T;
  }
}

// Eylemler (decide/answer/note/task/new_task/stop) çekirdekte parmak izi + Keystore imzası ister ve masaüstünün
// ack'ını bekler; ok:false ise masaüstünün hata metniyle reddedilir (md/telefon-t4c.md §4.7–9).
export const ipc = {
  state: () => call<AppState>('app_state'),
  pairBegin: (qr: string) => call<PairBegin>('pair_begin', { qr }),
  pairFinish: (same: boolean) => call<void>('pair_finish', { same }),
  pairCancel: () => call<void>('pair_cancel'),
  probe: () => call<Probe>('probe'),
  unlock: () => call<void>('unlock'),
  lock: () => call<void>('lock'),
  unpair: () => call<void>('unpair'),
  liveState: () => call<LiveState>('live_state'),
  liveResume: () => call<void>('live_resume'),
  liveSetEnabled: (on: boolean) => call<void>('live_set_enabled', { on }),
  remindersSet: (on: boolean) => call<void>('reminders_set', { on }),
  autostartSet: (on: boolean) => call<void>('autostart_set', { on }),
  liveDecide: (id: string, allow: boolean) => call<void>('live_decide', { id, allow }),
  liveAnswer: (id: string, answers: Record<string, string>) => call<void>('live_answer', { id, answers }),
  liveNote: (session: string, text: string, now: boolean) => call<void>('live_note', { session, text, now }),
  liveTask: (session: string, text: string) => call<void>('live_task', { session, text }),
  liveNewTask: (folder: string, prompt: string) => call<void>('live_new_task', { folder, prompt }),
  liveStop: (session: string) => call<void>('live_stop', { session }),
  liveFolders: () => call<FolderItem[]>('live_folders'),
  liveAgenda: () => call<AgendaView>('live_agenda'),
  batteryExempt: () => call<boolean>('battery_exempt'),
};

/** A-O1: Rust canlı durum değişince `live-changed` olayını yollar (yükü yok). Dönen işlev dinlemeyi bırakır.
 *  Tarayıcı denemesinde olay yok; ekran kendi eylemlerinden sonra zaten yeniden çeker. */
export async function onLiveChanged(cb: () => void): Promise<() => void> {
  if (mock) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen('live-changed', () => cb());
}

/** QR okur; telefon dışında sahte bir yük döner. İptal edilirse null. */
export async function scanQr(): Promise<string | null> {
  if (mock) {
    await wait(1500);
    return '{"v":1,"addr":"192.168.1.20:4000","token":"' + '00'.repeat(32) + '","fp":"' + '11'.repeat(32) + '"}';
  }
  const s = await import('@tauri-apps/plugin-barcode-scanner');
  let perm = await s.checkPermissions();
  if (perm !== 'granted') perm = await s.requestPermissions();
  if (perm !== 'granted') throw 'Kamera izni gerekli: QR okumak için izin ver.';
  document.documentElement.classList.add('scanning');
  try {
    const r = await s.scan({ windowed: true, formats: [s.Format.QRCode] });
    return r.content;
  } catch {
    return null;
  } finally {
    document.documentElement.classList.remove('scanning');
  }
}

export async function cancelScan(): Promise<void> {
  if (mock) return;
  try {
    const s = await import('@tauri-apps/plugin-barcode-scanner');
    await s.cancel();
  } catch {
    /* tarama zaten bitti */
  }
  document.documentElement.classList.remove('scanning');
}
