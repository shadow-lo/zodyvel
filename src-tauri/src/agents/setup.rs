//! Claude Code ve Codex bağlantı kurulumu.
//!
//! - Ayar dosyasında yalnız bizim kayıtlarımız (`--zodyvel-hook-v1`) eklenir/kaldırılır; diğer her şey korunur.
//! - Önce önizleme: hangi dosya, ne değişecek, yedek nereye. Uygulama yalnız önizlemeyle aynı dosya içeriğine yapılır.
//! - Bozuk JSON veya bağlantı (symlink) olan dosyalar otomatik düzenlenmez.
//! - Eski İzlek kayıtları yalnız kullanıcı isterse temizlenir.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Map, Value, json};
use tauri::State;
use zodyvel_hook::{AWAIT_FLAG, AWAIT_TIMEOUT_SECS, DECISION_TIMEOUT_SECS, MARKER, Mode, PLAN_TIMEOUT_SECS, Provider};

use super::{AgentHub, hookbin, now_ms};

const LEGACY_MARKER: &str = "--izlek-integration-v1";
const LEGACY_STATUS: &str = "Izlek local task panel";
const PREVIEW_TTL: Duration = Duration::from_secs(600);

const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "SessionEnd",
];
const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "SubagentStart",
    "SubagentStop",
    "Interrupt",
    "SessionEnd",
];

pub fn target_path(provider: Provider) -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    match provider {
        Provider::Claude => std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".claude")).join("settings.json"),
        Provider::Codex => std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex")).join("hooks.json"),
    }
}

fn timeout(event: &str, mode: Mode) -> u64 {
    match event {
        // Plan da bir onay isteğidir ve okunması uzun sürer: araç kendi onay penceresini aynı anda
        // gösterdiği için uzun bekleme kullanıcıyı araçta bekletmez.
        "PermissionRequest" if mode == Mode::Interactive => PLAN_TIMEOUT_SECS + 15,
        // Soru aracı PreToolUse'da adadan cevaplanır: onay kadar beklenebilmeli.
        "PreToolUse" if mode == Mode::Interactive => DECISION_TIMEOUT_SECS + 15,
        "SessionEnd" | "Interrupt" => 2,
        _ => 5,
    }
}

/// Codex komutu `cmd /C` ile çalıştırır: boşluklu yolda kısa (8.3) yol, o da yoksa tırnak.
fn shell_path(path: &str) -> String {
    if !path.contains(' ') {
        return path.to_owned();
    }
    let short = short_path(path).filter(|p| !p.contains(' '));
    short.unwrap_or_else(|| format!("\"{path}\""))
}

pub(super) fn short_path(path: &str) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetShortPathNameW;
    use windows::core::HSTRING;
    let wide = HSTRING::from(path);
    let mut buffer = vec![0u16; 1024];
    let len = unsafe { GetShortPathNameW(&wide, Some(&mut buffer)) } as usize;
    (len > 0 && len < buffer.len()).then(|| String::from_utf16_lossy(&buffer[..len]))
}

pub fn handler(provider: Provider, mode: Mode, event: &str, hook_path: &str) -> Value {
    let seconds = timeout(event, mode);
    match provider {
        // Exec biçimi: kabuk başlatılmaz, köprü doğrudan çalışır.
        Provider::Claude => json!({
            "type": "command",
            "command": hook_path,
            "args": [MARKER, "--provider", "claude", "--mode", mode.as_str()],
            "timeout": seconds,
        }),
        Provider::Codex => {
            let command = format!("{} {MARKER} --provider codex --mode {}", shell_path(hook_path), mode.as_str());
            let mut value = json!({ "type": "command", "command": command, "commandWindows": command, "timeout": seconds });
            if event == "PermissionRequest" && mode == Mode::Interactive {
                value["statusMessage"] = json!("Zodyvel adasında onayın bekleniyor");
            }
            value
        }
    }
}

/// Claude Code, adadan onay modu: tur sonunda arka planda adadan görev bekleyen ikinci kayıt
/// (`asyncRewake`: oturumu meşgul etmez, görev gelince Claude'u uyandırır).
fn await_handler(hook_path: &str) -> Value {
    json!({
        "type": "command",
        "command": hook_path,
        "args": [MARKER, "--provider", "claude", "--mode", Mode::Interactive.as_str(), AWAIT_FLAG],
        "timeout": AWAIT_TIMEOUT_SECS,
        "asyncRewake": true,
    })
}

fn handlers(provider: Provider, mode: Mode, event: &str, hook_path: &str) -> Vec<Value> {
    let mut list = vec![handler(provider, mode, event, hook_path)];
    if provider == Provider::Claude && mode == Mode::Interactive && event == "Stop" {
        list.push(await_handler(hook_path));
    }
    list
}

fn events(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => CLAUDE_EVENTS,
        Provider::Codex => CODEX_EVENTS,
    }
}

fn contains(handler: &Value, needle: &str) -> bool {
    let in_args = handler.get("args").and_then(Value::as_array).is_some_and(|a| a.iter().any(|v| v.as_str() == Some(needle)));
    let in_text = ["command", "commandWindows"].iter().any(|k| handler.get(*k).and_then(Value::as_str).is_some_and(|s| s.contains(needle)));
    in_args || in_text
}

fn is_ours(handler: &Value) -> bool {
    contains(handler, MARKER)
}

fn is_legacy(handler: &Value) -> bool {
    contains(handler, LEGACY_MARKER) || handler.get("statusMessage").and_then(Value::as_str) == Some(LEGACY_STATUS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install { mode: Mode, remove_legacy: bool },
    Remove { remove_legacy: bool },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub ours: usize,
    pub interactive: bool,
    pub legacy: usize,
    pub events: Vec<String>,
    /// Bizim PreToolUse kaydımız soru cevabını bekleyebilecek kadar uzun süreli mi.
    pub waits_on_pre_tool: bool,
    /// Bizim PermissionRequest kaydımız planın okunmasını bekleyebilecek kadar uzun süreli mi.
    pub waits_on_plan: bool,
    /// Tur sonunda adadan görev bekleyen kayıt var mı.
    pub awaits_task: bool,
}

pub fn parse(text: Option<&str>) -> Result<Map<String, Value>, String> {
    let Some(text) = text else { return Ok(Map::new()) };
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("Dosya bir JSON nesnesi değil; otomatik düzenlenmedi.".into()),
        Err(e) => Err(format!("Dosya geçerli JSON değil (satır {}); otomatik düzenlenmedi.", e.line())),
    }
}

pub fn scan(root: &Map<String, Value>) -> Found {
    let mut found = Found::default();
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else { return found };
    for (event, groups) in hooks {
        for group in groups.as_array().into_iter().flatten() {
            for handler in group.get("hooks").and_then(Value::as_array).into_iter().flatten() {
                if is_ours(handler) {
                    found.ours += 1;
                    found.interactive |= contains(handler, "interactive");
                    let timeout = handler.get("timeout").and_then(Value::as_u64).unwrap_or(0);
                    found.waits_on_pre_tool |= event == "PreToolUse" && timeout > DECISION_TIMEOUT_SECS;
                    found.waits_on_plan |= event == "PermissionRequest" && timeout > PLAN_TIMEOUT_SECS;
                    found.awaits_task |= event == "Stop" && contains(handler, AWAIT_FLAG);
                    if !found.events.contains(event) {
                        found.events.push(event.clone());
                    }
                } else if is_legacy(handler) {
                    found.legacy += 1;
                }
            }
        }
    }
    found
}

pub struct Plan {
    pub text: String,
    pub changes: Vec<String>,
}

/// Yeni dosya içeriğini ve değişiklik özetini üretir. Diskte hiçbir şey değiştirmez.
pub fn plan(provider: Provider, root: Map<String, Value>, action: Action, hook_path: &str) -> Result<Plan, String> {
    let mut root = root;
    let before = scan(&root);
    let remove_legacy = match action {
        Action::Install { remove_legacy, .. } | Action::Remove { remove_legacy } => remove_legacy,
    };
    if root.get("hooks").is_some_and(|h| !h.is_object()) {
        return Err("`hooks` alanı beklenen biçimde değil; otomatik düzenlenmedi.".into());
    }
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new())).as_object_mut().expect("nesne");

    for groups in hooks.values_mut() {
        let Some(list) = groups.as_array_mut() else { continue };
        for group in list.iter_mut() {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|h| !(is_ours(h) || (remove_legacy && is_legacy(h))));
            }
        }
        list.retain(|g| g.get("hooks").and_then(Value::as_array).is_none_or(|h| !h.is_empty()));
    }
    hooks.retain(|_, groups| groups.as_array().is_none_or(|g| !g.is_empty()));

    let mut changes = Vec::new();
    if let Action::Install { mode, .. } = action {
        for event in events(provider) {
            let entry = hooks.entry(event.to_string()).or_insert_with(|| Value::Array(Vec::new()));
            let Some(list) = entry.as_array_mut() else {
                return Err(format!("`{event}` kaydı beklenen biçimde değil; otomatik düzenlenmedi."));
            };
            list.push(json!({ "hooks": handlers(provider, mode, event, hook_path) }));
        }
        let how = match mode {
            Mode::Interactive => format!(
                "onaylar adadan verilir, karar {DECISION_TIMEOUT_SECS} sn (plan {} dk) beklenir, sonra araçtaki normal onaya döner",
                PLAN_TIMEOUT_SECS / 60
            ),
            Mode::Observe => "yalnız izleme, onaylar araçta kalır".into(),
        };
        changes.push(format!("{} olay için Zodyvel kaydı eklenecek ({how}).", events(provider).len()));
        if provider == Provider::Claude && mode == Mode::Interactive {
            changes.push(format!(
                "Tur bitince arka planda adadan görev beklenir (en çok {} saat); gönderdiğin görevle Claude uyanır.",
                AWAIT_TIMEOUT_SECS / 3600
            ));
        }
    }
    if before.ours > 0 {
        changes.push(format!("Önceki Zodyvel kayıtları ({}) kaldırılacak.", before.ours));
    }
    if remove_legacy && before.legacy > 0 {
        changes.push(format!("Eski İzlek kayıtları ({}) kaldırılacak.", before.legacy));
    } else if before.legacy > 0 {
        changes.push(format!("Eski İzlek kayıtları ({}) olduğu gibi kalacak.", before.legacy));
    }
    if matches!(action, Action::Remove { .. }) && before.ours == 0 && !(remove_legacy && before.legacy > 0) {
        changes.push("Kaldırılacak Zodyvel kaydı yok.".into());
    }
    changes.push("Diğer ayarların ve hook'ların değişmeden kalır.".into());
    if root.get("hooks").and_then(Value::as_object).is_some_and(Map::is_empty) {
        root.remove("hooks");
    }
    let mut text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(Plan { text, changes })
}

fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err("Dosya bir bağlantı (symlink); otomatik düzenlenmedi.".into()),
        Ok(_) => fs::read(path).map(Some).map_err(|e| format!("Dosya okunamadı: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Dosya okunamadı: {e}")),
    }
}

pub(super) fn decode(bytes: &Option<Vec<u8>>) -> Result<Option<String>, String> {
    match bytes {
        None => Ok(None),
        Some(b) => String::from_utf8(b.clone()).map(Some).map_err(|_| "Dosya UTF-8 değil; otomatik düzenlenmedi.".into()),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub provider: &'static str,
    pub path: String,
    pub exists: bool,
    pub installed: bool,
    pub mode: Option<&'static str>,
    pub events: Vec<String>,
    pub legacy: usize,
    pub error: Option<String>,
    pub hook_ready: bool,
    /// Adadan onay kurulu ama kayıtlar eski: seçenekli sorular ya da plan adadan yanıtlanamaz, yeniden kurulmalı.
    pub needs_update: bool,
    pub last_event_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewView {
    pub id: String,
    pub provider: &'static str,
    pub path: String,
    pub backup_path: Option<String>,
    pub changes: Vec<String>,
    pub content: String,
}

struct Preview {
    path: PathBuf,
    backup: Option<PathBuf>,
    before: Option<u64>,
    text: String,
    created: Instant,
}

#[derive(Default)]
pub struct Setup {
    previews: Mutex<HashMap<String, Preview>>,
    counter: AtomicU64,
}

pub fn inspect(provider: Provider, last_event_at: Option<u64>) -> Inspection {
    let path = target_path(provider);
    let mut out = Inspection {
        provider: provider.as_str(),
        path: path.display().to_string(),
        exists: path.exists(),
        installed: false,
        mode: None,
        events: Vec::new(),
        legacy: 0,
        error: None,
        hook_ready: hookbin::ready(),
        needs_update: false,
        last_event_at,
    };
    match read(&path).and_then(|b| decode(&b)).and_then(|t| parse(t.as_deref())) {
        Ok(root) => {
            let found = scan(&root);
            out.installed = found.ours > 0;
            out.mode = out.installed.then_some(if found.interactive { "interactive" } else { "observe" });
            out.events = found.events;
            out.legacy = found.legacy;
            out.needs_update =
                provider == Provider::Claude && found.interactive && !(found.waits_on_pre_tool && found.waits_on_plan && found.awaits_task);
        }
        Err(e) => out.error = Some(e),
    }
    out
}

fn parse_provider(value: &str) -> Result<Provider, String> {
    Provider::parse(value).ok_or_else(|| "Geçersiz araç.".into())
}

fn inspections(hub: &AgentHub) -> Vec<Inspection> {
    let last = hub.lock().snapshot().last_events;
    vec![inspect(Provider::Claude, last.claude), inspect(Provider::Codex, last.codex)]
}

#[tauri::command]
pub fn connections_inspect(hub: State<'_, std::sync::Arc<AgentHub>>) -> Vec<Inspection> {
    inspections(&hub)
}

#[tauri::command]
pub fn connections_prepare(
    setup: State<'_, Setup>,
    provider: String,
    action: String,
    mode: Option<String>,
    remove_legacy: bool,
) -> Result<PreviewView, String> {
    let provider = parse_provider(&provider)?;
    let action = match action.as_str() {
        "install" => Action::Install {
            mode: mode.as_deref().and_then(Mode::parse).ok_or("Geçersiz bağlantı modu.")?,
            remove_legacy,
        },
        "remove" => Action::Remove { remove_legacy },
        _ => return Err("Geçersiz işlem.".into()),
    };
    if matches!(action, Action::Install { .. }) && !hookbin::ready() {
        return Err("zodyvel-hook.exe bulunamadı; uygulamayı yeniden başlatmayı dene.".into());
    }
    let path = target_path(provider);
    let bytes = read(&path)?;
    let root = parse(decode(&bytes)?.as_deref())?;
    let hook_path = hookbin::installed_path().display().to_string();
    let mut plan = plan(provider, root, action, &hook_path)?;
    let codex_trust = provider == Provider::Codex && matches!(action, Action::Install { .. });
    let mut view = remember(&setup, provider, path, &bytes, &mut plan);
    if codex_trust {
        view.changes.push("Sonra Codex'te /hooks ile yeni kayıtları gözden geçirip güvenilir olarak işaretlemelisin.".into());
    }
    Ok(view)
}

/// Hazırlanan dosya içeriğini önizleme olarak saklar; `connections_apply` aynen uygular. Durum satırı
/// kurulumu da (statusline.rs) bunu kullanır. `bytes`: dosyanın önizleme anındaki içeriği.
pub(super) fn remember(setup: &Setup, provider: Provider, path: PathBuf, bytes: &Option<Vec<u8>>, plan: &mut Plan) -> PreviewView {
    let id = format!("prev-{}-{}", now_ms(), setup.counter.fetch_add(1, Ordering::Relaxed));
    // Önizlemede gösterilen yedek adı uygulamada aynen kullanılır.
    let backup = bytes.as_ref().map(|_| backup_path(&path));
    let mut changes = std::mem::take(&mut plan.changes);
    if let Some(backup) = &backup {
        changes.push(format!("Mevcut dosya önce yedeklenecek: {}", backup.display()));
    } else {
        changes.push("Dosya yok; yeni oluşturulacak.".into());
    }
    let view = PreviewView {
        id: id.clone(),
        provider: provider.as_str(),
        path: path.display().to_string(),
        backup_path: backup.as_ref().map(|b| b.display().to_string()),
        changes,
        content: plan.text.clone(),
    };
    let mut previews = setup.previews.lock().unwrap_or_else(|p| p.into_inner());
    previews.retain(|_, p| p.created.elapsed() < PREVIEW_TTL);
    previews.insert(id, Preview { path, backup, before: bytes.as_deref().map(digest), text: plan.text.clone(), created: Instant::now() });
    view
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!("{name}.zodyvel-backup-{}.json", now_ms()))
}

#[tauri::command]
pub fn connections_apply(
    setup: State<'_, Setup>,
    hub: State<'_, std::sync::Arc<AgentHub>>,
    id: String,
) -> Result<Vec<Inspection>, String> {
    let preview = {
        let mut previews = setup.previews.lock().unwrap_or_else(|p| p.into_inner());
        previews.remove(&id).ok_or("Önizlemenin süresi doldu; değişiklikleri yeniden göster.")?
    };
    if preview.created.elapsed() >= PREVIEW_TTL {
        return Err("Önizlemenin süresi doldu; değişiklikleri yeniden göster.".into());
    }
    let current = read(&preview.path)?;
    if current.as_deref().map(digest) != preview.before {
        return Err("Dosya önizlemeden sonra değişti; üzerine yazılmadı. Değişiklikleri yeniden göster.".into());
    }
    if let Some(dir) = preview.path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Klasör oluşturulamadı: {e}"))?;
    }
    if let Some(bytes) = &current {
        let backup = preview.backup.clone().unwrap_or_else(|| backup_path(&preview.path));
        fs::write(backup, bytes).map_err(|e| format!("Yedek alınamadı; dosyaya dokunulmadı: {e}"))?;
    }
    let tmp = preview.path.with_extension("json.zodyvel-tmp");
    fs::write(&tmp, &preview.text).map_err(|e| format!("Yazılamadı: {e}"))?;
    fs::rename(&tmp, &preview.path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Yazılamadı: {e}")
    })?;
    Ok(inspections(&hub))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOK: &str = r"C:\Users\a\AppData\Local\Zodyvel\bin\zodyvel-hook.exe";

    fn root(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    fn reparse(plan: &Plan) -> Map<String, Value> {
        parse(Some(&plan.text)).unwrap()
    }

    #[test]
    fn claude_install_keeps_other_settings_and_hooks() {
        let existing = root(json!({
            "model": "opus",
            "permissions": { "allow": ["Bash(ls)"] },
            "hooks": { "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-check.sh" }] }] }
        }));
        let plan = plan(Provider::Claude, existing, Action::Install { mode: Mode::Interactive, remove_legacy: false }, HOOK).unwrap();
        let out = reparse(&plan);
        assert_eq!(out["model"], "opus");
        assert_eq!(out["permissions"]["allow"][0], "Bash(ls)");
        let pre = out["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0]["hooks"][0]["command"], "my-check.sh");
        let ours = &pre[1]["hooks"][0];
        assert_eq!(ours["command"], HOOK);
        assert_eq!(ours["args"][0], MARKER);
        let perm = &out["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(perm["timeout"], PLAN_TIMEOUT_SECS + 15);
        assert_eq!(pre[1]["hooks"][0]["timeout"], DECISION_TIMEOUT_SECS + 15);
        // Stop'ta ikinci kayıt: tur sonunda adadan görev bekleyen arka plan köprüsü.
        assert_eq!(scan(&out).ours, CLAUDE_EVENTS.len() + 1);
        let waiter = &out["hooks"]["Stop"][0]["hooks"][1];
        assert_eq!((waiter["asyncRewake"].as_bool(), waiter["args"][5].as_str()), (Some(true), Some(AWAIT_FLAG)));
        assert!(scan(&out).awaits_task);
        assert!(scan(&out).interactive);
        // Anahtar sırası korunur.
        assert_eq!(out.keys().next().map(String::as_str), Some("model"));
    }

    #[test]
    fn reinstall_is_idempotent_and_mode_change_replaces() {
        let first = plan(Provider::Codex, Map::new(), Action::Install { mode: Mode::Interactive, remove_legacy: false }, HOOK).unwrap();
        let second = plan(Provider::Codex, reparse(&first), Action::Install { mode: Mode::Observe, remove_legacy: false }, HOOK).unwrap();
        let out = reparse(&second);
        let found = scan(&out);
        assert_eq!(found.ours, CODEX_EVENTS.len());
        assert!(!found.interactive);
        assert_eq!(out["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn codex_handler_uses_cmd_friendly_command() {
        let value = handler(Provider::Codex, Mode::Interactive, "PermissionRequest", HOOK);
        let cmd = value["commandWindows"].as_str().unwrap();
        assert!(cmd.starts_with(HOOK));
        assert!(cmd.ends_with("--provider codex --mode interactive"));
        assert!(value["statusMessage"].is_string());
        assert!(handler(Provider::Codex, Mode::Interactive, "Stop", HOOK).get("statusMessage").is_none());
        assert_eq!(handler(Provider::Codex, Mode::Observe, "Interrupt", HOOK)["timeout"], 2);
    }

    #[test]
    fn remove_takes_only_ours_and_legacy_on_request() {
        let legacy = json!({ "type": "command", "command": "'node' 'x/agent-hook.cjs' '--izlek-integration-v1'", "statusMessage": LEGACY_STATUS });
        let other = json!({ "type": "command", "command": "notify.exe" });
        let installed = plan(Provider::Codex, root(json!({ "hooks": { "Stop": [{ "hooks": [legacy, other] }] } })), Action::Install { mode: Mode::Interactive, remove_legacy: false }, HOOK).unwrap();
        let mid = reparse(&installed);
        assert_eq!(scan(&mid).legacy, 1);

        let keep_legacy = reparse(&plan(Provider::Codex, mid.clone(), Action::Remove { remove_legacy: false }, HOOK).unwrap());
        assert_eq!(scan(&keep_legacy), Found { ours: 0, interactive: false, legacy: 1, events: vec![], waits_on_pre_tool: false, waits_on_plan: false, awaits_task: false });

        let clean = reparse(&plan(Provider::Codex, mid, Action::Remove { remove_legacy: true }, HOOK).unwrap());
        assert_eq!(scan(&clean).legacy, 0);
        let stop = clean["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(stop[0]["hooks"][0]["command"], "notify.exe");
        assert!(clean["hooks"].get("PermissionRequest").is_none());
    }

    #[test]
    fn removing_everything_drops_empty_hooks_object() {
        let installed = plan(Provider::Codex, Map::new(), Action::Install { mode: Mode::Observe, remove_legacy: false }, HOOK).unwrap();
        let removed = plan(Provider::Codex, reparse(&installed), Action::Remove { remove_legacy: false }, HOOK).unwrap();
        assert_eq!(removed.text.trim(), "{}");
    }

    #[test]
    fn broken_files_are_never_rewritten() {
        assert!(parse(Some("{ bozuk")).is_err());
        assert!(parse(Some("[1,2]")).is_err());
        assert!(parse(Some("\u{feff}{}")).unwrap().is_empty());
        assert!(plan(Provider::Claude, root(json!({ "hooks": [] })), Action::Install { mode: Mode::Observe, remove_legacy: false }, HOOK).is_err());
    }

    #[test]
    fn digest_detects_change_and_backup_is_named_next_to_file() {
        let dir = std::env::temp_dir().join(format!("zodyvel-setup-test-{}", now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("hooks.json");
        fs::write(&file, "{}").unwrap();
        let before = read(&file).unwrap().as_deref().map(digest);
        fs::write(&file, "{\"x\":1}").unwrap();
        let after = read(&file).unwrap().as_deref().map(digest);
        assert_ne!(before, after);
        assert!(backup_path(&file).to_string_lossy().contains("hooks.json.zodyvel-backup-"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_interactive_install_needs_update_for_questions() {
        let fresh = reparse(&plan(Provider::Claude, Map::new(), Action::Install { mode: Mode::Interactive, remove_legacy: false }, HOOK).unwrap());
        assert!(scan(&fresh).waits_on_pre_tool);
        let mut old = fresh.clone();
        old["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"] = json!(5);
        let found = scan(&old);
        assert!(found.interactive && !found.waits_on_pre_tool);
    }

    #[test]
    fn old_interactive_install_needs_update_for_plans() {
        let fresh = reparse(&plan(Provider::Claude, Map::new(), Action::Install { mode: Mode::Interactive, remove_legacy: false }, HOOK).unwrap());
        assert!(scan(&fresh).waits_on_plan);
        let mut old = fresh.clone();
        old["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = json!(DECISION_TIMEOUT_SECS + 15);
        let found = scan(&old);
        assert!(found.interactive && found.waits_on_pre_tool && !found.waits_on_plan);
        // Gözlem kipinde uzun bekleme yok.
        let observe = reparse(&plan(Provider::Claude, Map::new(), Action::Install { mode: Mode::Observe, remove_legacy: false }, HOOK).unwrap());
        assert!(!scan(&observe).waits_on_plan);
        assert!(!scan(&observe).awaits_task);
    }
}
