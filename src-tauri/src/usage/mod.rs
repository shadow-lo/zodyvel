//! Claude kullanım özeti komutu (Ayarlar → Özellikler → Kullanım ve maliyet özeti). Kayıtlar yalnız
//! arayüz isteyince (AI paneli açıkken) ve arka plandaki bir iş parçacığında okunur; özellik kapalıyken
//! hiçbir dosya açılmaz. Ayrıntı: `core.rs`.

pub mod core;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, State};

use self::core::{Ledger, Span, Summary};

#[derive(Default)]
pub struct UsageHub {
    ledger: Arc<Mutex<Ledger>>,
}

/// Claude Code'un kayıt klasörü: `CLAUDE_CONFIG_DIR` ya da `%USERPROFILE%\.claude`, altında `projects`.
fn projects_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join(".claude"))
        .join("projects")
}

/// Özellik kapalıysa `None` (hiçbir şey okunmaz). Aralıkları arayüz yerel takvime göre verir.
#[tauri::command]
pub async fn usage_summary(
    app: AppHandle,
    hub: State<'_, UsageHub>,
    today: Span,
    week: Span,
    last_week: Span,
    days: Vec<Span>,
) -> Result<Option<Summary>, String> {
    if !crate::settings::current(&app).features.usage {
        return Ok(None);
    }
    let since = days.iter().chain([&today, &week, &last_week]).map(|s| s.from).min().unwrap_or(today.from);
    let ledger = hub.ledger.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut ledger = ledger.lock().map_err(|_| "Kullanım kayıtları okunamadı.".to_owned())?;
        ledger.refresh(&projects_dir(), since);
        Ok(Some(ledger.summary(today, week, last_week, &days)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kaydın sonundan okunan en çok bayt: son yanıt neredeyse her zaman buradadır.
const TAIL: u64 = 512 * 1024;

/// AI ayrıntısı: bir Claude oturumunun modeli ve tahmini bağlam doluluğu. `session`: aracın oturum
/// kimliği (yalnız harf, rakam ve tire). Özellik kapalıysa ya da kayıt bulunamazsa `None`.
#[tauri::command]
pub async fn usage_session(app: AppHandle, session: String) -> Result<Option<self::core::SessionContext>, String> {
    if !crate::settings::current(&app).features.agent_context {
        return Ok(None);
    }
    if session.is_empty() || session.len() > 64 || !session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::{Read, Seek, SeekFrom};
        let name = format!("{session}.jsonl");
        let file = std::fs::read_dir(projects_dir()).ok().into_iter().flatten().flatten().map(|d| d.path().join(&name)).find(|p| p.is_file());
        let Some(path) = file else { return Ok(None) };
        let mut handle = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let len = handle.metadata().map(|m| m.len()).unwrap_or(0);
        handle.seek(SeekFrom::Start(len.saturating_sub(TAIL))).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        handle.take(TAIL).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        Ok(self::core::session_context(&String::from_utf8_lossy(&bytes)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Özellik kapanınca okunmuş kayıtlar bellekten atılır (okuma sürüyorsa beklemeden geçer).
pub fn forget(app: &AppHandle) {
    use tauri::Manager;
    if let Some(hub) = app.try_state::<UsageHub>()
        && let Ok(mut ledger) = hub.ledger.try_lock()
    {
        *ledger = Ledger::default();
    }
}
