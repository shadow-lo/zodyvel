//! AI ajanları modülü: hook köprüsü, oturum durumu, onaylar ve bağlantı kurulumu.

mod content;
mod describe;
pub mod hookbin;
mod meta;
mod pipe;
pub mod setup;
mod state;
pub mod statusline;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use zodyvel_hook::{Decision, Mode, Provider};

pub use state::Snapshot;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub struct AgentHub {
    state: Mutex<state::State>,
    enabled: AtomicBool,
    app: AppHandle,
}

/// Telefona giden adım satırı (T4-c): tür, Türkçe fiil, maskeli kısa hedef. Ham veri (komut çıktısı, fark) gitmez.
fn phone_step(st: &state::Step) -> zodyvel_phone_proto::proto::StepView {
    use describe::Kind;
    let (kind, verb) = match st.kind {
        Kind::Read => ("read", "Okudu"),
        Kind::Edit => ("edit", "Düzenledi"),
        Kind::Write => ("edit", "Yazdı"),
        Kind::Bash => ("bash", "Çalıştırdı"),
        Kind::Search => ("search", "Aradı"),
        Kind::Web => ("search", "Web'de aradı"),
        Kind::Agent => ("other", "Alt ajan"),
        Kind::Plan => ("other", "Plan"),
        Kind::Question => ("other", "Soru"),
        Kind::Approval => ("other", "Onay"),
        Kind::Prompt => ("other", "İstek"),
        Kind::Done => ("other", "Bitti"),
        Kind::Error => ("other", "Hata"),
        Kind::Tool => ("other", "Araç"),
    };
    let redact = crate::redact::redact_snippet;
    // Etiket yalnız tür tek başına anlatmadığında doludur (bilinmeyen araç, onay kararı, tur özeti).
    let label = if st.label.trim().is_empty() { verb.to_owned() } else { redact(&st.label) };
    zodyvel_phone_proto::proto::StepView::new(kind, &label, &redact(&st.target))
}

impl AgentHub {
    fn lock(&self) -> MutexGuard<'_, state::State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn publish(&self) {
        let snapshot = self.lock().snapshot();
        let _ = self.app.emit("agents://snapshot", &snapshot);
        // Telefon köprüsü açıksa bağlı telefonlara değişen durum gider (kapalıyken hiçbir şey yapmaz).
        crate::phone::notify(&self.app);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Raftan bırakılan dosyanın yolu (K5): `(gerçekten eklenir mi, pencere)`; oturum yoksa `None`.
    pub(crate) fn drop_route(&self, session: &str) -> Option<(bool, Option<u64>)> {
        let state = self.lock();
        state.drop_route(session).map(|live| (live, state.window(session)))
    }

    /// Kapanınca bekleyen istekler araca bırakılır ve oturumlar silinir (modül kapalıyken hiçbir şey tutulmaz).
    pub fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::Relaxed) && !enabled {
            self.lock().release_all();
            self.publish();
        }
    }

    pub(crate) fn ingest(&self, provider: Provider, mode: Mode, payload: &Value) -> Option<state::Pending> {
        // Modül tam o an kapandıysa olay durumu yeniden doldurmasın.
        if !self.enabled() {
            return None;
        }
        let now = now_ms();
        let claude = provider == Provider::Claude;
        // Oturum adı ve uzaktan oturum işareti diskten, kilit dışında okunur.
        let annotated = meta::annotate(claude, payload, now);
        let annotated_at = now_ms();
        let plan_off = payload.get("tool_name").and_then(Value::as_str) == Some(zodyvel_hook::PLAN_TOOL)
            && !crate::settings::current(&self.app).features.plan_review;
        let pending = {
            let mut state = self.lock();
            state.plan_off = plan_off;
            state.ingest(provider, mode, annotated.as_ref().unwrap_or(payload), now)
        };
        self.publish();
        timing(payload, now, annotated_at);
        if let Some(session) = meta::wants_retitle(claude, payload) {
            self.retitle_later(session);
        }
        pending
    }

    /// Masaüstü başlığı tur bittikten sonra yazılır: 3 sn ve 15 sn sonra birer kez okunur, sonra durur
    /// (sürekli yoklama yok). Bulununca ad hemen güncellenir.
    fn retitle_later(&self, session: String) {
        let app = self.app.clone();
        let _ = std::thread::Builder::new().name("zodyvel-retitle".into()).spawn(move || {
            for (wait, last) in [(3, false), (12, true)] {
                std::thread::sleep(std::time::Duration::from_secs(wait));
                let Some(hub) = app.try_state::<Arc<AgentHub>>() else { return };
                if !hub.enabled() {
                    return;
                }
                if let Some(title) = meta::retitle(&session, last) {
                    let renamed = hub.lock().rename(&format!("claude:{session}"), &title);
                    if renamed {
                        hub.publish();
                    }
                    return;
                }
            }
        });
    }

    /// Zodyvel bir Claude oturumu başlattı (`session`: aracın oturum kimliği).
    pub(crate) fn launch(&self, session: &str, no: Option<u32>) {
        self.lock().launch(&format!("claude:{session}"), no);
        self.publish();
    }

    pub(crate) fn launch_ended(&self, session: &str) {
        self.lock().launch_ended(&format!("claude:{session}"));
        self.publish();
    }

    /// Kullanıcı Zodyvel'in görevini durdurdu (Faz 9, mini AI): oturum "Durduruldu" ile biter.
    pub(crate) fn stopped(&self, session: &str) {
        self.lock().stopped(&format!("claude:{session}"), now_ms());
        self.publish();
    }

    /// Adada açık Claude oturumlarının araç kimlikleri.
    pub(crate) fn claude_session_ids(&self) -> Vec<String> {
        self.lock().snapshot().agents.iter().filter_map(|s| s.id.strip_prefix("claude:").map(str::to_owned)).collect()
    }

    /// Telefon köprüsü (T2): telefona giden en az veri: oturum adı/durum/kısa adım ve onay özeti.
    /// Komut çıktısı, dosya içeriği, fark ve plan metni dahil değildir.
    pub(crate) fn phone_view(&self) -> zodyvel_phone_proto::proto::StateView {
        use zodyvel_phone_proto::proto::{ApprovalView, MAX_STEPS_VIEW, QuestionView, SessionView, StateView, clip};
        if !self.enabled() {
            return StateView::default();
        }
        let snap = self.lock().snapshot();
        let sessions = snap
            .agents
            .iter()
            .filter(|s| !s.demo)
            .take(20)
            .map(|s| SessionView {
                id: s.id.clone(),
                tool: s.tool.to_owned(),
                name: clip(&s.project, 60),
                status: match s.status {
                    state::Status::Running => "running",
                    state::Status::WaitingInput => "waiting_input",
                    state::Status::WaitingApproval => "waiting_approval",
                    state::Status::Done => "done",
                    state::Status::Error => "error",
                }
                .to_owned(),
                step: clip(&crate::redact::redact_snippet(&s.step), 80),
                reply: None,
                folder: s.folder.as_deref().map(|f| clip(f.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(""), 60)).unwrap_or_default(),
                started: i64::try_from(s.turn_started_at.unwrap_or(s.started_at) / 1000).ok().filter(|t| *t > 0),
                steps: {
                    let skip = s.steps.len().saturating_sub(MAX_STEPS_VIEW);
                    s.steps[skip..].iter().map(phone_step).collect()
                },
            })
            .collect();
        let approvals = snap
            .approvals
            .iter()
            .filter(|a| !a.demo && a.actionable && !a.native_only && !a.closed_early)
            .take(10)
            .map(|a| ApprovalView {
                id: a.id.clone(),
                session: a.session_id.clone(),
                kind: match a.kind {
                    state::RequestKind::Permission => "permission",
                    state::RequestKind::Question => "question",
                    state::RequestKind::Plan => "plan",
                }
                .to_owned(),
                action: clip(&crate::redact::redact_snippet(&a.action), 140),
                questions: a
                    .questions
                    .iter()
                    .take(4)
                    .map(|q| QuestionView {
                        q: clip(&crate::redact::redact_snippet(&q.question), 200),
                        options: q.options.iter().take(8).map(|o| clip(&crate::redact::redact_snippet(&o.label), 80)).collect(),
                        multi: q.multi_select,
                    })
                    .collect(),
            })
            .collect();
        StateView { sessions, approvals }
    }

    /// Telefondan gelen karar (izin ver/reddet, soru cevabı): ada ile aynı yol, hangisi önce cevaplarsa o geçerli.
    pub(crate) fn decide_external(&self, id: &str, decision: Decision) -> Result<(), String> {
        let result = self.lock().decide(id, decision);
        self.publish();
        result
    }

    pub(crate) fn expire(&self, id: &str) {
        self.lock().expire(id, now_ms());
        self.publish();
    }
}

/// "Bitti" gecikmesi ölçümü (Faz 7.2, yalnız tanı kaydı açıkken): köprü başladı → köprü gönderdi →
/// Zodyvel aldı → ad okundu → anlık görüntü yayınlandı. İçerik yazılmaz, yalnız olay adı ve süreler.
fn timing(payload: &Value, received: u64, annotated: u64) {
    if !crate::debuglog::enabled() {
        return;
    }
    let event = payload.get("hook_event_name").and_then(Value::as_str).unwrap_or("?");
    if !matches!(event, "Stop" | "StopFailure" | "UserPromptSubmit" | "Notification") {
        return;
    }
    let at = |key: &str| payload.get(key).and_then(Value::as_u64);
    let (spawned, sent) = (at("zodyvel_spawned"), at("zodyvel_sent"));
    let published = now_ms();
    let ms = |from: Option<u64>, to: u64| from.map_or("?".to_owned(), |f| to.saturating_sub(f).to_string());
    crate::debuglog::line(format!(
        "olcum: {event} kopru_basladi→gonderdi {} ms, gonderdi→aldi {} ms, ad {} ms, yayin {} ms",
        sent.map_or("?".to_owned(), |s| ms(spawned, s)),
        ms(sent, received),
        annotated.saturating_sub(received),
        published.saturating_sub(annotated),
    ));
}

pub fn init(app: &AppHandle, enabled: bool) {
    let hub = Arc::new(AgentHub { state: Mutex::new(state::State::default()), enabled: AtomicBool::new(enabled), app: app.clone() });
    app.manage(hub.clone());
    app.manage(setup::Setup::default());
    hookbin::ensure();
    pipe::spawn(hub);
}

#[tauri::command]
pub fn agents_snapshot(hub: State<'_, Arc<AgentHub>>) -> Snapshot {
    hub.lock().snapshot()
}

/// `note`: yalnız "revise"da (plan: "şunu değiştir"); araca kullanıcının notu olarak gider.
#[tauri::command]
pub fn agents_decide(hub: State<'_, Arc<AgentHub>>, id: String, decision: String, note: Option<String>) -> Result<(), String> {
    let decision = match decision.as_str() {
        "allow" => Decision::Allow,
        // Hangi kuralın kaydedileceğini istek kaydı belirler (aracın kendi önerisi).
        "always" => Decision::AllowAlways(0),
        "deny" => Decision::Deny,
        "revise" => Decision::Revise(note.unwrap_or_default()),
        _ => return Err("Geçersiz karar.".into()),
    };
    let result = hub.lock().decide(&id, decision);
    hub.publish();
    result
}

/// Seçenekli sorunun cevapları (soru metni → seçilen etiket[ler]).
#[tauri::command]
pub fn agents_answer(hub: State<'_, Arc<AgentHub>>, id: String, answers: Map<String, Value>) -> Result<(), String> {
    let result = hub.lock().decide(&id, Decision::Answer(answers));
    hub.publish();
    result
}

/// "Araçta cevapla": soru (ya da onay) aracın kendi penceresine bırakılır.
#[tauri::command]
pub fn agents_pass(hub: State<'_, Arc<AgentHub>>, id: String) -> Result<(), String> {
    let result = hub.lock().pass(&id);
    hub.publish();
    result
}

/// Ayrıntı ekranı: bir adımın içeriği. Dosya okuma kilit dışında yapılır.
#[tauri::command]
pub fn agents_step(hub: State<'_, Arc<AgentHub>>, session: String, step: u64) -> Option<content::StepContent> {
    let raw = hub.lock().step_raw(&session, step)?;
    Some(content::build(&raw))
}

/// Onay kartındaki dosyaya tıklanınca: isteğin tam farkı (dosya henüz değişmedi).
#[tauri::command]
pub fn agents_request_diff(hub: State<'_, Arc<AgentHub>>, id: String) -> Option<content::StepContent> {
    let raw = hub.lock().request_raw(&id)?;
    Some(content::build_request(&raw))
}

/// "Terminale git": oturumun çalıştığı pencereyi öne getirir.
#[tauri::command]
pub fn agents_focus(hub: State<'_, Arc<AgentHub>>, session: String) -> Result<(), String> {
    let window = hub.lock().window(&session).ok_or("Bu oturumun penceresi bilinmiyor.")?;
    if crate::win::bring_to_front(window) { Ok(()) } else { Err("Pencere bulunamadı ya da öne getirilemedi.".into()) }
}

/// Ajana not: `now` true ise bir sonraki araç çağrısında, değilse tur biterken iletilir.
#[tauri::command]
pub fn agents_note(app: AppHandle, hub: State<'_, Arc<AgentHub>>, session: String, text: String, now: bool) -> Result<(), String> {
    if !crate::settings::current(&app).features.agent_notes {
        return Err("Ajana not kapalı (Ayarlar → Özellikler).".into());
    }
    let result = hub.lock().add_note(&session, &text, now, now_ms());
    hub.publish();
    result
}

/// Boştaki Claude oturumuna adadan görev: tur sonunda bekleyen köprü Claude'u uyandırır.
#[tauri::command]
pub fn agents_task(app: AppHandle, hub: State<'_, Arc<AgentHub>>, session: String, text: String) -> Result<(), String> {
    if !crate::settings::current(&app).features.agent_notes {
        return Err("Ajana not kapalı (Ayarlar → Özellikler).".into());
    }
    // Zodyvel'in başlattığı oturumda köprü beklemez: görev sürecin girdisine yazılır.
    let direct = hub.lock().launched_live(&session);
    let result = if direct {
        let tool_id = session.strip_prefix("claude:").unwrap_or(&session).to_owned();
        let text = hub.lock().direct_task(&session, &text, now_ms());
        text.and_then(|text| crate::tasks::send(&app, &tool_id, &text))
    } else {
        hub.lock().send_task(&session, &text, now_ms())
    };
    hub.publish();
    result
}

/// Sıradaki notu geri al (henüz iletilmediyse).
#[tauri::command]
pub fn agents_note_remove(hub: State<'_, Arc<AgentHub>>, session: String, id: u64) -> Result<(), String> {
    let result = hub.lock().remove_note(&session, id);
    hub.publish();
    result
}

/// VS Code'un kurulu yeri (kullanıcı ya da makine kurulumu).
fn vscode() -> Option<std::path::PathBuf> {
    let under = |var: &str, rest: &[&str]| {
        let mut path = std::path::PathBuf::from(std::env::var_os(var)?);
        path.extend(rest);
        path.is_file().then_some(path)
    };
    under("LOCALAPPDATA", &["Programs", "Microsoft VS Code", "Code.exe"]).or_else(|| under("ProgramFiles", &["Microsoft VS Code", "Code.exe"]))
}

/// "VS Code ↗" düğmesi yalnız VS Code kuruluysa görünür.
#[tauri::command]
pub fn agents_editor_present() -> bool {
    vscode().is_some()
}

/// Oturumun klasörünü Gezgin'de (`folder`) ya da VS Code'da (`code`) açar. Yalnız var olan bir klasör açılır.
#[tauri::command]
pub fn agents_open(hub: State<'_, Arc<AgentHub>>, session: String, target: String) -> Result<(), String> {
    let folder = hub.lock().folder(&session).ok_or("Bu oturumun klasörü bilinmiyor.")?;
    if !std::path::Path::new(&folder).is_dir() {
        return Err("Klasör bulunamadı.".into());
    }
    match target.as_str() {
        "code" => {
            let code = vscode().ok_or("VS Code bulunamadı.")?;
            std::process::Command::new(code).arg(&folder).spawn().map(|_| ()).map_err(|_| "VS Code açılamadı.".into())
        }
        _ if crate::win::shell_open(&folder) => Ok(()),
        _ => Err("Klasör açılamadı.".into()),
    }
}

/// `kind`: "question", "plan" ya da (varsayılan) onay.
#[tauri::command]
pub fn agents_demo(hub: State<'_, Arc<AgentHub>>, kind: Option<String>) {
    let kind = match kind.as_deref() {
        Some("question") => state::RequestKind::Question,
        Some("plan") => state::RequestKind::Plan,
        _ => state::RequestKind::Permission,
    };
    hub.lock().demo_request(now_ms(), kind);
    hub.publish();
}
