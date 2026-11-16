// Modüllerin arayüze taşıdığı veri sözleşmesi. Rust çekirdeği aynı biçimi üretecek.

export type AgentTool = 'claude' | 'codex';
export type AgentStatus = 'running' | 'waiting_input' | 'waiting_approval' | 'done' | 'error';

export interface AgentSession {
  id: string;
  tool: AgentTool;
  /** Görünen ad: sohbetin başlığı ("Naber"); başlık yoksa proje/klasör adı. */
  project: string;
  /** Kategori: proje ya da klasör adı ("Ödev"). Eski sürümden gelen veride olmayabilir. */
  category?: string;
  status: AgentStatus;
  step: string;
  startedAt: number;
  /** Son olay (Unix ms). */
  lastSeen: number;
  /** Arayüzün hesabı: uzun süredir bekliyor/bitti, gezegeni uyuyor. */
  asleep?: boolean;
  demo: boolean;
  /** Son adımlar, eskiden yeniye (en çok 30). */
  steps: AgentStep[];
  /** Oturumun çalıştığı pencerenin uygulaması ("WindowsTerminal.exe"…); doluysa "Terminale git" var. */
  app: string | null;
  /** Geçerli durumun başladığı an (Unix ms); bekleme hatırlatması buna bakar. */
  statusSince: number;
  /** Son turun başlangıcı ve bitişi (Unix ms); tur sürerken bitiş null. */
  turnStartedAt: number | null;
  turnEndedAt: number | null;
  /** Bu turda değişen farklı dosya sayısı. */
  turnFiles: number;
  /** Bu turda değişen dosyalar, ilk değişene göre sırayla. */
  changed: ChangedFile[];
  /** Ajanın kendi yapılacaklar listesi. */
  todos: AgentTodo[];
  /** Son turun sonunda ajanın yazdığı cevap (Markdown, kırpılmış). */
  lastReply: string | null;
  /** İzin kipi: "default", "plan", "acceptEdits", "bypassPermissions"… */
  mode: string | null;
  /** Oturumun klasörü. */
  folder: string | null;
  /** Şu an çalışan alt ajan sayısı. */
  subagents: number;
  /** Sıradaki notlar, eskiden yeniye. */
  notes: AgentNote[];
  /** Not gönderilebilir (Claude Code, adadan onay modu, yeni köprü). */
  canNote: boolean;
  /** Boştaki oturuma adadan görev gönderilebilir (tur sonunda bekleyen köprü var). */
  canTask: boolean;
  /** Zodyvel'in yazma sahnesinden başlattığı görevin numarası (Zody'de rozet); Zodyvel kapanınca düşer. */
  taskNo?: number | null;
  /** Raftan bırakılan dosya bu oturuma gerçekten eklenir (Zodyvel'in başlattığı, süreci açık görev); değilse
   * yol panoya kopyalanır ve pencere öne gelir (K5). */
  attach?: boolean;
}

/** Yeni görev için son kullanılan klasörler (Rust: list_recent_projects). */
export interface RecentProject {
  path: string;
  name: string;
  lastUsed: number;
}

/** Kapanmış Claude Code oturumu; `--resume` ile kaldığı yerden sürer (Rust: list_closed_sessions). */
export interface ClosedSession {
  sessionId: string;
  cwd: string;
  project: string;
  firstPrompt: string;
  endedAt: number;
}

/** Yeni görevin izin kipi: "Sorarak" (default) ya da "Düzenlemeler serbest" (acceptEdits). */
export type TaskPermission = 'default' | 'acceptEdits';

/** Arka planda başlatılan görevin süreci hatayla bitti (Rust olayı "task-failed"). */
export interface TaskFailed {
  sessionId: string;
  taskNo?: number | null;
  message: string;
}

export type TodoStatus = 'pending' | 'in_progress' | 'completed';

export interface AgentTodo {
  text: string;
  /** Sürerken gösterilen biçim; araç vermediyse boş. */
  active: string;
  status: TodoStatus;
}

export interface ChangedFile {
  path: string;
  name: string;
  added: number;
  removed: number;
  /** Dosyayı son değiştiren adım (fark için). */
  step: number;
}

export interface AgentNote {
  id: number;
  text: string;
  /** true: bir sonraki araç çağrısında araya girer; false: tur biterken. */
  now: boolean;
  at: number;
}

/** Oturumun bağlam doluluğu (Rust usage::SessionContext). */
export interface SessionContext {
  model: string | null;
  tokens: number;
  window: number;
}

export type StepKind =
  | 'prompt' | 'read' | 'edit' | 'write' | 'bash' | 'search' | 'web'
  | 'agent' | 'plan' | 'question' | 'approval' | 'tool' | 'done' | 'error';
export type StepState = 'now' | 'done' | 'failed';

export interface AgentStep {
  id: number;
  kind: StepKind;
  /** Kısa hedef: dosya adı, komut… */
  target: string;
  /** Tür tek başına anlatmadığında (bilinmeyen araç, onay kararı, tur özeti). */
  label: string;
  at: number;
  state: StepState;
  /** Alt ajanın attığı adım. */
  sub: boolean;
}

/** Ayrıntı ekranında bir adımın içeriği (istenince Rust üretir). */
export interface StepContent {
  kind: 'file' | 'diff' | 'output' | 'none';
  path: string | null;
  lines: { n: number | null; text: string; mark: 'add' | 'del' | null }[];
  note: string | null;
}

export interface QuestionOption {
  label: string;
  description: string;
}

export interface Question {
  question: string;
  header: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

/** Onay kartında dosya bazında "+N −M". */
export interface FileStat {
  path: string;
  name: string;
  added: number;
  removed: number;
}

export interface ApprovalRequest {
  id: string;
  sessionId: string;
  tool: AgentTool;
  project: string;
  /** permission: Onayla/Reddet · question: seçenekli sorular · plan: plan modunda sunulan plan */
  kind: 'permission' | 'question' | 'plan';
  action: string;
  detail: string;
  questions: Question[];
  createdAt: number;
  /** Yalnız adadan karar verilebilen canlı isteklerde dolu. */
  expiresAt: number | null;
  /** false: karar ilgili araçta verilir, kart yalnız bilgi verir. */
  actionable: boolean;
  /** Araç, adadaki süre dolmadan beklemeyi bıraktı (eski kayıtta kanca süresi kısa). */
  closedEarly: boolean;
  /** Adadan hiç cevaplanamaz (yalnız izle, Codex sorusu, araç bildirimi); süre dolmuş değil. */
  nativeOnly: boolean;
  /** Dosya değiştiren isteklerde dosya bazında satır sayıları. */
  files: FileStat[];
  /** "Her zaman izin ver" seçeneğinin açıklaması (ne izinli olacak, nereye yazılacak); yoksa seçenek yok. */
  always: string | null;
  /** Yalnız plan isteklerinde: planın Markdown metni. */
  plan: string | null;
  demo: boolean;
}

export interface AgentsSnapshot {
  agents: AgentSession[];
  approvals: ApprovalRequest[];
  lastEvents: { claude: number | null; codex: number | null };
}

export type ConnectionMode = 'interactive' | 'observe';

export interface Inspection {
  provider: AgentTool;
  path: string;
  exists: boolean;
  installed: boolean;
  mode: ConnectionMode | null;
  events: string[];
  legacy: number;
  error: string | null;
  hookReady: boolean;
  /** Adadan onay kurulu ama kayıtlar eski (seçenekli sorular adadan cevaplanamaz). */
  needsUpdate: boolean;
  lastEventAt: number | null;
}

export interface SetupPreview {
  id: string;
  provider: AgentTool;
  path: string;
  backupPath: string | null;
  changes: string[];
  content: string;
}

/** Claude Code durum satırı köprüsünün durumu (Rust agents/statusline.rs StatusInspection). */
export interface StatusInspection {
  path: string;
  installed: boolean;
  /** Kullanıcının kendi durum satırı komutu (kuruluysa içinde saklanan). */
  original: string | null;
  error: string | null;
  hookReady: boolean;
}

/** Plan limitinin bir penceresi: kullanılan yüzde ve sıfırlanma anı (ms). */
export interface LimitWindow {
  pct: number;
  resetsAt: number;
}

/** Claude plan limitleri (Rust statusline.rs PlanLimits); süresi dolan pencere gelmez. */
export interface PlanLimits {
  fiveHour: LimitWindow | null;
  sevenDay: LimitWindow | null;
  at: number;
}

export interface NowPlaying {
  title: string;
  artist: string;
  app: string;
  playing: boolean;
  positionMs: number;
  durationMs: number;
  /** Konumun ölçüldüğü an (Unix ms); çalarken konum buradan ileri sayılır. */
  updatedAt: number;
  artId: string | null;
  canToggle: boolean;
  canNext: boolean;
  canPrev: boolean;
  /** Uygulama konuma gitmeye izin veriyor mu. */
  canSeek: boolean;
  demo: boolean;
}

export interface MusicUpdate {
  now: NowPlaying | null;
  /** Yalnız kapak değiştiğinde dolu (data: adresi). */
  art: string | null;
}

export interface VolumeState {
  /** 0–100 */
  level: number;
  muted: boolean;
  device: string | null;
}

/** Kontroller ekranı. null: bu bilgisayarda yok ya da okunamadı. */
export interface ControlsState {
  wifi: boolean | null;
  wifiName: string | null;
  bluetooth: boolean | null;
  volume: VolumeState | null;
  dark: boolean | null;
}

/** Etkin bir ses çıkışı aygıtı. */
export interface AudioOutput {
  id: string;
  /** Aygıtın gerçek adı ("Razer Barracuda X (BT)"). */
  name: string;
  /** Aygıt türü ("Kulaklıklar"); null: ad zaten türü söylüyor. */
  kind: string | null;
  /** Şu an varsayılan olan. */
  default: boolean;
}

export interface OutputSwitch {
  controls: ControlsState;
  outputs: AudioOutput[];
}

export interface CalEvent {
  id: number;
  /** Yerel tarih, "2026-10-14". */
  date: string;
  /** "14:00"; null: gün boyu. */
  time: string | null;
  title: string;
  /** Ajanda (Faz 5b). Eski etkinliklerde yok: süresiz, tekrarsız, tam saatinde haber. */
  minutes?: number;
  color?: string;
  repeat?: Repeat;
  /** `repeat` 'days' iken: bit 0 Pazartesi … bit 6 Pazar. */
  days?: number;
  notify?: boolean;
  /** Kaç dakika önce haber verilir. */
  lead?: number;
  /** Telefon eşliyse hatırlatma telefonda da çalar (varsayılan açık). */
  phone?: boolean;
}

export type Repeat = 'none' | 'daily' | 'weekdays' | 'days';

/** Yeni plan / düzenleme formu (Rust `PlanForm`). */
export interface PlanForm {
  date: string;
  time: string | null;
  title: string;
  minutes: number;
  color: string;
  repeat: Repeat;
  days: number;
  notify: boolean;
  lead: number;
  phone: boolean;
}

/** Rust `phone::Status` (Ayarlar › Telefon). */
export interface PhoneDevice {
  id: string;
  name: string;
  pairedAt: number;
  lastSeen: number;
  connected: boolean;
  /** O9: Key Attestation doğrulandı (donanım anahtarı + parmak izi/PIN şartı). `false` → "Korumasız", kısıtlı. */
  hw: boolean;
  /** "strongbox" | "tee" | "" */
  hwLevel: string;
  /** Korumasızsa kısa neden. */
  hwWhy: string;
  /** G4: 30 günden uzun süredir bağlanmadı; bağlanınca yeniden eşleştirme istenir. */
  expired: boolean;
}
export interface PhoneStatus {
  enabled: boolean;
  listening: boolean;
  addr: string | null;
  problem: string | null;
  devices: PhoneDevice[];
  pairingOpen: boolean;
  pairCode: { name: string } | null;
  audit: string[];
}
export interface PhonePairStart {
  svg: string;
  addr: string;
  expiresInSecs: number;
}

/** Saati gelen etkinlik (Rust today://event-due); `more`: aynı anda gelen diğerleri. */
export interface EventDue {
  key: number;
  id: number;
  title: string;
  time: string;
  more: number;
  /** Kaç dakika önce haber verildi (0: tam saatinde) ve planın rengi. Eski olaylarda yok. */
  lead?: number;
  color?: string;
}

export interface Todo {
  id: number;
  text: string;
  done: boolean;
}

/** Zodymetre (K8, Faz 5a): adlı, süreli, renkli zamanlayıcı; aynı anda en çok üç (Rust today.rs Meter). */
export interface MeterState {
  id: number;
  name: string;
  /** "#5b9cff"; boş: vurgu rengi. */
  color: string;
  /** Dolunca ses. */
  sound: boolean;
  minutes: number;
  /** Çalışıyorsa bitiş anı (Unix ms). */
  endsAt: number | null;
  /** Duraklatılmışken kalan; 0: doldu. */
  leftMs: number;
  /** Bugün kaç kez doldu ve toplamı (`day` yerel tarih; değişince sıfırlanır). */
  day: string;
  rounds: number;
  doneMs: number;
}

/** Süre gösterimi için yeter: Zodymetre ya da süreli mod. */
export type TimerState = Pick<MeterState, 'minutes' | 'endsAt' | 'leftMs'>;

/** Zodymetre doldu (`today://timer-done`): Bitiş bildirimi. */
export interface MeterDone {
  id: number;
  name: string;
  color: string;
  minutes: number;
  rounds: number;
  totalMs: number;
}

/** Odak (Pomodoro) aşaması; süreler today/focus.rs ile aynı. Faz 5a'dan beri modun içinde döner. */
export type FocusPhase = 'work' | 'break' | 'longBreak';

export interface FocusState {
  phase: FocusPhase;
  /** Kaçıncı odak (1–4); molada az önce biten odağın sırası. */
  round: number;
}

/** Bir odak aşaması doldu (`today://focus`); `next` null: döngü tamam. */
export interface FocusStep {
  ended: FocusPhase;
  next: FocusState | null;
}

export interface AppRef {
  name: string;
  /** Tam yol (.exe; açılacaklarda .lnk/.url da olur). */
  path: string;
}

export type ModeIcon = 'brief' | 'book' | 'moon' | 'game' | 'star' | 'leaf';

/** Mod tanımı (Faz 5a, K4; Rust today/modes.rs ModeDef). */
export interface ModeDef {
  id: string;
  name: string;
  icon: ModeIcon;
  color: string;
  /** 0: süresiz. */
  minutes: number;
  /** Mod açıkken çentikte yalnız AI bildirimleri ve kendi Zodymetre/mod Bitişleri. */
  onlyAi: boolean;
  /** true: müziğe dokunulmaz; false: başlarken duraklatılır. */
  music: boolean;
  /** Mod açıkken Zody'nin sesleri kısılır. */
  quiet: boolean;
  /** Modun içinde 25/5 odak döngüsü ("Odak modu" özelliği açıksa). */
  focus: boolean;
  closeApps: AppRef[];
  openApps: AppRef[];
}

/** Açık mod (Rust ModeRun). */
export interface ModeRun {
  id: string;
  startedAt: number;
  /** Seçilen süre; 0: süresiz. */
  minutes: number;
  endsAt: number | null;
  leftMs: number;
  paused: boolean;
  focus: FocusState | null;
  phaseEndsAt: number | null;
  phaseLeftMs: number;
  closed: AppRef[];
}

/** Mod bitti (`today://mode-done`, elle bitirilince de): Mod Bitişi. */
export interface ModeDone {
  id: string;
  name: string;
  icon: ModeIcon;
  color: string;
  minutes: number;
  /** Bu modun kapattığı uygulamalar (yeniden açma düğmesi). */
  reopen: string[];
}

/** Bugün: takvim, yapılacaklar, Zodymetreler ve Modlar (Rust today.json). */
export interface TodayData {
  version: number;
  events: CalEvent[];
  todos: Todo[];
  meters: MeterState[];
  modes: ModeDef[];
  mode: ModeRun | null;
  reopen: AppRef[];
  nextId: number;
}

export type TodayOp =
  | ({ op: 'addEvent' } & (PlanForm | { date: string; time: string | null; title: string }))
  | { op: 'editEvent'; id: number; plan: PlanForm }
  | { op: 'removeEvent'; id: number }
  | { op: 'loopDay'; date: string; on: boolean }
  | { op: 'addTodo'; text: string }
  | { op: 'toggleTodo'; id: number }
  | { op: 'removeTodo'; id: number }
  /** Zodymetre komutları; `id` yoksa ilk Zodymetre. */
  | { op: 'timerSet'; id?: number; minutes: number }
  | { op: 'timerStart'; id?: number }
  | { op: 'timerPause'; id?: number }
  | { op: 'timerReset'; id?: number }
  /** Süre ekler (çentikteki "+5 dk"); dolmuş Zodymetre bu kadar süreyle yeniden başlar. */
  | { op: 'timerAdd'; id?: number; minutes: number }
  | { op: 'meterAdd'; name: string; minutes: number; color: string; sound: boolean }
  | { op: 'meterEdit'; id: number; name: string; minutes: number; color: string; sound: boolean }
  | { op: 'meterRemove'; id: number }
  | { op: 'modeSave'; mode: ModeDef }
  | { op: 'modeRemove'; id: string }
  | { op: 'modeStart'; id: string; minutes?: number }
  | { op: 'modePause' }
  | { op: 'modeResume' }
  | { op: 'modeAdd'; minutes: number }
  | { op: 'modeStop' }
  | { op: 'modeReopen' }
  | { op: 'focusStart' }
  | { op: 'focusStop' };

export interface AppUsage {
  name: string;
  ramMb: number;
}

export interface SystemLoad {
  cpu: number;
  ramUsedGb: number;
  ramTotalGb: number;
  gpu: number | null;
  topApps: AppUsage[];
  demo: boolean;
}

/** Pil (Rust power.rs); pili olmayan bilgisayarda hiç gelmez. */
export interface Battery {
  percent: number;
  plugged: boolean;
  charging: boolean;
  saver: boolean;
  /** Windows'un tahmini (yalnız pilden çalışırken). */
  minutesLeft: number | null;
}

export type ShortcutAction = 'toggle' | 'approve' | 'deny' | 'mute' | 'write' | 'approval' | 'terminal' | 'stop';

/** Ses çalınan olaylar (Rust sound.rs Cue ile aynı). */
export type SoundCue = 'attention' | 'done' | 'error' | 'timer' | 'hello';

export interface Sounds {
  /** zody: kendi setimiz · windows: kullanıcının Windows ses düzeni. */
  pack: 'zody' | 'windows';
  attention: boolean;
  done: boolean;
  error: boolean;
  timer: boolean;
  hello: boolean;
  /** Windows "Rahatsız etme" açıkken hiç ses çalınmaz. */
  respectQuiet: boolean;
  /** Yüklenen sesler: olay → özgün dosya adı. */
  custom: Partial<Record<SoundCue, string>>;
}

export type ScreenMode = 'primary' | 'fixed' | 'cursor';

/** Ayarlar → Ada → "Hangi ekranda" listesindeki bir ekran (island_screens). */
export interface Screen {
  id: string;
  /** "Ekran 2 · 2560×1440 · %150" */
  label: string;
  primary: boolean;
}

/** Ses göstergesi olayı (`volume://osd`, osd.rs Osd ile aynı). */
export interface VolumeOsd {
  /** 0–100 */
  level: number;
  muted: boolean;
}

export interface Settings {
  version: 1;
  modules: { agents: boolean; music: boolean; system: boolean; today: boolean; modes: boolean; agenda: boolean; brain?: boolean; phone?: boolean };
  island: {
    hoverDelayMs: number;
    reduceMotion: boolean;
    sound: boolean;
    flash: boolean;
    hover: 'auto' | 'ai' | 'last';
    /** Üzerine gelince mini açılmadan önce maskot kafasını çıkarıp selam verir (peek). */
    peek: boolean;
    /** Açılış: hover = üzerine gelince mini; click = üzerine gelince yalnız Zody, tıkla mini, bir tık daha ayrıntı. */
    reveal: 'hover' | 'click';
    /** Uygulama açılınca kısa selam. */
    greet: boolean;
    /** Tanıtım turu görüldü; false: bir sonraki açılışta (ya da hemen) tur gösterilir. */
    welcomed: boolean;
    /** En son açılışta görülen sürüm; güncelleme sonrası tek seferlik not için ('' = ilk açılış). */
    lastSeenVersion: string;
    /** Küçük görünümde simge çubuğunun boş yerine tıklayınca ayrıntı açılır. */
    barOpen: boolean;
    /** Hangi ekranda: birincil, seçilen ekran (yoksa birincil) ya da imlecin olduğu ekran. */
    screen: ScreenMode;
    /** Seçilen ekranın Windows aygıt adı (screen === 'fixed'). */
    screenId: string;
    /** Ada boyutu yüzdesi (src/lib/screen.ts SIZE aralığı). */
    size: number;
    /** Ses tuşlarında Zody göstergesi (Faz 10, src-tauri/src/osd.rs). Açıkken Windows'un ses penceresi çıkmaz. */
    volumeOsd: boolean;
  };
  sounds: Sounds;
  appearance: {
    theme: string;
    surface: string;
    /** K2'den beri tek görünüm: Rust her zaman 'new' yazar; alan eski dosyalarla uyum için duruyor. */
    look: 'classic' | 'new';
    /** Eski görünüm sorusu kalktı; her zaman true. */
    lookAsked: boolean;
    /** Düzen (Faz 2b): 'zody' üzerine gelince mini AI; 'clock' en solda Saat ana ekranı. Sekmeler ikisinde aynı. */
    layout: Layout;
  };
  /** "Ctrl+Alt+Z" biçiminde; boş: kapalı. */
  shortcuts: Record<ShortcutAction, string>;
  /** Tek tek açılıp kapanan özellikler; kapalı özellik veri okumaz, ekranda yer kaplamaz. */
  features: Features;
  /** Yerel Git'in bakacağı klasörler (kendisi depo ya da 2 kat altında depolar). */
  git: { folders: string[] };
  /** Sekme çubuğunun sağındaki uygulama kısayolları (en çok MAX_APPS; Rust apps.rs). */
  apps: AppShortcut[];
  /** Saat ana ekranı (Faz 3): kartlar, büyük saat, not. Rust ClockPrefs. */
  clock: ClockPrefs;
  /** Canlı çentik (Faz 4, Ayarlar › Bildirimler › Çentik). Rust NotchPrefs. */
  notch: NotchPrefs;
  /** Zody asistanı (Faz 12, Deneysel): model seçimi. */
  brain?: { model: 'auto' | 'fast' | 'long'; access?: 'closed' | 'low' | 'mid'; /** Dosya kopyalama/taşıma için izinli klasörler (Faz 13b). */ allowedFolders?: string[] };
}

/** Fare çentiğe gelince: 'peek' bir tık büyüyüp ayrıntıyı yazar (ara boy), 'open' ilgili sekmeyi açar,
 * 'click' yalnız tıklayınca açar, 'mini' hap mini pencereye dönüşür (Faz 9; Mini AI/Oynatıcı/Kronometre). */
export type NotchHover = 'peek' | 'open' | 'click' | 'mini';

export interface NotchPrefs {
  /** Ada kapalıyken süren etkinlikler (Zodymetre, AI çalışıyor, müzik) çentikte kalıcı görünür;
   * false: yalnız olay olunca (onay, bitti, Bitiş…). */
  live: boolean;
  hover: NotchHover;
  /** Hangi süren etkinlikler çentikte görünsün. */
  timer: boolean;
  /** Açık mod (Faz 5a). */
  mode: boolean;
  ai: boolean;
  music: boolean;
  /** 'mini' kipinde hangi haplar mini pencere açar (Faz 9); kapalı olan ara boyda kalır. */
  miniAi: boolean;
  miniMusic: boolean;
  miniTimer: boolean;
}

export interface ClockPrefs {
  /** Seçilen kartlar, gösterim sırasıyla (en çok MAX_CARDS; lib/clock-cards.ts). */
  cards: string[];
  /** Büyük saat görünsün mü; kapalıysa kartlar bütün genişliği kullanır. */
  showTime: boolean;
  /** Not kartının metni. */
  note: string;
}

export type Layout = 'zody' | 'clock';

/** Sekme çubuğundaki uygulama kısayolu. `target`: .exe/.lnk yolu ya da http(s) adresi. */
export interface AppShortcut {
  id: string;
  name: string;
  target: string;
}

/** Rust apps.rs MAX_APPS ile aynı. */
export const MAX_APPS = 4;

export interface Features {
  /** Onay/soru ya da yanıt `waitAlertMin` dakikadır bekliyorsa bir kez hatırlat. */
  waitAlert: boolean;
  waitAlertMin: 5 | 10 | 20;
  /** "Bitti" bildiriminde tur süresi ve değişen dosya sayısı. */
  doneSummary: boolean;
  /** Bilgisayar ekranında pil (yalnız pili olan bilgisayarda). */
  battery: boolean;
  /** Şarja takılınca/çıkınca ve pil azalınca çentik bildirimi. */
  batteryAlerts: boolean;
  /** Yeni görünümde müzik kartı kapağın baskın rengini alır (kapak değişince bir kez hesaplanır). */
  coverTint: boolean;
  /** Zamanlayıcıda odak döngüsü: 25 dk odak, 5 dk mola; dördüncü odaktan sonra 15 dk uzun mola. */
  focus: boolean;
  /** Claude plan modunda planını sunduğunda planı adada okuyup onaylama. */
  planReview: boolean;
  /** Claude kullanım ve tahmini maliyet özeti (yerel kayıtlardan). */
  usage: boolean;
  /** Pazartesi geçen haftanın özetini çentikte bir kez göster. */
  weeklyRecap: boolean;
  /** Adaya bırakılan dosyalar rafta durur, oradan başka uygulamaya sürüklenir (yalnız yolları tutulur). */
  shelf: boolean;
  /** Çalışan ajana not bırakma (tur biterken ya da araya girerek). */
  agentNotes: boolean;
  /** Ayrıntıda model ve bağlam doluluğu (oturum kaydının sonundan okunur). */
  agentContext: boolean;
  /** Ajan aynı komutu tekrar tekrar deniyorsa uyarı şeridi. */
  loopAlert: boolean;
  /** Mini adada yazma sahnesi (✎ ve kısayol); "Ajana not" buraya taşınır. Yalnız Yeni görünümde. */
  writeScene: boolean;
  /** Yazma sahnesinden yeni Claude Code görevi ve kapanmış oturuma devam. */
  newTask: boolean;
  /** Onay/soru kartında "Sonra ⌄": kart cevaplanmadan katlanır. Yalnız Yeni görünümde. */
  foldCard: boolean;
  /** AI ayrıntısında Claude plan limiti hapı (Claude Code durum satırı köprüsünden). */
  planLimit: boolean;
  /** "Onaya git" ve "Terminale git" kısayolları. Yalnız Yeni görünümde. */
  jumpKeys: boolean;
  /** Rafta dosyaya "Claude'a sor": yazma sahnesi o dosya ekli açılır. Yalnız Yeni görünümde. */
  shelfAsk: boolean;
  /** Kaynaklar (ana anahtar): AI sekmesi Yeni görünümde "Kaynaklar" olur; Mail ve GitHub adada. */
  sources: boolean;
  /** Kaynak: Gmail (IMAP, uygulama şifresi Windows Kimlik Bilgisi Yöneticisi'nde). */
  sourceMail: boolean;
  /** Kaynak: GitHub (klasik token Windows Kimlik Bilgisi Yöneticisi'nde). */
  sourceGithub: boolean;
  /** GitHub › "Depolarım": bilgisayardaki depolarda değişiklikler, adadan commit ve push (onaylı). */
  localGit: boolean;
  /** Ada açılırken en son etkinlik olan bölüm seçilir (bekleyen onay her zaman önce). */
  recentTab: boolean;
  /** Üst çubukta Zody sekmesi: Zody ile yazışarak komut (kural tabanlı, model yok). Yalnız Yeni görünümde. */
  zodyChat: boolean;
}

/** Dosya rafındaki bir öğe (Rust shelf/list.rs Item). */
export interface ShelfItem {
  id: number;
  path: string;
  name: string;
  dir: boolean;
}

/** Dosya rafı (Rust shelf.json); en yeni başta. */
export interface ShelfData {
  items: ShelfItem[];
  nextId: number;
}

/** Adaya dosya sürükleniyor (`shelf://drag`): enter rafı açar, drop ekler, send bir AI oturumuna gönderir
 * (sonucu `shelf://sent`), leave kapatır. */
export interface ShelfDrag {
  kind: 'enter' | 'drop' | 'send' | 'leave';
  count: number;
}

/** Raftan AI oturumuna gönderimin sonucu (`shelf://sent`, `shelf_send`). */
export interface ShelfSent {
  session: string;
  /** attached: göreve eklendi · copied: yol panoya kopyalandı. */
  how: 'attached' | 'copied';
  /** Oturumun penceresi öne geldi. */
  focused: boolean;
  count: number;
  name: string;
  error?: string;
}

/** off: atanmamış · ok: hazır · conflict: başka bir uygulama aynı birleşimi kullanıyor. */
export type ShortcutStatus = 'off' | 'ok' | 'conflict';

export interface Autostart {
  enabled: boolean;
  disabledBySystem: boolean;
  otherPath: string | null;
  available: boolean;
}

export type ZodyMood = 'idle' | 'working' | 'attention' | 'done' | 'error' | 'music' | 'sleep';


export const defaultSettings: Settings = {
  version: 1,
  modules: { agents: true, music: true, system: true, today: true, modes: true, agenda: true, brain: false, phone: false },
  island: { hoverDelayMs: 140, reduceMotion: false, sound: true, flash: true, hover: 'auto', peek: true, reveal: 'hover', greet: true, welcomed: false, lastSeenVersion: '', barOpen: true, screen: 'primary', screenId: '', size: 100, volumeOsd: false },
  sounds: { pack: 'zody', attention: true, done: true, error: true, timer: true, hello: false, respectQuiet: true, custom: {} },
  appearance: { theme: 'gece', surface: 'siyah', look: 'new', lookAsked: true, layout: 'zody' },
  shortcuts: { toggle: 'Ctrl+Alt+Z', approve: 'Ctrl+Alt+Enter', deny: 'Ctrl+Alt+Backspace', mute: 'Ctrl+Alt+M', write: 'Ctrl+Alt+Y', approval: 'Ctrl+Alt+O', terminal: 'Ctrl+Alt+T', stop: 'Ctrl+Alt+X' },
  features: defaultFeatures(),
  git: { folders: [] },
  apps: [],
  clock: { cards: ['music', 'ai', 'agenda', 'shelf'], showTime: true, note: '' },
  notch: { live: true, hover: 'peek', timer: true, mode: true, ai: true, music: true, miniAi: true, miniMusic: true, miniTimer: true },
};

/** Özelliklerin varsayılanı (Rust Features::default ile aynı); "Varsayılana dön" bunu kullanır. */
export function defaultFeatures(): Features {
  return {
    waitAlert: false,
    waitAlertMin: 10,
    doneSummary: true,
    battery: true,
    batteryAlerts: true,
    coverTint: true,
    focus: true,
    planReview: true,
    usage: true,
    weeklyRecap: false,
    shelf: true,
    agentNotes: true,
    agentContext: true,
    loopAlert: true,
    writeScene: true,
    newTask: true,
    foldCard: true,
    planLimit: false,
    jumpKeys: true,
    shelfAsk: true,
    sources: false,
    sourceMail: false,
    sourceGithub: false,
    localGit: false,
    recentTab: false,
    zodyChat: true,
  };
}
