// Gerçek modüller bağlanana kadar adanın hissini denemek için açıkça etiketli örnek veri.
// Hiçbir gerçek AI aracını, müzik uygulamasını veya sistemi etkilemez.

import type { FileChange, Overview } from './localgit';
import type { SourceItem, SourceState } from './sources';
import { defaultModes, isoDate, MAX_METERS, pauseMode, resumeMode, startMode, timerAdd } from './today';
import type {
  AgentSession,
  AgentStep,
  AgentTodo,
  ApprovalRequest,
  Battery,
  ClosedSession,
  ControlsState,
  MeterState,
  NowPlaying,
  RecentProject,
  ShelfData,
  StepContent,
  SystemLoad,
  TodayData,
  TodayOp,
} from './types';

const minutesAgo = (m: number) => Date.now() - m * 60_000;

let stepId = 0;
function steps(list: [AgentStep['kind'], string, AgentStep['state'], string?, boolean?][]): AgentStep[] {
  return list.map(([kind, target, state, label = '', sub = false]) => ({ id: ++stepId, kind, target, label, state, sub, at: minutesAgo(1) }));
}

const todo = (text: string, status: AgentTodo['status'], active = '') => ({ text, status, active });

/** Örnek dizüstü pili (tarayıcı önizlemesi). */
export function demoBattery(): Battery {
  return { percent: 64, plugged: false, charging: false, saver: false, minutesLeft: 135 };
}

export function demoAgents(): AgentSession[] {
  return [
    {
      id: 'demo-c1',
      tool: 'claude',
      project: 'zodyvel',
      status: 'running',
      step: 'Düzenliyor · mood.ts',
      startedAt: minutesAgo(4),
      lastSeen: minutesAgo(0),
      demo: true,
      app: 'WindowsTerminal.exe',
      statusSince: minutesAgo(4),
      turnStartedAt: minutesAgo(4),
      turnEndedAt: null,
      turnFiles: 2,
      steps: steps([
        ['prompt', '', 'done', 'Yeni istek'],
        ['read', 'Island.svelte', 'done'],
        ['agent', 'Testleri bul', 'done'],
        ['search', 'moodFor', 'done', '', true],
        ['read', 'mood.test.ts', 'done', '', true],
        ['edit', 'format.ts', 'done'],
        ['bash', 'pnpm test', 'done'],
        ['edit', 'mood.ts', 'now'],
      ]),
      changed: [
        { path: 'C:\\work\\zodyvel\\src\\lib\\format.ts', name: 'format.ts', added: 12, removed: 3, step: stepId - 2 },
        { path: 'C:\\work\\zodyvel\\src\\lib\\mood.ts', name: 'mood.ts', added: 4, removed: 1, step: stepId },
      ],
      todos: [
        todo('Ruh hâli kurallarını oku', 'completed'),
        todo('Saat biçimini düzelt', 'completed'),
        todo('Hata durumunu öne al', 'in_progress', 'Hata durumunu öne alıyor'),
        todo('Testleri güncelle', 'pending'),
        todo('Belgeye not düş', 'pending'),
      ],
      lastReply: null,
      mode: 'acceptEdits',
      folder: 'C:\\work\\zodyvel',
      subagents: 0,
      notes: [{ id: 1, text: 'Bitince README’ye de bir satır ekle', now: false, at: minutesAgo(1) }],
      canNote: true,
      canTask: false,
    },
    {
      id: 'demo-x1',
      tool: 'codex',
      project: 'api-sunucu',
      status: 'waiting_input',
      step: 'Yanıtını bekliyor',
      startedAt: minutesAgo(12),
      lastSeen: minutesAgo(0),
      demo: true,
      app: 'Code.exe',
      statusSince: minutesAgo(12),
      turnStartedAt: minutesAgo(14),
      turnEndedAt: null,
      turnFiles: 0,
      steps: steps([
        ['read', 'routes.ts', 'done'],
        ['bash', 'pnpm test', 'failed'],
        ['bash', 'pnpm test', 'failed'],
        ['bash', 'pnpm test', 'failed'],
      ]),
      changed: [],
      todos: [todo('Rotaları oku', 'completed'), todo('Testleri düzelt', 'in_progress')],
      lastReply: null,
      mode: null,
      folder: 'C:\\work\\api-sunucu',
      subagents: 0,
      notes: [],
      canNote: false,
      canTask: false,
    },
    {
      id: 'demo-c2',
      tool: 'claude',
      project: 'blog',
      status: 'done',
      step: 'Tur tamamlandı · 3 dosya',
      startedAt: minutesAgo(31),
      lastSeen: minutesAgo(0),
      demo: true,
      app: null,
      statusSince: minutesAgo(2),
      turnStartedAt: minutesAgo(9),
      turnEndedAt: minutesAgo(2),
      turnFiles: 3,
      steps: steps([
        ['write', 'README.md', 'done'],
        ['edit', 'index.md', 'done'],
        ['edit', 'style.css', 'done'],
        ['done', '3 dosya güncellendi', 'done', 'Tur tamamlandı'],
      ]),
      changed: [
        { path: 'C:\\work\\blog\\README.md', name: 'README.md', added: 18, removed: 0, step: stepId - 3 },
        { path: 'C:\\work\\blog\\posts\\index.md', name: 'index.md', added: 6, removed: 2, step: stepId - 2 },
        { path: 'C:\\work\\blog\\style.css', name: 'style.css', added: 3, removed: 3, step: stepId - 1 },
      ],
      todos: [],
      lastReply: [
        '**3 dosya güncellendi.**',
        '',
        '- `README.md`: kurulum adımları eklendi',
        '- `index.md`: yazı listesi tarihe göre sıralı',
        '- `style.css`: koyu temada bağlantı rengi düzeltildi',
        '',
        'Testler geçti. Yayınlamadan önce `pnpm build` ile bir kez bakmanı öneririm.',
      ].join('\n'),
      mode: 'default',
      folder: 'C:\\work\\blog',
      subagents: 0,
      notes: [],
      canNote: true,
      canTask: true,
      taskNo: 1,
      attach: true,
    },
  ];
}

/** Örnek adım içeriği: türüne göre okunan satırlar, düzenleme farkı ya da komut çıktısı. */
/** Örnek düzenleme isteğinin tam farkı. */
export function demoRequestDiff(): StepContent {
  const line = (n: number | null, text: string, mark: 'add' | 'del' | null = null) => ({ n, text, mark });
  return {
    kind: 'diff',
    path: 'C:\\work\\zodyvel\\src\\lib\\mood.ts',
    note: null,
    lines: [
      line(14, 'export function moodFor(i: MoodInput): ZodyMood {'),
      line(15, "  if (i.approvals > 0) return 'attention';"),
      line(16, "  if (i.asleep) return 'sleep';", 'del'),
      line(16, "  if (i.errors) return 'error';", 'add'),
      line(17, "  if (i.asleep) return 'sleep';", 'add'),
      line(18, "  if (i.working > 0) return 'working';"),
      line(19, '}'),
    ],
  };
}

export function demoStepContent(step: AgentStep): StepContent {
  const line = (n: number | null, text: string, mark: 'add' | 'del' | null = null) => ({ n, text, mark });
  switch (step.kind) {
    case 'read':
      return {
        kind: 'file',
        path: `src/island/${step.target}`,
        note: null,
        lines: [
          line(1, "import { onMount } from 'svelte';"),
          line(2, "import { context, data } from '../lib/store.svelte';"),
          line(3, ''),
          line(4, 'let island = $state(initial);'),
          line(5, 'const ctx = $derived(context());'),
          line(6, 'const size = $derived(sizeFor(island.level, tab));'),
        ],
      };
    case 'edit':
    case 'write':
      return {
        kind: 'diff',
        path: `src/lib/${step.target}`,
        note: null,
        lines: [
          line(14, 'export function moodFor(i: MoodInput): ZodyMood {'),
          line(15, "  if (i.approvals > 0) return 'attention';"),
          line(16, "  if (i.asleep) return 'sleep';", 'del'),
          line(16, "  if (i.errors) return 'error';", 'add'),
          line(17, "  if (i.working > 0) return 'working';"),
          line(18, '}'),
        ],
      };
    case 'bash':
      return {
        kind: 'output',
        path: null,
        note: null,
        lines: [line(null, `$ ${step.target}`), line(null, ' ✓ tests/mood.test.ts (4)'), line(null, '      Tests  20 passed (20)')],
      };
    default:
      return { kind: 'none', path: null, lines: [], note: null };
  }
}

export function demoMusic(): NowPlaying {
  return {
    title: 'Gece Yolculuğu',
    artist: 'Örnek Sanatçı',
    app: 'Spotify',
    playing: true,
    positionMs: 74_000,
    durationMs: 213_000,
    updatedAt: Date.now(),
    artId: null,
    canToggle: true,
    canNext: true,
    canPrev: true,
    canSeek: true,
    demo: true,
  };
}

export function demoControls(): ControlsState {
  return {
    wifi: true,
    wifiName: 'Örnek ağ',
    bluetooth: false,
    volume: { level: 64, muted: false, device: 'Hoparlör' },
    dark: true,
  };
}

export function demoToday(): TodayData {
  const day = (offset: number) => isoDate(new Date(Date.now() + offset * 86_400_000));
  return {
    // Ajanda önizlemesi (Faz 5b, eskiz paket 2 §1): döngüdeki sabah/akşam planları ve tek seferlikler.
    events: [
      { id: 1, date: day(-7), time: '08:00', title: 'Sabah sporu', minutes: 60, color: '#2ec4b6', repeat: 'daily', notify: true, lead: 5 },
      { id: 2, date: day(-7), time: '09:30', title: 'Çalışma', minutes: 180, color: '#5b9cff', repeat: 'weekdays', notify: true, lead: 5 },
      { id: 9, date: day(0), time: '13:00', title: 'Öğle', minutes: 60, color: '#9aa3b5', repeat: 'none', notify: false, lead: 0 },
      { id: 10, date: day(0), time: '14:00', title: 'Zodyvel', minutes: 180, color: '#8b7cff', repeat: 'none', notify: true, lead: 5 },
      { id: 11, date: day(-7), time: '17:30', title: 'Yürüyüş', minutes: 45, color: '#4fcf86', repeat: 'daily', notify: true, lead: 5 },
      { id: 12, date: day(0), time: '19:00', title: 'Yemek', minutes: 60, color: '#9aa3b5', repeat: 'none', notify: false, lead: 0 },
      { id: 13, date: day(-7), time: '20:30', title: 'Okuma', minutes: 75, color: '#f472b6', repeat: 'daily', notify: true, lead: 5 },
      { id: 14, date: day(-7), time: '22:00', title: 'Ders', minutes: 90, color: '#ff7a59', repeat: 'weekdays', notify: true, lead: 5 },
      { id: 2001, date: day(1), time: '15:00', title: 'Diş hekimi', minutes: 60, color: '#4fcf86', repeat: 'none', notify: true, lead: 30 },
      { id: 3, date: day(3), time: null, title: 'Doğum günü' },
    ],
    todos: [
      { id: 4, text: 'Eskizi onayla', done: true },
      { id: 5, text: 'Zodyvel 0.6 notları', done: false },
      { id: 6, text: 'Market', done: false },
    ],
    meters: [
      { id: 7, name: 'Okuma', color: '', sound: true, minutes: 25, endsAt: null, leftMs: 25 * 60_000, day: day(0), rounds: 2, doneMs: 50 * 60_000 },
      { id: 8, name: 'Çay', color: '#ffb020', sound: true, minutes: 4, endsAt: null, leftMs: 4 * 60_000, day: '', rounds: 0, doneMs: 0 },
    ],
    modes: defaultModes(),
    mode: null,
    reopen: [],
    version: 2,
    nextId: 3000,
  };
}

/** Tarayıcı önizlemesi için today.rs::apply'ın kısa karşılığı (doğrulama Rust'ta). */
export function demoTodayApply(t: TodayData, op: TodayOp, now: number): void {
  const id = () => ++t.nextId;
  const sortEvents = () => t.events.sort((a, b) => (a.date + (a.time ?? '')).localeCompare(b.date + (b.time ?? '')));
  if (op.op === 'addEvent') {
    const { op: _op, ...plan } = op;
    t.events.push({ ...plan, id: id(), title: plan.title.trim() });
    sortEvents();
  } else if (op.op === 'editEvent') {
    t.events = t.events.map(e => (e.id === op.id ? { ...op.plan, id: e.id, title: op.plan.title.trim() } : e));
    sortEvents();
  } else if (op.op === 'loopDay') {
    t.events = t.events.map(e =>
      e.date !== op.date || !e.time ? e : op.on && (e.repeat ?? 'none') === 'none' ? { ...e, repeat: 'daily' } : !op.on && e.repeat === 'daily' ? { ...e, repeat: 'none' } : e,
    );
  } else if (op.op === 'removeEvent') t.events = t.events.filter(e => e.id !== op.id);
  else if (op.op === 'addTodo') t.todos.push({ id: id(), text: op.text.trim(), done: false });
  else if (op.op === 'toggleTodo') t.todos = t.todos.map(x => (x.id === op.id ? { ...x, done: !x.done } : x));
  else if (op.op === 'removeTodo') t.todos = t.todos.filter(x => x.id !== op.id);
  else if (op.op === 'meterAdd' && t.meters.length < MAX_METERS)
    t.meters.push({ id: id(), name: op.name.trim() || 'Zodymetre', color: op.color, sound: op.sound, minutes: op.minutes, endsAt: null, leftMs: op.minutes * 60_000, day: '', rounds: 0, doneMs: 0 });
  else if (op.op === 'meterRemove') t.meters = t.meters.filter(m => m.id !== op.id);
  else if (op.op === 'meterEdit')
    t.meters = t.meters.map(m =>
      m.id !== op.id
        ? m
        : { ...m, name: op.name.trim() || 'Zodymetre', color: op.color, sound: op.sound, ...(m.minutes !== op.minutes && { minutes: op.minutes, endsAt: null, leftMs: op.minutes * 60_000 }) },
    );
  else if (op.op.startsWith('timer')) {
    if (!t.meters.length) t.meters.push({ id: id(), name: 'Zodymetre', color: '', sound: true, minutes: 25, endsAt: null, leftMs: 25 * 60_000, day: '', rounds: 0, doneMs: 0 });
    const target = 'id' in op && op.id !== undefined ? op.id : t.meters[0]!.id;
    t.meters = t.meters.map(m => (m.id === target ? demoTimerOp(m, op, now) : m));
  } else if (op.op === 'modeStart' || op.op === 'focusStart') {
    const def = op.op === 'modeStart' ? t.modes.find(m => m.id === op.id) : t.modes.find(m => m.focus);
    if (def && !t.mode) t.mode = startMode(op.op === 'focusStart' ? { ...def, focus: true } : def, op.op === 'modeStart' ? op.minutes : undefined, now, true);
  } else if (op.op === 'modePause' && t.mode) t.mode = pauseMode(t.mode, now);
  else if (op.op === 'modeResume' && t.mode) t.mode = resumeMode(t.mode, now);
  else if (op.op === 'modeAdd' && t.mode && t.mode.minutes > 0) {
    const add = op.minutes * 60_000;
    t.mode = { ...t.mode, minutes: t.mode.minutes + op.minutes, ...(t.mode.endsAt !== null ? { endsAt: t.mode.endsAt + add } : { leftMs: t.mode.leftMs + add }) };
  } else if (op.op === 'modeStop' || (op.op === 'focusStop' && t.mode?.focus)) {
    t.reopen = t.mode?.closed ?? [];
    t.mode = null;
  } else if (op.op === 'modeReopen') t.reopen = [];
  else if (op.op === 'modeSave') {
    const i = t.modes.findIndex(m => op.mode.id && m.id === op.mode.id);
    if (i >= 0) t.modes[i] = { ...op.mode, name: op.mode.name.trim() };
    else t.modes.push({ ...op.mode, id: `m${id()}`, name: op.mode.name.trim() });
  } else if (op.op === 'modeRemove' && t.mode?.id !== op.id) t.modes = t.modes.filter(m => m.id !== op.id);
}

function demoTimerOp(m: MeterState, op: TodayOp, now: number): MeterState {
  if (op.op === 'timerSet') return { ...m, minutes: op.minutes, endsAt: null, leftMs: op.minutes * 60_000 };
  if (op.op === 'timerStart' && m.endsAt === null) {
    const left = m.leftMs === 0 ? m.minutes * 60_000 : m.leftMs;
    return { ...m, leftMs: left, endsAt: now + left };
  }
  if (op.op === 'timerPause' && m.endsAt !== null) return { ...m, leftMs: Math.max(0, m.endsAt - now), endsAt: null };
  if (op.op === 'timerReset') return { ...m, endsAt: null, leftMs: m.minutes * 60_000 };
  if (op.op === 'timerAdd') return timerAdd(m, op.minutes, now);
  return m;
}

export function demoSystem(): SystemLoad {
  return {
    cpu: 14,
    ramUsedGb: 9.6,
    ramTotalGb: 15.9,
    gpu: 7,
    topApps: [
      { name: 'Visual Studio Code', ramMb: 1480 },
      { name: 'Opera GX', ramMb: 1120 },
      { name: 'Discord', ramMb: 610 },
      { name: 'Spotify', ramMb: 390 },
    ],
    demo: true,
  };
}

let counter = 0;

// Rust'taki örnek planla aynı metin (src-tauri/src/agents/state.rs DEMO_PLAN).
const DEMO_PLAN = [
  '# Kullanım özeti kartı',
  '',
  'AI ayrıntısına bugünün ve bu haftanın token/maliyet özetini ekle.',
  '',
  '## Adımlar',
  '',
  '1. `usage.rs`: Claude günlüklerini **yalnız panel açıkken** oku',
  '2. Model başına fiyat tablosu, bilinmeyen modelde maliyet yok',
  '3. Ada: mini görünümde küçük hap, ayrıntıda 7 günlük çubuklar',
  '',
  '## Dikkat',
  '',
  '- Boştayken hiçbir şey okunmaz',
  '- Kapalıyken hiçbir veri okunmaz',
  '',
  '```ts',
  'export function cost(usage: Usage): number',
  '```',
  '',
].join('\n');

export function demoApproval(session: AgentSession, kind: 'permission' | 'question' | 'plan' = 'permission'): ApprovalRequest {
  const question = kind === 'question';
  counter += 1;
  const now = Date.now();
  const base = {
    id: `demo-req-${counter}`,
    sessionId: session.id,
    tool: session.tool,
    project: session.project,
    createdAt: now,
    expiresAt: now + 90_000,
    actionable: true,
    closedEarly: false,
    nativeOnly: false,
    files: [],
    always: null,
    plan: null,
    demo: true,
  };
  if (kind === 'plan') {
    return {
      ...base,
      kind: 'plan',
      action: 'Planını sundu',
      detail: 'Kullanım özeti kartı',
      questions: [],
      expiresAt: Date.now() + 600_000,
      always: 'dosya düzenlemelerini sormadan kabul et · bu oturum boyunca',
      plan: DEMO_PLAN,
    };
  }
  // Örnek onaylar sırayla komut ve dosya düzenleme: ikisi de "her zaman izin ver" ile.
  if (!question && counter % 2 === 1) {
    return {
      ...base,
      kind: 'permission',
      action: 'Komut çalıştırmak istiyor',
      detail: 'pnpm test --filter api',
      questions: [],
      always: 'Bash(pnpm test:*) · bu projede, yalnız sende',
    };
  }
  if (!question) {
    return {
      ...base,
      kind: 'permission',
      action: 'Dosya değiştirmek istiyor',
      detail: 'src/lib/mood.ts',
      questions: [],
      files: [{ path: 'C:\\work\\zodyvel\\src\\lib\\mood.ts', name: 'mood.ts', added: 2, removed: 1 }],
      always: 'dosya düzenlemelerini sormadan kabul et · bu oturum boyunca',
    };
  }
  return {
    ...base,
    kind: 'question',
    action: '2 soru soruyor',
    detail: 'Hangi tema kullanılsın?',
    questions: [
      {
        question: 'Hangi tema kullanılsın?',
        header: 'Tema',
        options: [
          { label: 'Koyu', description: 'Gece için' },
          { label: 'Açık', description: 'Gündüz için' },
        ],
        multiSelect: false,
      },
      {
        question: 'Hangi bölümler açık olsun?',
        header: 'Bölümler',
        options: [
          { label: 'Müzik', description: '' },
          { label: 'Bugün', description: '' },
          { label: 'Bilgisayar', description: '' },
        ],
        multiSelect: true,
      },
    ],
  };
}

export function demoShelf(): ShelfData {
  const items = [
    { id: 3, path: 'C:\\Users\\demo\\Desktop\\zody-logo.png', name: 'zody-logo.png', dir: false },
    { id: 2, path: 'C:\\Users\\demo\\Belgeler\\Teklif\\teklif-ekim.pdf', name: 'teklif-ekim.pdf', dir: false },
    { id: 1, path: 'C:\\Users\\demo\\work\\Psrc', name: 'Psrc', dir: true },
  ];
  return { items, nextId: 3 };
}

/** Örnek son projeler (yeni görev klasör listesi). */
export function demoRecentProjects(): RecentProject[] {
  return [
    { path: 'C:\\work\\Psrc', name: 'Psrc', lastUsed: minutesAgo(5) },
    { path: 'C:\\work\\blog', name: 'blog', lastUsed: minutesAgo(60 * 20) },
    { path: 'C:\\work\\fatura-api', name: 'fatura-api', lastUsed: minutesAgo(60 * 26) },
  ];
}

/** Örnek kapanmış oturumlar ("Kapanmış ▾"). */
export function demoClosedSessions(): ClosedSession[] {
  return [
    { sessionId: 'demo-k1', cwd: 'C:\\work\\zodyvel', project: 'zodyvel', firstPrompt: 'Ayarlar sayfasını üç gruba ayır', endedAt: minutesAgo(60 * 21) },
    { sessionId: 'demo-k2', cwd: 'C:\\work\\fatura-api', project: 'fatura-api', firstPrompt: 'PDF çıktısında Türkçe karakterler', endedAt: minutesAgo(60 * 24 * 2) },
    { sessionId: 'demo-k3', cwd: 'C:\\work\\blog', project: 'blog', firstPrompt: 'RSS beslemesi ekle', endedAt: minutesAgo(60 * 24 * 8) },
  ];
}

/** Örnek yeni görev: listeye numaralı, çalışan bir Claude oturumu ekler. */
export function demoStartTask(agents: AgentSession[], cwd: string, prompt: string): { sessionId: string; taskNo: number } {
  const taskNo = agents.reduce((n, a) => Math.max(n, a.taskNo ?? 0), 0) + 1;
  const sessionId = `demo-t${taskNo}`;
  const project = cwd.split(/[\\/]/).filter(Boolean).at(-1) ?? cwd;
  agents.push({
    ...demoAgents()[2]!,
    id: sessionId,
    project,
    folder: cwd,
    status: 'running',
    step: prompt,
    startedAt: Date.now(),
    lastSeen: Date.now(),
    statusSince: Date.now(),
    turnStartedAt: Date.now(),
    turnEndedAt: null,
    turnFiles: 0,
    steps: [],
    changed: [],
    lastReply: null,
    notes: [],
    canNote: true,
    canTask: false,
    taskNo,
  });
  return { sessionId, taskNo };
}

/** Kaynaklar (Mail, GitHub) örnek durumları; tarayıcı önizlemesi ve ekran görüntüleri için. */
export function demoSources(): SourceState[] {
  const row = (id: string, title: string, summary: string, min: number, unread: boolean, extra: Partial<SourceItem> = {}): SourceItem => ({
    id,
    title,
    summary,
    timeMs: minutesAgo(min),
    unread,
    tag: null,
    url: 'https://example.com',
    actions: ['open', 'mark_read', 'preview'],
    ...extra,
  });
  return [
    {
      id: 'mail',
      status: 'ok',
      error: null,
      account: '2 hesap',
      badge: 3,
      urgency: 2,
      items: [
        row('1:4', 'Ayşe Kaya', 'Teklif son hâli, cuma görüşelim mi?', 9, true, { account: 'is.ornek@gmail.com' }),
        row('0:3', 'Kargo', 'Siparişiniz yola çıktı', 40, true, { account: 'ornek@gmail.com' }),
        row('1:2', 'GitHub', '[zodyvel] Review requested on #12', 62, true, { account: 'is.ornek@gmail.com' }),
        row('0:1', 'Banka', 'Ekim ekstreniz hazır', 60 * 26, false, { account: 'ornek@gmail.com' }),
      ],
      accounts: [
        {
          slot: 0,
          address: 'ornek@gmail.com',
          status: 'ok',
          error: null,
          badge: 12,
          syncedMs: minutesAgo(1),
          cats: [
            { key: 'primary', unread: 12, items: [row('0:30', 'Ayşe Yılmaz', 'Cuma toplantısı notları', 12, true), row('0:29', 'Proje ekibi', 'Taslak hazır, bakar mısın?', 33, true), row('0:28', 'Murat Kaya', 'Hafta sonu planı', 180, true), row('0:21', 'Zeynep Demir', 'Sunum dosyaları ekte', 240, true), row('0:20', 'Kütüphane', 'Ayırttığın kitap geldi', 420, true), row('0:19', 'Ev sahibi', 'Kombi bakımı perşembe', 600, true), row('0:18', 'Can Öztürk', 'Doğum günü listesi', 60 * 22, true), row('0:17', 'Proje ekibi', 'Haftalık özet', 60 * 30, true)] },
            { key: 'updates', unread: 34, items: [row('0:27', 'Kargo Takip', 'Paketin dağıtıma çıktı', 104, true), row('0:26', 'Banka', 'Ekstren hazır', 312, true), row('0:25', 'Okul Sistemi', 'Ödev notu girildi', 510, true)] },
            { key: 'promotions', unread: 128, items: [row('0:24', 'Mağaza Bülteni', 'Hafta sonuna özel indirim', 64, true), row('0:23', 'Kahve Dükkânı', 'Yeni çekirdekler geldi', 404, true), row('0:22', 'Etkinlik Bülteni', 'Bu ay şehirde neler var', 60 * 26, true)] },
          ],
        },
        {
          slot: 1,
          address: 'is.ornek@gmail.com',
          status: 'ok',
          error: null,
          badge: 2,
          syncedMs: minutesAgo(3),
          cats: [
            { key: 'primary', unread: 2, items: [row('1:4', 'Ayşe Kaya', 'Teklif son hâli, cuma görüşelim mi?', 9, true), row('1:2', 'GitHub', '[zodyvel] Review requested on #12', 62, true)] },
            { key: 'updates', unread: 0, items: [] },
            { key: 'promotions', unread: 5, items: [row('1:1', 'Yazılım Haftası', 'Bilet satışları başladı', 300, true)] },
          ],
        },
      ],
    },
    {
      id: 'github',
      status: 'ok',
      error: null,
      account: 'Demo',
      badge: 3,
      urgency: 3,
      items: [
        row('ci-1', 'zodyvel', 'main · test (windows-latest)', 2, true, { tag: 'ci_failed', actions: ['open', 'rerun', 'fix_with_claude'] }),
        row('pr-12', 'zodyvel#12', 'Ada üst kenar tıklaması', 60, true, { tag: 'review', actions: ['open'] }),
        row('n-7', 'tauri', 'Yeni sürüm 2.9.1', 180, true, { tag: 'notif', actions: ['open', 'mark_read'] }),
      ],
      repos: [
        { slug: 'ornek/zodyvel', url: 'https://github.com/ornek/zodyvel', timeMs: minutesAgo(50), private: true, branch: 'main', ci: 'failed' },
        { slug: 'ornek/barinak', url: 'https://github.com/ornek/barinak', timeMs: minutesAgo(60 * 5), private: false, branch: 'main', ci: 'ok' },
        { slug: 'ornek/gezgin', url: 'https://github.com/ornek/gezgin', timeMs: minutesAgo(60 * 30), private: true, branch: 'main', ci: 'none' },
        { slug: 'ornek/api', url: 'https://github.com/ornek/api', timeMs: minutesAgo(60 * 24 * 6), private: true, branch: 'master', ci: 'none' },
      ],
    },
  ];
}

/** Tarayıcı önizlemesi: Yerel Git "Depolarım" örnek depoları. */
export function demoLocalRepos(): Overview {
  const f = (path: string, kind: FileChange['kind'], extra: Partial<FileChange> = {}): FileChange => ({ path, orig: null, kind, sensitive: false, big: false, ...extra });
  const base = 'C:\\Users\\demo\\work\\';
  return {
    git: true,
    repos: [
      {
        path: base + 'Psrc',
        name: 'Psrc',
        branch: 'bulut-senkron',
        upstream: null,
        ahead: 0,
        behind: 0,
        files: [f('src/island/Island.svelte', 'modified'), f('src/lib/localgit.ts', 'untracked'), f('src-tauri/src/localgit/mod.rs', 'untracked'), f('.env', 'untracked', { sensitive: true })],
        more: 0,
        slug: null,
        error: null,
      },
      {
        path: base + 'Barinak',
        name: 'Barinak',
        branch: 'main',
        upstream: 'origin/main',
        ahead: 1,
        behind: 0,
        files: [f('Assets/Scripts/Player.cs', 'modified'), f('Assets/Scripts/OldEnemy.cs', 'deleted'), f('Assets/Scripts/Enemy.cs', 'renamed', { orig: 'Assets/Scripts/Foe.cs' }), f('Build/level1.bundle', 'untracked', { big: true })],
        more: 0,
        slug: 'ornek/barinak',
        error: null,
      },
      { path: base + 'Gezgin', name: 'Gezgin', branch: 'main', upstream: 'origin/main', ahead: 2, behind: 0, files: [], more: 0, slug: 'ornek/gezgin', error: null },
      { path: base + 'Api', name: 'Api', branch: 'master', upstream: 'origin/master', ahead: 0, behind: 0, files: [], more: 0, slug: 'ornek/api', error: null },
    ],
  };
}
