//! Zody beyni (Faz 12, "Deneysel"). `Beyin` arayüzü ve ilk bağdaştırıcı: Claude aboneliği (claude.rs).
//! Erişim Kapalıyken beyin hiçbir eylem yapmaz, yalnız konuşur ve önerir; Düşük/Orta seviyede yalnız Zodyvel araç sunucusunu kullanır. Modül kapalıyken
//! süreç de bellek de yoktur; açıkken süreç yalnız istek gelince başlar ve 5 dk boşta kalınca kapanır.

mod claude;
mod server;
/// Yerel araç sunucusu çekirdeği (Faz 13a, saf JSON-RPC); bağlantı katmanı `server.rs` (Faz 13b).
mod tools;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Boşta bu kadar kalınca süreç kapanır; sonraki istekte yeniden açılır.
const BOSTA: Duration = Duration::from_secs(5 * 60);
/// İstek metninin üst sınırı (karakter).
const EN_UZUN_ISTEK: usize = 4000;

/// Hızlı komutlar Haiku, uzun işler Sonnet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Hizli,
    Uzun,
}

impl Model {
    pub fn oku(ad: &str) -> Option<Self> {
        match ad {
            "fast" => Some(Self::Hizli),
            "long" => Some(Self::Uzun),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Durum {
    pub kurulu: bool,
    pub surum: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Giris {
    Var,
    Yok,
    Belirsiz(String),
}

/// Bir yapay zekâ bağlantısı. İlk uygulayan: Claude aboneliği; API anahtarı bağdaştırıcısı sonra.
pub trait Beyin: Send {
    fn ad(&self) -> &'static str;
    /// Süreç şu an açık mı (bellek tutuyor mu).
    fn acik(&self) -> bool;
    /// Kurulu mu? Kimlik dosyalarına bakmaz.
    fn durum(&self) -> Durum;
    /// Küçük bir deneme isteğiyle giriş yapılmış mı.
    fn giris_dene(&self) -> Giris;
    /// Tek istek (aynı anda bir tane). `iptal` kalkarsa erken döner.
    /// `araclar`: Zodyvel'in yerel araç sunucusu (erişim seviyesi Kapalı değilse), yoksa araçsız konuşur.
    fn sor(&mut self, istek: &str, model: Model, araclar: Option<&server::Baglanti>, iptal: &AtomicBool) -> Result<String, String>;
    fn kapat(&mut self);
}

#[derive(Default)]
struct Ic {
    acik: bool,
    son: Option<Instant>,
    bekci: bool,
}

pub struct BeyinHub {
    ic: Mutex<Ic>,
    beyin: Mutex<Box<dyn Beyin>>,
    /// "Durdur" ve kapatma için: çalışan sürecin kimliği ve iptal bayrağı.
    pid: Arc<AtomicU32>,
    iptal: AtomicBool,
    /// Yerel araç sunucusu: yalnız seviye Kapalı değilken ve istek gelince açılır, süreçle birlikte kapanır.
    sunucu: Mutex<Option<server::Sunucu>>,
}

fn kilit<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn init(app: &AppHandle, acik: bool) {
    let pid = Arc::new(AtomicU32::new(0));
    let hub = BeyinHub { ic: Mutex::new(Ic { acik, ..Ic::default() }), beyin: Mutex::new(Box::new(claude::Claude::yeni(pid.clone()))), pid, iptal: AtomicBool::new(false), sunucu: Mutex::new(None) };
    app.manage(Arc::new(hub));
}

/// Ayarlar → Zody asistanı açılıp kapanınca. Kapanınca süreç hemen kapanır.
pub fn set_enabled(app: &AppHandle, acik: bool) {
    let Some(hub) = app.try_state::<Arc<BeyinHub>>() else { return };
    kilit(&hub.ic).acik = acik;
    if !acik {
        hub.durdur();
    }
    let _ = app.emit("brain://status", ());
}

impl BeyinHub {
    /// Çalışan isteği iptal eder ve süreci kapatır (kilit tutulsa bile).
    fn durdur(&self) {
        self.iptal.store(true, Ordering::SeqCst);
        let pid = self.pid.swap(0, Ordering::SeqCst);
        if pid != 0 {
            let _ = crate::tasks::kill_tree(pid);
        }
        // Süreç kilit dışında kapandıysa bağdaştırıcı bir sonraki istekte kendi durumunu düzeltir.
        if let Ok(mut b) = self.beyin.try_lock() {
            b.kapat();
        }
        self.sunucu_kapat();
    }

    fn sunucu_kapat(&self) {
        if let Some(s) = kilit(&self.sunucu).take() {
            s.kapat();
        }
    }

    /// Seviye Kapalı değilse sunucuyu (gerekirse başlatıp) yeni tur için hazırlar ve bağlantıyı verir; Kapalıysa sunucuyu kapatır.
    fn araclari_hazirla(&self, app: &AppHandle, istek: &str) -> Result<Option<server::Baglanti>, String> {
        let seviye = crate::actions::access::erisim_seviyesi(app);
        if seviye == crate::actions::spec::Erisim::Kapali {
            self.sunucu_kapat();
            return Ok(None);
        }
        let mut s = kilit(&self.sunucu);
        if s.is_none() {
            *s = Some(server::Sunucu::baslat(app)?);
        }
        let sunucu = s.as_ref().ok_or("Araç sunucusu hazır değil.")?;
        sunucu.yeni_tur(istek);
        Ok(Some(sunucu.baglanti(seviye)))
    }
    fn acik_mi(&self) -> bool {
        kilit(&self.ic).acik
    }
}

/// İlk istekle bir bekçi iş parçacığı başlar; süreç 5 dk boşta kalınca kapatır ve kendisi biter
/// (süreç yokken hiçbir şey yoklanmaz).
fn bekci_kur(hub: &Arc<BeyinHub>) {
    {
        let mut ic = kilit(&hub.ic);
        if ic.bekci {
            return;
        }
        ic.bekci = true;
    }
    let hub = hub.clone();
    let _ = std::thread::Builder::new().name("zody-beyin-bekci".into()).spawn(move || {
        loop {
            let kalan = kilit(&hub.ic).son.map_or(Duration::ZERO, |s| BOSTA.saturating_sub(s.elapsed()));
            if !kalan.is_zero() {
                std::thread::sleep(kalan);
                continue;
            }
            match hub.beyin.try_lock() {
                Ok(mut b) => {
                    let mut ic = kilit(&hub.ic);
                    if ic.son.is_some_and(|s| s.elapsed() < BOSTA) {
                        continue;
                    }
                    b.kapat();
                    hub.sunucu_kapat();
                    ic.bekci = false;
                    return;
                }
                // İstek sürüyor: biraz sonra yeniden bak.
                Err(_) => std::thread::sleep(Duration::from_secs(10)),
            }
        }
    });
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Durumu {
    pub enabled: bool,
    pub installed: bool,
    pub version: Option<String>,
    pub running: bool,
    pub name: &'static str,
}

fn hub(app: &AppHandle) -> Result<Arc<BeyinHub>, String> {
    app.try_state::<Arc<BeyinHub>>().map(|h| h.inner().clone()).ok_or_else(|| "Zody asistanı hazır değil.".to_owned())
}

/// Ayarlar sayfasının durumu: kurulu mu (`claude --version`), süreç açık mı. Kapalıyken süreç başlatılmaz.
#[tauri::command]
pub async fn brain_status(app: AppHandle) -> Result<Durumu, String> {
    let hub = hub(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let enabled = hub.acik_mi();
        // İstek sürerken kilit uzun tutulur: o zaman süreç açık ve kuruludur (sürüm yeniden sorulmaz).
        match hub.beyin.try_lock() {
            Ok(b) => {
                let d = b.durum();
                Durumu { enabled, installed: d.kurulu, version: d.surum, running: b.acik(), name: b.ad() }
            }
            Err(_) => Durumu { enabled, installed: true, version: None, running: true, name: "Claude aboneliği" },
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// "Giriş yapılmış mı" denemesi (düğmeyle): küçük bir istek gider. "yes" | "no" | "unknown:<neden>".
#[tauri::command]
pub async fn brain_check_login(app: AppHandle) -> Result<String, String> {
    let hub = hub(&app)?;
    if !hub.acik_mi() {
        return Err("Zody asistanı kapalı.".into());
    }
    tauri::async_runtime::spawn_blocking(move || match kilit(&hub.beyin).giris_dene() {
        Giris::Var => "yes".to_owned(),
        Giris::Yok => "no".to_owned(),
        Giris::Belirsiz(n) => format!("unknown:{n}"),
    })
    .await
    .map_err(|e| e.to_string())
}

/// Anlaşılmayan istek beyne gider. `model`: "fast" | "long".
#[tauri::command]
pub async fn brain_ask(app: AppHandle, text: String, model: String) -> Result<String, String> {
    let hub = hub(&app)?;
    if !hub.acik_mi() {
        return Err("Zody asistanı kapalı (Ayarlar → Zody asistanı).".into());
    }
    let metin = text.trim().to_owned();
    if metin.is_empty() || metin.chars().count() > EN_UZUN_ISTEK {
        return Err("İstek boş ya da çok uzun.".into());
    }
    let model = Model::oku(&model).ok_or("Bilinmeyen model.")?;
    bekci_kur(&hub);
    let h = hub.clone();
    let app2 = app.clone();
    let cevap = tauri::async_runtime::spawn_blocking(move || {
        let mut beyin = kilit(&h.beyin);
        // Kapatma isteği bu arada geldiyse hiç başlama.
        if !h.acik_mi() {
            return Err("Zody asistanı kapalı.".to_owned());
        }
        h.iptal.store(false, Ordering::SeqCst);
        let araclar = h.araclari_hazirla(&app2, &metin)?;
        let sonuc = beyin.sor(&metin, model, araclar.as_ref(), &h.iptal);
        kilit(&h.ic).son = Some(Instant::now());
        sonuc
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = app.emit("brain://status", ());
    cevap
}

/// Acil durdurma: çalışan isteği iptal eder ve süreci kapatır (modül kapalıysa hiçbir şey yapmaz).
pub fn cancel(app: &AppHandle) {
    if let Some(hub) = app.try_state::<Arc<BeyinHub>>() {
        hub.durdur();
    }
}

/// Süren isteği durdurur ve süreci kapatır.
#[tauri::command]
pub fn brain_stop(app: AppHandle) -> Result<(), String> {
    hub(&app)?.durdur();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bosta_bes_dakika() {
        assert_eq!(BOSTA, Duration::from_secs(300));
    }

    #[test]
    fn model_adlari() {
        assert_eq!(Model::oku("fast"), Some(Model::Hizli));
        assert_eq!(Model::oku("long"), Some(Model::Uzun));
        assert_eq!(Model::oku("opus"), None);
    }

    /// Kabul: Zodyvel Claude'un kimlik dosyalarına hiç erişmez. Beyin modülünün kaynağı taranır: dosya sistemi
    /// arayüzü, kimlik dosyası adları ve anahtar/jeton değişkenleri hiçbir yerde geçmez (yasak sözcükler
    /// parçalı yazılır ki bu sınama kendini yakalamasın).
    #[test]
    fn kimlik_dosyalarina_erisim_yok() {
        let yasak: Vec<String> = [
            ["std::", "fs"].concat(),
            ["fs::", "read"].concat(),
            ["File::", "open"].concat(),
            ["cred", "entials"].concat(),
            [".claude", ".json"].concat(),
            ["OAUTH", "_TOKEN"].concat(),
            ["ANTHROPIC", "_API_KEY"].concat(),
            ["Credential", "Read"].concat(),
            ["USERPRO", "FILE"].concat(),
            ["CLAUDE_CONFIG", "_DIR"].concat(),
        ]
        .into();
        let dizin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join("brain");
        let mut tarandi = 0;
        for girdi in std::fs::read_dir(&dizin).unwrap() {
            let yol = girdi.unwrap().path();
            let kaynak = std::fs::read_to_string(&yol).unwrap();
            // Yalnız üretim kodu: sınama bölümü (bu dosyadaki liste dahil) taranmaz.
            let kod = kaynak.split("#[cfg(test)]").next().unwrap();
            for k in &yasak {
                assert!(!kod.contains(k.as_str()), "{} içinde yasak sözcük: {k}", yol.display());
            }
            tarandi += 1;
        }
        assert!(tarandi >= 4, "mod, claude, tools ve server taranmalı");
    }

    #[test]
    fn modul_kapaliyken_surec_yok() {
        // Kapalı: istek hiç işlenmez, bağdaştırıcı süreç tutmaz.
        let pid = Arc::new(AtomicU32::new(0));
        let hub = BeyinHub { ic: Mutex::new(Ic::default()), beyin: Mutex::new(Box::new(claude::Claude::yeni(pid.clone()))), pid, iptal: AtomicBool::new(false), sunucu: Mutex::new(None) };
        assert!(!hub.acik_mi());
        assert!(kilit(&hub.sunucu).is_none(), "kapalıyken araç sunucusu da yok");
        assert!(!kilit(&hub.beyin).acik());
        hub.durdur();
        assert_eq!(hub.pid.load(Ordering::SeqCst), 0);
    }
}
