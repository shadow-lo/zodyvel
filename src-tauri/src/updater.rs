//! Güncelleme sistemi (Faz 11): GitHub Releases + Tauri güncelleyicisi.
//!
//! - Açılıştan 20 sn sonra bir kez denetler (arka planda sürekli denetim yok; iş bitince iş parçacığı kapanır).
//! - Yeni sürüm varsa ortada ayrı bir pencere açılır; kullanıcı seçene kadar kalır.
//! - Reddet yalnız bu açılış için kapatır; bir sonraki açılışta denetim yeniden çalışır ve ekran yine çıkar.
//! - İmza doğrulamayı eklenti yapar (tauri.conf.json'daki `pubkey`); imzası tutmayan dosya kurulmaz.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::settings::BROWSER_ARGS;

const LABEL: &str = "update";
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct Pending(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    update: Option<Update>,
    /// İndirilmiş kurulum dosyası (indirme bitince dolar).
    bytes: Option<Vec<u8>>,
    downloading: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: String,
    /// "offer" (seçim bekliyor) | "downloading" | "ready" (indirildi, kurulmaya hazır)
    pub phase: &'static str,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: Option<u64>,
}

fn info_of(inner: &Inner) -> Option<UpdateInfo> {
    let update = inner.update.as_ref()?;
    let phase = if inner.bytes.is_some() {
        "ready"
    } else if inner.downloading {
        "downloading"
    } else {
        "offer"
    };
    Some(UpdateInfo {
        version: update.version.clone(),
        current: update.current_version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        phase,
    })
}

/// Sunucuya sorar; yeni sürüm varsa bekleyen güncellemeyi saklar ve bilgisini döner.
async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    let state: State<Pending> = app.state();
    let mut inner = state.0.lock().map_err(|_| "kilit hatası".to_string())?;
    match found {
        Some(update) => {
            // Aynı sürüm zaten indirilmişse onu bozma.
            let same = inner.update.as_ref().is_some_and(|u| u.version == update.version);
            if !same {
                *inner = Inner { update: Some(update), ..Inner::default() };
            }
            Ok(info_of(&inner))
        }
        None => {
            *inner = Inner::default();
            Ok(None)
        }
    }
}

fn open_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("update.html".into()))
        .title("Zodyvel · Güncelleme")
        .inner_size(460.0, 380.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .background_color(tauri::webview::Color(16, 17, 25, 255))
        .additional_browser_args(BROWSER_ARGS)
        .center()
        .focused(true)
        .build();
    if let Err(error) = built {
        eprintln!("Güncelleme penceresi açılamadı: {error}");
    }
}

/// Açılışta bir kez, 20 sn gecikmeyle. Ağ yoksa ya da hata olursa sessizce vazgeçer (sonraki açılışta yine dener).
pub fn init(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        match tauri::async_runtime::block_on(check(&app)) {
            Ok(Some(_)) => open_window(&app),
            Ok(None) => {}
            Err(error) => eprintln!("Güncelleme denetimi yapılamadı: {error}"),
        }
    });
}

/// Ayarlar › Hakkında › "Güncellemeleri denetle". Yeni sürüm varsa pencere açılır.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let found = check(&app).await?;
    if found.is_some() {
        open_window(&app);
    }
    Ok(found)
}

/// Güncelleme penceresi açılırken bekleyen güncellemenin durumu.
#[tauri::command]
pub fn update_state(state: State<Pending>) -> Option<UpdateInfo> {
    state.0.lock().ok().and_then(|inner| info_of(&inner))
}

/// "Güncellemeyi indir": indirir (ilerleme `update-progress` olayıyla), kurmaz.
#[tauri::command]
pub async fn update_download(app: AppHandle) -> Result<(), String> {
    let update = {
        let state: State<Pending> = app.state();
        let mut inner = state.0.lock().map_err(|_| "kilit hatası".to_string())?;
        if inner.downloading || inner.bytes.is_some() {
            return Ok(());
        }
        let Some(update) = inner.update.clone() else {
            return Err("Bekleyen güncelleme yok.".into());
        };
        inner.downloading = true;
        update
    };
    let mut done: u64 = 0;
    let emitter = app.clone();
    let result = update
        .download(
            move |chunk, total| {
                done += chunk as u64;
                let _ = emitter.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await;
    let state: State<Pending> = app.state();
    let mut inner = state.0.lock().map_err(|_| "kilit hatası".to_string())?;
    inner.downloading = false;
    match result {
        Ok(bytes) => {
            inner.bytes = Some(bytes);
            Ok(())
        }
        // İmza/ağ hatası: dosya saklanmaz, kurulmaz.
        Err(error) => Err(error.to_string()),
    }
}

/// "Yeniden başlat ve kur": indirilen dosyayı kurar. Windows'ta kurulum uygulamayı kapatıp yeniden açar.
#[tauri::command]
pub fn update_install(app: AppHandle) -> Result<(), String> {
    let (update, bytes) = {
        let state: State<Pending> = app.state();
        let mut inner = state.0.lock().map_err(|_| "kilit hatası".to_string())?;
        match (inner.update.clone(), inner.bytes.take()) {
            (Some(update), Some(bytes)) => (update, bytes),
            _ => return Err("Önce güncellemeyi indirmelisin.".into()),
        }
    };
    update.install(bytes).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// "Reddet" ya da pencereyi kapatma: yalnız bu açılış için kapanır.
#[tauri::command]
pub fn update_dismiss(app: AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.close();
    }
}
