//! Telefon uygulaması çekirdeği (T4-a). Tauri'ye ve Android'e bağlı değildir: platform işleri (Keystore,
//! BiometricPrompt, cihaz kilidi) `Platform` arayüzünün arkasındadır; böylece masaüstünde/bulutta sınanır.
//!
//! Güvenlik (Plan 3 · 2.5):
//! - Noise özel anahtarı diske YALNIZ Keystore'daki AES anahtarıyla sarılı olarak yazılır (`wrap`).
//! - Eylem imzası kimlik doğrulamalı Keystore anahtarındadır (`sign` → BiometricPrompt); anahtar uygulamada yok.
//! - Telefonda ekran kilidi yoksa eşleştirme başlamaz.
//! - Günlüğe anahtar, jeton, kod ya da içerik yazılmaz (hiç günlük yok; `Debug` gizli).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use zodyvel_phone_proto::client::{connect_normal, start_pairing_with, PairingSession, Signer};
use zodyvel_phone_proto::pairing::{hex, hex_decode, QrPayload};
use zodyvel_phone_proto::session::Keypair;

mod discover;
mod live;
pub mod remind;
pub use remind::Reminder;
pub use live::{ApprovalInfo, LiveState, Notice, QuestionInfo, SessionInfo, StepInfo, ERR_CANCELED, ERR_DROPPED, ERR_NO_ANSWER, ERR_OFFLINE};

/// Ayrıntı okuma penceresi: doğrulamadan sonra bu kadar süre içerik görünür (Plan: 2 dk).
pub const READ_WINDOW: Duration = Duration::from_secs(120);

/// BiometricPrompt metni: ne onaylandığı tek cümleyle yazılır.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prompt {
    pub title: String,
    pub subtitle: String,
}

pub struct KeyInfo {
    /// ECDSA P-256 açık anahtarı, SEC1 sıkıştırılmamış (hex).
    pub apub: String,
    /// Android Key Attestation sertifika zinciri (hex DER, yaprak önce); desteklenmezse boş.
    pub attestation: Vec<String>,
}

pub trait Platform: Send + Sync {
    /// Telefonda ekran kilidi (PIN/desen/parola/biyometrik) kurulu mu?
    fn device_secure(&self) -> bool;
    fn device_name(&self) -> String;
    /// Kimlik doğrulamalı imza anahtarını üretir (eskisi silinir). `challenge`: eşleşmeye özgü rastgele bayt.
    fn create_signing_key(&self, challenge: &[u8]) -> Result<KeyInfo, String>;
    /// `msg`'yi imzalar (DER ECDSA-SHA256); önce BiometricPrompt çıkar. İptal/hata → Err.
    fn sign(&self, msg: &[u8], prompt: &Prompt) -> Result<Vec<u8>, String>;
    /// İmza gerektirmeyen kilit doğrulaması (ayrıntı okuma penceresi için).
    fn authenticate(&self, prompt: &Prompt) -> Result<(), String>;
    /// Keystore AES-GCM anahtarıyla sarma / açma (kimlik doğrulaması istemez; arka plan hizmeti kullanır).
    fn wrap(&self, data: &[u8]) -> Result<Vec<u8>, String>;
    fn unwrap(&self, data: &[u8]) -> Result<Vec<u8>, String>;
    /// Tüm Keystore anahtarlarını siler.
    fn delete_keys(&self) -> Result<(), String>;
    /// Ön plan hizmetini (Android) açar/kapatır; canlı bağlantı başlayınca/durunca çekirdek çağırır.
    fn service(&self, _on: bool) {}
    /// Bildirim olayı (içeriksiz); Android bildirimlerine çevrilir.
    fn notice(&self, _n: &Notice) {}
    /// G3: Ajanda hatırlatıcılarını BAŞTAN kurar (boş liste = hepsini sil). Android listeyi Keystore ile şifreli saklar,
    /// AlarmManager'a kurar, yeniden başlatmada/saat dilimi değişince kendisi yeniden kurar.
    fn reminders(&self, _list: &[Reminder]) {}
    /// G3: mDNS araması sürerken çok yöne yayın kilidi (Android `MulticastLock`) aç/kapat.
    fn multicast(&self, _on: bool) {}
    /// G3: bilgisayarı yerel ağda arar (aday adresler). `None` → çekirdeğin kendi mDNS araması (sınamada değiştirilir).
    fn discover(&self) -> Option<Vec<String>> {
        None
    }
    /// A-O1: canlı durum değişti (bağlantı, oturumlar, onaylar, hatırlatma sayısı). Arayüz varsa yeniden çeker;
    /// ekransızken hiçbir şey yapmaz.
    fn changed(&self) {}
}

#[derive(Serialize, Deserialize, Clone)]
struct Saved {
    v: u8,
    id: String,
    name: String,
    addr: String,
    desktop_pub: String,
    noise_pub: String,
    /// Sarılı özel anahtar (hex); düz hâli diske hiç inmez.
    noise_priv_wrapped: String,
    apub: String,
    paired_at: u64,
    /// O9: eşleşmede masaüstünün kanıt kararı (eski kayıtlarda yok).
    #[serde(default)]
    hw: Option<bool>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct AppState {
    /// "none" | "paired"
    pub phase: &'static str,
    pub device_secure: bool,
    pub desktop_name: Option<String>,
    pub fingerprint: Option<String>,
    pub unlocked: bool,
}

#[derive(Serialize, Debug)]
pub struct PairBegin {
    pub code: String,
    /// Bilgisayarın kimlik izi, kısa: "7F3A · C1E0 · 92B4".
    pub fingerprint: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Probe {
    pub sessions: usize,
    pub approvals: usize,
}

/// Yarım eşleşme: kanal, Noise anahtarı, imza açık anahtarı, kanıt zinciri.
type PendingPair = (PairingSession, Keypair, String, Vec<String>);

pub struct Core {
    platform: Box<dyn Platform>,
    dir: PathBuf,
    pending: Mutex<Option<PendingPair>>,
    unlocked_until: Mutex<Option<Instant>>,
    live: Mutex<Option<live::LiveHandle>>,
    live_shared: Mutex<live::Shared>,
    /// T-O4: hatırlatıcı ayarının denetimi ile alarm kurma/silme aynı kilit altında (kapatma ile eşitleme yarışmaz).
    remind: Mutex<()>,
}

/// Kimlik izi: açık anahtarın ilk 6 baytı, dörder haneli gruplar.
pub fn short_fingerprint(fp_hex: &str) -> String {
    let h = fp_hex.to_uppercase();
    let h: String = h.chars().take(12).collect();
    h.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join(" · ")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Core {
    pub fn new(platform: Box<dyn Platform>, dir: PathBuf) -> Self {
        Core { platform, dir, pending: Mutex::new(None), unlocked_until: Mutex::new(None), live: Mutex::new(None), live_shared: Mutex::new(Default::default()), remind: Mutex::new(()) }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("pair.json")
    }
    fn load(&self) -> Option<Saved> {
        serde_json::from_str(&std::fs::read_to_string(self.file()).ok()?).ok()
    }
    fn save(&self, s: &Saved) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|_| "Kayıt klasörü oluşturulamadı.".to_string())?;
        let tmp = self.dir.join("pair.json.tmp");
        std::fs::write(&tmp, serde_json::to_string(s).map_err(|_| "Kayıt hatası.")?).map_err(|_| "Kayıt yazılamadı.".to_string())?;
        std::fs::rename(&tmp, self.file()).map_err(|_| "Kayıt yazılamadı.".to_string())
    }

    pub fn state(&self) -> AppState {
        let s = self.load();
        AppState {
            phase: if s.is_some() { "paired" } else { "none" },
            device_secure: self.platform.device_secure(),
            desktop_name: s.as_ref().map(|s| s.name.clone()),
            fingerprint: s.as_ref().map(|s| short_fingerprint(&s.desktop_pub)),
            unlocked: self.is_unlocked(),
        }
    }

    /// QR okundu: el sıkışır, 6 haneli kodu döndürür. Telefonda ekran kilidi yoksa başlamaz.
    pub fn pair_begin(&self, qr_json: &str) -> Result<PairBegin, String> {
        if !self.platform.device_secure() {
            return Err("Önce telefonuna bir ekran kilidi (PIN, desen ya da parmak izi) kur; Zodyvel onsuz eşleşmez.".into());
        }
        let qr = QrPayload::from_json(qr_json)?;
        let kp = zodyvel_phone_proto::session::generate_keypair();
        let name = self.platform.device_name();
        // O9: imza anahtarı el sıkışmadan SONRA üretilir; Key Attestation sınaması el sıkışma özetidir, masaüstü
        // zincirin bu eşleştirmeye ait olduğunu böyle doğrular.
        let mut key = None;
        let ps = start_pairing_with(&qr, &kp, &name, |hh| {
            let k = self.platform.create_signing_key(hh)?;
            let out = (k.apub.clone(), k.attestation.clone());
            key = Some(k);
            Ok(out)
        })
        .inspect_err(|_| {
            if self.load().is_none() {
                let _ = self.platform.delete_keys();
            }
        })?;
        let key = key.ok_or("Güvenli anahtar üretilemedi.")?;
        let begin = PairBegin { code: ps.confirm.clone(), fingerprint: short_fingerprint(&qr.fp) };
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some((ps, kp, key.apub, key.attestation));
        Ok(begin)
    }

    /// Kullanıcı kodu bilgisayara yazdı (`same=true`) ya da vazgeçti. Başarıda anahtarlar kaydedilir.
    pub fn pair_finish(&self, same: bool) -> Result<(), String> {
        let Some((ps, kp, apub, _att)) = self.pending.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            return Err("Eşleştirme başlamadı.".into());
        };
        let paired = match ps.finish(same) {
            Ok(p) => p,
            Err(e) => {
                let _ = self.platform.delete_keys();
                return Err(e);
            }
        };
        let wrapped = self.platform.wrap(&kp.private)?;
        let saved = Saved {
            v: 1,
            id: paired.id,
            name: "Bilgisayar".into(),
            addr: paired.addr,
            desktop_pub: hex(&paired.desktop_pub),
            noise_pub: hex(&kp.public),
            noise_priv_wrapped: hex(&wrapped),
            apub,
            paired_at: now_secs(),
            hw: paired.hw,
        };
        self.save(&saved).inspect_err(|_| {
            let _ = self.platform.delete_keys();
        })
    }

    pub fn pair_cancel(&self) {
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if self.load().is_none() {
            let _ = self.platform.delete_keys();
        }
    }

    /// G3: mDNS ile bulunan ve el sıkışmayla DOĞRULANMIŞ yeni adresi kaydeder.
    fn set_addr(&self, addr: &str) -> Result<(), String> {
        let mut s = self.load().ok_or("Henüz eşleşmedin.")?;
        if s.addr != addr {
            s.addr = addr.to_owned();
            self.save(&s)?;
        }
        Ok(())
    }

    /// Kayıtlı bilgisayar adresi (sınama ve tanı için; ağ adresi gizli değildir).
    pub fn saved_addr(&self) -> Option<String> {
        self.load().map(|s| s.addr)
    }

    fn identity(&self) -> Result<(Saved, Keypair, Vec<u8>), String> {
        let s = self.load().ok_or("Henüz eşleşmedin.")?;
        let wrapped = hex_decode(&s.noise_priv_wrapped).ok_or("Kayıt bozuk; yeniden eşleş.")?;
        let private = self.platform.unwrap(&wrapped).map_err(|_| "Anahtar açılamadı; yeniden eşleşmen gerekebilir.".to_string())?;
        let public = hex_decode(&s.noise_pub).ok_or("Kayıt bozuk; yeniden eşleş.")?;
        let desk = hex_decode(&s.desktop_pub).ok_or("Kayıt bozuk; yeniden eşleş.")?;
        Ok((s, Keypair { private, public }, desk))
    }

    /// Eşli bilgisayara bağlanıp durumu okur (T4-a: uçtan uca deneme; T4-b hizmeti aynı yolu kullanır).
    pub fn probe(&self) -> Result<Probe, String> {
        let (s, kp, desk) = self.identity()?;
        let mut link = connect_normal(&s.addr, &kp, &desk)?;
        let mut m = link.recv(Duration::from_secs(5))?.ok_or("Bilgisayar yanıt vermedi.")?;
        if m.get("t").and_then(|t| t.as_str()) == Some("device") {
            m = link.recv(Duration::from_secs(5))?.ok_or("Bilgisayar yanıt vermedi.")?;
        }
        match m.get("t").and_then(|t| t.as_str()) {
            Some("state") => Ok(Probe {
                sessions: m.get("sessions").and_then(|v| v.as_array()).map_or(0, Vec::len),
                approvals: m.get("approvals").and_then(|v| v.as_array()).map_or(0, Vec::len),
            }),
            Some("bye") => Err(match m.get("why").and_then(|w| w.as_str()) {
                Some("repair") => "30 gündür bağlanmadın; yeniden eşleşmen gerekiyor.",
                Some("network") | Some("locked") => "Bilgisayar şu an bağlantıya kapalı (ağ Özel değil ya da kilitli).",
                Some("revoked") => "Bu telefonun eşleşmesi bilgisayardan kaldırıldı.",
                _ => "Bilgisayar bağlantıyı kapattı.",
            }
            .into()),
            _ => Err("Beklenmeyen yanıt.".into()),
        }
    }

    /// Cihaz kilidiyle ayrıntı okuma penceresini açar (2 dk, yalnız bellekte).
    pub fn unlock(&self) -> Result<(), String> {
        self.platform.authenticate(&Prompt { title: "Zodyvel · Ayrıntıları aç".into(), subtitle: "AI oturumlarının içeriğini görmek için doğrula".into() })?;
        *self.unlocked_until.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + READ_WINDOW);
        Ok(())
    }
    pub fn lock(&self) {
        *self.unlocked_until.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    pub fn is_unlocked(&self) -> bool {
        self.unlocked_until.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| Instant::now() < t)
    }
    /// Ayrıntı penceresinin bitişi (ms, Unix); kapalıysa `None`.
    pub fn unlocked_until(&self) -> Option<u64> {
        let t = (*self.unlocked_until.lock().unwrap_or_else(|e| e.into_inner()))?;
        let left = t.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        Some(now + left.as_millis() as u64)
    }

    /// Telefondaki eşleşmeyi siler (bilgisayardaki kayıt ayrıca "Kaldır" ile silinir).
    pub fn unpair(&self) -> Result<(), String> {
        // T-D2: canlı bağlantı kilidi silme bitene dek tutulur; araya giren `live_start` yeni iş parçacığı açamaz.
        let mut live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        self.stop_locked(&mut live);
        let _ = std::fs::remove_file(self.dir.join("live.off"));
        // G3: hatırlatıcılar ve açılış ayarı da gider (ayarlar varsayılana döner).
        {
            let _r = self.remind.lock().unwrap_or_else(|e| e.into_inner());
            self.platform.reminders(&[]);
            for f in ["remind.off", "remind.n", "boot.on"] {
                let _ = std::fs::remove_file(self.dir.join(f));
            }
            let mut s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            s.remind_sig = None;
            s.agenda = None;
        }
        self.lock();
        let _ = std::fs::remove_file(self.file());
        self.platform.delete_keys()?;
        if self.file().exists() {
            return Err("Kayıt silinemedi.".into());
        }
        Ok(())
    }

    /// İmzalayıcı olarak platformu verir (T4-b: onay/ret her seferinde BiometricPrompt ile imzalar).
    pub fn signer<'a>(&'a self, prompt: Prompt) -> impl Signer + 'a {
        struct S<'a>(&'a dyn Platform, Prompt);
        impl Signer for S<'_> {
            fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, String> {
                self.0.sign(msg, &self.1)
            }
        }
        S(self.platform.as_ref(), prompt)
    }
}
