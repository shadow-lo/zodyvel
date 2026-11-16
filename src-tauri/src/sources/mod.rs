//! Kaynaklar: AI dışı bilgi kaynakları (Mail, GitHub) için ortak kayıt, olay ve komutlar.
//!
//! Her kaynak `Source`'u uygular ve kendi iş parçacığında çalışır; durumunu `publish` ile bildirir. Merkez
//! (bu dosya) neyin ne zaman çalışacağına karar verir: Özellikler → Kaynaklar (ana anahtar) ve kaynağın kendi
//! anahtarı açık, tepsiden duraklatılmamış, Windows oturumu kilitli değil ve gizli bilgisi var. Biri bozulunca
//! `stop()` çağrılır, ağ tamamen durur ve bellekteki satırlar silinir. Güvenlik kuralları `model.rs`'te.
//!
//! Olaylar: `sources://update` (tek `SourceState`, yalnız değişince), `sources://paused` (bool),
//! `sources://new` (kaynak kimliği; yeni okunmamış satır geldi, Zody tek seferlik tepki verir).

pub mod github;
pub mod mail;
pub mod model;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK};

use crate::settings::Settings;
use crate::{debuglog, secrets, sound};
// `SourceItem` kaynakların satırları için (mail/, github/).
#[allow(unused_imports)]
pub use model::{SourceItem, SourceState};
use model::{Gate, gate};

/// Bir kaynak. Çağrılar arayüz iş parçacığını tutmasın diye `start`/`stop` hemen döner (iş kendi iş
/// parçacığında); `action`/`preview` bir arka plan iş parçacığından çağrılır, ağ beklemesi olabilir.
pub trait Source: Send + Sync {
    fn id(&self) -> &'static str;
    /// Özellik açık ve bilgi varken. Gizli bilgiyi `secrets::read` ile kendisi okur; durumunu `publish` ile
    /// bildirir ("connecting" → "ok" / "error").
    fn start(&self, app: AppHandle);
    /// Özellik kapandı / Duraklat / kilit / bağlantı kaldırıldı. Ağ tamamen durur, bellek boşalır.
    fn stop(&self);
    fn action(&self, item_id: &str, action: &str) -> Result<(), String>;
    /// İletinin düz metin önizlemesi (okundu yapmaz).
    fn preview(&self, _item_id: &str) -> Result<String, String> {
        Err("yok".into())
    }
    /// "Yenile": varsayılan olarak bağlantıyı baştan kurar; kaynak daha ucuz bir yol biliyorsa ezer.
    fn refresh(&self, app: AppHandle) {
        self.stop();
        self.start(app);
    }
}

/// Kayıtlı kaynaklar (sıra ayarlar sayfasıyla aynı). Yeni kaynak = bir satır + kendi klasörü.
fn registry() -> Vec<Arc<dyn Source>> {
    vec![mail::source(), github::source()]
}

#[derive(Default)]
struct Inner {
    states: BTreeMap<&'static str, SourceState>,
    running: BTreeSet<&'static str>,
    /// Özellikler → Kaynaklar ve kaynak başına anahtar.
    hub_on: bool,
    on: BTreeMap<&'static str, bool>,
    /// Tepsi → Kaynakları duraklat (yalnız bu oturum için; yeniden başlayınca kalkar).
    paused: bool,
    /// Windows oturumu kilitli.
    locked: bool,
}

pub struct SourcesHub {
    app: AppHandle,
    sources: Vec<Arc<dyn Source>>,
    inner: Mutex<Inner>,
}

static HUB: OnceLock<Arc<SourcesHub>> = OnceLock::new();

impl SourcesHub {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn find(&self, id: &str) -> Option<&Arc<dyn Source>> {
        self.sources.iter().find(|s| s.id() == id)
    }

    /// Kim çalışmalı, kim durmalı; durum değiştiyse arayüze bildirir. Başlat/durdur kilit dışında çağrılır.
    fn reconcile(&self) {
        let (mut start, mut stop) = (Vec::new(), Vec::new());
        let mut changed = Vec::new();
        {
            let mut inner = self.lock();
            for source in &self.sources {
                let id = source.id();
                let source_on = inner.on.get(id).copied().unwrap_or(false);
                let wanted = gate(inner.hub_on, source_on, inner.paused, inner.locked, true);
                // Gizli bilgi yalnız gerekince sorulur (Kimlik Bilgisi Yöneticisi okuması).
                let gate = if wanted == Gate::Run && !has_secrets(id) { Gate::NeedsSetup } else { wanted };
                let running = inner.running.contains(id);
                match gate {
                    Gate::Run if !running => {
                        inner.running.insert(id);
                        start.push(source.clone());
                    }
                    Gate::Run => {}
                    _ => {
                        if running {
                            inner.running.remove(id);
                            stop.push(source.clone());
                        }
                        let status = if gate == Gate::NeedsSetup { "needs_setup" } else { "off" };
                        let next = SourceState::idle(id, status);
                        if inner.states.get(id) != Some(&next) {
                            inner.states.insert(id, next.clone());
                            changed.push(next);
                        }
                    }
                }
            }
        }
        for source in stop {
            debuglog::line(format!("kaynak: {} durdu", source.id()));
            source.stop();
        }
        for state in changed {
            let _ = self.app.emit("sources://update", &state);
        }
        for source in start {
            debuglog::line(format!("kaynak: {} başladı", source.id()));
            source.start(self.app.clone());
        }
    }

    fn snapshot(&self) -> SourcesSnapshot {
        let inner = self.lock();
        let sources = self
            .sources
            .iter()
            .map(|s| inner.states.get(s.id()).cloned().unwrap_or_else(|| SourceState::idle(s.id(), "off")))
            .collect();
        SourcesSnapshot { paused: inner.paused, locked: inner.locked, sources }
    }
}

fn has_secrets(id: &str) -> bool {
    model::ready(id, secrets::exists)
}

fn hub() -> Option<&'static Arc<SourcesHub>> {
    HUB.get()
}

/// Kaynağın durumunu bildirir (kaynakların kendi iş parçacığından). Kaynak o an çalışmıyorsa (az önce
/// durdurulduysa) yok sayılır. Yalnız değişince yayılır. Yeni okunmamış satır geldiyse ve önceki durum da
/// "ok" idiyse (ilk yükleme sessiz) `sources://new` ve dikkat sesi.
pub fn publish(state: SourceState) {
    let Some(hub) = hub() else { return };
    let state = state.normalized();
    let Some(source) = hub.find(&state.id) else { return };
    let id = source.id();
    let fresh = {
        let mut inner = hub.lock();
        if !inner.running.contains(id) {
            return;
        }
        let previous = inner.states.get(id);
        if previous == Some(&state) {
            return;
        }
        let fresh = previous.is_some_and(|p| model::has_fresh(p, &state));
        inner.states.insert(id, state.clone());
        fresh
    };
    let _ = hub.app.emit("sources://update", &state);
    if fresh {
        let _ = hub.app.emit("sources://new", id);
        sound::play(&hub.app, sound::Cue::Attention);
    }
}

/// Ayarlar değişince (ve açılışta): anahtarlar okunur, gerekirse kaynaklar başlar/durur.
pub fn configure(_app: &AppHandle, settings: &Settings) {
    let Some(hub) = hub() else { return };
    {
        let mut inner = hub.lock();
        inner.hub_on = settings.features.sources;
        inner.on.insert("mail", settings.features.source_mail);
        inner.on.insert("github", settings.features.source_github);
    }
    hub.reconcile();
    crate::tray::sync_sources(&hub.app, settings.features.sources, hub.lock().paused);
}

/// Tepsi → Kaynakları duraklat. Yeni durumu döndürür.
pub fn toggle_paused(_app: &AppHandle) -> bool {
    let Some(hub) = hub() else { return false };
    let paused = {
        let mut inner = hub.lock();
        inner.paused = !inner.paused;
        inner.paused
    };
    hub.reconcile();
    let _ = hub.app.emit("sources://paused", paused);
    paused
}

fn set_locked(locked: bool) {
    let Some(hub) = hub() else { return };
    {
        let mut inner = hub.lock();
        if inner.locked == locked {
            return;
        }
        inner.locked = locked;
    }
    debuglog::line(format!("kaynak: oturum {}", if locked { "kilitlendi" } else { "açıldı" }));
    hub.reconcile();
}

pub fn init(app: &AppHandle) {
    let hub = Arc::new(SourcesHub { app: app.clone(), sources: registry(), inner: Mutex::new(Inner::default()) });
    if HUB.set(hub.clone()).is_err() {
        return;
    }
    app.manage(hub);
    watch_session_lock(app);
    configure(app, &crate::settings::current(app));
}

/// Windows oturumu kilitlenince/açılınca ada penceresine gelen ileti. Yoklama yok.
fn watch_session_lock(app: &AppHandle) {
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_WTSSESSION_CHANGE {
            match wparam.0 as u32 {
                WTS_SESSION_LOCK => set_locked(true),
                WTS_SESSION_UNLOCK => set_locked(false),
                _ => {}
            }
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }
    let Some(window) = app.get_webview_window(crate::island::LABEL) else { return };
    let Ok(hwnd) = window.hwnd() else { return };
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(proc), 4, 0);
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcesSnapshot {
    pub paused: bool,
    pub locked: bool,
    pub sources: Vec<SourceState>,
}

fn running(id: &str) -> Result<Arc<dyn Source>, String> {
    let hub = hub().ok_or("Kaynaklar hazır değil.")?;
    let source = hub.find(id).ok_or("Bilinmeyen kaynak.")?.clone();
    if !hub.lock().running.contains(source.id()) {
        return Err("Bu kaynak şu an bağlı değil.".into());
    }
    Ok(source)
}

#[tauri::command]
pub fn sources_snapshot() -> SourcesSnapshot {
    hub().map(|h| h.snapshot()).unwrap_or(SourcesSnapshot { paused: false, locked: false, sources: Vec::new() })
}

/// Satır eylemi ("open", "mark_read", "rerun", "fix_with_claude"). Ağ beklemesi arka planda.
#[tauri::command]
pub async fn source_action(id: String, item_id: String, action: String) -> Result<(), String> {
    if !model::ITEM_ACTIONS.contains(&action.as_str()) || action == "preview" {
        return Err("Bilinmeyen eylem.".into());
    }
    let source = running(&id)?;
    tauri::async_runtime::spawn_blocking(move || source.action(&item_id, &action)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn source_preview(id: String, item_id: String) -> Result<String, String> {
    let source = running(&id)?;
    tauri::async_runtime::spawn_blocking(move || source.preview(&item_id)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn source_refresh(app: AppHandle, id: String) -> Result<(), String> {
    let source = running(&id)?;
    tauri::async_runtime::spawn_blocking(move || source.refresh(app)).await.map_err(|e| e.to_string())
}

/// Kurulum formu: alanlar (`model::FIELDS`) Kimlik Bilgisi Yöneticisi'ne yazılır, kaynak (açıksa) baştan
/// başlar ve sonucu "connecting" → "ok"/"error" olarak `sources://update` ile bildirir.
/// Mail'de `slot`: değiştirilen hesabın yuvası; yoksa yeni hesap ilk boş yuvaya yazılır.
#[tauri::command]
pub async fn source_connect(id: String, fields: BTreeMap<String, String>, slot: Option<usize>) -> Result<(), String> {
    let hub = hub().ok_or("Kaynaklar hazır değil.")?;
    let source = hub.find(&id).ok_or("Bilinmeyen kaynak.")?.clone();
    let pairs: Vec<(String, String)> = fields.into_iter().collect();
    // Alanlar Windows'a dokunmadan önce de denetlensin (yuva sonra seçilir).
    model::map_fields(&id, 0, &pairs)?;
    tauri::async_runtime::spawn_blocking(move || {
        let slot = if source.id() == "mail" { mail_slot(&pairs, slot)? } else { 0 };
        let values = model::map_fields(source.id(), slot, &pairs)?;
        let was_running = hub.lock().running.remove(source.id());
        if was_running {
            source.stop();
        }
        for (key, value) in &values {
            secrets::write(key, value)?;
        }
        debuglog::line(format!("kaynak: {} bilgisi kaydedildi", source.id()));
        hub.reconcile();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kurulum formundaki yardım bağlantıları ("Uygulama şifresi nasıl alınır?", "Token oluştur"): yalnız
/// `model::LINKS` öneklerindeki adresler varsayılan tarayıcıda açılır.
#[tauri::command]
pub fn source_open_link(url: String) -> Result<(), String> {
    if !model::link_allowed(&url) {
        return Err("Bu bağlantı açılamaz.".into());
    }
    if crate::win::shell_open(&url) { Ok(()) } else { Err("Tarayıcı açılamadı.".into()) }
}

/// Yeni Gmail hesabının yuvası: verilen yuva ya da ilk boş yuva. Aynı adres başka yuvada bağlıysa hata
/// (aynı hesaba iki bağlantı açılmasın).
fn mail_slot(fields: &[(String, String)], slot: Option<usize>) -> Result<usize, String> {
    let address = fields.iter().find(|(k, _)| k == "address").map(|(_, v)| v.trim().to_lowercase()).unwrap_or_default();
    let saved: Vec<Option<String>> = model::MAIL_KEYS.iter().map(|(key, _)| secrets::read(key)).collect();
    if saved.iter().enumerate().any(|(i, a)| Some(i) != slot && a.as_deref().is_some_and(|a| a.to_lowercase() == address)) {
        return Err("Bu Gmail hesabı zaten bağlı.".into());
    }
    match slot {
        Some(slot) if slot < model::MAIL_SLOTS => Ok(slot),
        Some(_) => Err("Geçersiz hesap.".into()),
        None => saved.iter().position(Option::is_none).ok_or_else(|| format!("En çok {} Gmail hesabı bağlanabilir.", model::MAIL_SLOTS)),
    }
}

/// Bağlantıyı kaldır: durur, bellekteki satırlar ve Windows'taki gizli bilgi silinir. Mail'de `slot`
/// verilirse yalnız o hesap kaldırılır, diğerleri yeniden bağlanır.
#[tauri::command]
pub async fn source_disconnect(id: String, slot: Option<usize>) -> Result<(), String> {
    let hub = hub().ok_or("Kaynaklar hazır değil.")?;
    let source = hub.find(&id).ok_or("Bilinmeyen kaynak.")?.clone();
    let keys = match slot {
        None => model::secret_keys(source.id()),
        Some(slot) if source.id() == "mail" && slot < model::MAIL_SLOTS => vec![model::MAIL_KEYS[slot].0, model::MAIL_KEYS[slot].1],
        Some(_) => return Err("Geçersiz hesap.".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        if hub.lock().running.remove(source.id()) {
            source.stop();
        }
        for key in keys {
            secrets::delete(key)?;
        }
        debuglog::line(format!("kaynak: {} bağlantısı kaldırıldı", source.id()));
        hub.reconcile();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
