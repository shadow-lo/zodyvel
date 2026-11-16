//! Telefon köprüsü (Plan 3 · T2): Windows tarafı. Asıl mantık `zodyvel-phone-proto` kutusunda (eşleştirme,
//! Noise kanalı, imzalı eylemler, sınamalar); burası onu ajan modülüne, ayarlara ve Windows'a bağlar.
//!
//! Kurallar (Plan 3 · 2.5): modül kapalıyken iş parçacığı, soket, duyuru, RAM yok · yalnız Özel ağda ve yalnız
//! yerel adreste dinler · anahtarlar Kimlik Bilgisi Yöneticisi'nde · telefondan yalnız izinli eylemler
//! (erişim seviyesi yükseltme, "her zaman izin ver", klasör yolu yok) · günlüğe içerik yazılmaz.

use std::collections::VecDeque;
use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use zodyvel_hook::Decision;
use zodyvel_phone_proto::attest::{MAX_STATUS_BYTES, Revocations};
use zodyvel_phone_proto::devices::{REPAIR_AFTER_SECS, Vault};
use zodyvel_phone_proto::discovery::{Announcement, qr_svg};
use zodyvel_phone_proto::net::{is_local_bind, listening_allowed, query_profiles};
use zodyvel_phone_proto::proto::{self, Action, AgendaView, FolderItem, StateView, folder_id};
use zodyvel_phone_proto::server::{Bridge, Event, Host, now_secs};

use crate::agents::AgentHub;
use crate::tasks::TaskHub;

/// Telefonların önce denediği port (doluysa sistem seçer; telefon mDNS ile bulur).
const PREFERRED_PORT: u16 = 47821;
const NET_CACHE: Duration = Duration::from_secs(30);
const IP_CACHE: Duration = Duration::from_secs(1);
const VAULT_KEY: &str = crate::secrets::PHONE_VAULT;
const MAX_AUDIT: usize = 50;
/// G4: Google Key Attestation iptal listesi. Herkese açık JSON; istekte kimlik/çerez/telefon verisi yok.
const STATUS_URL: &str = "https://android.googleapis.com/attestation/status";
/// Liste bu kadar eskiyse yenilenir.
const STATUS_REFRESH: Duration = Duration::from_secs(24 * 3600);
/// Disk kopyası bu kadar eskiyse kullanılmaz (internet yoksa telefon "denetlenemedi" kalır).
const STATUS_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
/// Başarısız indirmeden sonra en az bu kadar beklenir.
const STATUS_RETRY: Duration = Duration::from_secs(30 * 60);
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Windows Kimlik Bilgisi Yöneticisi'ndeki tek gizli kayıt (Noise anahtarı + eşli cihaz listesi).
struct CredVault;
impl Vault for CredVault {
    fn load(&self) -> Option<String> {
        crate::secrets::read(VAULT_KEY)
    }
    fn load_checked(&self) -> Result<Option<String>, String> {
        crate::secrets::read_checked(VAULT_KEY)
    }
    fn store(&self, blob: &str) -> Result<(), String> {
        crate::secrets::write(VAULT_KEY, blob)
    }
    fn wipe(&self) -> Result<(), String> {
        crate::secrets::delete(VAULT_KEY)
    }
}

/// Köprünün ajan modülüne ve Windows'a bakan yüzü.
struct AppHost {
    app: AppHandle,
    net: Mutex<Option<(Instant, bool)>>,
    /// Dinlenen yerel adres; değişirse (ağ değişti) dinleme anında kapanır.
    bound: Mutex<Option<IpAddr>>,
    ip_check: Mutex<Option<(Instant, bool)>>,
    audit: Mutex<VecDeque<String>>,
    /// Klasör kimliklerinin tuzu (süreç başına rastgele). Paylaşılan, değişebilen bir liste tutulmaz:
    /// "Yeni görev" kimliği her seferinde güncel son-klasörler listesinden çözülür (O6).
    folder_salt: u64,
    /// G4: iptal listesi (indirildiği an, liste). Yalnız köprü çalışırken doldurulur.
    rev: Mutex<Option<(SystemTime, Arc<Revocations>)>>,
    rev_busy: AtomicBool,
    rev_tried: Mutex<Option<Instant>>,
}

/// `%LOCALAPPDATA%\Zodyvel\attestation-status.json`: herkese açık listenin son kopyası (gizli değil).
fn status_file() -> PathBuf {
    crate::debuglog::dir().join("attestation-status.json")
}

/// Disk kopyası (yeterince tazeyse).
fn load_status_file() -> Option<(SystemTime, Arc<Revocations>)> {
    let path = status_file();
    let at = std::fs::metadata(&path).ok()?.modified().ok()?;
    if !at.elapsed().is_ok_and(|e| e <= STATUS_MAX_AGE) {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    Some((at, Arc::new(Revocations::from_json(&text)?)))
}

/// Listeyi indirir (Windows'un kendi HTTP istemcisi; yönlendirme ve önbellek kapalı, kimlik penceresi açılmaz).
fn fetch_status() -> Result<String, String> {
    use std::sync::mpsc;
    use windows::Foundation::Uri;
    use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior, HttpCacheWriteBehavior};
    use windows::Web::Http::{HttpClient, HttpCompletionOption};
    use windows::core::HSTRING;
    const ERR: &str = "İptal listesi indirilemedi.";
    // M-O4: başlıklar ve gövde birlikte `STATUS_TIMEOUT` içinde gelmeli; süre dolunca işlem iptal edilir.
    let deadline = Instant::now() + STATUS_TIMEOUT;
    macro_rules! wait {
        ($op:expr) => {{
            let op = $op;
            let (tx, rx) = mpsc::channel();
            op.when(move |r| {
                let _ = tx.send(r);
            })?;
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(r) => r?,
                Err(_) => {
                    let _ = op.Cancel();
                    return Ok(None);
                }
            }
        }};
    }
    let run = || -> windows::core::Result<Option<(u16, String)>> {
        let filter = HttpBaseProtocolFilter::new()?;
        filter.SetAllowUI(false)?;
        filter.SetAllowAutoRedirect(false)?;
        let cache = filter.CacheControl()?;
        cache.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
        cache.SetWriteBehavior(HttpCacheWriteBehavior::NoCache)?;
        let client = HttpClient::Create(&filter)?;
        // Yalnız başlıklar beklenir; gövde boyutu denetlendikten sonra okunur.
        let resp = wait!(client.GetWithOptionAsync(&Uri::CreateUri(&HSTRING::from(STATUS_URL))?, HttpCompletionOption::ResponseHeadersRead)?);
        let status = resp.StatusCode()?.0 as u16;
        if status != 200 {
            return Ok(Some((status, String::new())));
        }
        let content = resp.Content()?;
        // `Content-Length` (sıkıştırılmışsa sıkıştırılmış boyut) sınırı aşıyorsa gövde hiç okunmaz; açılmış metin
        // aşağıda ayrıca denetlenir.
        if content.Headers().and_then(|h| h.ContentLength()).and_then(|l| l.Value()).is_ok_and(|n| n > MAX_STATUS_BYTES as u64) {
            return Ok(None);
        }
        let body = wait!(content.ReadAsStringAsync()?);
        Ok(Some((status, body.to_string())))
    };
    match run() {
        Ok(Some((200, body))) if !body.is_empty() && body.len() <= MAX_STATUS_BYTES => Ok(body),
        _ => Err(ERR.into()),
    }
}

/// G4: liste tazeyse hiçbir şey yapmaz; değilse arka planda indirir, diske yazar ve eşli cihazları yeniden
/// denetler (`Bridge::apply_revocations`). Aynı anda tek indirme; başarısızsa 30 dk beklenir. Yalnız köprü
/// çalışırken çağrılır (modül kapalıyken ağ isteği yok).
fn refresh_revocations(app: &AppHandle, host: &Arc<AppHost>) {
    let fresh = lock(&host.rev).as_ref().is_some_and(|(at, _)| at.elapsed().is_ok_and(|e| e < STATUS_REFRESH));
    if fresh || lock(&host.rev_tried).is_some_and(|t| t.elapsed() < STATUS_RETRY) {
        return;
    }
    if host.rev_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    *lock(&host.rev_tried) = Some(Instant::now());
    let (app, host_t) = (app.clone(), host.clone());
    let spawned = std::thread::Builder::new().name("phone-attest-status".into()).spawn(move || {
        let host = host_t;
        if let Ok(body) = fetch_status()
            && let Some(rev) = Revocations::from_json(&body)
        {
            let path = status_file();
            let tmp = path.with_extension("json.tmp");
            if std::fs::create_dir_all(crate::debuglog::dir()).is_ok() && std::fs::write(&tmp, &body).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
            let rev = Arc::new(rev);
            *lock(&host.rev) = Some((SystemTime::now(), rev.clone()));
            *lock(&host.rev_tried) = None;
            if let Some(hub) = app.try_state::<Arc<PhoneHub>>() {
                let old = {
                    let mut inner = lock(&hub.inner);
                    let failed = inner.running.as_ref().and_then(|r| r.bridge.apply_revocations(&rev).err());
                    failed.and_then(|e| {
                        // Kayıt yazılamadı: köprü kilitlendi; iptaldeki gibi dinleme kapanır, hata görünür.
                        let old = PhoneHub::stop(&mut inner);
                        inner.problem = Some(format!("Telefon kaydı güncellenemedi: {e}"));
                        old
                    })
                };
                // Köprü kilit dışında kapanır (iş parçacıkları beklenirken hub kilidi tutulmaz; M-D1).
                drop(old);
            }
            let _ = app.emit("phone://status", ());
        }
        host.rev_busy.store(false, Ordering::SeqCst);
    });
    if spawned.is_err() {
        host.rev_busy.store(false, Ordering::SeqCst);
    }
}

impl AppHost {
    fn agents(&self) -> State<'_, Arc<AgentHub>> {
        self.app.state::<Arc<AgentHub>>()
    }

    /// Masaüstünün sunduğu son klasörler: (kimlik, yol, ad). Her çağrıda yeniden okunur.
    fn recent_folders(&self) -> Vec<(String, String, String)> {
        crate::tasks::history::recent(&crate::tasks::projects_dir(), 8)
            .into_iter()
            .map(|r| (folder_id(self.folder_salt, &r.path), r.path, proto::clip(&r.name, 60)))
            .collect()
    }
}

/// O7: Not/Görev/Durdur yalnız telefona gösterilen oturumlara (`view.sessions`) uygulanır.
fn session_in_view(view: &StateView, session: &str) -> bool {
    let bare = |s: &str| s.strip_prefix("claude:").unwrap_or(s).to_owned();
    let want = bare(session);
    view.sessions.iter().any(|s| bare(&s.id) == want)
}

impl Host for AppHost {
    fn state(&self) -> StateView {
        self.agents().phone_view()
    }

    fn act(&self, _device: &str, a: Action) -> Result<(), String> {
        let hub = self.agents();
        if !hub.enabled() {
            return Err("AI modülü kapalı.".into());
        }
        // Yalnız şu an telefona gösterilen onay/soru kararlanabilir (Codex soruları, yalnız-izle kayıtları dışarıda).
        let view = self.state();
        let approval = |id: &str| view.approvals.iter().find(|x| x.id == id);
        match a {
            Action::Decide { id, d } => {
                let ap = approval(&id).ok_or("Bu istek artık yok.")?;
                if ap.kind == "question" {
                    return Err("Soruya cevap ver, izin verilmez.".into());
                }
                let decision = if d == "allow" { Decision::Allow } else { Decision::Deny };
                hub.decide_external(&id, decision)
            }
            Action::Answer { id, answers } => {
                let ap = approval(&id).ok_or("Bu istek artık yok.")?;
                if ap.kind != "question" {
                    return Err("Bu bir soru değil.".into());
                }
                hub.decide_external(&id, Decision::Answer(answers))
            }
            Action::Note { session, text, now } => {
                if !session_in_view(&view, &session) {
                    return Err("Bu oturum telefona açık değil.".into());
                }
                crate::agents::agents_note(self.app.clone(), self.agents(), session, text, now)
            }
            Action::Task { session, text } => {
                if !session_in_view(&view, &session) {
                    return Err("Bu oturum telefona açık değil.".into());
                }
                crate::agents::agents_task(self.app.clone(), self.agents(), session, text)
            }
            Action::NewTask { folder, prompt } => {
                // Kimlik güncel listede yoksa reddedilir (liste değiştiyse telefon yeniden ister); sıra no yok.
                let cwd = self.recent_folders().into_iter().find(|f| f.0 == folder).map(|f| f.1).ok_or("Klasör artık listede yok; listeyi yenile.")?;
                // Telefondan erişim seviyesi yükseltilemez: izin kipi her zaman "default".
                crate::tasks::start_task(self.app.clone(), self.app.state::<Arc<TaskHub>>(), cwd, prompt, "default".into()).map(|_| ())
            }
            Action::Stop { session } => {
                if !session_in_view(&view, &session) {
                    return Err("Bu oturum telefona açık değil.".into());
                }
                crate::tasks::task_stop(self.app.state::<Arc<TaskHub>>(), session)
            }
        }
    }

    fn folders(&self) -> Vec<FolderItem> {
        self.recent_folders().into_iter().map(|f| FolderItem { id: f.0, name: f.2 }).collect()
    }

    fn agenda(&self) -> AgendaView {
        crate::today::phone_agenda(&self.app)
    }

    fn audit(&self, device: &str, what: &str, result: &str) {
        // G4: kalıcı denetim kaydı (`denetim/*.jsonl`), ayarlar sayfası için son 50 satır da bellekte.
        crate::actions::kaydet_telefon(&self.app, device, what, result);
        let line = if result == "ok" { format!("{device} telefondan {what}") } else { format!("{device} telefondan {what} ({result})") };
        let mut a = lock(&self.audit);
        a.push_back(line);
        while a.len() > MAX_AUDIT {
            a.pop_front();
        }
        drop(a);
        let _ = self.app.emit("phone://status", ());
    }

    fn event(&self, _e: Event) {
        // Arayüz durumu `phone_status` ile yeniden okur (kod, cihaz listesi, bağlantı).
        let _ = self.app.emit("phone://status", ());
    }

    fn network_ok(&self) -> bool {
        // Ağ değişimi (Wi-Fi/ağ değişti, dinlenen adres artık varsayılan arayüzün adresi değil) en geç
        // 1 sn'de yakalanır: ucuz yerel sorgu, PowerShell yok. Profil (Özel/Ortak) sorgusu 30 sn önbellekli.
        if let Some(bound) = *lock(&self.bound) {
            let mut ipc = lock(&self.ip_check);
            let fresh = ipc.is_some_and(|(at, _)| at.elapsed() < IP_CACHE);
            if !fresh {
                let same = local_ip() == Some(bound);
                *ipc = Some((Instant::now(), same));
                if !same {
                    *lock(&self.net) = None; // profil de yeniden sorulsun
                }
            }
            if !ipc.is_some_and(|(_, ok)| ok) {
                return false;
            }
        }
        let mut cache = lock(&self.net);
        if let Some((at, ok)) = *cache
            && at.elapsed() < NET_CACHE
        {
            return ok;
        }
        let ok = query_profiles().is_some_and(|p| listening_allowed(&p));
        *cache = Some((Instant::now(), ok));
        ok
    }

    fn revocations(&self) -> Option<Arc<Revocations>> {
        let mut rev = lock(&self.rev);
        // M-O2: bellekteki kopya da `STATUS_MAX_AGE`'den eskiyse kullanılmaz (indirme hep başarısız olsa bile
        // haftalık eski liste geçerli sayılmaz); disk kopyası da eskiyse liste yok sayılır.
        let stale = rev.as_ref().is_some_and(|(at, _)| !at.elapsed().is_ok_and(|e| e <= STATUS_MAX_AGE));
        if rev.is_none() || stale {
            *rev = load_status_file();
        }
        rev.as_ref().map(|(_, r)| r.clone())
    }
}

/// Çalışan köprü; yalnız modül açıkken vardır.
struct Running {
    bridge: Bridge,
    _announce: Option<Announcement>,
    host: Arc<AppHost>,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    enabled: bool,
    /// Açık ama dinlenemiyorsa nedeni (arayüz açıkça söyler).
    problem: Option<String>,
}

pub struct PhoneHub {
    app: AppHandle,
    inner: Mutex<Inner>,
}

/// Varsayılan yol arayüzünün yerel IPv4 adresi (paket göndermez).
fn local_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("10.255.255.255:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    is_local_bind(ip).then_some(ip)
}

impl PhoneHub {
    fn start(&self, inner: &mut Inner) {
        if inner.running.is_some() {
            return;
        }
        let host = Arc::new(AppHost {
            app: self.app.clone(),
            net: Mutex::new(None),
            bound: Mutex::new(None),
            ip_check: Mutex::new(None),
            audit: Mutex::new(VecDeque::new()),
            folder_salt: {
                use std::hash::{BuildHasher, Hasher};
                std::collections::hash_map::RandomState::new().build_hasher().finish()
            },
            rev: Mutex::new(None),
            rev_busy: AtomicBool::new(false),
            rev_tried: Mutex::new(None),
        });
        if !host.network_ok() {
            inner.problem = Some("Ağ 'Özel' değil (Windows'ta Ortak/Genel). Ev ağını Özel yap, sonra yeniden dene.".into());
            return;
        }
        let Some(ip) = local_ip() else {
            inner.problem = Some("Yerel ağ adresi bulunamadı (Wi-Fi/Ethernet bağlı mı?).".into());
            return;
        };
        *lock(&host.bound) = Some(ip);
        match Bridge::start(host.clone(), Arc::new(CredVault), ip, PREFERRED_PORT) {
            Ok(bridge) => {
                // Duyuru başarısız olursa köprü yine çalışır (telefon son adresi dener).
                let announce = Announcement::start(ip, bridge.addr().port()).ok();
                inner.problem = None;
                // G4: disk kopyası varsa hemen uygulanır (önceden "beklemede" kalan cihazlar), sonra tazelenir.
                if let Some(rev) = host.revocations() {
                    let _ = bridge.apply_revocations(&rev);
                }
                refresh_revocations(&self.app, &host);
                inner.running = Some(Running { bridge, _announce: announce, host });
            }
            Err(e) => inner.problem = Some(e),
        }
    }

    /// Köprüyü durumdan çıkarır. Dönen değer düşürülünce dinleme, bağlantılar ve duyuru kapanır; M-D1: bunu
    /// çağıran hub kilidini bıraktıktan sonra yapar (kapanış iş parçacıkları beklerken kilit tutulmaz).
    #[must_use]
    fn stop(inner: &mut Inner) -> Option<Running> {
        inner.problem = None;
        inner.running.take()
    }

    /// Köprüsüz durumda hub kilidini döndürür; çalışan köprü kilit dışında kapatılır.
    fn lock_stopped(&self) -> MutexGuard<'_, Inner> {
        loop {
            let mut inner = lock(&self.inner);
            match Self::stop(&mut inner) {
                Some(old) => {
                    drop(inner);
                    drop(old);
                }
                None => return inner,
            }
        }
    }
}

pub fn init(app: &AppHandle, enabled: bool) {
    let hub = Arc::new(PhoneHub { app: app.clone(), inner: Mutex::new(Inner::default()) });
    app.manage(hub.clone());
    set_enabled(app, enabled);
}

/// Ayarlar → Modüller → Telefon açılıp kapanınca (kapalıyken hiçbir şey çalışmaz).
pub fn set_enabled(app: &AppHandle, enabled: bool) {
    let Some(hub) = app.try_state::<Arc<PhoneHub>>() else { return };
    let old = {
        let mut inner = lock(&hub.inner);
        if inner.enabled == enabled && (!enabled || inner.running.is_some()) {
            return;
        }
        inner.enabled = enabled;
        if enabled {
            hub.start(&mut inner);
            None
        } else {
            PhoneHub::stop(&mut inner)
        }
    };
    drop(old);
    let _ = app.emit("phone://status", ());
}

/// Ajan durumu her yayımlandığında (agents://snapshot) bağlı telefonlara değişen durum gider.
pub fn notify(app: &AppHandle) {
    let Some(hub) = app.try_state::<Arc<PhoneHub>>() else { return };
    // Kilidi bırakmadan push: push_state ajan durumunu okur; hub kilidi ajan kilidiyle iç içe alınmaz.
    let inner = lock(&hub.inner);
    if let Some(r) = &inner.running {
        r.bridge.push_state();
        // G4: köprü uzun süre açık kalırsa liste günde bir tazelenir (ucuz zaman denetimi; indirme arka planda).
        refresh_revocations(app, &r.host);
    }
}

/// Ajanda değişince (today.rs) bağlı telefonlara yeni 14 günlük liste gider; değişmediyse hiçbir şey gitmez.
pub fn notify_agenda(app: &AppHandle) {
    let Some(hub) = app.try_state::<Arc<PhoneHub>>() else { return };
    let inner = lock(&hub.inner);
    if let Some(r) = &inner.running {
        r.bridge.push_agenda();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceOut {
    id: String,
    name: String,
    paired_at: u64,
    last_seen: u64,
    connected: bool,
    /// O9: Key Attestation doğrulandı mı; `false` → arayüzde "Korumasız" (kısıtlı: not/görev/yeni görev kapalı).
    hw: bool,
    /// "strongbox" | "tee" | "".
    hw_level: String,
    /// Korumasızsa kısa neden (içerik yok).
    hw_why: String,
    /// G4: 30 günden uzun süredir bağlanmadı (bağlanınca yeniden eşleştirme istenir).
    expired: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCode {
    /// Telefonun söylediği ad (doğrulanmadı). Kod arayüze GİTMEZ: kullanıcı telefondakini yazar.
    name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    listening: bool,
    addr: Option<String>,
    problem: Option<String>,
    devices: Vec<DeviceOut>,
    pairing_open: bool,
    pair_code: Option<PairCode>,
    audit: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStart {
    /// QR görseli (SVG metni); jeton arayüze ayrıca verilmez.
    svg: String,
    addr: String,
    expires_in_secs: u64,
}

fn devices_from_vault() -> Vec<DeviceOut> {
    use zodyvel_phone_proto::devices::VaultData;
    CredVault
        .load()
        .and_then(|b| serde_json::from_str::<VaultData>(&b).ok())
        .map(|d| {
            d.devices
                .into_iter()
                .map(|x| DeviceOut {
                    expired: now_secs().saturating_sub(x.last_seen) > REPAIR_AFTER_SECS,
                    id: x.id,
                    name: x.name,
                    paired_at: x.paired_at,
                    last_seen: x.last_seen,
                    connected: false,
                    hw: x.hw,
                    hw_level: x.hw_level,
                    hw_why: x.hw_why,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn phone_status(hub: State<'_, Arc<PhoneHub>>) -> Status {
    let inner = lock(&hub.inner);
    match &inner.running {
        Some(r) => Status {
            enabled: true,
            listening: true,
            addr: Some(r.bridge.addr().to_string()),
            problem: None,
            devices: r.bridge.devices().into_iter().map(|d| DeviceOut { id: d.id, name: d.name, paired_at: d.paired_at, last_seen: d.last_seen, connected: d.connected, hw: d.hw, hw_level: d.hw_level, hw_why: d.hw_why, expired: d.expired }).collect(),
            pairing_open: r.bridge.pairing_open(),
            pair_code: r.bridge.pairing_name().map(|name| PairCode { name }),
            audit: lock(&r.host.audit).iter().rev().cloned().collect(),
        },
        None => Status {
            enabled: inner.enabled,
            listening: false,
            addr: None,
            problem: inner.problem.clone(),
            devices: devices_from_vault(),
            pairing_open: false,
            pair_code: None,
            audit: Vec::new(),
        },
    }
}

/// "Telefon ekle": QR (tek kullanımlık, 2 dk).
#[tauri::command]
pub fn phone_pair_start(hub: State<'_, Arc<PhoneHub>>) -> Result<PairStart, String> {
    let inner = lock(&hub.inner);
    let r = inner.running.as_ref().ok_or("Telefon köprüsü çalışmıyor (modül kapalı ya da ağ Özel değil).")?;
    let qr = r.bridge.open_pairing()?;
    // G4: eşleştirme bitene dek iptal listesi hazır olsun (taze değilse arka planda indirilir).
    refresh_revocations(&hub.app, &r.host);
    let svg = qr_svg(&qr.to_json())?;
    Ok(PairStart { svg, addr: qr.addr.clone(), expires_in_secs: 120 })
}

/// Eşleştirme onayı: kullanıcı TELEFONDA gösterilen 6 haneli kodu yazar (tek deneme; yanlışsa iptal).
#[tauri::command]
pub fn phone_pair_confirm(hub: State<'_, Arc<PhoneHub>>, code: String) -> Result<(), String> {
    let inner = lock(&hub.inner);
    let r = inner.running.as_ref().ok_or("Telefon köprüsü çalışmıyor.")?;
    r.bridge.confirm_pairing_code(&code)
}

#[tauri::command]
pub fn phone_pair_cancel(hub: State<'_, Arc<PhoneHub>>) {
    if let Some(r) = &lock(&hub.inner).running {
        r.bridge.cancel_pairing();
    }
}

/// Eşli telefonu anında kaldırır: güven silinir, açık bağlantı kesilir.
#[tauri::command]
pub fn phone_revoke(app: AppHandle, hub: State<'_, Arc<PhoneHub>>, id: String) -> Result<(), String> {
    let mut inner = lock(&hub.inner);
    if let Some(r) = &inner.running {
        let res = r.bridge.revoke(&id);
        return match res {
            Ok(true) => Ok(()),
            Ok(false) => Err("Telefon bulunamadı (zaten kaldırılmış olabilir).".into()),
            Err(e) => {
                // Kayıt silinemedi: güvenli taraf, dinleme kapanır, hata arayüzde görünür.
                let old = PhoneHub::stop(&mut inner);
                inner.problem = Some(e.clone());
                drop(inner);
                drop(old);
                let _ = app.emit("phone://status", ());
                Err(e)
            }
        };
    }
    drop(inner);
    // Köprü kapalıyken de kayıttan silinebilir.
    use zodyvel_phone_proto::devices::VaultData;
    let mut data: VaultData = CredVault.load_checked()?.and_then(|b| serde_json::from_str(&b).ok()).ok_or("Kayıt bulunamadı.")?;
    let before = data.devices.len();
    data.devices.retain(|d| d.id != id);
    if data.devices.len() == before {
        return Err("Telefon bulunamadı (zaten kaldırılmış olabilir).".into());
    }
    CredVault.store(&serde_json::to_string(&data).map_err(|_| "Kayıt hatası.")?)?;
    let _ = app.emit("phone://status", ());
    Ok(())
}

/// Hepsini kaldırır ve Noise anahtarını da yeniler (telefon kaybolduysa tek tıkla kes).
/// Kayıt silinemezse (doğrulanır) dinleme kapalı kalır ve hata döner.
#[tauri::command]
pub fn phone_revoke_all(app: AppHandle, hub: State<'_, Arc<PhoneHub>>) -> Result<(), String> {
    let was_running = lock(&hub.inner).running.is_some();
    let mut inner = hub.lock_stopped();
    let result = zodyvel_phone_proto::devices::wipe_verified(&CredVault);
    match &result {
        Ok(()) => {
            if was_running || inner.enabled {
                hub.start(&mut inner);
            }
        }
        Err(e) => inner.problem = Some(e.clone()),
    }
    drop(inner);
    let _ = app.emit("phone://status", ());
    result
}

/// Ağ Özel yapıldıktan / Wi-Fi değiştikten sonra dinlemeyi yeniden dener.
#[tauri::command]
pub fn phone_retry(app: AppHandle, hub: State<'_, Arc<PhoneHub>>) {
    {
        if !lock(&hub.inner).enabled {
            return;
        }
        let mut inner = hub.lock_stopped();
        if !inner.enabled {
            return;
        }
        hub.start(&mut inner);
    }
    let _ = app.emit("phone://status", ());
}

#[cfg(test)]
mod tests {
    use super::*;
    use zodyvel_phone_proto::proto::SessionView;

    fn view(ids: &[&str]) -> StateView {
        StateView {
            sessions: ids.iter().map(|id| SessionView { id: (*id).into(), tool: "claude".into(), name: "x".into(), status: "running".into(), step: String::new(), reply: None, ..Default::default() }).collect(),
            approvals: Vec::new(),
        }
    }

    #[test]
    fn o7_only_sessions_shown_to_the_phone_are_addressable() {
        let v = view(&["claude:abc"]);
        assert!(session_in_view(&v, "claude:abc"));
        assert!(session_in_view(&v, "abc"), "önek olsa da olmasa da aynı oturum");
        assert!(!session_in_view(&v, "claude:other"));
        assert!(!session_in_view(&v, ""));
        assert!(!session_in_view(&view(&[]), "claude:abc"));
    }

    #[test]
    fn o6_folder_ids_differ_per_path_and_salt() {
        assert_ne!(folder_id(1, "C:\\a"), folder_id(1, "C:\\b"));
        assert_ne!(folder_id(1, "C:\\a"), folder_id(2, "C:\\a"));
    }
}
