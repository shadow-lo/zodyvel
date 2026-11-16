//! AI ajan oturumları ve onay istekleri. Saf durum: zaman dışarıdan verilir, test edilebilir.
//!
//! Kurallar:
//! - Bir onay isteği tek kullanımlıktır; karar, süre dolumu veya oturumun ilerlemesiyle kapanır.
//! - Adadan karar yalnız "adadan onay" modundaki canlı isteklerde mümkündür. Diğerleri bilgi kartıdır.
//! - Süre dolan canlı istek otomatik onaylanmaz; araçta bekleyen bilgi kartına dönüşür.
//! - Seçenekli soru da bir istektir: yalnız sorulan her sorunun cevabıyla kapanır; "araçta cevapla"
//!   ya da süre dolumu soruyu aracın kendi penceresine bırakır.
//! - Her oturum için son adımlar tutulur (ayrıntı ekranı); içerik ayrıca, istenince üretilir.
//! - "Her zaman izin ver" yalnız aracın kendi önerdiği güvenli bir kural varsa sunulur; kural köprüde,
//!   aracın özgün önerisinden yazılır (ada kural metni üretmez).
//! - Codex'in seçenekli sorusu hook ile cevaplanamaz: yalnız bilgi kartıdır ("Terminale git").
//! - Plan incelemesi (Claude plan modu): plan bir istektir; Onayla, Reddet ya da notla geri çevir.
//!   Özellik kapalıysa kart açılmaz, karar araçta kalır. Oturum yeni bir araç adımına geçtiyse
//!   (plan araçta onaylandı) kart kalkar.
//! - Ajanın kendi yapılacaklar listesi (`TodoWrite`, `TaskCreate/TaskUpdate`, Codex `update_plan`),
//!   tur sonundaki cevabı, izin kipi, klasörü ve turda değişen dosyalar (+N −M) oturumda tutulur.
//! - Ajana not (yalnız Claude Code, adadan onay modu, notu okuyabilen köprü): sıradaki not tur
//!   biterken ("sonra") ya da bir sonraki araç çağrısında ("şimdi") bir kez teslim edilir.
//! - Adadan görev (boştaki Claude Code oturumu): tur sonunda arka planda bekleyen köprü (`asyncRewake`)
//!   oturum başına bir tanedir; görev gelince Claude uyanır. Yeni tur, oturum sonu ya da kopan köprü
//!   bekleyeni kaldırır.

use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use zodyvel_hook::{ASK_TOOL, CODEX_ASK_TOOL, Decision, Mode, PLAN_TOOL, Provider};

use super::describe::{self, Kind, Raw};

const MAX_SESSIONS: usize = 24;
const SESSION_TTL_MS: u64 = 3 * 60 * 60 * 1000;
const MAX_STEPS: usize = 30;
const MAX_TODOS: usize = 30;
const MAX_CHANGED: usize = 40;
const MAX_NOTES: usize = 5;
const MAX_REPLY: usize = 4000;

/// Örnek plan (Ayarlar ve ada "Örnek plan"): kartın Markdown görünümünü denemek için.
const DEMO_PLAN: &str = "# Kullanım özeti kartı\n\nAI ayrıntısına bugünün ve bu haftanın token/maliyet özetini ekle.\n\n## Adımlar\n\n1. `usage.rs`: Claude günlüklerini **yalnız panel açıkken** oku\n2. Model başına fiyat tablosu, bilinmeyen modelde maliyet yok\n3. Ada: mini görünümde küçük hap, ayrıntıda 7 günlük çubuklar\n\n## Dikkat\n\n- Boştayken hiçbir şey okunmaz\n- Kapalıyken hiçbir veri okunmaz\n\n```ts\nexport function cost(usage: Usage): number\n```\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Now,
    Done,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: u64,
    pub kind: Kind,
    /// Kısa hedef: dosya adı, komut, onay ayrıntısı…
    pub target: String,
    /// Yalnız tür tek başına anlatmadığında (bilinmeyen araç, onay kararı, tur özeti).
    pub label: String,
    pub at: u64,
    pub state: StepState,
    /// Alt ajanın attığı adım (ayrıntıda girintili).
    pub sub: bool,
    #[serde(skip)]
    pub tool: String,
    #[serde(skip)]
    pub raw: Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Permission,
    Question,
    /// Claude'un plan modunda sunduğu plan.
    Plan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

fn questions(input: &Value) -> Vec<Question> {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    input
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|q| !s(q, "question").is_empty())
        .map(|q| Question {
            question: s(q, "question"),
            header: s(q, "header"),
            options: q
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|o| QuestionOption { label: s(o, "label"), description: s(o, "description") })
                .filter(|o| !o.label.is_empty())
                .collect(),
            multi_select: q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

/// Her sorunun boş olmayan bir cevabı olmalı; sorulmayan anahtarlar atılır. Cevap bir seçenek
/// etiketi, çoklu seçimde virgülle ayrılmış etiketler ya da kullanıcının yazdığı metindir ("Diğer").
fn valid_answers(asked: &[Question], answers: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    let mut out = Map::new();
    for q in asked {
        let answer = answers.get(&q.question).and_then(Value::as_str).map(str::trim).filter(|a| !a.is_empty());
        let Some(answer) = answer else { return Err("Her soruya bir cevap seç.".into()) };
        out.insert(q.question.clone(), Value::String(answer.chars().take(500).collect()));
    }
    Ok(out)
}

/// Onay kartında dosya bazında "+N −M".
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    pub path: String,
    pub name: String,
    pub added: u64,
    pub removed: u64,
}

fn stat(path: &str, added: u64, removed: u64) -> FileStat {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_owned();
    FileStat { path: path.to_owned(), name, added, removed }
}

fn file_stats(stats: &Value) -> Vec<FileStat> {
    stats
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let path = f.get("path")?.as_str()?.to_owned();
            let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_owned();
            Some(FileStat { path, name, added: f.get("added")?.as_u64()?, removed: f.get("removed")?.as_u64()? })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl TodoStatus {
    fn parse(value: &str) -> Self {
        match value {
            "completed" | "done" => Self::Completed,
            "in_progress" | "active" => Self::InProgress,
            _ => Self::Pending,
        }
    }
}

/// Ajanın kendi yapılacaklar listesindeki bir iş.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub text: String,
    /// Sürerken gösterilen biçim ("Testleri yazıyor"); araç vermediyse boş.
    pub active: String,
    pub status: TodoStatus,
    /// `TaskUpdate`'in eşleştirdiği kimlik (yalnız görev araçlarında).
    #[serde(skip)]
    key: String,
}

/// Turda değişen bir dosya: satır sayıları ve son değiştiren adım (fark için).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub path: String,
    pub name: String,
    pub added: u64,
    pub removed: u64,
    pub step: u64,
}

/// Kullanıcının ajana sıraya koyduğu not. `now`: bir sonraki araç çağrısında teslim edilir.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: u64,
    pub text: String,
    pub now: bool,
    pub at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    WaitingInput,
    WaitingApproval,
    Done,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub tool: &'static str,
    /// Görünen ad: sohbetin başlığı ("Naber"); yoksa proje/klasör adı.
    pub project: String,
    /// Kategori: proje (manifest) ya da klasör adı ("Ödev"); ayrıntıda "Ödev kategorisi" çipi.
    pub category: String,
    pub status: Status,
    pub step: String,
    pub started_at: u64,
    /// Bu oturumdan gelen son olay (Unix ms); arayüz sessiz kalan "çalışıyor"u buna göre düzeltir.
    pub last_seen: u64,
    pub demo: bool,
    /// Son adımlar, eskiden yeniye.
    pub steps: Vec<Step>,
    /// Oturumun çalıştığı pencerenin uygulaması (ör. "WindowsTerminal.exe"); doluysa "Terminale git" var.
    pub app: Option<String>,
    /// Köprünün bulduğu pencere tutamacı.
    #[serde(skip)]
    pub window: Option<u64>,
    /// Geçerli durumun başladığı an (bekleme hatırlatması: "12 dk'dır yanıt bekliyor").
    pub status_since: u64,
    /// Son turun başladığı an (kullanıcının isteği); hiç tur yoksa `None`.
    pub turn_started_at: Option<u64>,
    /// Son turun bittiği an; tur sürerken `None`.
    pub turn_ended_at: Option<u64>,
    /// Bu turda değişen farklı dosya sayısı (başarıyla biten düzenleme/yazma/yama).
    pub turn_files: usize,
    /// Bu turda değişen dosyalar, ilk değişene göre sırayla.
    pub changed: Vec<Changed>,
    /// Ajanın yapılacaklar listesi (araç her güncellemede tamamını ya da tek işi gönderir).
    pub todos: Vec<Todo>,
    /// Son turun sonunda ajanın yazdığı cevap (köprü sınırıyla kırpılmış).
    pub last_reply: Option<String>,
    /// Aracın izin kipi: "default", "plan", "acceptEdits", "bypassPermissions"…
    pub mode: Option<String>,
    /// Oturumun çalıştığı klasör ("Klasör ↗", "VS Code ↗").
    pub folder: Option<String>,
    /// Şu an çalışan alt ajan sayısı.
    pub subagents: u32,
    /// Sıradaki notlar (eskiden yeniye).
    pub notes: Vec<Note>,
    /// Ajana not gönderilebilir: Claude Code, adadan onay modu, notu okuyabilen köprü.
    pub can_note: bool,
    /// Boştaki oturuma adadan görev gönderilebilir (tur sonunda bekleyen köprü var; anlık görüntüde dolar).
    pub can_task: bool,
    /// Zodyvel'in "+ Yeni görev" ile başlattığı oturumun numarası (Zody'deki rozet); anlık görüntüde dolar.
    pub task_no: Option<u32>,
    /// Raftan bırakılan dosya bu oturuma gerçekten eklenir: Zodyvel başlattı ve süreci açık (girdisine
    /// yazılır). Değilse yol panoya kopyalanır, pencere öne gelir (K5). Anlık görüntüde dolar.
    pub attach: bool,
    #[serde(skip)]
    task_seq: u32,
    /// Claude masaüstünde Remote Control / proje dizisinden açılmış oturum: yerel istemde kimse
    /// beklemez, boşta kalınca "yanıt bekliyor" yerine bitmiş sayılır.
    #[serde(skip)]
    remote: bool,
}

impl Session {
    fn new(id: String, tool: &'static str, project: String, status: Status, step: &str, now: u64, demo: bool) -> Self {
        Self {
            id,
            tool,
            category: project.clone(),
            project,
            status,
            step: step.into(),
            started_at: now,
            last_seen: now,
            demo,
            steps: Vec::new(),
            app: None,
            window: None,
            status_since: now,
            turn_started_at: None,
            turn_ended_at: None,
            turn_files: 0,
            changed: Vec::new(),
            todos: Vec::new(),
            last_reply: None,
            mode: None,
            folder: None,
            subagents: 0,
            notes: Vec::new(),
            can_note: false,
            can_task: false,
            task_no: None,
            attach: false,
            task_seq: 0,
            remote: false,
        }
    }

    /// Başarıyla biten bir adımın değiştirdiği dosyalar turun listesine eklenir. `stats`: köprünün tam
    /// metinden saydığı satırlar; yoksa (eski köprü) kırpılmış girdiden yaklaşık sayılır.
    fn count_files(&mut self, raw: &Raw, stats: Vec<FileStat>, step: u64) {
        let mut stats = stats;
        if stats.is_empty() {
            stats = match raw {
                Raw::Edit { path, old, new } if !path.is_empty() => {
                    let (added, removed) = zodyvel_hook::diff::count(old, new);
                    vec![stat(path, added as u64, removed as u64)]
                }
                Raw::Write { path, content } if !path.is_empty() => vec![stat(path, content.lines().count() as u64, 0)],
                Raw::Patch { text } => file_stats(&zodyvel_hook::diff::edit_stats("apply_patch", &serde_json::json!({ "input": text })).unwrap_or_default()),
                _ => return,
            };
        }
        for file in stats {
            if let Some(entry) = self.changed.iter_mut().find(|c| c.path.eq_ignore_ascii_case(&file.path)) {
                entry.added += file.added;
                entry.removed += file.removed;
                entry.step = step;
            } else if self.changed.len() < MAX_CHANGED {
                self.changed.push(Changed { path: file.path, name: file.name, added: file.added, removed: file.removed, step });
            }
        }
        self.turn_files = self.changed.len();
    }

    /// Ajanın yapılacaklar listesi: `TodoWrite` ve `update_plan` tamamını, görev araçları tek işi gönderir.
    fn apply_todos(&mut self, tool: &str, input: &Value) {
        let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_owned();
        let list = |key: &str, text: &str| -> Vec<Todo> {
            input
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|t| !s(t, text).is_empty())
                .take(MAX_TODOS)
                .map(|t| Todo { text: s(t, text), active: s(t, "activeForm"), status: TodoStatus::parse(&s(t, "status")), key: String::new() })
                .collect()
        };
        match tool {
            "TodoWrite" => self.todos = list("todos", "content"),
            "update_plan" => self.todos = list("plan", "step"),
            "TaskCreate" => {
                self.task_seq += 1;
                let text = s(input, "subject");
                if !text.is_empty() && self.todos.len() < MAX_TODOS {
                    self.todos.push(Todo { text, active: s(input, "activeForm"), status: TodoStatus::Pending, key: self.task_seq.to_string() });
                }
            }
            "TaskUpdate" => {
                let key = match input.get("taskId") {
                    Some(Value::String(k)) => k.trim().to_owned(),
                    Some(Value::Number(n)) => n.to_string(),
                    _ => return,
                };
                let status = s(input, "status");
                if status == "deleted" {
                    self.todos.retain(|t| t.key != key);
                    return;
                }
                if let Some(todo) = self.todos.iter_mut().find(|t| t.key == key) {
                    if !status.is_empty() {
                        todo.status = TodoStatus::parse(&status);
                    }
                    let subject = s(input, "subject");
                    if !subject.is_empty() {
                        todo.text = subject;
                    }
                    let active = s(input, "activeForm");
                    if !active.is_empty() {
                        todo.active = active;
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalView {
    pub id: String,
    pub session_id: String,
    pub tool: &'static str,
    pub project: String,
    pub kind: RequestKind,
    pub action: String,
    pub detail: String,
    /// Yalnız soru isteklerinde dolu.
    pub questions: Vec<Question>,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub actionable: bool,
    /// Araç, adadaki süre dolmadan beklemeyi bıraktı (ör. eski kayıtta kanca süresi 5 sn).
    pub closed_early: bool,
    /// Adadan hiç cevaplanamaz (yalnız izle modu, Codex sorusu, araç bildirimi); süre dolmuş değil.
    pub native_only: bool,
    /// Dosya değiştiren isteklerde dosya bazında satır sayıları.
    pub files: Vec<FileStat>,
    /// "Her zaman izin ver" seçeneğinin açıklaması; yoksa seçenek gösterilmez.
    pub always: Option<String>,
    /// Yalnız plan isteklerinde: planın Markdown metni.
    pub plan: Option<String>,
    pub demo: bool,
}

struct Approval {
    view: ApprovalView,
    tool_name: String,
    input_key: String,
    sender: Option<SyncSender<Option<Decision>>>,
    /// Aracın `permission_suggestions` dizisinde kalıcı izin önerisinin sırası.
    always_index: Option<usize>,
    /// Tam fark ekranı için isteğin ham verisi.
    raw: Raw,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastEvents {
    pub claude: Option<u64>,
    pub codex: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub agents: Vec<Session>,
    pub approvals: Vec<ApprovalView>,
    pub last_events: LastEvents,
}

/// Canlı bir onay isteği için köprü bağlantısının beklediği kanal.
pub struct Pending {
    pub id: String,
    pub receiver: Receiver<Option<Decision>>,
    /// Adada kararın beklendiği süre (sn).
    pub timeout_secs: u64,
}

#[derive(Default)]
pub struct State {
    sessions: Vec<Session>,
    approvals: Vec<Approval>,
    last: LastEvents,
    counter: u64,
    /// Plan incelemesi kapalı (Ayarlar → Özellikler): plan kartı açılmaz, karar araçta kalır.
    pub plan_off: bool,
    /// Tur sonunda adadan görev bekleyen köprüler (oturum başına en çok bir).
    waiters: Vec<Waiter>,
    /// Zodyvel'in kendisinin başlattığı (yeni görev ya da `--resume`) arka plan oturumları.
    launched: Vec<Launched>,
}

/// Zodyvel'in başlattığı oturum: terminali yoktur; görev girdisine doğrudan yazılır (`crate::tasks`).
struct Launched {
    session_id: String,
    no: Option<u32>,
    /// Süreç çalışıyor ve girdisi açık.
    live: bool,
}

/// Aklında tutulan en çok başlatılmış oturum (numara rozeti için; eskiler düşer).
const MAX_LAUNCHED: usize = 32;

struct Waiter {
    session_id: String,
    id: String,
    sender: SyncSender<Option<Decision>>,
}

fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// Oturumun adı: köprü dışında okunan ad (`meta`: masaüstü oturum başlığı ya da proje adı), yoksa klasör.
fn session_name(payload: &Value, cwd: &str) -> Option<String> {
    let name = text(payload, "zodyvel_name").map(str::trim).filter(|n| !n.is_empty());
    name.map(str::to_owned).or_else(|| (!cwd.is_empty()).then(|| describe::project(cwd)))
}

impl State {
    pub fn snapshot(&self) -> Snapshot {
        let mut agents = self.sessions.clone();
        for agent in &mut agents {
            let launched = self.launched.iter().find(|l| l.session_id == agent.id);
            let direct = launched.is_some_and(|l| l.live) && !matches!(agent.status, Status::Running | Status::WaitingApproval);
            agent.can_task = direct || self.waiters.iter().any(|w| w.session_id == agent.id);
            agent.task_no = launched.and_then(|l| l.no);
            agent.attach = launched.is_some_and(|l| l.live);
        }
        // Aynı klasördeki oturumlar ayırt edilsin: başlama sırasına göre "Psrc", "Psrc 2", "Psrc 3".
        agents.sort_by_key(|s| s.started_at);
        let mut seen: Vec<String> = Vec::new();
        for agent in &mut agents {
            seen.push(agent.project.clone());
            let n = seen.iter().filter(|p| **p == agent.project).count();
            if n > 1 {
                agent.project = format!("{} {n}", agent.project);
            }
        }
        let approvals = self
            .approvals
            .iter()
            .map(|a| {
                let mut view = a.view.clone();
                if let Some(agent) = agents.iter().find(|s| s.id == view.session_id) {
                    view.project = agent.project.clone();
                }
                view
            })
            .collect();
        agents.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
        Snapshot { agents, approvals, last_events: self.last.clone() }
    }

    /// Gecikmeli okunan sohbet başlığı (Faz 7.1). Oturum yoksa ya da ad aynıysa false.
    pub fn rename(&mut self, id: &str, name: &str) -> bool {
        match self.sessions.iter_mut().find(|s| s.id == id) {
            Some(session) if session.project != name => {
                session.project = name.to_owned();
                true
            }
            _ => false,
        }
    }

    fn next_id(&mut self, prefix: &str, now: u64) -> String {
        self.counter += 1;
        format!("{prefix}-{now}-{}", self.counter)
    }

    /// Bir hook olayını işler. Canlı onay isteğinde köprünün bekleyeceği kanalı döndürür.
    pub fn ingest(&mut self, provider: Provider, mode: Mode, payload: &Value, now: u64) -> Option<Pending> {
        let event = text(payload, "hook_event_name")?;
        let source = text(payload, "session_id").filter(|s| !s.is_empty())?;
        match provider {
            Provider::Claude => self.last.claude = Some(now),
            Provider::Codex => self.last.codex = Some(now),
        }
        self.prune(now);
        let id = format!("{}:{}", provider.as_str(), source);

        if payload.get("zodyvel_await").and_then(Value::as_bool) == Some(true) {
            return self.await_task(&id, provider, mode, event, now);
        }
        if event == "SessionEnd" {
            self.cancel_for(&id);
            self.waiters.retain(|w| w.session_id != id);
            self.sessions.retain(|s| s.id != id);
            return None;
        }

        let cwd = text(payload, "cwd").unwrap_or("");
        let name = session_name(payload, cwd);
        let category = text(payload, "zodyvel_category")
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
            .or_else(|| (!cwd.is_empty()).then(|| describe::project(cwd)));
        // Zodyvel'in başlattığı oturumun da terminali yok: boşta kalınca bitmiş sayılır.
        let remote = payload.get("zodyvel_remote").and_then(Value::as_bool).unwrap_or(false)
            || self.sessions.iter().any(|s| s.id == id && s.remote)
            || self.launched.iter().any(|l| l.session_id == id);
        // Boşta duran oturum: yerelde istemde yeni isteğini bekler; uzaktan oturumda beklenen bir şey yok.
        let idle = if remote { Status::Done } else { Status::WaitingInput };
        if !self.sessions.iter().any(|s| s.id == id) {
            let project = name.clone().unwrap_or_else(|| describe::project(cwd));
            self.sessions.push(Session::new(id.clone(), provider.as_str(), project, idle, "Oturum başladı", now, false));
        }
        // Köprü aracın penceresini bulduysa "Terminale git" için saklanır.
        let window = payload.get("window").and_then(|w| Some((w.get("hwnd")?.as_u64()?, text(w, "app").unwrap_or("").to_owned())));
        if let (Some((hwnd, app)), Some(session)) = (window, self.sessions.iter_mut().find(|s| s.id == id)) {
            session.window = Some(hwnd);
            session.app = Some(app);
        }
        let subagent = payload.get("agent_id").and_then(Value::as_str).is_some_and(|a| !a.is_empty());
        let tool_name = text(payload, "tool_name").unwrap_or("");
        let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
        // Notu okuyabilen köprü tur sonu ve araç çağrısında bunu işaretler; eski köprü işaretlemez.
        let reads_notes = payload.get("zodyvel_notes").and_then(Value::as_bool) == Some(true);
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            if !cwd.is_empty() {
                session.folder = Some(cwd.to_owned());
            }
            if let Some(mode) = text(payload, "permission_mode").filter(|m| !m.is_empty()) {
                session.mode = Some(mode.to_owned());
            }
            let interactive = provider == Provider::Claude && mode == Mode::Interactive;
            if !interactive || ((event == "Stop" || event == "PreToolUse") && tool_name != ASK_TOOL) {
                session.can_note = interactive && reads_notes;
            }
        }
        // Sıradaki not: tur biterken her not, araç çağrısında yalnız "şimdi" notu teslim edilir.
        let note = match event {
            _ if !reads_notes => None,
            "Stop" => self.take_note(&id, false),
            "PreToolUse" if tool_name != ASK_TOOL && tool_name != CODEX_ASK_TOOL => self.take_note(&id, true),
            _ => None,
        };

        let mut pending = None;
        let mut update: Option<(Status, String)> = None;
        match event {
            // Not teslimi: Stop'ta tur bitmez, PreToolUse'ta çağrı yapılmaz; ajan notu okuyup devam eder.
            "Stop" | "PreToolUse" if note.is_some() => {
                let note = note.unwrap_or_default();
                if event == "Stop" {
                    self.cancel_for(&id);
                }
                let label = if event == "Stop" { "Notun iletildi" } else { "Araya girildi" };
                self.push_step(&id, Kind::Prompt, &describe::short(&note, 40), label, "", Raw::None, now);
                let (tx, rx) = sync_channel(1);
                let _ = tx.try_send(Some(Decision::Note(note)));
                pending = Some(Pending { id: self.next_id("note", now), receiver: rx, timeout_secs: 1 });
                update = Some((Status::Running, "Notunu okuyor".into()));
            }
            "SessionStart" => {}
            "UserPromptSubmit" => {
                self.cancel_for(&id);
                // Kullanıcı araçta yazdı: tur sonundaki görev bekleyicisi boş yanıtla kapanır.
                self.waiters.retain(|w| w.session_id != id);
                self.push_step(&id, Kind::Prompt, "", "Yeni istek", "", Raw::None, now);
                self.start_turn(&id, now);
                update = Some((Status::Running, "İsteğin üzerinde çalışıyor".into()));
            }
            "PreToolUse" => {
                // Plan araçta onaylandıysa oturum işe geçer: bekleyen plan kartı kalkar.
                self.drop_plans(&id);
                let (kind, target, raw) = describe::classify(tool_name, &input);
                self.push_step(&id, kind, &target, "", tool_name, raw, now);
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.apply_todos(tool_name, &input);
                    if let Some(step) = session.steps.last_mut().filter(|_| subagent) {
                        step.sub = true;
                    }
                }
                if tool_name == ASK_TOOL || tool_name == CODEX_ASK_TOOL {
                    let asked = questions(&input);
                    let live = mode == Mode::Interactive && !asked.is_empty() && tool_name == ASK_TOOL;
                    let (sender, receiver) = if live {
                        let (tx, rx) = sync_channel(1);
                        (Some(tx), Some(rx))
                    } else {
                        (None, None)
                    };
                    let request_id = self.next_id("ask", now);
                    let action = format!("{} soru soruyor", asked.len().max(1));
                    let detail = asked.first().map(|q| describe::short(&q.question, 160)).unwrap_or_default();
                    self.push_request(&id, provider, RequestKind::Question, action, detail, asked, live, now, request_id.clone(), sender);
                    if let Some(approval) = self.approvals.last_mut() {
                        approval.tool_name = tool_name.to_owned();
                    }
                    pending = receiver.map(|receiver| Pending { id: request_id, receiver, timeout_secs: zodyvel_hook::DECISION_TIMEOUT_SECS });
                    update = Some((Status::WaitingApproval, "Sorularını bekliyor".into()));
                } else {
                    update = Some((Status::Running, describe::step(tool_name, &input)));
                }
            }
            "PostToolUse" => {
                let output = text(payload, "tool_output").map(str::to_owned);
                let stats = payload.get("diff_stat").map(file_stats).unwrap_or_default();
                self.finish_step(&id, tool_name, output, StepState::Done, stats);
                self.resolve_matching(&id, tool_name, &input);
                if !self.approvals.iter().any(|a| a.view.session_id == id) {
                    update = Some((Status::Running, self.session(&id).map(|s| s.step.clone()).unwrap_or_default()));
                }
            }
            "PostToolUseFailure" => {
                self.finish_step(&id, tool_name, None, StepState::Failed, Vec::new());
                self.resolve_matching(&id, tool_name, &input);
            }
            // Soru adadan cevaplanmadıysa araç kendi penceresinde sorar; burada beklenecek karar yok.
            "PermissionRequest" if tool_name == ASK_TOOL => {
                update = Some((Status::WaitingInput, "Soruyu araçta cevaplaman bekleniyor".into()));
            }
            "PermissionRequest" if tool_name == PLAN_TOOL => {
                let plan = text(payload, "plan").or_else(|| input.get("plan").and_then(Value::as_str)).unwrap_or("").to_owned();
                let title = describe::plan_title(&plan);
                // Araç planı sunarken PreToolUse da gönderdiyse aynı adım kullanılır.
                let presented = self.sessions.iter_mut().find(|s| s.id == id).and_then(|s| s.steps.last_mut()).filter(|st| st.tool == PLAN_TOOL && st.state == StepState::Now);
                match presented {
                    Some(step) => (step.target, step.label) = (title.clone(), "Plan sundu".into()),
                    None => self.push_step(&id, Kind::Plan, &title, "Plan sundu", PLAN_TOOL, Raw::None, now),
                }
                if self.plan_off {
                    update = Some((Status::WaitingInput, "Planını araçta onaylaman bekleniyor".into()));
                } else {
                    let request_id = self.next_id("plan", now);
                    let live = mode == Mode::Interactive;
                    let (sender, receiver) = if live {
                        let (tx, rx) = sync_channel(1);
                        (Some(tx), Some(rx))
                    } else {
                        (None, None)
                    };
                    self.push_request(&id, provider, RequestKind::Plan, "Planını sundu".into(), title, Vec::new(), live, now, request_id.clone(), sender);
                    let always_index = (provider == Provider::Claude).then(|| zodyvel_hook::always_choice(payload)).flatten();
                    let always = always_index.and_then(|i| payload.get("permission_suggestions")?.get(i)).and_then(describe::always_label);
                    if let Some(approval) = self.approvals.last_mut() {
                        approval.tool_name = PLAN_TOOL.to_owned();
                        approval.always_index = always.as_ref().and(always_index);
                        approval.view.always = always;
                        approval.view.plan = Some(plan);
                    }
                    pending = receiver.map(|receiver| Pending { id: request_id, receiver, timeout_secs: zodyvel_hook::PLAN_TIMEOUT_SECS });
                    update = Some((Status::WaitingApproval, "Planını onaylaman bekleniyor".into()));
                }
            }
            "PermissionRequest" => {
                let (action, detail) = describe::approval(tool_name, &input);
                let request_id = self.next_id("req", now);
                let live = mode == Mode::Interactive;
                let (sender, receiver) = if live {
                    let (tx, rx) = sync_channel(1);
                    (Some(tx), Some(rx))
                } else {
                    (None, None)
                };
                self.push_step(&id, Kind::Approval, &describe::short(&detail, 40), "Onay istedi", "", Raw::None, now);
                let (tool, key) = (tool_name.to_owned(), input.to_string());
                self.push_request(&id, provider, RequestKind::Permission, action, detail, Vec::new(), live, now, request_id.clone(), sender);
                // Kalıcı izin yalnız Claude Code'da (Codex bu alanı henüz desteklemiyor).
                let always_index = (provider == Provider::Claude).then(|| zodyvel_hook::always_choice(payload)).flatten();
                let always = always_index.and_then(|i| payload.get("permission_suggestions")?.get(i)).and_then(describe::always_label);
                // Yeni köprü sayıları tam metinden gönderir; eski köprüde kırpılmış metinden hesaplanır
                // ("Write" için dosya okunmaz).
                let stats = payload.get("diff_stat").cloned().or_else(|| {
                    (tool_name != "Write").then(|| zodyvel_hook::diff::edit_stats(tool_name, &input)).flatten()
                });
                let files = stats.as_ref().map(file_stats).unwrap_or_default();
                let raw = if files.is_empty() { Raw::None } else { describe::classify(tool_name, &input).2 };
                if let Some(approval) = self.approvals.last_mut() {
                    approval.tool_name = tool;
                    approval.input_key = key;
                    approval.always_index = always.as_ref().and(always_index);
                    approval.raw = raw;
                    approval.view.always = always;
                    approval.view.files = files;
                }
                pending = receiver.map(|receiver| Pending { id: request_id, receiver, timeout_secs: zodyvel_hook::DECISION_TIMEOUT_SECS });
                update = Some((Status::WaitingApproval, "Onayını bekliyor".into()));
            }
            "Notification" => match text(payload, "notification_type") {
                Some("permission_prompt") => {
                    if !self.approvals.iter().any(|a| a.view.session_id == id) {
                        let detail = text(payload, "message").map(|m| describe::short(m, 160)).unwrap_or_default();
                        let request_id = self.next_id("note", now);
                        self.push_request(&id, provider, RequestKind::Permission, "İzin istiyor".into(), detail, Vec::new(), false, now, request_id, None);
                    }
                    update = Some((Status::WaitingApproval, "Onayını bekliyor".into()));
                }
                // Claude Code, tur bittikten ~1 dk sonra istem boşta kaldığında da bunu gönderir: bu bir soru
                // değil. Biten/bekleyen oturumun durumu değişmez; yalnız Stop gelmeden biten tur (Esc) kapanır.
                Some("idle_prompt") if self.session(&id).is_some_and(|s| s.status == Status::Running) => {
                    self.finish_step(&id, "", None, StepState::Failed, Vec::new());
                    update = Some((idle, "Durduruldu".into()));
                }
                _ => {}
            },
            "Stop" => {
                self.cancel_for(&id);
                let reply = text(payload, "last_assistant_message").map(str::trim).filter(|m| !m.is_empty());
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.last_reply = reply.map(|m| m.chars().take(MAX_REPLY).collect());
                    session.subagents = 0;
                }
                let summary = reply.map(|m| describe::short(m, 60));
                let step = summary.clone().map(|m| format!("Tur tamamlandı · {m}")).unwrap_or_else(|| "Tur tamamlandı".into());
                self.push_step(&id, Kind::Done, &summary.unwrap_or_default(), "Tur tamamlandı", "", Raw::None, now);
                self.finish_step(&id, "", None, StepState::Done, Vec::new());
                self.end_turn(&id, now);
                update = Some((Status::Done, step));
            }
            "StopFailure" => {
                self.cancel_for(&id);
                self.push_step(&id, Kind::Error, "", "Tur hatayla durdu", "", Raw::None, now);
                self.finish_step(&id, "", None, StepState::Failed, Vec::new());
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.subagents = 0;
                }
                self.end_turn(&id, now);
                update = Some((Status::Error, "Tur hatayla durdu".into()));
            }
            "SubagentStart" | "SubagentStop" => {
                let start = event == "SubagentStart";
                let mut working = false;
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    session.subagents = if start { session.subagents.saturating_add(1).min(16) } else { session.subagents.saturating_sub(1) };
                    working = session.status == Status::Running;
                }
                // Tur bittikten sonra aracın kendi arka plan ajanları (ör. özet) da bu olayları gönderir;
                // biten oturum "çalışıyor"a dönmesin, yoksa "Sonra" notu hiç gelmeyecek bir tur sonunu bekler.
                if working {
                    update = Some((Status::Running, if start { "Alt ajan çalışıyor" } else { "Alt ajan bitti" }.into()));
                }
            }
            "Interrupt" => {
                self.cancel_for(&id);
                self.finish_step(&id, "", None, StepState::Failed, Vec::new());
                update = Some((idle, "Durduruldu".into()));
            }
            _ => {}
        }

        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            session.last_seen = now;
            session.remote = remote;
            if let Some(name) = name {
                session.project = name;
            }
            if let Some(category) = category {
                session.category = category;
            }
            if let Some((status, step)) = update {
                let keep_waiting = session.status == Status::WaitingApproval && status == Status::Running && event == "PreToolUse";
                if !keep_waiting {
                    if session.status != status {
                        session.status_since = now;
                    }
                    session.status = status;
                }
                session.step = if subagent && !step.starts_with("Alt ajan") { format!("Alt ajan · {step}") } else { step };
            }
        }
        pending
    }

    /// Yeni adım: öncekinin işi bitmiştir (araçlar sırayla çalışır).
    #[allow(clippy::too_many_arguments)]
    fn push_step(&mut self, id: &str, kind: Kind, target: &str, label: &str, tool: &str, raw: Raw, now: u64) {
        self.counter += 1;
        let step_id = self.counter;
        let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) else { return };
        for step in session.steps.iter_mut().filter(|s| s.state == StepState::Now) {
            step.state = StepState::Done;
        }
        session.steps.push(Step { id: step_id, kind, target: target.to_owned(), label: label.to_owned(), at: now, state: StepState::Now, sub: false, tool: tool.to_owned(), raw });
        if session.steps.len() > MAX_STEPS {
            session.steps.remove(0);
        }
    }

    /// Sürmekte olan adımı kapatır (aynı araçsa ya da araç belirtilmemişse); komut çıktısını ekler.
    /// Başarıyla biten düzenleme turun değişen dosya sayısına eklenir.
    fn finish_step(&mut self, id: &str, tool: &str, output: Option<String>, state: StepState, stats: Vec<FileStat>) {
        let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) else { return };
        let Some(step) = session.steps.iter_mut().rev().find(|s| s.state == StepState::Now && (tool.is_empty() || s.tool == tool)) else {
            return;
        };
        step.state = state;
        if let (Raw::Bash { output: slot, .. }, Some(text)) = (&mut step.raw, output) {
            *slot = Some(text);
        }
        if state == StepState::Done {
            let (raw, step_id) = (step.raw.clone(), step.id);
            session.count_files(&raw, stats, step_id);
        }
    }

    /// Tur bitti (ya da hatayla durdu): bitiş anı "Bitti" bildirimindeki süre için saklanır.
    fn end_turn(&mut self, id: &str, now: u64) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
            session.turn_ended_at = Some(now);
        }
    }

    /// Onay ya da soru isteği kaydı.
    #[allow(clippy::too_many_arguments)]
    fn push_request(
        &mut self,
        id: &str,
        provider: Provider,
        kind: RequestKind,
        action: String,
        detail: String,
        questions: Vec<Question>,
        live: bool,
        now: u64,
        request_id: String,
        sender: Option<SyncSender<Option<Decision>>>,
    ) {
        let project = self.session(id).map(|s| s.project.clone()).unwrap_or_default();
        self.approvals.push(Approval {
            view: ApprovalView {
                id: request_id,
                session_id: id.to_owned(),
                tool: provider.as_str(),
                project,
                kind,
                action,
                detail,
                questions,
                created_at: now,
                expires_at: live.then(|| now + zodyvel_hook::decision_timeout(if kind == RequestKind::Plan { PLAN_TOOL } else { "" }) * 1000),
                actionable: live,
                closed_early: false,
                native_only: !live,
                files: Vec::new(),
                always: None,
                plan: None,
                demo: false,
            },
            tool_name: if kind == RequestKind::Question { ASK_TOOL.to_owned() } else { String::new() },
            input_key: String::new(),
            sender,
            always_index: None,
            raw: Raw::None,
        });
    }

    /// Ayrıntı ekranı için bir adımın ham verisi.
    pub fn step_raw(&self, session_id: &str, step_id: u64) -> Option<Raw> {
        self.session(session_id)?.steps.iter().find(|s| s.id == step_id).map(|s| s.raw.clone())
    }

    /// Onay isteğinin tam farkı için ham veri (yalnız dosya değiştiren isteklerde).
    pub fn request_raw(&self, approval_id: &str) -> Option<Raw> {
        self.approvals.iter().find(|a| a.view.id == approval_id).map(|a| a.raw.clone()).filter(|raw| *raw != Raw::None)
    }

    /// Oturumun çalıştığı pencere ("Terminale git").
    pub fn window(&self, session_id: &str) -> Option<u64> {
        self.session(session_id)?.window
    }

    /// Raftan dosya bırakılan oturum (K5): `Some(true)` Zodyvel'in başlattığı, süreci açık oturum (dosya
    /// girdiye eklenir), `Some(false)` diğerleri (yol panoya, pencere öne); oturum yoksa `None`.
    pub fn drop_route(&self, session_id: &str) -> Option<bool> {
        self.session(session_id)?;
        Some(self.launched_live(session_id))
    }

    /// Ajana not: sıraya girer, tur biterken ya da (şimdi) bir sonraki araç çağrısında bir kez teslim edilir.
    pub fn add_note(&mut self, session_id: &str, text: &str, now_mode: bool, at: u64) -> Result<(), String> {
        let text: String = text.trim().chars().take(zodyvel_hook::MAX_NOTE).collect();
        if text.is_empty() {
            return Err("Notu yaz.".into());
        }
        self.counter += 1;
        let id = self.counter;
        let session = self.sessions.iter_mut().find(|s| s.id == session_id).ok_or("Bu oturum artık yok.")?;
        if !session.can_note {
            return Err("Bu oturuma not gönderilemiyor (yalnız Claude Code, adadan onay modunda).".into());
        }
        if session.notes.len() >= MAX_NOTES {
            return Err("Sırada çok not var; önce biri iletilsin.".into());
        }
        session.notes.push(Note { id, text, now: now_mode, at });
        Ok(())
    }

    pub fn remove_note(&mut self, session_id: &str, note_id: u64) -> Result<(), String> {
        let session = self.sessions.iter_mut().find(|s| s.id == session_id).ok_or("Bu oturum artık yok.")?;
        let before = session.notes.len();
        session.notes.retain(|n| n.id != note_id);
        if session.notes.len() == before { Err("Bu not zaten iletildi.".into()) } else { Ok(()) }
    }

    /// Tur sonunda arka planda görev bekleyen köprü (yalnız Claude Code, adadan onay modu). Oturumun
    /// önceki bekleyicisi kalkar (göndereni düşer, eski köprü boş yanıtla çıkar).
    fn await_task(&mut self, id: &str, provider: Provider, mode: Mode, event: &str, now: u64) -> Option<Pending> {
        if provider != Provider::Claude || mode != Mode::Interactive || event != "Stop" || self.session(id).is_none() {
            return None;
        }
        let (sender, receiver) = sync_channel(1);
        let waiter = self.next_id("task", now);
        self.waiters.retain(|w| w.session_id != id);
        self.waiters.push(Waiter { session_id: id.to_owned(), id: waiter.clone(), sender });
        Some(Pending { id: waiter, receiver, timeout_secs: zodyvel_hook::AWAIT_TIMEOUT_SECS })
    }

    /// Görev metni ve oturumun görev alabilir olduğu denetlenir.
    fn task_text(&self, session_id: &str, text: &str) -> Result<String, String> {
        let text: String = text.trim().chars().take(zodyvel_hook::MAX_NOTE).collect();
        if text.is_empty() {
            return Err("Görevi yaz.".into());
        }
        let session = self.session(session_id).ok_or("Bu oturum artık yok.")?;
        if matches!(session.status, Status::Running | Status::WaitingApproval) {
            return Err("Oturum şu an çalışıyor; not olarak gönder.".into());
        }
        Ok(text)
    }

    /// Zodyvel bir oturumu başlattı (`no`: yeni görevin numarası; `--resume`'da yok).
    pub fn launch(&mut self, session_id: &str, no: Option<u32>) {
        self.launched.retain(|l| l.session_id != session_id);
        self.launched.push(Launched { session_id: session_id.to_owned(), no, live: true });
        if self.launched.len() > MAX_LAUNCHED {
            self.launched.remove(0);
        }
    }

    /// Başlatılan sürecin girdisi kapandı (süreç bitti); numara rozeti kalır.
    pub fn launch_ended(&mut self, session_id: &str) {
        if let Some(l) = self.launched.iter_mut().find(|l| l.session_id == session_id) {
            l.live = false;
        }
    }

    /// Görev durduruldu (süreç kapatıldı): tur biter, durum "Durduruldu" olur. Bitti bildirimi çıkmasın diye
    /// yalnız çalışan oturumda değişir.
    pub fn stopped(&mut self, session_id: &str, now: u64) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id && s.status == Status::Running) {
            session.status = Status::Done;
            session.status_since = now;
            session.step = "Durduruldu".into();
            session.last_seen = now;
            session.turn_ended_at = Some(now);
        }
    }

    /// Oturum Zodyvel'in başlattığı, girdisi açık bir süreç mi?
    pub fn launched_live(&self, session_id: &str) -> bool {
        self.launched.iter().any(|l| l.session_id == session_id && l.live)
    }

    /// Zodyvel'in başlattığı oturuma görev: köprü beklemez; dönen metni çağıran sürecin girdisine yazar.
    /// Adım, araç istemi alınca gelen `UserPromptSubmit` ile eklenir; burada yalnız durum çalışıyora geçer.
    pub fn direct_task(&mut self, session_id: &str, text: &str, now: u64) -> Result<String, String> {
        let text = self.task_text(session_id, text)?;
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.status = Status::Running;
            session.status_since = now;
            session.step = "Görevini okuyor".into();
            session.last_seen = now;
        }
        Ok(text)
    }

    /// Boştaki oturuma adadan görev: bekleyen köprü görevi alıp Claude'u uyandırır, yeni tur başlar.
    pub fn send_task(&mut self, session_id: &str, text: &str, now: u64) -> Result<(), String> {
        let text = self.task_text(session_id, text)?;
        let index = self
            .waiters
            .iter()
            .position(|w| w.session_id == session_id)
            .ok_or("Bu oturum adadan görev almıyor (Ayarlar → Bağlantılar → Claude → Güncelle).")?;
        let waiter = self.waiters.remove(index);
        if waiter.sender.try_send(Some(Decision::Note(text.clone()))).is_err() {
            return Err("Görev iletilemedi; oturum kapanmış olabilir.".into());
        }
        self.begin_task(session_id, &text, now);
        Ok(())
    }

    fn begin_task(&mut self, session_id: &str, text: &str, now: u64) {
        self.push_step(session_id, Kind::Prompt, &describe::short(text, 40), "Adadan yeni istek", "", Raw::None, now);
        self.start_turn(session_id, now);
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.status = Status::Running;
            session.status_since = now;
            session.step = "Görevini okuyor".into();
            session.last_seen = now;
        }
    }

    fn start_turn(&mut self, session_id: &str, now: u64) {
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.turn_started_at = Some(now);
            session.turn_ended_at = None;
            session.changed.clear();
            session.turn_files = 0;
        }
    }

    /// Teslim edilecek ilk not (sırayla); `only_now`: yalnız "şimdi" notları.
    fn take_note(&mut self, session_id: &str, only_now: bool) -> Option<String> {
        let session = self.sessions.iter_mut().find(|s| s.id == session_id)?;
        let index = session.notes.iter().position(|n| n.now || !only_now)?;
        Some(session.notes.remove(index).text)
    }

    /// Oturumun klasörü ("Klasör ↗", "VS Code ↗").
    pub fn folder(&self, session_id: &str) -> Option<String> {
        self.session(session_id)?.folder.clone()
    }

    fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Oturum ilerlediyse (araçta karar verildi, tur bitti…) bekleyen istekler geçersizdir.
    fn cancel_for(&mut self, session_id: &str) {
        for approval in self.approvals.iter_mut().filter(|a| a.view.session_id == session_id) {
            if let Some(sender) = approval.sender.take() {
                let _ = sender.try_send(None);
            }
        }
        self.approvals.retain(|a| a.view.session_id != session_id);
    }

    /// Oturum yeni bir araç adımına geçti: plan araçta karara bağlandı, plan kartı kalkar.
    fn drop_plans(&mut self, session_id: &str) {
        let plan = |a: &Approval| a.view.session_id == session_id && a.view.kind == RequestKind::Plan;
        for approval in self.approvals.iter_mut().filter(|a| plan(a)) {
            if let Some(sender) = approval.sender.take() {
                let _ = sender.try_send(None);
            }
        }
        self.approvals.retain(|a| !plan(a));
    }

    /// Araç adımı tamamladı: o adımın isteği (onay ya da soru) artık beklemiyor.
    fn resolve_matching(&mut self, session_id: &str, tool_name: &str, input: &Value) {
        let key = input.to_string();
        // Soru isteğinin girdisi cevaplarla değişir; yalnız araç adına bakılır.
        let matches = |a: &Approval| {
            a.view.session_id == session_id
                && (a.tool_name.is_empty() || (a.tool_name == tool_name && (a.input_key.is_empty() || a.input_key == key)))
        };
        for approval in self.approvals.iter_mut().filter(|a| matches(a)) {
            if let Some(sender) = approval.sender.take() {
                let _ = sender.try_send(None);
            }
        }
        self.approvals.retain(|a| !matches(a));
    }

    /// Adadan verilen karar. Yalnız canlı ve hâlâ bekleyen isteklere bir kez uygulanır.
    /// Onaya Onayla/Reddet, soruya yalnız sorulan her sorunun cevabı geçer.
    pub fn decide(&mut self, approval_id: &str, decision: Decision) -> Result<(), String> {
        let index = self.approvals.iter().position(|a| a.view.id == approval_id).ok_or("Bu istek artık geçerli değil.")?;
        let approval = &mut self.approvals[index];
        if !approval.view.actionable {
            return Err("Bu isteğe ilgili araçta karar verilmeli.".into());
        }
        let decision = match (approval.view.kind, decision) {
            (RequestKind::Question, Decision::Answer(answers)) => Decision::Answer(valid_answers(&approval.view.questions, &answers)?),
            (RequestKind::Question, _) => return Err("Bu soru seçeneklerle cevaplanmalı.".into()),
            (_, Decision::Answer(_)) => return Err("Bu istek bir soru değil.".into()),
            (_, Decision::Note(_)) => return Err("Not bu yoldan iletilmez.".into()),
            (RequestKind::Permission, Decision::Revise(_)) => return Err("Bu isteğe not yazılamaz.".into()),
            (RequestKind::Plan, Decision::Revise(note)) => {
                let note: String = note.trim().chars().take(2000).collect();
                if note.is_empty() {
                    return Err("Neyin değişmesini istediğini yaz.".into());
                }
                Decision::Revise(note)
            }
            // Sıra adadan değil, isteğin kendi kaydından gelir.
            (_, Decision::AllowAlways(_)) => Decision::AllowAlways(approval.always_index.ok_or("Bu istek için kalıcı izin seçeneği yok.")?),
            (_, other) => other,
        };
        let plan = approval.view.kind == RequestKind::Plan;
        let label = match &decision {
            Decision::Allow if plan => "Plan onaylandı",
            Decision::AllowAlways(_) if plan => "Plan onaylandı · düzenlemeler sorulmayacak",
            Decision::Deny if plan => "Plan reddedildi",
            Decision::Revise(_) => "Değişiklik istendi",
            Decision::Allow => "Onaylandı",
            Decision::AllowAlways(_) => "Her zaman izin verildi",
            Decision::Deny => "Reddedildi",
            Decision::Answer(_) => "Cevaplandı",
            Decision::Note(_) => "Not iletildi",
        };
        if !approval.view.demo {
            let sender = approval.sender.take().ok_or("Bu istek artık geçerli değil.")?;
            if sender.try_send(Some(decision)).is_err() {
                self.approvals.remove(index);
                return Err("Araç bu isteği artık beklemiyor.".into());
            }
        }
        let approval = self.approvals.remove(index);
        self.settle(&approval.view, label, Status::Running);
        Ok(())
    }

    /// "Araçta cevapla": istek aracın kendi penceresine bırakılır (köprü hiçbir şey yazmaz).
    pub fn pass(&mut self, approval_id: &str) -> Result<(), String> {
        let index = self.approvals.iter().position(|a| a.view.id == approval_id).ok_or("Bu istek artık geçerli değil.")?;
        let approval = self.approvals.remove(index);
        if let Some(sender) = approval.sender {
            let _ = sender.try_send(None);
        }
        self.settle(&approval.view, "Araçta cevaplanacak", Status::WaitingInput);
        Ok(())
    }

    /// AI modülü kapandı: bekleyen her istek ve görev bekleyicisi araca bırakılır (araç kendi onay akışında
    /// devam eder, adada cevapsız kart kalmaz), oturumlar bellekten silinir. Zodyvel'in başlattığı süreçlerin
    /// kaydı (`launched`) kalır; süreçler modülden bağımsız sürer.
    pub fn release_all(&mut self) {
        for approval in self.approvals.drain(..) {
            if let Some(sender) = approval.sender {
                let _ = sender.try_send(None);
            }
        }
        for waiter in self.waiters.drain(..) {
            let _ = waiter.sender.try_send(None);
        }
        self.sessions.clear();
    }

    /// Kapanan isteğin adımını ve oturum durumunu günceller.
    fn settle(&mut self, view: &ApprovalView, label: &str, status: Status) {
        let Some(session) = self.sessions.iter_mut().find(|s| s.id == view.session_id) else { return };
        session.status = status;
        let approved = matches!(label, "Onaylandı" | "Her zaman izin verildi" | "Plan onaylandı" | "Plan onaylandı · düzenlemeler sorulmayacak");
        session.step = match (view.kind, label) {
            (RequestKind::Plan, _) if approved => "Plan onaylandı · uyguluyor".into(),
            (RequestKind::Plan, "Plan reddedildi" | "Değişiklik istendi") => "Planı yeniden düşünüyor".into(),
            (RequestKind::Plan, _) => "Planını araçta onaylaman bekleniyor".into(),
            (RequestKind::Permission, "Onaylandı" | "Her zaman izin verildi") => format!("{label} · devam ediyor"),
            (RequestKind::Question, "Cevaplandı") => "Sorular cevaplandı · devam ediyor".into(),
            (RequestKind::Question, _) => "Soruyu araçta cevaplaman bekleniyor".into(),
            _ => label.to_owned(),
        };
        let kind = match view.kind {
            RequestKind::Question => Kind::Question,
            RequestKind::Permission => Kind::Approval,
            RequestKind::Plan => Kind::Plan,
        };
        if let Some(step) = session.steps.iter_mut().rev().find(|s| s.kind == kind) {
            step.label = label.to_owned();
            if kind != Kind::Question {
                step.state = if approved { StepState::Done } else { StepState::Failed };
            }
        }
    }

    /// Adadaki süre doldu veya köprü bağlantısı koptu: istek araçtaki normal akışa döner.
    /// Süre dolmadan kopmuşsa (araç beklemeyi bıraktı) kart bunu ayrıca söyler.
    pub fn expire(&mut self, approval_id: &str, now: u64) {
        self.waiters.retain(|w| w.id != approval_id);
        if let Some(approval) = self.approvals.iter_mut().find(|a| a.view.id == approval_id) {
            approval.sender = None;
            approval.view.actionable = false;
            // Köprü saniyede bir denetlenir: son birkaç saniyedeki kopma "süre doldu" sayılır.
            approval.view.closed_early = approval.view.expires_at.is_some_and(|end| now + 3_000 < end);
            approval.view.expires_at = None;
            approval.view.action = format!("{} · araçta bekliyor", approval.view.action);
        }
    }

    /// Ayarlardan ve adadan denemek için açıkça etiketli örnek istek (onay, soru ya da plan).
    pub fn demo_request(&mut self, now: u64, kind: RequestKind) {
        let session_id = "demo:zodyvel".to_owned();
        if !self.sessions.iter().any(|s| s.id == session_id) {
            self.sessions.push(Session::new(session_id.clone(), "claude", "örnek-proje".into(), Status::Running, "Örnek oturum", now, true));
        }
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            if session.status != Status::WaitingApproval {
                session.status_since = now;
            }
            session.status = Status::WaitingApproval;
            session.step = match kind {
                RequestKind::Question => "Sorularını bekliyor",
                RequestKind::Permission => "Onayını bekliyor",
                RequestKind::Plan => "Planını onaylaman bekleniyor",
            }
            .into();
            session.last_seen = now;
        }
        let id = self.next_id("demo", now);
        let (action, detail, asked, plan) = match kind {
            RequestKind::Question => {
                let option = |label: &str, description: &str| QuestionOption { label: label.into(), description: description.into() };
                let asked = vec![
                    Question {
                        question: "Hangi tema kullanılsın?".into(),
                        header: "Tema".into(),
                        options: vec![option("Koyu", "Gece için"), option("Açık", "Gündüz için")],
                        multi_select: false,
                    },
                    Question {
                        question: "Hangi bölümler açık olsun?".into(),
                        header: "Bölümler".into(),
                        options: vec![option("Müzik", ""), option("Bugün", ""), option("Bilgisayar", "")],
                        multi_select: true,
                    },
                ];
                ("2 soru soruyor".to_owned(), "Hangi tema kullanılsın?".to_owned(), asked, None)
            }
            RequestKind::Permission => ("Komut çalıştırmak istiyor".to_owned(), "pnpm test".to_owned(), Vec::new(), None),
            RequestKind::Plan => ("Planını sundu".to_owned(), describe::plan_title(DEMO_PLAN), Vec::new(), Some(DEMO_PLAN.to_owned())),
        };
        let step_kind = match kind {
            RequestKind::Question => Kind::Question,
            RequestKind::Permission => Kind::Approval,
            RequestKind::Plan => Kind::Plan,
        };
        self.push_step(&session_id, step_kind, &detail, "", "", Raw::None, now);
        let timeout = zodyvel_hook::decision_timeout(if kind == RequestKind::Plan { PLAN_TOOL } else { "" });
        let always = match kind {
            RequestKind::Permission => Some("Bash(pnpm test:*) · bu projede, yalnız sende".to_owned()),
            RequestKind::Plan => Some("dosya düzenlemelerini sormadan kabul et · bu oturum boyunca".to_owned()),
            RequestKind::Question => None,
        };
        self.approvals.push(Approval {
            view: ApprovalView {
                id,
                session_id,
                tool: "claude",
                project: "örnek-proje".into(),
                kind,
                action,
                detail,
                questions: asked,
                created_at: now,
                expires_at: Some(now + timeout * 1000),
                actionable: true,
                closed_early: false,
                native_only: false,
                files: Vec::new(),
                always: always.clone(),
                plan,
                demo: true,
            },
            tool_name: String::new(),
            input_key: String::new(),
            sender: None,
            always_index: always.map(|_| 0),
            raw: Raw::None,
        });
    }

    fn prune(&mut self, now: u64) {
        // Görev bekleyen köprüsü olan oturum açıktır; köprü kopunca bekleyici zaten kalkar.
        let waiting = |s: &Session| self.waiters.iter().any(|w| w.session_id == s.id);
        let stale: Vec<String> = self
            .sessions
            .iter()
            .filter(|s| now.saturating_sub(s.last_seen) > SESSION_TTL_MS && !waiting(s))
            .map(|s| s.id.clone())
            .collect();
        for id in &stale {
            self.cancel_for(id);
        }
        self.sessions.retain(|s| !stale.contains(&s.id));
        if self.sessions.len() >= MAX_SESSIONS {
            self.sessions.sort_by_key(|s| std::cmp::Reverse(s.last_seen));
            for dropped in self.sessions.split_off(MAX_SESSIONS - 1) {
                self.cancel_for(&dropped.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(name: &str, extra: Value) -> Value {
        let mut base = json!({ "hook_event_name": name, "session_id": "s1", "cwd": "C:\\work\\api" });
        base.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
        base
    }

    #[test]
    fn same_folder_sessions_get_numbered_names() {
        let mut s = State::default();
        let start = |id: &str| json!({ "hook_event_name": "SessionStart", "session_id": id, "cwd": "C:\\work\\Psrc" });
        s.ingest(Provider::Claude, Mode::Observe, &start("a"), 1);
        s.ingest(Provider::Claude, Mode::Observe, &start("b"), 2);
        s.ingest(Provider::Claude, Mode::Observe, &json!({ "hook_event_name": "SessionStart", "session_id": "c", "cwd": "C:\\work\\api" }), 3);
        s.ingest(Provider::Claude, Mode::Observe, &json!({ "hook_event_name": "UserPromptSubmit", "session_id": "a", "cwd": "C:\\work\\Psrc" }), 4);
        let names = |s: &State| {
            let mut v: Vec<(String, String)> = s.snapshot().agents.into_iter().map(|a| (a.id, a.project)).collect();
            v.sort();
            v
        };
        assert_eq!(names(&s), [("claude:a".into(), "Psrc".into()), ("claude:b".into(), "Psrc 2".into()), ("claude:c".into(), "api".into())]);
        s.ingest(Provider::Claude, Mode::Observe, &json!({ "hook_event_name": "SessionEnd", "session_id": "a" }), 5);
        assert_eq!(names(&s), [("claude:b".into(), "Psrc".into()), ("claude:c".into(), "api".into())]);
    }

    #[test]
    fn idle_notification_after_a_turn_is_not_a_question() {
        let mut s = State::default();
        let idle = || event("Notification", json!({ "notification_type": "idle_prompt", "message": "Claude is waiting for your input" }));
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1_000);
        s.ingest(Provider::Claude, Mode::Observe, &event("Stop", json!({ "last_assistant_message": "Bitti." })), 2_000);
        s.ingest(Provider::Claude, Mode::Observe, &idle(), 62_000);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.status, agent.step.as_str(), agent.status_since), (Status::Done, "Tur tamamlandı · Bitti.", 2_000));
        assert_eq!(agent.last_seen, 62_000);
        // Stop gelmeden biten tur (Esc ile kesildi): boşta bildirimi çalışıyor durumunu kapatır.
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 70_000);
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })), 71_000);
        s.ingest(Provider::Claude, Mode::Observe, &idle(), 131_000);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.status, agent.step.as_str()), (Status::WaitingInput, "Durduruldu"));
        assert_eq!(agent.steps.last().map(|st| st.state), Some(StepState::Failed));
    }

    #[test]
    fn stopped_task_ends_quietly_as_stopped() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1);
        let id = s.snapshot().agents[0].id.clone();
        assert_eq!(s.snapshot().agents[0].status, Status::Running);
        s.stopped(&id, 5);
        let agent = &s.snapshot().agents[0];
        assert_eq!((agent.status, agent.step.as_str(), agent.status_since, agent.turn_ended_at), (Status::Done, "Durduruldu", 5, Some(5)));
        // Bitmiş oturumda ikinci çağrı bir şey değiştirmez.
        s.stopped(&id, 9);
        assert_eq!(s.snapshot().agents[0].status_since, 5);
    }

    #[test]
    fn remote_session_never_waits_at_a_local_prompt() {
        let mut s = State::default();
        let remote = |name: &str, extra: Value| {
            let mut e = event(name, extra);
            e.as_object_mut().unwrap().insert("zodyvel_remote".into(), json!(true));
            e
        };
        s.ingest(Provider::Claude, Mode::Observe, &remote("SessionStart", json!({})), 1);
        assert_eq!(s.snapshot().agents[0].status, Status::Done);
        s.ingest(Provider::Claude, Mode::Observe, &remote("UserPromptSubmit", json!({})), 2);
        assert_eq!(s.snapshot().agents[0].status, Status::Running);
        // İşaret sonraki olaylarda yoksa da oturum uzaktan sayılmaya devam eder.
        s.ingest(Provider::Claude, Mode::Observe, &event("Notification", json!({ "notification_type": "idle_prompt" })), 3);
        assert_eq!((s.snapshot().agents[0].status, s.snapshot().agents[0].step.clone()), (Status::Done, "Durduruldu".into()));
        // Araçta cevaplanacak soru gerçekten cevap bekler.
        s.ingest(Provider::Claude, Mode::Observe, &remote("UserPromptSubmit", json!({})), 4);
        s.ingest(Provider::Claude, Mode::Observe, &event("PermissionRequest", json!({ "tool_name": ASK_TOOL, "tool_input": {} })), 5);
        assert_eq!(s.snapshot().agents[0].status, Status::WaitingInput);
    }

    #[test]
    fn desktop_title_names_the_session() {
        let mut s = State::default();
        let titled = |id: &str, title: Option<&str>| {
            let mut e = json!({ "hook_event_name": "UserPromptSubmit", "session_id": id, "cwd": "C:\\work\\Psrc" });
            if let Some(title) = title {
                e.as_object_mut().unwrap().insert("zodyvel_name".into(), json!(title));
            }
            e
        };
        s.ingest(Provider::Claude, Mode::Observe, &titled("a", Some("Zodyvel")), 1);
        s.ingest(Provider::Claude, Mode::Observe, &titled("b", Some("Zodyvel")), 2);
        let mut names: Vec<String> = s.snapshot().agents.into_iter().map(|a| a.project).collect();
        names.sort();
        assert_eq!(names, ["Zodyvel", "Zodyvel 2"]);
        // Başlık ilk turdan sonra gelir: farklı başlıklı oturumlar numara almaz.
        s.ingest(Provider::Claude, Mode::Observe, &titled("a", Some("Oturum adı ve yanlış durum")), 3);
        s.ingest(Provider::Claude, Mode::Observe, &titled("b", Some("Dalga A devamı")), 4);
        let mut names: Vec<String> = s.snapshot().agents.into_iter().map(|a| a.project).collect();
        names.sort();
        assert_eq!(names, ["Dalga A devamı", "Oturum adı ve yanlış durum"]);
        // Ad gelmezse klasör adı.
        s.ingest(Provider::Claude, Mode::Observe, &titled("c", None), 5);
        assert!(s.snapshot().agents.iter().any(|a| a.id == "claude:c" && a.project == "Psrc"));
    }

    #[test]
    fn category_and_late_title() {
        let mut s = State::default();
        let mut e = json!({ "hook_event_name": "UserPromptSubmit", "session_id": "a", "cwd": "C:\\work\\Ödev" });
        s.ingest(Provider::Claude, Mode::Observe, &e, 1);
        let a = &s.snapshot().agents[0];
        assert_eq!((a.project.as_str(), a.category.as_str()), ("Ödev", "Ödev"));
        // Başlık tur bittikten sonra okunur (Faz 7.1): ad değişir, kategori klasör kalır.
        assert!(s.rename("claude:a", "Naber"));
        assert!(!s.rename("claude:a", "Naber"));
        assert!(!s.rename("claude:yok", "Naber"));
        let a = &s.snapshot().agents[0];
        assert_eq!((a.project.as_str(), a.category.as_str()), ("Naber", "Ödev"));
        // Manifest adı varsa kategori odur.
        e.as_object_mut().unwrap().insert("zodyvel_name".into(), json!("Naber"));
        e.as_object_mut().unwrap().insert("zodyvel_category".into(), json!("Zodyvel"));
        s.ingest(Provider::Claude, Mode::Observe, &e, 2);
        assert_eq!(s.snapshot().agents[0].category, "Zodyvel");
    }

    #[test]
    fn session_lifecycle() {
        let mut s = State::default();
        s.ingest(Provider::Codex, Mode::Observe, &event("SessionStart", json!({})), 1);
        let snap = s.snapshot();
        assert_eq!(snap.agents.len(), 1);
        assert_eq!(snap.agents[0].project, "api");
        assert_eq!(snap.agents[0].status, Status::WaitingInput);
        assert_eq!(snap.last_events.codex, Some(1));

        s.ingest(Provider::Codex, Mode::Observe, &event("UserPromptSubmit", json!({ "prompt": "x" })), 2);
        s.ingest(Provider::Codex, Mode::Observe, &event("PreToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })), 3);
        assert_eq!(s.snapshot().agents[0].step, "Komut · ls");
        s.ingest(Provider::Codex, Mode::Observe, &event("Stop", json!({ "last_assistant_message": "Testler geçti." })), 4);
        assert_eq!(s.snapshot().agents[0].status, Status::Done);
        assert_eq!(s.snapshot().agents[0].step, "Tur tamamlandı · Testler geçti.");
        s.ingest(Provider::Codex, Mode::Observe, &event("SessionEnd", json!({})), 5);
        assert!(s.snapshot().agents.is_empty());
    }

    #[test]
    fn interactive_request_is_decided_once() {
        let mut s = State::default();
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "rm -rf x" } }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &req, 10).expect("canlı istek");
        let view = &s.snapshot().approvals[0];
        assert!(view.actionable);
        assert_eq!(view.detail, "rm -rf x");
        assert_eq!(s.snapshot().agents[0].status, Status::WaitingApproval);

        s.decide(&pending.id, Decision::Allow).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), Some(Decision::Allow));
        assert!(s.decide(&pending.id, Decision::Deny).is_err());
        assert!(s.snapshot().approvals.is_empty());
        assert_eq!(s.snapshot().agents[0].status, Status::Running);
    }

    #[test]
    fn observe_mode_never_waits_and_cannot_be_decided() {
        let mut s = State::default();
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        assert!(s.ingest(Provider::Codex, Mode::Observe, &req, 1).is_none());
        let id = s.snapshot().approvals[0].id.clone();
        assert!(!s.snapshot().approvals[0].actionable);
        assert!(s.decide(&id, Decision::Allow).is_err());
    }

    #[test]
    fn progress_in_the_tool_cancels_the_pending_request() {
        let mut s = State::default();
        let input = json!({ "command": "npm test" });
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": input }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &req, 1).unwrap();
        // Masaüstü uygulamasında kullanıcı aracın kendi penceresinden onayladı: araç çalıştı.
        s.ingest(Provider::Claude, Mode::Interactive, &event("PostToolUse", json!({ "tool_name": "Bash", "tool_input": input })), 2);
        assert_eq!(pending.receiver.try_recv().unwrap(), None);
        assert!(s.snapshot().approvals.is_empty());
        assert!(s.decide(&pending.id, Decision::Allow).is_err());
    }

    #[test]
    fn expired_request_turns_into_info_card_without_approving() {
        let mut s = State::default();
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        let pending = s.ingest(Provider::Codex, Mode::Interactive, &req, 1).unwrap();
        s.expire(&pending.id, 5_001);
        let view = &s.snapshot().approvals[0];
        assert!(!view.actionable);
        assert!(view.closed_early, "5 sn'de kopan istek erken kapanmış sayılır");
        assert!(view.expires_at.is_none());
        assert!(s.decide(&pending.id, Decision::Allow).is_err());
        assert!(pending.receiver.try_recv().is_err());
    }

    #[test]
    fn requests_do_not_cross_sessions() {
        let mut s = State::default();
        let a = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "a" } }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &a, 1).unwrap();
        let mut other = event("UserPromptSubmit", json!({}));
        other["session_id"] = json!("s2");
        s.ingest(Provider::Claude, Mode::Interactive, &other, 2);
        // Başka oturumdaki yeni tur bu isteği iptal etmez.
        assert_eq!(s.snapshot().approvals.len(), 1);
        // Aynı oturum kimliği başka araçta ayrı bir oturumdur.
        s.ingest(Provider::Codex, Mode::Interactive, &event("UserPromptSubmit", json!({})), 3);
        assert_eq!(s.snapshot().approvals.len(), 1);
        assert_eq!(s.snapshot().agents.len(), 3);
        s.decide(&pending.id, Decision::Deny).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), Some(Decision::Deny));
    }

    #[test]
    fn native_permission_notification_creates_info_card_once() {
        let mut s = State::default();
        let note = event("Notification", json!({ "notification_type": "permission_prompt", "message": "Claude needs your permission to use Bash" }));
        s.ingest(Provider::Claude, Mode::Observe, &note, 1);
        s.ingest(Provider::Claude, Mode::Observe, &note, 2);
        let snap = s.snapshot();
        assert_eq!(snap.approvals.len(), 1);
        assert!(!snap.approvals[0].actionable);
        s.ingest(Provider::Claude, Mode::Observe, &event("PostToolUse", json!({ "tool_name": "Bash", "tool_input": {} })), 3);
        assert!(s.snapshot().approvals.is_empty());
    }

    #[test]
    fn demo_request_is_labelled_and_decidable() {
        let mut s = State::default();
        s.demo_request(1, RequestKind::Permission);
        let view = s.snapshot().approvals[0].clone();
        assert!(view.demo && view.actionable);
        s.decide(&view.id, Decision::Allow).unwrap();
        assert!(s.snapshot().approvals.is_empty());
    }

    #[test]
    fn invalid_events_are_ignored() {
        let mut s = State::default();
        assert!(s.ingest(Provider::Claude, Mode::Interactive, &json!({ "hook_event_name": "PermissionRequest" }), 1).is_none());
        assert!(s.snapshot().agents.is_empty() && s.snapshot().approvals.is_empty());
    }

    #[test]
    fn keeps_step_history_with_output() {
        let mut s = State::default();
        let pre = |tool: &str, input: Value| event("PreToolUse", json!({ "tool_name": tool, "tool_input": input }));
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Observe, &pre("Read", json!({ "file_path": "C:/p/a.ts" })), 2);
        s.ingest(Provider::Claude, Mode::Observe, &pre("Bash", json!({ "command": "pnpm test" })), 3);
        let post = event("PostToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": "pnpm test" }, "tool_output": "17 passed" }));
        s.ingest(Provider::Claude, Mode::Observe, &post, 4);
        s.ingest(Provider::Claude, Mode::Observe, &event("Stop", json!({})), 5);

        let steps = &s.snapshot().agents[0].steps;
        let kinds: Vec<Kind> = steps.iter().map(|st| st.kind).collect();
        assert_eq!(kinds, vec![Kind::Prompt, Kind::Read, Kind::Bash, Kind::Done]);
        assert!(steps.iter().all(|st| st.state == StepState::Done));
        assert_eq!(steps[1].target, "a.ts");
        let id = s.snapshot().agents[0].id.clone();
        assert_eq!(s.step_raw(&id, steps[2].id), Some(Raw::Bash { command: "pnpm test".into(), output: Some("17 passed".into()) }));
    }

    #[test]
    fn step_history_is_bounded() {
        let mut s = State::default();
        for i in 0..(MAX_STEPS as u64 + 10) {
            s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", json!({ "tool_name": "Grep", "tool_input": {} })), i + 1);
        }
        let steps = &s.snapshot().agents[0].steps;
        assert_eq!(steps.len(), MAX_STEPS);
        assert_eq!(steps.iter().filter(|st| st.state == StepState::Now).count(), 1);
    }

    fn ask() -> Value {
        event(
            "PreToolUse",
            json!({ "tool_name": ASK_TOOL, "tool_input": { "questions": [
                { "question": "Tema?", "header": "Tema", "options": [{ "label": "Koyu", "description": "gece" }, { "label": "Açık" }] },
                { "question": "Bölümler?", "header": "Bölüm", "options": [{ "label": "Müzik" }], "multiSelect": true }
            ] } }),
        )
    }

    #[test]
    fn question_is_answered_only_with_every_answer() {
        let mut s = State::default();
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &ask(), 1).expect("soru adadan cevaplanabilir");
        let view = s.snapshot().approvals[0].clone();
        assert_eq!(view.kind, RequestKind::Question);
        assert_eq!(view.questions.len(), 2);
        assert!(view.questions[1].multi_select);
        assert_eq!(view.questions[0].options[0], QuestionOption { label: "Koyu".into(), description: "gece".into() });
        assert_eq!(s.snapshot().agents[0].status, Status::WaitingApproval);

        assert!(s.decide(&view.id, Decision::Allow).is_err());
        let partial = json!({ "Tema?": "Koyu" }).as_object().cloned().unwrap();
        assert!(s.decide(&view.id, Decision::Answer(partial)).is_err());
        assert!(pending.receiver.try_recv().is_err());

        let full = json!({ "Tema?": "Koyu", "Bölümler?": "Müzik", "fazla": "x" }).as_object().cloned().unwrap();
        s.decide(&view.id, Decision::Answer(full)).unwrap();
        let sent = pending.receiver.try_recv().unwrap();
        assert_eq!(sent, Some(Decision::Answer(json!({ "Tema?": "Koyu", "Bölümler?": "Müzik" }).as_object().cloned().unwrap())));
        assert!(s.snapshot().approvals.is_empty());
        assert_eq!(s.snapshot().agents[0].status, Status::Running);
    }

    #[test]
    fn question_can_be_left_to_the_tool() {
        let mut s = State::default();
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &ask(), 1).unwrap();
        let id = s.snapshot().approvals[0].id.clone();
        s.pass(&id).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), None);
        assert!(s.snapshot().approvals.is_empty());
        assert_eq!(s.snapshot().agents[0].status, Status::WaitingInput);
        // Ardından gelen onay isteği soru için beklenmez ve kart açmaz.
        let perm = event("PermissionRequest", json!({ "tool_name": ASK_TOOL, "tool_input": {} }));
        assert!(s.ingest(Provider::Claude, Mode::Interactive, &perm, 2).is_none());
        assert!(s.snapshot().approvals.is_empty());
    }

    #[test]
    fn question_in_observe_mode_is_informational() {
        let mut s = State::default();
        assert!(s.ingest(Provider::Claude, Mode::Observe, &ask(), 1).is_none());
        let view = &s.snapshot().approvals[0];
        assert!(!view.actionable);
        // Araç soruyu kendi penceresinde cevaplatınca kart kalkar (girdi cevaplarla değişmiş olsa da).
        let post = event("PostToolUse", json!({ "tool_name": ASK_TOOL, "tool_input": { "answers": { "Tema?": "Koyu" } } }));
        s.ingest(Provider::Claude, Mode::Observe, &post, 2);
        assert!(s.snapshot().approvals.is_empty());
    }

    #[test]
    fn always_allow_uses_the_tools_own_suggestion() {
        let mut s = State::default();
        let suggestions = json!([
            { "type": "setMode", "mode": "bypassPermissions", "destination": "session" },
            { "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }], "behavior": "allow", "destination": "localSettings" }
        ]);
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "npm test" }, "permission_suggestions": suggestions }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &req, 1).unwrap();
        let view = s.snapshot().approvals[0].clone();
        assert_eq!(view.always.as_deref(), Some("Bash(npm test:*) · bu projede, yalnız sende"));
        // Adadan gelen sıra yok sayılır; isteğin kendi önerisi gider.
        s.decide(&view.id, Decision::AllowAlways(0)).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), Some(Decision::AllowAlways(1)));
        assert_eq!(s.snapshot().agents[0].step, "Her zaman izin verildi · devam ediyor");
    }

    #[test]
    fn always_allow_is_not_offered_without_a_safe_suggestion() {
        let mut s = State::default();
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &req, 1).unwrap();
        assert!(s.snapshot().approvals[0].always.is_none());
        assert!(s.decide(&pending.id, Decision::AllowAlways(0)).is_err());
        // Codex'te öneri gelse bile sunulmaz.
        let codex = event(
            "PermissionRequest",
            json!({ "tool_name": "Bash", "tool_input": {}, "permission_suggestions": [{ "type": "addRules", "rules": [{ "toolName": "Bash" }], "behavior": "allow" }] }),
        );
        s.ingest(Provider::Codex, Mode::Interactive, &codex, 2);
        assert!(s.snapshot().approvals.iter().all(|a| a.always.is_none()));
    }

    #[test]
    fn edit_request_carries_line_counts_and_diff() {
        let mut s = State::default();
        let input = json!({ "file_path": "C:\\p\\src\\App.svelte", "old_string": "a\nb", "new_string": "a\nc\nd" });
        // Eski köprü: sayılar kırpılmış metinden hesaplanır.
        let req = event("PermissionRequest", json!({ "tool_name": "Edit", "tool_input": input }));
        s.ingest(Provider::Claude, Mode::Interactive, &req, 1);
        let view = s.snapshot().approvals[0].clone();
        assert_eq!(view.files, vec![FileStat { path: "C:\\p\\src\\App.svelte".into(), name: "App.svelte".into(), added: 2, removed: 1 }]);
        assert!(matches!(s.request_raw(&view.id), Some(Raw::Edit { .. })));
        // Yeni köprü: tam metinden gelen sayı kullanılır.
        let mut req = event("PermissionRequest", json!({ "tool_name": "Write", "tool_input": { "file_path": "b.md", "content": "x" } }));
        req["session_id"] = json!("s2");
        req["diff_stat"] = json!([{ "path": "b.md", "added": 120, "removed": 4 }]);
        s.ingest(Provider::Claude, Mode::Interactive, &req, 2);
        let write = s.snapshot().approvals.iter().find(|a| a.session_id.ends_with("s2")).cloned().unwrap();
        assert_eq!((write.files[0].added, write.files[0].removed), (120, 4));
        // Komut isteğinde dosya yok.
        let mut bash = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        bash["session_id"] = json!("s3");
        s.ingest(Provider::Claude, Mode::Interactive, &bash, 3);
        let bash = s.snapshot().approvals.iter().find(|a| a.session_id.ends_with("s3")).cloned().unwrap();
        assert!(bash.files.is_empty() && s.request_raw(&bash.id).is_none());
    }

    #[test]
    fn session_remembers_its_window() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Observe, &event("SessionStart", json!({ "window": { "hwnd": 4242, "app": "WindowsTerminal.exe" } })), 1);
        // Penceresiz olay öncekini silmez.
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", json!({ "tool_name": "Read", "tool_input": {} })), 2);
        let id = s.snapshot().agents[0].id.clone();
        assert_eq!(s.window(&id), Some(4242));
        assert_eq!(s.snapshot().agents[0].app.as_deref(), Some("WindowsTerminal.exe"));
        assert_eq!(s.window("claude:yok"), None);
    }

    #[test]
    fn codex_question_is_an_info_card() {
        let mut s = State::default();
        let ask = event(
            "PreToolUse",
            json!({ "tool_name": CODEX_ASK_TOOL, "tool_input": { "questions": [{ "id": "t", "header": "Tema", "question": "Tema?", "options": [{ "label": "Koyu", "description": "" }] }] } }),
        );
        assert!(s.ingest(Provider::Codex, Mode::Interactive, &ask, 1).is_none(), "Codex sorusu için köprü beklemez");
        let view = s.snapshot().approvals[0].clone();
        assert_eq!(view.kind, RequestKind::Question);
        assert!(!view.actionable && view.native_only);
        assert_eq!(view.questions[0].question, "Tema?");
        // Codex soruyu kendi penceresinde cevaplatınca kart kalkar.
        s.ingest(Provider::Codex, Mode::Interactive, &event("PostToolUse", json!({ "tool_name": CODEX_ASK_TOOL, "tool_input": {} })), 2);
        assert!(s.snapshot().approvals.is_empty());
    }

    #[test]
    fn turn_duration_and_changed_files() {
        let mut s = State::default();
        let edit = |path: &str| json!({ "tool_name": "Edit", "tool_input": { "file_path": path, "old_string": "a", "new_string": "b" } });
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1_000);
        for (path, at) in [("C:/work/api/a.rs", 2_000), ("C:/work/api/b.rs", 3_000), ("C:/work/api/A.rs", 4_000)] {
            s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", edit(path)), at);
            s.ingest(Provider::Claude, Mode::Observe, &event("PostToolUse", edit(path)), at + 10);
        }
        // Başarısız düzenleme sayılmaz.
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", edit("C:/work/api/c.rs")), 5_000);
        s.ingest(Provider::Claude, Mode::Observe, &event("PostToolUseFailure", edit("C:/work/api/c.rs")), 5_010);
        let running = s.snapshot().agents[0].clone();
        assert_eq!((running.turn_started_at, running.turn_ended_at, running.turn_files), (Some(1_000), None, 2));
        s.ingest(Provider::Claude, Mode::Observe, &event("Stop", json!({})), 61_000);
        let done = s.snapshot().agents[0].clone();
        assert_eq!((done.turn_ended_at, done.turn_files, done.status_since), (Some(61_000), 2, 61_000));
        // Yeni tur sayacı sıfırlar.
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 70_000);
        let next = s.snapshot().agents[0].clone();
        assert_eq!((next.turn_started_at, next.turn_ended_at, next.turn_files, next.status_since), (Some(70_000), None, 0, 70_000));
    }

    #[test]
    fn waiting_since_marks_when_the_wait_began() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1_000);
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })), 2_000);
        s.ingest(Provider::Claude, Mode::Observe, &event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } })), 3_000);
        // Aynı durumda gelen başka olay bekleme başlangıcını değiştirmez.
        s.ingest(Provider::Claude, Mode::Observe, &event("Notification", json!({ "notification_type": "permission_prompt" })), 9_000);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.status, agent.status_since), (Status::WaitingApproval, 3_000));
    }

    fn plan_event(plan: &str) -> Value {
        event(
            "PermissionRequest",
            json!({
                "tool_name": PLAN_TOOL,
                "tool_input": { "plan": "# Kısa", "planFilePath": "C:/p.md" },
                "plan": plan,
                "permission_suggestions": [{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }]
            }),
        )
    }

    #[test]
    fn plan_is_reviewed_on_the_island() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Interactive, &event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Interactive, &event("PreToolUse", json!({ "tool_name": PLAN_TOOL, "tool_input": {} })), 2);
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &plan_event("Giriş\n\n# Kullanım özeti\n1. a"), 1_000).expect("plan adadan onaylanır");
        assert_eq!(pending.timeout_secs, zodyvel_hook::PLAN_TIMEOUT_SECS);
        let snap = s.snapshot();
        let view = &snap.approvals[0];
        assert_eq!(view.kind, RequestKind::Plan);
        assert_eq!(view.detail, "Kullanım özeti");
        assert!(view.plan.as_deref().unwrap().contains("1. a"), "tam metin ekten gelir");
        assert_eq!(view.expires_at, Some(1_000 + zodyvel_hook::PLAN_TIMEOUT_SECS * 1000));
        assert!(view.always.as_deref().unwrap().starts_with("dosya düzenlemelerini"));
        let agent = &snap.agents[0];
        assert_eq!((agent.status, agent.step.as_str()), (Status::WaitingApproval, "Planını onaylaman bekleniyor"));
        let plans: Vec<&Step> = agent.steps.iter().filter(|st| st.kind == Kind::Plan).collect();
        assert_eq!(plans.len(), 1, "PreToolUse ve onay isteği tek adım");
        assert_eq!(plans[0].target, "Kullanım özeti");

        assert!(s.decide(&view.id, Decision::Answer(Map::new())).is_err());
        assert!(s.decide(&view.id, Decision::Revise("  ".into())).is_err(), "boş not gönderilmez");
        s.decide(&view.id, Decision::Revise("Testleri de ekle".into())).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), Some(Decision::Revise("Testleri de ekle".into())));
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.status, agent.step.as_str()), (Status::Running, "Planı yeniden düşünüyor"));
        assert_eq!(agent.steps.iter().rfind(|st| st.kind == Kind::Plan).map(|st| st.state), Some(StepState::Failed));

        // İkinci plan: onaylanır, düzenlemeleri otomatik kabul ile.
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &plan_event("# İkinci"), 2_000).unwrap();
        let id = s.snapshot().approvals[0].id.clone();
        s.decide(&id, Decision::AllowAlways(5)).unwrap();
        assert_eq!(pending.receiver.try_recv().unwrap(), Some(Decision::AllowAlways(0)), "sıra isteğin kaydından");
        assert_eq!(s.snapshot().agents[0].step, "Plan onaylandı · uyguluyor");
    }

    #[test]
    fn plan_decided_in_the_tool_closes_the_card() {
        let mut s = State::default();
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &plan_event("# P"), 1).unwrap();
        s.expire(&s.snapshot().approvals[0].id.clone(), 2);
        assert!(!s.snapshot().approvals[0].actionable);
        s.ingest(Provider::Claude, Mode::Interactive, &event("PreToolUse", json!({ "tool_name": "Edit", "tool_input": { "file_path": "a.rs" } })), 3);
        assert!(s.snapshot().approvals.is_empty());
        drop(pending);
        // Gözlem kipinde plan bilgi kartıdır.
        assert!(s.ingest(Provider::Claude, Mode::Observe, &plan_event("# P"), 4).is_none());
        assert!(s.snapshot().approvals[0].native_only);
    }

    #[test]
    fn plan_review_off_leaves_the_plan_in_the_tool() {
        let mut s = State { plan_off: true, ..State::default() };
        assert!(s.ingest(Provider::Claude, Mode::Interactive, &plan_event("# P"), 1).is_none(), "kapalıyken köprü beklemez");
        let snap = s.snapshot();
        assert!(snap.approvals.is_empty(), "kapalıyken kart yok");
        assert_eq!((snap.agents[0].status, snap.agents[0].step.as_str()), (Status::WaitingInput, "Planını araçta onaylaman bekleniyor"));
    }

    #[test]
    fn demo_plan_is_decidable() {
        let mut s = State::default();
        s.demo_request(1, RequestKind::Plan);
        let view = s.snapshot().approvals[0].clone();
        assert!(view.demo && view.plan.as_deref().is_some_and(|p| p.starts_with("# ")));
        assert_eq!(view.expires_at, Some(1 + zodyvel_hook::PLAN_TIMEOUT_SECS * 1000));
        s.decide(&view.id, Decision::Revise("Daha kısa".into())).unwrap();
        assert!(s.snapshot().approvals.is_empty());
    }

    fn notes_event(name: &str, extra: Value) -> Value {
        let mut e = event(name, extra);
        e.as_object_mut().unwrap().insert("zodyvel_notes".into(), json!(true));
        e
    }

    fn delivered(pending: Option<Pending>) -> Option<String> {
        match pending?.receiver.try_recv().ok()?? {
            Decision::Note(text) => Some(text),
            _ => None,
        }
    }

    #[test]
    fn launched_session_takes_tasks_directly_and_finishes_as_done() {
        let mut s = State::default();
        s.launch("claude:s1", Some(3));
        s.ingest(Provider::Claude, Mode::Interactive, &event("SessionStart", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Interactive, &event("UserPromptSubmit", json!({})), 2);
        assert!(!s.snapshot().agents[0].can_task, "çalışırken görev alınmaz");
        assert!(s.direct_task("claude:s1", "x", 3).is_err());
        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 4);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!(agent.task_no, Some(3));
        assert_eq!(agent.status, Status::Done, "terminali yok: boşta bitmiş sayılır");
        assert!(agent.can_task);
        assert_eq!(s.direct_task("claude:s1", "  devam et  ", 5).as_deref(), Ok("devam et"));
        assert_eq!(s.snapshot().agents[0].status, Status::Running);
        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 6);
        s.launch_ended("claude:s1");
        assert!(!s.launched_live("claude:s1"));
        let agent = s.snapshot().agents[0].clone();
        assert!(!agent.can_task);
        assert_eq!(agent.task_no, Some(3));
    }

    #[test]
    fn shelf_drops_attach_only_to_live_launched_sessions() {
        let mut s = State::default();
        s.launch("claude:s1", Some(1));
        s.ingest(Provider::Claude, Mode::Interactive, &event("SessionStart", json!({})), 1);
        assert_eq!(s.drop_route("claude:s1"), Some(true));
        assert!(s.snapshot().agents[0].attach);
        assert_eq!(s.drop_route("claude:yok"), None);
        s.launch_ended("claude:s1");
        assert_eq!(s.drop_route("claude:s1"), Some(false), "süreç kapandı: yol panoya");
        assert!(!s.snapshot().agents[0].attach);
    }

    #[test]
    fn note_needs_a_bridge_that_reads_it() {
        let mut s = State::default();
        // Eski köprü işaret göndermez: not sıraya alınmaz.
        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 1);
        assert!(!s.snapshot().agents[0].can_note);
        assert!(s.add_note("claude:s1", "testleri de yaz", false, 2).is_err());
        // İzle modunda ya da Codex'te de not yok.
        s.ingest(Provider::Claude, Mode::Observe, &notes_event("Stop", json!({})), 3);
        assert!(!s.snapshot().agents[0].can_note);
        s.ingest(Provider::Claude, Mode::Interactive, &notes_event("Stop", json!({})), 4);
        assert!(s.snapshot().agents[0].can_note);
        assert!(s.add_note("claude:s1", "   ", false, 5).is_err());
        assert!(s.add_note("claude:yok", "x", false, 5).is_err());
    }

    #[test]
    fn later_note_is_delivered_when_the_turn_ends() {
        let mut s = State::default();
        let bash = json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
        s.ingest(Provider::Claude, Mode::Interactive, &notes_event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Interactive, &notes_event("PreToolUse", bash.clone()), 2);
        s.add_note("claude:s1", "README'yi de güncelle", false, 3).unwrap();
        assert_eq!(s.snapshot().agents[0].notes.len(), 1);
        // "Sonra" notu araç çağrısında teslim edilmez.
        assert!(s.ingest(Provider::Claude, Mode::Interactive, &notes_event("PreToolUse", bash), 4).is_none());
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &notes_event("Stop", json!({ "last_assistant_message": "Bitti." })), 5);
        assert_eq!(delivered(pending).as_deref(), Some("README'yi de güncelle"));
        let agent = s.snapshot().agents[0].clone();
        assert!(agent.notes.is_empty());
        assert_eq!((agent.status, agent.turn_ended_at), (Status::Running, None));
        assert_eq!(agent.steps.last().map(|st| st.label.as_str()), Some("Notun iletildi"));
        // Not bir kez teslim edilir; sonraki Stop turu bitirir.
        assert!(s.ingest(Provider::Claude, Mode::Interactive, &notes_event("Stop", json!({})), 6).is_none());
        assert_eq!(s.snapshot().agents[0].status, Status::Done);
    }

    #[test]
    fn now_note_interrupts_the_next_tool_call() {
        let mut s = State::default();
        let bash = json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
        s.ingest(Provider::Claude, Mode::Interactive, &notes_event("PreToolUse", bash.clone()), 1);
        s.add_note("claude:s1", "sonra", false, 2).unwrap();
        s.add_note("claude:s1", "dur, önce testler", true, 3).unwrap();
        // Soru aracı araya girmek için kullanılmaz.
        let ask = notes_event("PreToolUse", json!({ "tool_name": ASK_TOOL, "tool_input": {} }));
        let _ = s.ingest(Provider::Claude, Mode::Interactive, &ask, 4);
        assert_eq!(s.snapshot().agents[0].notes.len(), 2);
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &notes_event("PreToolUse", bash), 5);
        assert_eq!(delivered(pending).as_deref(), Some("dur, önce testler"));
        let notes = s.snapshot().agents[0].notes.clone();
        assert_eq!(notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["sonra"]);
        // Sıradaki not iptal edilebilir, ikinci kez edilemez.
        assert!(s.remove_note("claude:s1", notes[0].id).is_ok());
        assert!(s.remove_note("claude:s1", notes[0].id).is_err());
        // Not, kararla verilemez.
        let req = notes_event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "rm x" } }));
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &req, 6).unwrap();
        assert!(s.decide(&pending.id, Decision::Note("x".into())).is_err());
    }

    #[test]
    fn todos_follow_the_agents_list() {
        let mut s = State::default();
        let todo = |tool: &str, input: Value| event("PreToolUse", json!({ "tool_name": tool, "tool_input": input }));
        s.ingest(Provider::Claude, Mode::Observe, &todo("TodoWrite", json!({ "todos": [
            { "content": "Testleri yaz", "activeForm": "Testleri yazıyor", "status": "completed" },
            { "content": "Belgeyi güncelle", "activeForm": "Belgeyi güncelliyor", "status": "in_progress" },
            { "content": "", "status": "pending" },
        ] })), 1);
        let todos = s.snapshot().agents[0].todos.clone();
        assert_eq!(todos.len(), 2);
        assert_eq!((todos[0].status, todos[1].status, todos[1].active.as_str()), (TodoStatus::Completed, TodoStatus::InProgress, "Belgeyi güncelliyor"));
        // Görev araçları tek tek ekler ve günceller.
        s.ingest(Provider::Claude, Mode::Observe, &todo("TaskCreate", json!({ "subject": "Sürüm notu" })), 2);
        s.ingest(Provider::Claude, Mode::Observe, &todo("TaskUpdate", json!({ "taskId": "1", "status": "in_progress" })), 3);
        assert_eq!(s.snapshot().agents[0].todos.last().map(|t| t.status), Some(TodoStatus::InProgress));
        s.ingest(Provider::Claude, Mode::Observe, &todo("TaskUpdate", json!({ "taskId": 1, "status": "deleted" })), 4);
        assert_eq!(s.snapshot().agents[0].todos.len(), 2);
        // Codex planı listenin tamamını değiştirir.
        s.ingest(Provider::Codex, Mode::Observe, &todo("update_plan", json!({ "plan": [{ "step": "Oku", "status": "completed" }] })), 5);
        let codex = s.snapshot().agents.into_iter().find(|a| a.tool == "codex").unwrap();
        assert_eq!(codex.todos.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["Oku"]);
    }

    #[test]
    fn changed_files_carry_counts_and_last_step() {
        let mut s = State::default();
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "C:/work/api/a.rs", "old_string": "a", "new_string": "b\nc" } });
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", edit.clone()), 2);
        s.ingest(Provider::Claude, Mode::Observe, &event("PostToolUse", edit.clone()), 3);
        // Köprünün tam metinden saydığı satırlar önceliklidir.
        let mut post = edit.clone();
        post["diff_stat"] = json!([{ "path": "C:/work/api/a.rs", "added": 10, "removed": 4 }]);
        s.ingest(Provider::Claude, Mode::Observe, &event("PreToolUse", edit), 4);
        s.ingest(Provider::Claude, Mode::Observe, &event("PostToolUse", post), 5);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!(agent.changed.len(), 1);
        let file = &agent.changed[0];
        assert_eq!((file.name.as_str(), file.added, file.removed), ("a.rs", 12, 5));
        assert_eq!(Some(file.step), agent.steps.iter().rev().find(|st| st.tool == "Edit").map(|st| st.id));
        assert_eq!(agent.folder.as_deref(), Some("C:\\work\\api"));
    }

    #[test]
    fn last_reply_mode_and_subagents() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({ "permission_mode": "plan" })), 1);
        s.ingest(Provider::Claude, Mode::Observe, &event("SubagentStart", json!({})), 2);
        s.ingest(Provider::Claude, Mode::Observe, &event("SubagentStart", json!({})), 3);
        let sub = event("PreToolUse", json!({ "agent_id": "x", "tool_name": "Read", "tool_input": { "file_path": "C:/a.rs" } }));
        s.ingest(Provider::Claude, Mode::Observe, &sub, 4);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.subagents, agent.mode.as_deref()), (2, Some("plan")));
        assert!(agent.steps.last().is_some_and(|st| st.sub));
        s.ingest(Provider::Claude, Mode::Observe, &event("SubagentStop", json!({})), 5);
        assert_eq!(s.snapshot().agents[0].subagents, 1);
        s.ingest(Provider::Claude, Mode::Observe, &event("Stop", json!({ "last_assistant_message": "  **Bitti.** Testler geçti.  " })), 6);
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.subagents, agent.last_reply.as_deref()), (0, Some("**Bitti.** Testler geçti.")));
    }

    #[test]
    fn turning_the_module_off_hands_every_request_back_to_the_tool() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Interactive, &event("UserPromptSubmit", json!({})), 1);
        let req = event("PermissionRequest", json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        let approval = s.ingest(Provider::Claude, Mode::Interactive, &req, 2).unwrap();
        let other = |e: &str| json!({ "hook_event_name": e, "session_id": "s2", "cwd": "C:\\work\\web" });
        s.ingest(Provider::Claude, Mode::Interactive, &other("Stop"), 3);
        let mut waiting = other("Stop");
        waiting.as_object_mut().unwrap().insert("zodyvel_await".into(), json!(true));
        let waiter = s.ingest(Provider::Claude, Mode::Interactive, &waiting, 4).unwrap();
        assert_eq!((s.snapshot().agents.len(), s.snapshot().approvals.len()), (2, 1));

        s.release_all();
        // İkisi de "araçta cevapla" alır (None): araç kendi akışına döner, beklemez.
        assert!(matches!(approval.receiver.try_recv(), Ok(None)));
        assert!(matches!(waiter.receiver.try_recv(), Ok(None)));
        let snap = s.snapshot();
        assert!(snap.agents.is_empty() && snap.approvals.is_empty());
        assert!(s.decide(&approval.id, Decision::Allow).is_err());
    }

    fn await_event() -> Value {
        let mut e = event("Stop", json!({}));
        e.as_object_mut().unwrap().insert("zodyvel_await".into(), json!(true));
        e
    }

    #[test]
    fn idle_session_takes_a_task_through_the_waiting_bridge() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Interactive, &event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 2);
        // Gözlem kipinde ya da Stop dışında bekleyici kurulmaz.
        assert!(s.ingest(Provider::Claude, Mode::Observe, &await_event(), 3).is_none());
        let pending = s.ingest(Provider::Claude, Mode::Interactive, &await_event(), 3).unwrap();
        assert_eq!(pending.timeout_secs, zodyvel_hook::AWAIT_TIMEOUT_SECS);
        assert!(s.snapshot().agents[0].can_task);
        // Bekleyici olay değildir: oturumun durumu ve adımları değişmez.
        assert_eq!(s.snapshot().agents[0].status, Status::Done);

        s.send_task("claude:s1", "  Testleri çalıştır  ", 4).unwrap();
        assert!(matches!(pending.receiver.try_recv(), Ok(Some(Decision::Note(t))) if t == "Testleri çalıştır"));
        let agent = s.snapshot().agents[0].clone();
        assert_eq!((agent.status, agent.can_task, agent.turn_started_at), (Status::Running, false, Some(4)));
        assert_eq!(agent.steps.last().map(|st| st.label.as_str()), Some("Adadan yeni istek"));
        // Bekleyici tek kullanımlık; çalışan oturuma görev gitmez.
        assert!(s.send_task("claude:s1", "bir daha", 5).is_err());
    }

    #[test]
    fn waiting_bridge_closes_on_new_prompt_session_end_and_expiry() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 1);
        let first = s.ingest(Provider::Claude, Mode::Interactive, &await_event(), 1).unwrap();
        // Yeni bekleyici eskisinin yerini alır: eski köprü boş yanıtla çıkar.
        let second = s.ingest(Provider::Claude, Mode::Interactive, &await_event(), 2).unwrap();
        assert!(matches!(first.receiver.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)));
        s.ingest(Provider::Claude, Mode::Interactive, &event("UserPromptSubmit", json!({})), 3);
        assert!(matches!(second.receiver.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)));
        assert!(!s.snapshot().agents[0].can_task);

        s.ingest(Provider::Claude, Mode::Interactive, &event("Stop", json!({})), 4);
        let third = s.ingest(Provider::Claude, Mode::Interactive, &await_event(), 4).unwrap();
        s.expire(&third.id, 5);
        assert!(!s.snapshot().agents[0].can_task);
        let fourth = s.ingest(Provider::Claude, Mode::Interactive, &await_event(), 6).unwrap();
        s.ingest(Provider::Claude, Mode::Interactive, &event("SessionEnd", json!({})), 7);
        assert!(matches!(fourth.receiver.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)));
    }

    #[test]
    fn background_subagent_after_stop_keeps_session_done() {
        let mut s = State::default();
        s.ingest(Provider::Claude, Mode::Observe, &event("UserPromptSubmit", json!({})), 1);
        s.ingest(Provider::Claude, Mode::Observe, &event("Stop", json!({})), 2);
        s.ingest(Provider::Claude, Mode::Observe, &event("SubagentStart", json!({ "agent_id": "ozet" })), 3);
        s.ingest(Provider::Claude, Mode::Observe, &event("SubagentStop", json!({ "agent_id": "ozet" })), 4);
        assert_eq!(s.snapshot().agents[0].status, Status::Done);
    }
}
