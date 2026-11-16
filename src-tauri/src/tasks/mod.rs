//! Adadan yeni görev ve kapanmış oturuma devam (plan 1.3, 1.4). Claude Code arka planda, pencere açmadan
//! `claude -p` olarak oturumun klasöründe başlar; oturum hook köprüsüyle adaya düşer, onaylar mevcut
//! onay kartından gelir (`-p`'de onay isteyen her araç PermissionRequest hook'una sorulur). İstek
//! `--input-format stream-json` ile girdiden verilir: komut satırında metin yok (Windows tırnaklama,
//! uzunluk sınırı), girdi açık kaldıkça oturum sonraki görevleri de alır. Boşta `IDLE` kalan sürecin
//! girdisi kapanır, süreç biter; oturum o zaman "Kapanmış" listesine düşer ve `--resume` ile sürer.

pub mod history;

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::agents::AgentHub;

/// Boştaki (tur bitmiş) başlatılmış sürecin girdisi bu kadar sonra kapanır.
const IDLE: Duration = Duration::from_secs(30 * 60);
/// Hata mesajı için saklanan en çok stderr baytı.
const STDERR_KEEP: usize = 4096;
/// İstek metninin üst sınırı (karakter).
const MAX_PROMPT: usize = 20_000;

#[derive(Default)]
pub struct TaskHub {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Son verilen görev numarası (uygulama açıkken artar).
    last_no: u32,
    running: Vec<Running>,
    reaper: bool,
}

struct Running {
    /// Aracın oturum kimliği (UUID).
    session: String,
    stdin: Option<ChildStdin>,
    /// Tur sürüyor (girdiye yazıldı, sonuç gelmedi).
    busy: bool,
    last: Instant,
    /// Sürecin kimliği: mini AI'daki "Durdur" bu süreci (ve alt süreçlerini) kapatır (Faz 9).
    pid: u32,
    /// Kullanıcı durdurdu: süreç kapanınca hata bildirimi çıkmaz.
    stopped: bool,
}

impl TaskHub {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    pub task_no: u32,
    pub session_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Failed {
    session_id: String,
    task_no: Option<u32>,
    message: String,
}

pub fn init(app: &AppHandle) {
    app.manage(Arc::new(TaskHub::default()));
}

fn enabled(app: &AppHandle) -> Result<(), String> {
    if !crate::settings::current(app).features.new_task {
        return Err("Yeni görev kapalı (Ayarlar → Özellikler).".into());
    }
    let agents = app.state::<Arc<AgentHub>>();
    if !agents.enabled() {
        return Err("AI modülü kapalı (Ayarlar → Modüller).".into());
    }
    Ok(())
}

/// Claude Code'un kayıt klasörü: `CLAUDE_CONFIG_DIR` ya da `%USERPROFILE%\.claude`, altında `projects`.
pub(crate) fn projects_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(".claude"))
        .join("projects")
}

/// `claude` komutu: PATH'te `claude.exe` (yerel kurulum) ya da `claude.cmd` (npm), yoksa bilinen yerler.
pub(crate) fn claude_exe() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in names {
        if let Some(found) = std::env::split_paths(&path).map(|dir| dir.join(name)).find(|p| p.is_file()) {
            return Some(found);
        }
    }
    let under = |var: &str, rest: &[&str]| {
        let mut path = PathBuf::from(std::env::var_os(var)?);
        path.extend(rest);
        path.is_file().then_some(path)
    };
    under("USERPROFILE", &[".local", "bin", "claude.exe"]).or_else(|| under("APPDATA", &["npm", "claude.cmd"]))
}

/// Girdiye yazılan bir kullanıcı mesajı (stream-json).
fn user_line(session: &str, text: &str) -> String {
    message_line(session, Value::from(text))
}

/// İçeriği metin ya da blok dizisi (metin + resim; raftan eklenen dosyalar) olan kullanıcı mesajı.
fn message_line(session: &str, content: Value) -> String {
    let line = json!({
        "type": "user",
        "message": { "role": "user", "content": content },
        "parent_tool_use_id": null,
        "session_id": session,
    });
    format!("{line}\n")
}

/// Sürüm 4 biçiminde rastgele bir UUID (std'nin rastgele tohumlu karmasından; kimlik için yeterli).
fn new_uuid() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

fn clean_prompt(prompt: &str) -> Result<String, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Görevi yaz.".into());
    }
    if prompt.chars().count() > MAX_PROMPT {
        return Err("Görev çok uzun.".into());
    }
    Ok(prompt.to_owned())
}

fn folder(cwd: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(cwd.trim());
    if cwd.trim().is_empty() || !path.is_absolute() || !path.is_dir() {
        return Err("Klasör bulunamadı.".into());
    }
    Ok(path)
}

/// Claude Code'u klasörde başlatır, ilk isteği girdiye yazar, çıktıyı ve bitişi izleyen iş parçacığını kurar.
fn spawn(app: &AppHandle, cwd: &Path, args: &[&str], session: &str, no: Option<u32>, prompt: &str) -> Result<(), String> {
    let exe = claude_exe().ok_or("Claude Code bulunamadı (claude komutu PATH'te yok).")?;
    let mut command = Command::new(exe);
    command
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Konsol penceresi açılmasın.
        command.creation_flags(0x0800_0000);
    }
    let agents = app.state::<Arc<AgentHub>>();
    // Oturum adaya düşmeden önce numara ve "terminalsiz" işareti hazır olsun.
    agents.launch(session, no);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            agents.launch_ended(session);
            return Err("Claude Code başlatılamadı.".into());
        }
    };
    let mut stdin = child.stdin.take();
    let written = stdin.as_mut().is_some_and(|s| s.write_all(user_line(session, prompt).as_bytes()).and_then(|_| s.flush()).is_ok());
    if !written {
        let _ = child.kill();
        let _ = child.wait();
        agents.launch_ended(session);
        return Err("Görev Claude Code'a iletilemedi.".into());
    }
    let hub = app.state::<Arc<TaskHub>>().inner().clone();
    let pid = child.id();
    hub.lock().running.push(Running { session: session.to_owned(), stdin, busy: true, last: Instant::now(), pid, stopped: false });
    start_reaper(&hub);

    let stderr = child.stderr.take();
    let stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut err) = stderr {
            let _ = err.by_ref().take(64 * 1024).read_to_end(&mut bytes);
            std::io::copy(&mut err, &mut std::io::sink()).ok();
        }
        let text = String::from_utf8_lossy(&bytes).trim().to_owned();
        text.chars().rev().take(STDERR_KEEP).collect::<Vec<_>>().into_iter().rev().collect::<String>()
    });
    let stdout = child.stdout.take();
    let app = app.clone();
    let session = session.to_owned();
    std::thread::spawn(move || {
        let mut reported = false;
        let mut started = false;
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                match value.get("type").and_then(Value::as_str) {
                    Some("system") => started = true,
                    Some("result") => {
                        if let Some(r) = hub.lock().running.iter_mut().find(|r| r.session == session) {
                            r.busy = false;
                            r.last = Instant::now();
                        }
                        if value.get("is_error").and_then(Value::as_bool) == Some(true) {
                            let text = value.get("result").and_then(Value::as_str).unwrap_or_default();
                            fail(&app, &session, no, &short(if text.is_empty() { "Görev hatayla bitti." } else { text }));
                            reported = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        let status = child.wait();
        let err = stderr.join().unwrap_or_default();
        let stopped = {
            let mut inner = hub.lock();
            let stopped = inner.running.iter().any(|r| r.session == session && r.stopped);
            inner.running.retain(|r| r.session != session);
            stopped
        };
        let agents = app.state::<Arc<AgentHub>>();
        agents.launch_ended(&session);
        if stopped {
            agents.stopped(&session);
            return;
        }
        if !reported && !status.is_ok_and(|s| s.success()) {
            let message = if !err.is_empty() {
                short(&err)
            } else if started {
                "Claude Code beklenmedik biçimde kapandı.".into()
            } else {
                "Claude Code başlayamadı.".into()
            };
            fail(&app, &session, no, &message);
        }
    });
    Ok(())
}

fn short(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or(text).trim();
    if line.chars().count() > 160 { format!("{}…", line.chars().take(159).collect::<String>()) } else { line.to_owned() }
}

fn fail(app: &AppHandle, session: &str, task_no: Option<u32>, message: &str) {
    let _ = app.emit("task-failed", Failed { session_id: session.to_owned(), task_no, message: message.to_owned() });
}

/// Boşta kalan süreçlerin girdisini kapatan iş parçacığı: yalnız başlatılmış süreç varken çalışır.
fn start_reaper(hub: &Arc<TaskHub>) {
    {
        let mut inner = hub.lock();
        if inner.reaper {
            return;
        }
        inner.reaper = true;
    }
    let hub = hub.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let mut inner = hub.lock();
            for r in inner.running.iter_mut().filter(|r| !r.busy && r.last.elapsed() >= IDLE) {
                // Girdi kapanınca süreç biter; bitişi çıktıyı izleyen iş parçacığı işler.
                r.stdin = None;
            }
            if inner.running.is_empty() {
                inner.reaper = false;
                return;
            }
        }
    });
}

/// Zodyvel'in başlattığı, girdisi açık oturuma yeni istek (adadaki görev kutusundan, `agents_task`).
pub fn send(app: &AppHandle, session: &str, text: &str) -> Result<(), String> {
    write_line(app, session, &user_line(session, text))
}

/// Raftan bırakılan dosyalar (K5): metin ve resim bloklarıyla bir kullanıcı mesajı. Tur sürerken de
/// yazılabilir; Claude Code girdideki mesajı sıradaki tur olarak alır.
pub fn send_content(app: &AppHandle, session: &str, content: Value) -> Result<(), String> {
    write_line(app, session, &message_line(session, content))
}

fn write_line(app: &AppHandle, session: &str, line: &str) -> Result<(), String> {
    let hub = app.state::<Arc<TaskHub>>();
    let mut inner = hub.lock();
    let running = inner.running.iter_mut().find(|r| r.session == session).ok_or("Bu oturum artık açık değil.")?;
    let stdin = running.stdin.as_mut().ok_or("Bu oturum artık açık değil.")?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|_| "Görev iletilemedi; oturum kapanmış olabilir.".to_owned())?;
    running.busy = true;
    running.last = Instant::now();
    Ok(())
}

/// Mini AI › Durdur (Faz 9): Zodyvel'in başlattığı görevin sürecini alt süreçleriyle kapatır. Dışarıdaki
/// oturumlar (terminal, masaüstü) buradan durdurulamaz; arayüz onlarda pencereyi öne getirir.
#[tauri::command]
pub fn task_stop(hub: State<'_, Arc<TaskHub>>, session: String) -> Result<(), String> {
    let session = session.strip_prefix("claude:").unwrap_or(&session).to_owned();
    let pid = {
        let mut inner = hub.lock();
        let running = inner.running.iter_mut().find(|r| r.session == session).ok_or("Bu görev artık çalışmıyor.")?;
        running.stopped = true;
        running.stdin = None;
        running.pid
    };
    kill_tree(pid)
}

#[cfg(windows)]
pub(crate) fn kill_tree(pid: u32) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status()
        .map_err(|_| "Görev durdurulamadı.".to_owned())?;
    if status.success() { Ok(()) } else { Err("Görev durdurulamadı.".into()) }
}

#[cfg(not(windows))]
pub(crate) fn kill_tree(pid: u32) -> Result<(), String> {
    let status = Command::new("kill").arg(pid.to_string()).status().map_err(|_| "Görev durdurulamadı.".to_owned())?;
    if status.success() { Ok(()) } else { Err("Görev durdurulamadı.".into()) }
}

/// "+ Yeni görev" klasör listesi: son kullanılan proje klasörleri (en çok 8).
#[tauri::command]
pub async fn list_recent_projects(app: AppHandle) -> Result<Vec<history::Recent>, String> {
    enabled(&app)?;
    tauri::async_runtime::spawn_blocking(|| history::recent(&projects_dir(), 8)).await.map_err(|e| e.to_string())
}

/// "Kapanmış ▾": adada açık olmayan son Claude oturumları.
#[tauri::command]
pub async fn list_closed_sessions(app: AppHandle, hub: State<'_, Arc<AgentHub>>, limit: Option<usize>) -> Result<Vec<history::Closed>, String> {
    enabled(&app)?;
    let open = hub.claude_session_ids();
    let limit = limit.unwrap_or(10).clamp(1, 30);
    tauri::async_runtime::spawn_blocking(move || history::closed(&projects_dir(), &open, limit)).await.map_err(|e| e.to_string())
}

/// "Başka klasör…": Windows klasör seçicisi. Vazgeçilirse `None`.
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    enabled(&app)?;
    let owner = app.get_webview_window("island").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    tauri::async_runtime::spawn_blocking(move || crate::win::pick_folder(owner, "Görev için klasör seç")).await.map_err(|e| e.to_string())
}

/// Yeni görev: Claude Code klasörde arka planda başlar. `permission`: "default" ya da "acceptEdits".
#[tauri::command]
pub fn start_task(app: AppHandle, hub: State<'_, Arc<TaskHub>>, cwd: String, prompt: String, permission: String) -> Result<Started, String> {
    enabled(&app)?;
    let cwd = folder(&cwd)?;
    let prompt = clean_prompt(&prompt)?;
    let mode = match permission.as_str() {
        "default" | "acceptEdits" => permission.as_str(),
        _ => return Err("Geçersiz izin kipi.".into()),
    };
    let session = new_uuid();
    let no = {
        let mut inner = hub.lock();
        inner.last_no += 1;
        inner.last_no
    };
    spawn(&app, &cwd, &["--session-id", &session, "--permission-mode", mode], &session, Some(no), &prompt)?;
    Ok(Started { task_no: no, session_id: session })
}

/// Kapanmış oturuma devam: `claude -p --resume <kimlik>` oturumun kendi klasöründe.
#[tauri::command]
pub fn resume_session(app: AppHandle, agents: State<'_, Arc<AgentHub>>, session_id: String, cwd: String, prompt: String) -> Result<(), String> {
    enabled(&app)?;
    if !history::valid_session_id(&session_id) {
        return Err("Geçersiz oturum.".into());
    }
    if agents.claude_session_ids().contains(&session_id) {
        return Err("Bu oturum zaten açık; ona adadan yaz.".into());
    }
    let prompt = clean_prompt(&prompt)?;
    // Klasör kaydın kendisinden alınır; arayüzden gelen yalnız kayıt bulunamazsa kullanılır.
    let recorded = history::session_cwd(&projects_dir(), &session_id);
    let cwd = folder(recorded.as_deref().unwrap_or(&cwd))?;
    spawn(&app, &cwd, &["--resume", &session_id], &session_id, None, &prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_are_valid_and_differ() {
        let a = new_uuid();
        let b = new_uuid();
        assert!(history::valid_session_id(&a), "{a}");
        assert_eq!(&a[14..15], "4");
        assert_ne!(a, b);
    }

    #[test]
    fn user_line_is_one_json_line() {
        let line = user_line("s", "çok \"satırlı\"\nistek");
        assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["message"]["content"], "çok \"satırlı\"\nistek");
        assert_eq!(v["type"], "user");
    }

    #[test]
    fn prompts_and_errors_are_trimmed() {
        assert_eq!(clean_prompt("  yap  ").as_deref(), Ok("yap"));
        assert!(clean_prompt("   ").is_err());
        assert!(clean_prompt(&"a".repeat(MAX_PROMPT + 1)).is_err());
        assert_eq!(short("\n  Not logged in · Please run /login\nmore"), "Not logged in · Please run /login");
        assert!(folder("göreli/yol").is_err());
    }
}
