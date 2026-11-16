//! Claude Code'un yerel kayıtlarından (`~/.claude/projects/<klasör>/<oturum>.jsonl`) yazma sahnesinin
//! listeleri: son proje klasörleri ("+ Yeni görev") ve kapanmış oturumlar ("Kapanmış ▾", `--resume`).
//! Saf modül: Tauri'ye dokunmaz, yalnız dosya okur. Her kaydın yalnız başı okunur.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

/// Bir kaydın başından okunan en çok bayt: klasör ve ilk istek hemen her zaman ilk satırlardadır.
const HEAD: u64 = 256 * 1024;
/// Kapanmış oturum listesi için bakılan en yeni kayıt sayısı (açık olanlar ve boş kayıtlar elenir).
const SCAN: usize = 80;
/// İlk istekten gösterilen en çok karakter.
const PROMPT_CHARS: usize = 120;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub path: String,
    pub name: String,
    pub last_used: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Closed {
    pub session_id: String,
    pub cwd: String,
    pub project: String,
    pub first_prompt: String,
    pub ended_at: u64,
}

/// Oturum kimliği biçimi (UUID): yalnız onaltılık rakam ve tire. Komut satırına giden her kimlik buradan geçer.
pub fn valid_session_id(id: &str) -> bool {
    id.len() == 36
        && id.char_indices().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() })
}

/// Klasörün görünen adı: son parçası.
pub fn folder_name(cwd: &str) -> String {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default();
    if name.is_empty() { cwd.to_owned() } else { name.to_owned() }
}

/// Geçici klasörde açılmış oturumlar (Claude masaüstünün scratch klasörleri vb.) listelere girmez.
fn temporary(cwd: &str) -> bool {
    let lower = cwd.to_lowercase().replace('\\', "/");
    folder_name(cwd).to_lowercase().starts_with("scratch-") || lower.contains("/appdata/local/temp/") || lower.starts_with("/tmp/")
}

fn mtime(path: &Path) -> u64 {
    path.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Ana oturum kayıtları (alt ajan kayıtları alt klasörlerdedir, girmez), yeniden eskiye.
fn sessions(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if !dir.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for file in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let path = file.path();
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            if file.file_type().is_ok_and(|t| t.is_file()) && path.extension().is_some_and(|e| e == "jsonl") && valid_session_id(stem) {
                let at = mtime(&path);
                out.push((path, at));
            }
        }
    }
    out.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    out
}

/// Kullanıcının yazdığı bir istek mi (araç sonucu, komut çıktısı, sistem notu değil)?
fn prompt_text(line: &Value) -> Option<String> {
    if line.get("type").and_then(Value::as_str) != Some("user")
        || line.get("isMeta").and_then(Value::as_bool) == Some(true)
        || line.get("isSidechain").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let content = line.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() || text.starts_with('<') || text.starts_with("Caveat:") {
        return None;
    }
    Some(if text.chars().count() > PROMPT_CHARS { format!("{}…", text.chars().take(PROMPT_CHARS - 1).collect::<String>()) } else { text })
}

/// Kaydın başından klasör ve ilk istek.
fn head(path: &Path) -> (Option<String>, Option<String>) {
    let mut bytes = Vec::new();
    if File::open(path).and_then(|f| f.take(HEAD).read_to_end(&mut bytes)).is_err() {
        return (None, None);
    }
    let text = String::from_utf8_lossy(&bytes);
    let (mut cwd, mut prompt) = (None, None);
    // Son satır yarım kalmış olabilir; ayrıştırılamazsa atlanır.
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else { continue };
        if cwd.is_none() {
            cwd = value.get("cwd").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_owned);
        }
        if prompt.is_none() {
            prompt = prompt_text(&value);
        }
        if cwd.is_some() && prompt.is_some() {
            break;
        }
    }
    (cwd, prompt)
}

/// Son kullanılan proje klasörleri (var olanlar), en yeni önce, klasör başına bir satır.
pub fn recent(root: &Path, limit: usize) -> Vec<Recent> {
    let mut out: Vec<Recent> = Vec::new();
    let mut seen_dirs: Vec<PathBuf> = Vec::new();
    for (path, at) in sessions(root) {
        if out.len() >= limit {
            break;
        }
        // Aynı kayıt klasöründen (aynı proje) yalnız en yeni kayıt okunur.
        let dir = path.parent().map(PathBuf::from).unwrap_or_default();
        if seen_dirs.contains(&dir) {
            continue;
        }
        seen_dirs.push(dir);
        let (Some(cwd), _) = head(&path) else { continue };
        let key = cwd.trim_end_matches(['/', '\\']).to_lowercase();
        if temporary(&cwd) || !Path::new(&cwd).is_dir() || out.iter().any(|r| r.path.trim_end_matches(['/', '\\']).to_lowercase() == key) {
            continue;
        }
        out.push(Recent { name: folder_name(&cwd), path: cwd, last_used: at });
    }
    out
}

/// Kapanmış oturumlar: `open` (adada açık oturum kimlikleri) dışındakiler, yeniden eskiye. İstek
/// yazılmamış (boş) kayıtlar ve klasörü artık olmayanlar girmez.
pub fn closed(root: &Path, open: &[String], limit: usize) -> Vec<Closed> {
    let mut out = Vec::new();
    for (path, at) in sessions(root).into_iter().take(SCAN) {
        if out.len() >= limit {
            break;
        }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        if open.contains(&id) {
            continue;
        }
        let (Some(cwd), Some(first_prompt)) = head(&path) else { continue };
        if temporary(&cwd) || !Path::new(&cwd).is_dir() {
            continue;
        }
        out.push(Closed { session_id: id, project: folder_name(&cwd), cwd, first_prompt, ended_at: at });
    }
    out
}

/// Kaydı bulunan oturumun klasörü (devam ettirmeden önce denetlenir).
pub fn session_cwd(root: &Path, session_id: &str) -> Option<String> {
    let name = format!("{session_id}.jsonl");
    let path = std::fs::read_dir(root).ok()?.flatten().map(|d| d.path().join(&name)).find(|p| p.is_file())?;
    head(&path).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    const C: &str = "00000000-0000-4000-8000-000000000003";

    fn temp(name: &str) -> PathBuf {
        // Sistemin geçici klasörü listelerden elendiği için deneme klasörü derleme klasöründe.
        let dir = std::env::current_dir().unwrap().join("target").join(format!("zodyvel-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(root: &Path, project: &str, id: &str, lines: &[String], age_secs: u64) {
        let dir = root.join(project);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.jsonl"));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let when = SystemTime::now() - Duration::from_secs(age_secs);
        File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
    }

    fn user(cwd: &Path, content: Value) -> String {
        serde_json::json!({ "type": "user", "cwd": cwd.to_string_lossy(), "message": { "role": "user", "content": content } }).to_string()
    }

    #[test]
    fn session_ids_are_strict_uuids() {
        assert!(valid_session_id(A));
        assert!(!valid_session_id("11111111-2222-4333-8444-55555555555"));
        assert!(!valid_session_id("11111111-2222-4333-8444-55555555555g"));
        assert!(!valid_session_id("11111111x2222-4333-8444-555555555555"));
        assert!(!valid_session_id("--resume x"));
    }

    #[test]
    fn closed_sessions_skip_open_empty_and_noise() {
        let root = temp("closed");
        let api = root.join("work").join("api");
        let site = root.join("work").join("site");
        std::fs::create_dir_all(&api).unwrap();
        std::fs::create_dir_all(&site).unwrap();
        let summary = serde_json::json!({ "type": "summary", "summary": "x" }).to_string();
        let meta = serde_json::json!({ "type": "user", "isMeta": true, "cwd": api.to_string_lossy(), "message": { "content": "Caveat: gizli" } }).to_string();
        write(
            &root,
            "p-api",
            A,
            &[summary, meta, user(&api, Value::from("<command-name>/model</command-name>")), user(&api, Value::from("Testleri   düzelt\nlütfen"))],
            30,
        );
        write(&root, "p-site", B, &[user(&site, serde_json::json!([{ "type": "text", "text": "Ana sayfayı yenile" }]))], 10);
        // İstek yok: liste dışı.
        write(&root, "p-site", C, &[serde_json::json!({ "type": "system", "cwd": site.to_string_lossy() }).to_string()], 5);
        // Alt ajan kaydı (alt klasörde) sayılmaz.
        std::fs::create_dir_all(root.join("p-site").join(B).join("subagents")).unwrap();

        let list = closed(&root, &[], 10);
        assert_eq!(list.iter().map(|c| c.session_id.as_str()).collect::<Vec<_>>(), [B, A]);
        assert_eq!(list[0].first_prompt, "Ana sayfayı yenile");
        assert_eq!(list[0].project, "site");
        assert_eq!(list[1].first_prompt, "Testleri düzelt lütfen");
        assert!(list[0].ended_at > list[1].ended_at);

        assert_eq!(closed(&root, &[B.to_owned()], 10).len(), 1);
        assert_eq!(closed(&root, &[], 1).len(), 1);
        assert_eq!(session_cwd(&root, A).as_deref(), Some(api.to_string_lossy().as_ref()));
        assert_eq!(session_cwd(&root, "33333333-2222-4333-8444-555555555555"), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recent_projects_are_unique_existing_and_newest_first() {
        let root = temp("recent");
        let api = root.join("work").join("api");
        let gone = root.join("work").join("gone");
        let scratch = root.join("work").join("scratch-12ab");
        std::fs::create_dir_all(&api).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        write(&root, "p-api", A, &[user(&api, Value::from("bir"))], 100);
        write(&root, "p-api", B, &[user(&api, Value::from("iki"))], 50);
        write(&root, "p-gone", C, &[user(&gone, Value::from("üç"))], 10);
        write(&root, "p-scratch", "00000000-0000-4000-8000-000000000004", &[user(&scratch, Value::from("dört"))], 5);

        let list = recent(&root, 8);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "api");
        assert_eq!(list[0].path, api.to_string_lossy());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn long_prompts_are_cut() {
        let line = serde_json::json!({ "type": "user", "message": { "content": "a".repeat(500) } });
        let text = prompt_text(&line).unwrap();
        assert_eq!(text.chars().count(), PROMPT_CHARS);
        assert!(text.ends_with('…'));
        assert_eq!(folder_name("C:\\work\\Psrc\\"), "Psrc");
    }
}
