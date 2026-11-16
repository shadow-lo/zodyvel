//! Yerel Git (GitHub kaynağının "Depolarım" sekmesi): bilgisayardaki depolarda commitlenmemiş değişiklikler,
//! adadan commit, push, fetch, pull (yalnız ileri sarma) ve geri al (discard; dosya Geri Dönüşüm Kutusu'na).
//! GitHub Desktop'un yerine geçmez: birleştirme, dal değiştirme, çakışma çözme yok; onlar için GitHub Desktop.
//!
//! Hangi depolar: Ayarlar › Kaynaklar › GitHub'da eklenen klasörler (kendisi ya da 2 kat altındaki depolar),
//! "Yeni görev"in son klasörleri ve isteğe bağlı GitHub Desktop listesi (tek seferlik içe aktarma).
//! Ne zaman: yalnız kullanıcı isteyince (ayrıntı açılınca bir kez, "Yenile", commit/push sonrası). Yoklama,
//! dosya izleme, arka plan işi yok.
//!
//! Veri: dosya içeriği okunmaz, hiçbir yere gönderilmez; yalnız `git status` ve dosya boyutu. Ağa çıkan tek
//! şey kullanıcının Push, Fetch ve Pull'u (kendi GitHub'ına; Zodyvel'e bağlı GitHub tokenıyla, yoksa bilgisayarın kendi git girişiyle). Yeni gizli bilgi saklanmaz. Kayda yol ya
//! da dosya adı yazılmaz, yalnız sayılar. Commit ve push adada onay ister; hassas dosyalar ayrıca onaysız
//! commitlenemez (`core::plan_commit`).

pub mod core;
pub mod exec;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::debuglog;
use core::FileChange;

/// Son taramada bulunan depolar: commit/push yalnız bunlarda (başka yol kabul edilmez).
static KNOWN: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// O an commit/push süren depolar (aynı depoda iki işlem üst üste binmesin).
static BUSY: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
static GIT: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoView {
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    pub more: u32,
    /// GitHub "sahip/depo" (küçük harf); GitHub'da değilse yok.
    pub slug: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    /// `git.exe` bulundu mu.
    pub git: bool,
    pub repos: Vec<RepoView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResult {
    pub sha: String,
    pub pushed: bool,
    pub push_error: Option<String>,
}

fn enabled(app: &AppHandle) -> Result<(), String> {
    let f = crate::settings::current(app).features;
    if f.sources && f.source_github && f.local_git { Ok(()) } else { Err("Yerel Git kapalı (Ayarlar › Kaynaklar › GitHub).".into()) }
}

fn git() -> Result<PathBuf, String> {
    GIT.get_or_init(exec::find_git).clone().ok_or_else(|| "Git bulunamadı. Git for Windows'u kur ya da GitHub Desktop'u güncelle.".into())
}

/// GitHub kaynağına bağlı token (Windows Kimlik Bilgisi Yöneticisi'nden; varsa). Push/fetch/pull'da yalnız
/// github.com'a giden isteklere eklenir; yoksa bilgisayarın kendi git girişi kullanılır.
fn token() -> Option<String> {
    crate::secrets::read("github-token")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Son taramadaki depo; yoksa hata (adadan gelen yol yalnız bu listeden biri olabilir).
fn known(path: &str) -> Result<PathBuf, String> {
    lock(&KNOWN).iter().find(|p| p.to_string_lossy() == path).cloned().ok_or_else(|| "Bu depo listede yok; Yenile'ye bas.".into())
}

/// Depoda tek işlem: bitince (hata olsa da) kilit kalkar.
fn exclusive<T>(repo: &Path, work: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    if !lock(&BUSY).insert(repo.to_path_buf()) {
        return Err("Bu depoda başka bir işlem sürüyor.".into());
    }
    let result = work();
    lock(&BUSY).remove(repo);
    result
}

fn view(git: &Path, repo: &Path) -> RepoView {
    let name = repo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| repo.to_string_lossy().into_owned());
    let base = RepoView {
        path: repo.to_string_lossy().into_owned(),
        name,
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        files: Vec::new(),
        more: 0,
        slug: None,
        error: None,
    };
    match exec::status(git, repo) {
        Ok(st) => RepoView {
            branch: st.branch,
            upstream: st.upstream,
            ahead: st.ahead,
            behind: st.behind,
            files: st.files,
            more: st.more,
            slug: exec::github_remote(git, repo),
            ..base
        },
        Err(e) => RepoView { error: Some(e), ..base },
    }
}

fn scan(app: &AppHandle) -> Result<Overview, String> {
    let folders = crate::settings::current(app).git.folders;
    let recent: Vec<PathBuf> = crate::tasks::history::recent(&crate::tasks::projects_dir(), 12).into_iter().map(|r| PathBuf::from(r.path)).collect();
    let repos = core::discover(&folders, &recent);
    *lock(&KNOWN) = repos.clone();
    let Ok(git) = git() else { return Ok(Overview { git: false, repos: Vec::new() }) };
    // Birkaç depo aynı anda (her biri ayrı `git status`).
    let mut views: Vec<RepoView> = Vec::with_capacity(repos.len());
    for chunk in repos.chunks(4) {
        std::thread::scope(|s| {
            let handles: Vec<_> = chunk.iter().map(|r| s.spawn(|| view(&git, r))).collect();
            views.extend(handles.into_iter().filter_map(|h| h.join().ok()));
        });
    }
    let dirty = views.iter().filter(|v| !v.files.is_empty()).count();
    debuglog::line(format!("yerel git: {} depo, {} değişiklikli", views.len(), dirty));
    Ok(Overview { git: true, repos: views })
}

/// Depolar ve değişiklikleri (adada "Depolarım" açılınca ve "Yenile"de).
#[tauri::command]
pub async fn git_overview(app: AppHandle) -> Result<Overview, String> {
    enabled(&app)?;
    tauri::async_runtime::spawn_blocking(move || scan(&app)).await.map_err(|e| e.to_string())?
}

/// Seçilen dosyaları commitler; `push` ise ardından gönderir. Push hatası commit'i geri almaz.
#[tauri::command]
pub async fn git_commit(app: AppHandle, path: String, message: String, files: Vec<String>, allow_sensitive: Vec<String>, push: bool) -> Result<CommitResult, String> {
    enabled(&app)?;
    let repo = known(&path)?;
    let git = git()?;
    tauri::async_runtime::spawn_blocking(move || {
        exclusive(&repo, || {
            let sha = exec::commit(&git, &repo, &message, &files, &allow_sensitive)?;
            debuglog::line(format!("yerel git: commit ({} dosya)", files.len()));
            let push_error = push.then(|| exec::push(&git, &repo, token().as_deref()).err()).flatten();
            if push {
                debuglog::line(if push_error.is_some() { "yerel git: push başarısız" } else { "yerel git: push" });
            }
            Ok(CommitResult { sha, pushed: push && push_error.is_none(), push_error })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Gönderilmemiş commitleri dalın uzak dalına gönderir (zorlamasız).
#[tauri::command]
pub async fn git_push(app: AppHandle, path: String) -> Result<(), String> {
    enabled(&app)?;
    let repo = known(&path)?;
    let git = git()?;
    tauri::async_runtime::spawn_blocking(move || {
        exclusive(&repo, || {
            let result = exec::push(&git, &repo, token().as_deref());
            debuglog::line(if result.is_ok() { "yerel git: push" } else { "yerel git: push başarısız" });
            result
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Seçilen dosyaların değişikliklerini geri alır; dosyaların şimdiki hâli Geri Dönüşüm Kutusu'na gider.
#[tauri::command]
pub async fn git_discard(app: AppHandle, path: String, files: Vec<String>) -> Result<u32, String> {
    enabled(&app)?;
    let repo = known(&path)?;
    let git = git()?;
    tauri::async_runtime::spawn_blocking(move || {
        exclusive(&repo, || {
            let result = exec::discard(&git, &repo, &files);
            debuglog::line(match &result {
                Ok(n) => format!("yerel git: geri al ({n} dosya)"),
                Err(_) => "yerel git: geri al başarısız".into(),
            });
            result.map(|n| n as u32)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// GitHub'daki durumu indirir (dosyalar değişmez).
#[tauri::command]
pub async fn git_fetch(app: AppHandle, path: String) -> Result<(), String> {
    enabled(&app)?;
    let repo = known(&path)?;
    let git = git()?;
    tauri::async_runtime::spawn_blocking(move || exclusive(&repo, || exec::fetch(&git, &repo, token().as_deref()))).await.map_err(|e| e.to_string())?
}

/// GitHub'daki yeni commitleri alır (yalnız ileri sarma; birleştirme yok).
#[tauri::command]
pub async fn git_pull(app: AppHandle, path: String) -> Result<(), String> {
    enabled(&app)?;
    let repo = known(&path)?;
    let git = git()?;
    tauri::async_runtime::spawn_blocking(move || {
        exclusive(&repo, || {
            let result = exec::pull(&git, &repo, token().as_deref());
            debuglog::line(if result.is_ok() { "yerel git: pull" } else { "yerel git: pull başarısız" });
            result
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Deponun klasörünü Gezgin'de açar.
#[tauri::command]
pub fn git_open_folder(app: AppHandle, path: String) -> Result<(), String> {
    enabled(&app)?;
    let repo = known(&path)?;
    if crate::win::shell_open(&repo.to_string_lossy()) { Ok(()) } else { Err("Klasör açılamadı.".into()) }
}

/// Ayarlar › "Klasör ekle": Windows klasör seçicisi. Vazgeçilirse `None`.
#[tauri::command]
pub async fn git_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let owner = app.get_webview_window("settings").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    let picked = tauri::async_runtime::spawn_blocking(move || crate::win::pick_folder(owner, "Depo ya da depoların olduğu klasörü seç"))
        .await
        .map_err(|e| e.to_string())?;
    Ok(picked.and_then(|p| core::clean_folder(&p)))
}

/// Ayarlar › "GitHub Desktop'tan al": GitHub Desktop'un depo listesindeki, diskte hâlâ duran depolar.
/// Yalnız yol okunur; GitHub Desktop'un hiçbir dosyası değişmez.
#[tauri::command]
pub async fn git_desktop_repos() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).ok_or("GitHub Desktop bulunamadı.")?;
        let db = base.join("GitHub Desktop").join("IndexedDB").join("file__0.indexeddb.leveldb");
        let entries = std::fs::read_dir(&db).map_err(|_| "GitHub Desktop'un depo listesi bulunamadı.".to_string())?;
        let mut found = BTreeSet::new();
        let mut budget: u64 = 64 * 1024 * 1024;
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            if !matches!(ext.as_str(), "log" | "ldb") || size > budget {
                continue;
            }
            budget -= size;
            let Ok(bytes) = std::fs::read(&path) else { continue };
            for p in core::desktop_paths(&bytes) {
                if core::is_repo(Path::new(&p)) {
                    found.insert(p);
                }
            }
        }
        debuglog::line(format!("yerel git: GitHub Desktop'tan {} depo", found.len()));
        Ok(found.into_iter().take(core::MAX_REPOS).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
