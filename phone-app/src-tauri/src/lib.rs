//! Zodyvel telefon uygulaması kabuğu. İş mantığı `zodyvel-phone-core`'dadır; burada yalnız
//! Android Keystore'u `Platform` olarak bağlayan ince katman ve webview'a açılan KÜÇÜK komut listesi var.
//! Webview'a açık komutlar: durum, eşleştirme (başlat/bitir/iptal), bağlantı denemesi, kilit aç/kapat, eşleşmeyi kaldır,
//! canlı durum ve AI eylemleri (T4-c: onay, cevap, not, görev, yeni görev, durdur — hepsi parmak izi + Keystore imzası),
//! klasör ve Ajanda istekleri (salt okunur). Sözleşme: md/telefon-t4c.md.
//! Anahtar, jeton, imza ya da ham ileti webview'a hiç gitmez.
//!
//! G3: çekirdek süreç başına TEKTİR (`core_for`). Telefon açılınca (ekran yokken) Kotlin açılış alıcısı ön plan hizmetini
//! başlatır, hizmet `Headless.nativeStart` ile çekirdeği ekransız kurar (`headless.rs`, JNI platformu). Uygulama sonra
//! açılırsa aynı çekirdek kullanılır, platform Tauri eklentisine geçer (`Switch`). Ekransızken imza/parmak izi yoktur:
//! eylemler uygulama açılınca yapılır.
//!
//! T-D1: Activity kapanınca (süreç ön plan hizmetiyle yaşıyorsa) platform JNI yedeğine döner; ölü Activity'nin
//! eklentisine giden çağrı takılmaz. Yeni Activity'nin sayfası yüklenince Tauri eklentisi geri gelir.
//! A-O1: canlı durum değişince webview'a `live-changed` olayı gider; arayüz yoklamaz, yalnız bu olayda çeker.
use std::sync::{Arc, OnceLock, RwLock};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_zkeystore::KeystoreExt;
use zodyvel_phone_core::{AppState, Core, KeyInfo, LiveState, Notice, PairBegin, Platform, Probe, Prompt, Reminder};
use zodyvel_phone_proto::pairing::{hex, hex_decode};

#[cfg(target_os = "android")]
mod headless;

/// Etkin platform: önce ekransız JNI (varsa), uygulama açılınca Tauri eklentisi.
static PLATFORM: RwLock<Option<Arc<dyn Platform>>> = RwLock::new(None);
static CORE: OnceLock<Arc<Core>> = OnceLock::new();

/// `replace=false`: yalnız boşsa kurar (ekransız giriş, uygulama zaten açıksa dokunmaz).
pub(crate) fn set_platform(p: Arc<dyn Platform>, replace: bool) {
    let mut g = PLATFORM.write().unwrap_or_else(|e| e.into_inner());
    if replace || g.is_none() {
        *g = Some(p);
    }
}
fn cur() -> Option<Arc<dyn Platform>> {
    PLATFORM.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Çağrıyı o anki platforma iletir (kilit çağrı sürerken tutulmaz: parmak izi penceresi uzun sürebilir).
struct Switch;
impl Platform for Switch {
    fn device_secure(&self) -> bool {
        cur().is_some_and(|p| p.device_secure())
    }
    fn device_name(&self) -> String {
        cur().map(|p| p.device_name()).unwrap_or_else(|| "Telefon".into())
    }
    fn create_signing_key(&self, challenge: &[u8]) -> Result<KeyInfo, String> {
        cur().ok_or("Uygulamayı aç.")?.create_signing_key(challenge)
    }
    fn sign(&self, msg: &[u8], prompt: &Prompt) -> Result<Vec<u8>, String> {
        cur().ok_or("Uygulamayı aç.")?.sign(msg, prompt)
    }
    fn authenticate(&self, prompt: &Prompt) -> Result<(), String> {
        cur().ok_or("Uygulamayı aç.")?.authenticate(prompt)
    }
    fn wrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        cur().ok_or("Anahtar saklanamadı.")?.wrap(data)
    }
    fn unwrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        cur().ok_or("Anahtar açılamadı.")?.unwrap(data)
    }
    fn delete_keys(&self) -> Result<(), String> {
        cur().ok_or("Anahtarlar silinemedi.")?.delete_keys()
    }
    fn service(&self, on: bool) {
        if let Some(p) = cur() {
            p.service(on)
        }
    }
    fn notice(&self, n: &Notice) {
        if let Some(p) = cur() {
            p.notice(n)
        }
    }
    fn reminders(&self, list: &[Reminder]) {
        if let Some(p) = cur() {
            p.reminders(list)
        }
    }
    fn multicast(&self, on: bool) {
        if let Some(p) = cur() {
            p.multicast(on)
        }
    }
    fn changed(&self) {
        if let Some(p) = cur() {
            p.changed()
        }
    }
}

/// A-O1: arayüzün dinlediği olay (yükü yok; arayüz `live_state` ile çeker).
const LIVE_CHANGED: &str = "live-changed";

/// T-D1: Activity kapandı. JNI yedeği varsa ona geçilir; yoksa platform boşaltılır (çağrılar boş döner, takılmaz).
fn activity_gone() {
    #[cfg(target_os = "android")]
    if let Some(p) = headless::fallback() {
        set_platform(p, true);
        return;
    }
    *PLATFORM.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Süreç başına tek çekirdek (ilk çağıranın klasörüyle).
pub(crate) fn core_for(dir: std::path::PathBuf) -> Arc<Core> {
    CORE.get_or_init(|| Arc::new(Core::new(Box::new(Switch), dir))).clone()
}

struct Android<R: Runtime>(AppHandle<R>);

fn map_err(e: String) -> String {
    if e.contains("canceled") {
        "İptal edildi.".into()
    } else if e.contains("key_invalidated") {
        "Cihaz kilidi/parmak izi değişti; güvenlik için yeniden eşleşmen gerekiyor.".into()
    } else if e.contains("no_key") {
        "İmza anahtarı yok; yeniden eşleş.".into()
    } else {
        "Cihaz doğrulaması tamamlanamadı.".into()
    }
}

impl<R: Runtime> Platform for Android<R> {
    fn device_secure(&self) -> bool {
        self.0.keystore().device_info().map(|i| i.secure).unwrap_or(false)
    }
    fn device_name(&self) -> String {
        self.0.keystore().device_info().map(|i| i.name).unwrap_or_else(|_| "Telefon".into())
    }
    fn create_signing_key(&self, challenge: &[u8]) -> Result<KeyInfo, String> {
        let k = self.0.keystore().create_signing_key(&hex(challenge)).map_err(|_| "Güvenli anahtar üretilemedi.".to_string())?;
        Ok(KeyInfo { apub: k.apub, attestation: k.attestation })
    }
    fn sign(&self, msg: &[u8], prompt: &Prompt) -> Result<Vec<u8>, String> {
        let s = self.0.keystore().sign(&hex(msg), &prompt.title, &prompt.subtitle).map_err(map_err)?;
        hex_decode(&s).ok_or_else(|| "İmza okunamadı.".to_string())
    }
    fn authenticate(&self, prompt: &Prompt) -> Result<(), String> {
        self.0.keystore().authenticate(&prompt.title, &prompt.subtitle).map_err(map_err)
    }
    fn wrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let s = self.0.keystore().wrap(&hex(data)).map_err(|_| "Anahtar saklanamadı.".to_string())?;
        hex_decode(&s).ok_or_else(|| "Anahtar saklanamadı.".to_string())
    }
    fn unwrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let s = self.0.keystore().unwrap(&hex(data)).map_err(|_| "Anahtar açılamadı.".to_string())?;
        hex_decode(&s).ok_or_else(|| "Anahtar açılamadı.".to_string())
    }
    fn delete_keys(&self) -> Result<(), String> {
        self.0.keystore().delete_keys().map_err(|_| "Anahtarlar silinemedi.".to_string())
    }
    fn service(&self, on: bool) {
        let ks = self.0.keystore();
        let _ = if on { ks.service_start() } else { ks.service_stop() };
    }
    fn notice(&self, n: &Notice) {
        let ks = self.0.keystore();
        let _ = match n {
            Notice::Service(t) => ks.post_notice("service", "", "", t, false),
            Notice::Approval { id, session, kind } => ks.post_notice("approval", id, session, "", kind == "permission"),
            Notice::Gone { id } => ks.post_notice("gone", id, "", "", false),
            Notice::Finished { session } => ks.post_notice("finished", "", session, "", false),
            Notice::Failed { session } => ks.post_notice("failed", "", session, "", false),
            Notice::Info(t) => ks.post_notice("info", "", "", t, false),
        };
    }
    fn reminders(&self, list: &[Reminder]) {
        if let Ok(j) = serde_json::to_string(list) {
            let _ = self.0.keystore().reminders_sync(&j);
        }
    }
    fn multicast(&self, on: bool) {
        let _ = self.0.keystore().multicast(on);
    }
    fn changed(&self) {
        let _ = self.0.emit(LIVE_CHANGED, ());
    }
}

// Komutlar `async`: eşzamanlı komutlar ana iş parçacığında çalışır ve BiometricPrompt'u bekleyince donardı.
type Shared<'a> = State<'a, Arc<Core>>;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|_| "Beklenmeyen hata.".to_string())
}

#[tauri::command]
async fn app_state(core: Shared<'_>) -> Result<AppState, String> {
    let c = core.inner().clone();
    blocking(move || c.state()).await
}
#[tauri::command]
async fn pair_begin(core: Shared<'_>, qr: String) -> Result<PairBegin, String> {
    let c = core.inner().clone();
    blocking(move || c.pair_begin(&qr)).await?
}
#[tauri::command]
async fn pair_finish(core: Shared<'_>, same: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || {
        c.pair_finish(same)?;
        // Eşleşme tamam: canlı bağlantı ve ön plan hizmeti başlar (kullanıcı kapatmadıysa).
        let _ = c.live_start();
        Ok(())
    })
    .await?
}
#[tauri::command]
async fn pair_cancel(core: Shared<'_>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.pair_cancel()).await
}
#[tauri::command]
async fn probe(core: Shared<'_>) -> Result<Probe, String> {
    let c = core.inner().clone();
    blocking(move || c.probe()).await?
}
#[tauri::command]
async fn unlock(core: Shared<'_>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.unlock()).await?
}
#[tauri::command]
async fn lock(core: Shared<'_>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.lock()).await
}
#[tauri::command]
async fn unpair(core: Shared<'_>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.unpair()).await?
}

#[tauri::command]
async fn live_state(core: Shared<'_>) -> Result<LiveState, String> {
    let c = core.inner().clone();
    blocking(move || c.live_state()).await
}
/// Uygulama açılınca: eşleşme varsa canlı bağlantıyı (ve hizmeti) başlatır; kapalıysa hiçbir şey yapmaz.
#[tauri::command]
async fn live_resume(core: Shared<'_>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.live_start()).await?
}
#[tauri::command]
async fn live_set_enabled(core: Shared<'_>, on: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.live_set_enabled(on)).await?
}
/// Uygulama içi onay/ret düğmesi: BiometricPrompt + Keystore imzası; masaüstünün `ack`'ı beklenir.
#[tauri::command]
async fn live_decide(core: Shared<'_>, id: String, allow: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.decide_wait(&id, allow)).await?
}
/// Soru formu: anahtar = soru metni, değer = seçenek(ler) ya da serbest metin.
#[tauri::command]
async fn live_answer(core: Shared<'_>, id: String, answers: serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.answer(&id, answers)).await?
}
#[tauri::command]
async fn live_note(core: Shared<'_>, session: String, text: String, now: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.note(&session, &text, now)).await?
}
#[tauri::command]
async fn live_task(core: Shared<'_>, session: String, text: String) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.task(&session, &text)).await?
}
/// `folder`: `live_folders`'tan gelen opak kimlik (yol değil).
#[tauri::command]
async fn live_new_task(core: Shared<'_>, folder: String, prompt: String) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.new_task(&folder, &prompt)).await?
}
/// Oturumu durdurur (canlı bağlantıyı DEĞİL; o `live_set_enabled`).
#[tauri::command]
async fn live_stop(core: Shared<'_>, session: String) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.stop_session(&session)).await?
}
#[tauri::command]
async fn live_folders(core: Shared<'_>) -> Result<serde_json::Value, String> {
    let c = core.inner().clone();
    blocking(move || c.folders()).await?
}
#[tauri::command]
async fn live_agenda(core: Shared<'_>) -> Result<serde_json::Value, String> {
    let c = core.inner().clone();
    blocking(move || c.agenda()).await?
}
/// G3: Ajanda hatırlatıcıları aç/kapat (kapatınca tüm alarmlar ve şifreli kayıt silinir).
#[tauri::command]
async fn reminders_set(core: Shared<'_>, on: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.reminders_set_enabled(on)).await?
}
/// G3: telefon açılınca bağlan (varsayılan kapalı).
#[tauri::command]
async fn autostart_set(core: Shared<'_>, on: bool) -> Result<(), String> {
    let c = core.inner().clone();
    blocking(move || c.autostart_set(on)).await?
}
#[tauri::command]
async fn battery_exempt<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    blocking(move || app.keystore().battery_exempt().map_err(|_| "Pil ayarı açılamadı.".to_string())).await?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let b = tauri::Builder::default().plugin(tauri_plugin_zkeystore::init());
    #[cfg(mobile)]
    let b = b.plugin(tauri_plugin_barcode_scanner::init());
    b.setup(|app| {
        let dir = app.path().app_data_dir().map_err(|_| "Veri klasörü bulunamadı")?;
        // Açılış alıcısı (ekransız başlatma) aynı klasörü kullansın.
        let _ = app.keystore().set_core_dir(&dir.to_string_lossy());
        set_platform(Arc::new(Android(app.handle().clone())), true);
        let core = core_for(dir);
        app.manage(core.clone());
        // Bildirimden gelen İzin ver/Reddet (Kotlin → Rust). Kimlik doğrulama ve imza canlı iş parçacığında yapılır.
        let handle = app.handle().clone();
        let c = core.clone();
        std::thread::spawn(move || {
            #[cfg(mobile)]
            {
                let c2 = c.clone();
                let _ = handle.keystore().set_event_handler(move |ev| match ev {
                    tauri_plugin_zkeystore::Event::Decide { id, allow } => {
                        let _ = c2.decide(&id, allow);
                    }
                });
            }
            #[cfg(not(mobile))]
            let _ = handle;
            // Süreç bildirim eylemi ya da simgeyle soğuk açıldıysa bağlantı hemen kurulsun.
            let _ = c.live_start();
        });
        Ok(())
    })
    .on_page_load(|wv, p| {
        // T-D1: (yeniden) açılan Activity'nin sayfası hazır: çağrılar yine Tauri eklentisine gider; yedek JNI
        // platformu da (ekransız başlatma olmadıysa) bu Activity'den hazırlanır.
        if matches!(p.event(), tauri::webview::PageLoadEvent::Finished) {
            set_platform(Arc::new(Android(wv.app_handle().clone())), true);
            #[cfg(target_os = "android")]
            let _ = wv.with_webview(|w| w.jni_handle().exec(|env, activity, _| headless::adopt(env, activity)));
        }
    })
    .invoke_handler(tauri::generate_handler![
        app_state, pair_begin, pair_finish, pair_cancel, probe, unlock, lock, unpair, live_state, live_resume, live_set_enabled, live_decide,
        live_answer, live_note, live_task, live_new_task, live_stop, live_folders, live_agenda, reminders_set, autostart_set, battery_exempt
    ])
    .build(tauri::generate_context!())
    .expect("Zodyvel telefon uygulaması başlatılamadı")
    .run(|_app, ev| {
        if let tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::Destroyed, .. } = ev {
            activity_gone();
        }
    });
}
