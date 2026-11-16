//! Oturumun adı ve türü için diskte okunan bilgiler. Durum (`state`) saf kalsın diye kilit dışında,
//! ingest'ten önce okunur ve olaya eklenir.
//!
//! - Proje adı: klasör adı yerine projenin kendi manifestindeki ad ("Psrc" değil "Zodyvel").
//! - Claude masaüstü oturum kaydı: oturumun başlığı ve Remote Control / proje dizisinden açılıp
//!   açılmadığı. Uzaktan oturumda kimse yerel istemde beklemez; "yanıt bekliyor" gösterilmez.
//!
//! Maliyet: proje adı klasör başına bir kez okunur. Oturum kaydı yalnız tur başı/sonunda tazelenir;
//! bulunamayan oturum için klasör en çok dakikada bir taranır ve yalnız son saatlerde değişen dosyalar
//! okunur.

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

const RESCAN_MS: u64 = 60_000;
const RECENT: Duration = Duration::from_secs(3 * 60 * 60);
const MAX_CACHE: usize = 64;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Desktop {
    pub title: Option<String>,
    pub remote: bool,
}

#[derive(Default)]
struct Entry {
    path: Option<PathBuf>,
    scanned_at: Option<u64>,
    info: Desktop,
    /// Gecikmeli başlık okuması planlandı (`wants_retitle`).
    retitling: bool,
}

#[derive(Default)]
struct Cache {
    names: HashMap<String, Option<String>>,
    sessions: HashMap<String, Entry>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Olaya oturum adını (`zodyvel_name`) ve uzaktan oturum işaretini (`zodyvel_remote`) ekler.
pub fn annotate(claude: bool, payload: &Value, now: u64) -> Option<Value> {
    let event = payload.get("hook_event_name")?.as_str()?;
    let session = payload.get("session_id").and_then(Value::as_str).unwrap_or("");
    let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
    let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let cache = guard.get_or_insert_with(Cache::default);

    if event == "SessionEnd" {
        cache.sessions.remove(session);
        return None;
    }
    let desktop = if claude && !session.is_empty() { cache.desktop(session, event, now) } else { Desktop::default() };
    // Kategori: proje (manifest) adı; sohbetin adı masaüstü başlığı, yoksa yine proje adı (Faz 7.1).
    let category = if cwd.is_empty() { None } else { cache.project_name(cwd) };
    let name = desktop.title.clone().or_else(|| category.clone());
    if name.is_none() && !desktop.remote {
        return None;
    }
    let mut out = payload.clone();
    let obj = out.as_object_mut()?;
    if let Some(name) = name {
        obj.insert("zodyvel_name".into(), Value::String(name));
    }
    if let Some(category) = category {
        obj.insert("zodyvel_category".into(), Value::String(category));
    }
    if desktop.remote {
        obj.insert("zodyvel_remote".into(), Value::Bool(true));
    }
    Some(out)
}

/// Tur başında/sonunda başlığı henüz üretilmemiş masaüstü oturumu: Claude masaüstü başlığı ilk
/// mesajdan sonra yazar; kısa bir turda tur sonu olayı başlıktan önce gelir. Bu durumda çağıran bir
/// kez gecikmeli yeniden okuma planlar (`retitle`). Oturum başına aynı anda en çok bir plan.
pub fn wants_retitle(claude: bool, payload: &Value) -> Option<String> {
    let event = payload.get("hook_event_name")?.as_str()?;
    if !claude || !matches!(event, "UserPromptSubmit" | "Stop") {
        return None;
    }
    let session = payload.get("session_id")?.as_str()?;
    let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let entry = guard.as_mut()?.sessions.get_mut(session)?;
    let pending = entry.path.is_some() && entry.info.title.is_none() && !entry.retitling;
    if pending {
        entry.retitling = true;
    }
    pending.then(|| session.to_owned())
}

/// Gecikmeli yeniden okuma: kayıttaki başlık geldiyse onu döndürür ve önbelleğe yazar. `last`:
/// son deneme (sonra plan kalkar; bir sonraki tur yeniden planlayabilir).
pub fn retitle(session: &str, last: bool) -> Option<String> {
    let path = {
        let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        let entry = guard.as_mut()?.sessions.get_mut(session)?;
        if last {
            entry.retitling = false;
        }
        entry.path.clone()?
    };
    // Dosya kilit dışında okunur.
    let info = read_record(&path, session)?;
    let title = info.title.clone()?;
    let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(entry) = guard.as_mut().and_then(|c| c.sessions.get_mut(session)) {
        entry.info = info;
        entry.retitling = false;
    }
    Some(title)
}

impl Cache {
    fn project_name(&mut self, cwd: &str) -> Option<String> {
        if self.names.len() > MAX_CACHE {
            self.names.clear();
        }
        self.names.entry(cwd.to_owned()).or_insert_with(|| manifest_name(Path::new(cwd))).clone()
    }

    fn desktop(&mut self, session: &str, event: &str, now: u64) -> Desktop {
        if self.sessions.len() > MAX_CACHE && !self.sessions.contains_key(session) {
            self.sessions.clear();
        }
        let entry = self.sessions.entry(session.to_owned()).or_default();
        let fresh = entry.scanned_at.is_none();
        // Başlık ilk turdan sonra üretilir ya da sonradan değiştirilebilir: tur başı/sonunda tazelenir.
        let turn = matches!(event, "SessionStart" | "UserPromptSubmit" | "Stop");
        if let Some(path) = entry.path.clone().filter(|_| turn) {
            match read_record(&path, session) {
                Some(info) => entry.info = info,
                None => entry.path = None,
            }
        } else if entry.path.is_none() && (fresh || (turn && entry.scanned_at.is_some_and(|t| now.saturating_sub(t) >= RESCAN_MS))) {
            entry.scanned_at = Some(now);
            if let Some((path, info)) = find_record(session) {
                entry.path = Some(path);
                entry.info = info;
            }
        }
        entry.info.clone()
    }
}

/// Claude masaüstünün oturum kayıtları: `claude-code-sessions\<hesap>\<kuruluş>\local_*.json`.
/// Mağaza (MSIX) kurulumunda AppData paketin kendi klasörüne yönlendirilir.
fn record_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join("Claude").join("claude-code-sessions"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        // Yeni masaüstü sürümleri kayıtları `%LOCALAPPDATA%\Claude-Data\claude_<kimlik>\claude-code-sessions`
        // altına taşıdı (Faz 7 doğrulaması, 10 Kasım); eski yerlerde yalnız işaret dosyası kaldı.
        roots.extend(data_roots(&local.join("Claude-Data")));
        let packages = local.join("Packages");
        for dir in fs::read_dir(packages).into_iter().flatten().flatten() {
            if dir.file_name().to_string_lossy().starts_with("Claude_") {
                roots.push(dir.path().join("LocalCache").join("Roaming").join("Claude").join("claude-code-sessions"));
            }
        }
    }
    roots
}

/// `Claude-Data\claude_*\claude-code-sessions` klasörleri (var olanlar).
fn data_roots(base: &Path) -> Vec<PathBuf> {
    subdirs(base)
        .filter(|d| d.file_name().is_some_and(|n| n.to_string_lossy().starts_with("claude_")))
        .map(|d| d.join("claude-code-sessions"))
        .filter(|d| d.is_dir())
        .collect()
}

fn subdirs(dir: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path())
}

fn find_record(session: &str) -> Option<(PathBuf, Desktop)> {
    let now = SystemTime::now();
    for root in record_roots() {
        for org in subdirs(&root).flat_map(|account| subdirs(&account).collect::<Vec<_>>()) {
            for file in fs::read_dir(&org).into_iter().flatten().flatten() {
                let name = file.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with("local_") || !name.ends_with(".json") {
                    continue;
                }
                let recent = file.metadata().and_then(|m| m.modified()).is_ok_and(|m| now.duration_since(m).unwrap_or_default() < RECENT);
                if !recent {
                    continue;
                }
                if let Some(info) = read_record(&file.path(), session) {
                    return Some((file.path(), info));
                }
            }
        }
    }
    None
}

fn read_record(path: &Path, session: &str) -> Option<Desktop> {
    let text = fs::read_to_string(path).ok()?;
    if !text.contains(session) {
        return None;
    }
    parse_record(&text, session)
}

/// Kayıt bu oturumunsa başlığı ve uzaktan açılıp açılmadığı. Henüz üretilmemiş genel başlık
/// ("Remote Control session …") yok sayılır.
pub fn parse_record(text: &str, session: &str) -> Option<Desktop> {
    let record: Value = serde_json::from_str(text).ok()?;
    if record.get("cliSessionId").and_then(Value::as_str) != Some(session) {
        return None;
    }
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty() && !t.starts_with("Remote Control session"))
        .map(|t| super::describe::short(t, 48));
    let remote = record.get("remoteControlSpawn").is_some_and(Value::is_object);
    Some(Desktop { title, remote })
}

/// Klasörden yukarı, depo köküne kadar ilk manifestteki proje adı. Depo dışındaki klasörde yalnız
/// klasörün kendi manifestine bakılır (ev klasöründeki ilgisiz bir package.json'a çıkılmasın). Geçici
/// klasörler klasör adıyla ("Geçici klasör") kalır.
fn manifest_name(cwd: &Path) -> Option<String> {
    if super::describe::project(&cwd.to_string_lossy()) == super::describe::TEMP_NAME {
        return None;
    }
    manifest_in(cwd)
}

fn manifest_in(cwd: &Path) -> Option<String> {
    let root = cwd.ancestors().position(|dir| dir.join(".git").exists()).unwrap_or(0);
    cwd.ancestors().take(root + 1).find_map(|dir| {
        let package = fs::read_to_string(dir.join("package.json")).ok().and_then(|t| package_name(&t));
        package.or_else(|| fs::read_to_string(dir.join("Cargo.toml")).ok().and_then(|t| cargo_name(&t)))
    })
}

pub fn package_name(text: &str) -> Option<String> {
    let json: Value = serde_json::from_str(text).ok()?;
    let name = ["productName", "displayName", "name"].iter().find_map(|k| json.get(*k)?.as_str().filter(|s| !s.trim().is_empty()))?;
    Some(display(name.rsplit('/').next().unwrap_or(name)))
}

/// `[package]` bölümündeki `name` (çalışma alanı kökünde paket yoksa ad yok).
pub fn cargo_name(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if in_package && let Some(value) = line.strip_prefix("name").map(str::trim_start).and_then(|l| l.strip_prefix('=')) {
            let name = value.trim().trim_matches('"');
            return (!name.is_empty()).then(|| display(name));
        }
    }
    None
}

/// Paket adları genelde küçük harfle yazılır: "zodyvel" → "Zodyvel".
fn display(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if name.chars().all(|c| !c.is_uppercase()) => first.to_uppercase().chain(chars).collect(),
        _ => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_claude_data_roots() {
        let base = std::env::temp_dir().join(format!("zodyvel-data-roots-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("claude_pzs8").join("claude-code-sessions").join("hesap")).unwrap();
        fs::create_dir_all(base.join("claude_bos")).unwrap();
        fs::create_dir_all(base.join("baska").join("claude-code-sessions")).unwrap();
        let roots = data_roots(&base);
        assert_eq!(roots, vec![base.join("claude_pzs8").join("claude-code-sessions")]);
        assert!(data_roots(&base.join("yok")).is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reads_desktop_record() {
        let record = r#"{"sessionId":"local_1","cliSessionId":"abc","title":"Oturum adı ve yanlış durum","remoteControlSpawn":{"rcChild":true}}"#;
        assert_eq!(parse_record(record, "abc"), Some(Desktop { title: Some("Oturum adı ve yanlış durum".into()), remote: true }));
        assert_eq!(parse_record(record, "xyz"), None);
        let local = r#"{"cliSessionId":"abc","title":"Remote Control session (from a project)","remoteControlSpawn":null}"#;
        assert_eq!(parse_record(local, "abc"), Some(Desktop { title: None, remote: false }));
    }

    #[test]
    fn project_name_from_manifests() {
        assert_eq!(package_name(r#"{"name":"zodyvel","version":"1"}"#).as_deref(), Some("Zodyvel"));
        assert_eq!(package_name(r#"{"name":"@acme/web-app"}"#).as_deref(), Some("Web-app"));
        assert_eq!(package_name(r#"{"productName":"Gezgin","name":"track"}"#).as_deref(), Some("Gezgin"));
        assert_eq!(cargo_name("[workspace]\nmembers = [\"a\"]\n\n[package]\nname = \"zodyvel\"\nversion = \"1\"").as_deref(), Some("Zodyvel"));
        assert_eq!(cargo_name("[workspace]\nmembers = [\"a\"]\n[dependencies]\nname = \"x\""), None);
    }

    #[test]
    fn manifest_search_stops_at_the_repository() {
        let base = std::env::temp_dir().join(format!("zodyvel-meta-{}", std::process::id()));
        let repo = base.join("repo");
        let deep = repo.join("src").join("ui");
        let loose = base.join("loose");
        fs::create_dir_all(&deep).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(&loose).unwrap();
        fs::write(base.join("package.json"), r#"{"name":"ev-klasoru"}"#).unwrap();
        fs::write(repo.join("Cargo.toml"), "[package]\nname = \"zodyvel\"\n").unwrap();
        // Depo içinde alt klasörden köke çıkılır; depo dışına taşılmaz.
        assert_eq!(manifest_in(&deep).as_deref(), Some("Zodyvel"));
        fs::remove_file(repo.join("Cargo.toml")).unwrap();
        assert_eq!(manifest_in(&deep), None);
        // Depo dışındaki klasörde yalnız kendi manifesti: üstteki package.json alınmaz.
        assert_eq!(manifest_in(&loose), None);
        assert_eq!(manifest_name(Path::new("C:\\Users\\a\\scratch-2026-10-14-ab12")), None);
        let _ = fs::remove_dir_all(&base);
    }
}
