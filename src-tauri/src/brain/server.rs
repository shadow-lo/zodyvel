//! Zodyvel'in yerel araç sunucusunun bağlantı katmanı (Faz 13b). `tools.rs` JSON-RPC'yi işler; burası ona
//! Claude Code'un konuşabileceği bir kapı açar: yalnız `127.0.0.1`, rastgele port, her başlatmada yeni rastgele
//! jeton (`Authorization: Bearer`). Tarayıcıdan gelen istekler (`Origin` başlığı) reddedilir.
//!
//! Sunucu yalnız Zody asistanı açık **ve** erişim seviyesi Kapalı değilken, ilk istekle başlar; asistan süreci kapanınca
//! (boşta 5 dk, Durdur, acil durdurma, modülü kapatma, seviye Kapalı) birlikte kapanır. Kabul döngüsü engelleyicidir:
//! beklerken işlemci harcamaz. Her araç çağrısı `tools::isle` → `rules::hazirla` kapısından ve oradan
//! `actions::arac_calistir` onay hattından geçer; yani seviye, kart, 5 eylem sınırı ve denetim kaydı aynen geçerlidir.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use rand::RngCore;
use serde_json::{Value, json};
use tauri::AppHandle;

use super::tools::{self, Yanit};
use crate::actions::rules::Tur;
use crate::actions::spec::{self, Erisim};
use crate::actions::{self, Outcome, access};

/// Gövde ve başlık üst sınırları.
const GOVDE_EN_UZUN: usize = 256 * 1024;
const BASLIK_EN_UZUN: usize = 16 * 1024;
/// Yavaş istemciye takılmamak için okuma/yazma süresi.
const SOKET_SURESI: Duration = Duration::from_secs(15);
/// Araç sonucunun Zody'ye giden metin sınırı (karakter).
const SONUC_EN_UZUN: usize = 6000;

/// Claude Code'a verilen bağlantı bilgisi. Seviye de içindedir: seviye değişince süreç yeniden başlar ki araç listesi tazelensin.
#[derive(Clone, Debug, PartialEq)]
pub struct Baglanti {
    pub url: String,
    pub jeton: String,
    pub seviye: Erisim,
}

impl Baglanti {
    /// `--mcp-config` değeri (JSON metni).
    pub fn yapilandirma(&self) -> String {
        json!({ "mcpServers": { "zodyvel": { "type": "http", "url": self.url, "headers": { "Authorization": format!("Bearer {}", self.jeton) } } } }).to_string()
    }
}

pub struct Sunucu {
    adres: SocketAddr,
    jeton: String,
    dur: Arc<AtomicBool>,
    tur: Arc<Mutex<Tur>>,
}

fn kilit<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn rastgele_jeton() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Sunucu {
    pub fn baslat(app: &AppHandle) -> Result<Self, String> {
        let dinleyici = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| "Yerel araç sunucusu açılamadı.".to_owned())?;
        let adres = dinleyici.local_addr().map_err(|_| "Yerel araç sunucusu adresi okunamadı.".to_owned())?;
        let (jeton, dur, tur) = (rastgele_jeton(), Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(Tur::default())));
        let (app2, jeton2, dur2, tur2) = (app.clone(), jeton.clone(), dur.clone(), tur.clone());
        std::thread::Builder::new()
            .name("zody-arac-sunucusu".into())
            .spawn(move || {
                for akim in dinleyici.incoming() {
                    if dur2.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(akim) = akim else { continue };
                    let (app, jeton, tur) = (app2.clone(), jeton2.clone(), tur2.clone());
                    let _ = std::thread::Builder::new().name("zody-arac-baglanti".into()).spawn(move || baglanti_isle(&app, akim, &jeton, &tur));
                }
            })
            .map_err(|_| "Yerel araç sunucusu başlatılamadı.".to_owned())?;
        Ok(Self { adres, jeton, dur, tur })
    }

    pub fn baglanti(&self, seviye: Erisim) -> Baglanti {
        Baglanti { url: format!("http://{}/mcp", self.adres), jeton: self.jeton.clone(), seviye }
    }

    /// Yeni kullanıcı mesajı: sayaç, döngü kaydı ve dış içerik bayrağı sıfırlanır.
    pub fn yeni_tur(&self, istek: &str) {
        *kilit(&self.tur) = Tur::yeni(istek, false);
    }

    /// Dinlemeyi bırakır: bayrak kalkar, bekleyen `accept` kendi kendine bağlanılarak uyandırılır.
    pub fn kapat(&self) {
        self.dur.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.adres, Duration::from_millis(500));
    }
}

/// Ayrıştırılmış HTTP isteği (yalnız gereken alanlar).
#[derive(Debug, PartialEq)]
pub struct Http {
    pub yontem: String,
    pub yol: String,
    pub jeton: Option<String>,
    pub origin: bool,
    pub govde: Vec<u8>,
}

/// HTTP isteğini okur: istek satırı, başlıklar, `Content-Length` kadar gövde. Sınırları aşan istek reddedilir.
pub fn http_oku(girdi: impl Read) -> Result<Http, &'static str> {
    let mut oku = BufReader::new(girdi);
    let mut satir = String::new();
    oku.by_ref().take(BASLIK_EN_UZUN as u64).read_line(&mut satir).map_err(|_| "okunamadı")?;
    let mut parca = satir.split_whitespace();
    let (yontem, yol) = (parca.next().ok_or("boş istek")?.to_owned(), parca.next().ok_or("yol yok")?.to_owned());
    let (mut uzunluk, mut jeton, mut origin, mut toplam) = (0usize, None, false, satir.len());
    loop {
        let mut b = String::new();
        let n = oku.by_ref().take(BASLIK_EN_UZUN as u64).read_line(&mut b).map_err(|_| "okunamadı")?;
        toplam += n;
        if n == 0 || toplam > BASLIK_EN_UZUN {
            return Err("başlıklar çok uzun ya da eksik");
        }
        let b = b.trim_end();
        if b.is_empty() {
            break;
        }
        if let Some((ad, deger)) = b.split_once(':') {
            let (ad, deger) = (ad.trim().to_ascii_lowercase(), deger.trim());
            match ad.as_str() {
                "content-length" => uzunluk = deger.parse().map_err(|_| "Content-Length geçersiz")?,
                "authorization" => jeton = deger.strip_prefix("Bearer ").map(str::to_owned),
                "origin" => origin = true,
                _ => {}
            }
        }
    }
    if uzunluk > GOVDE_EN_UZUN {
        return Err("gövde çok büyük");
    }
    let mut govde = vec![0u8; uzunluk];
    oku.read_exact(&mut govde).map_err(|_| "gövde eksik")?;
    Ok(Http { yontem, yol, jeton, origin, govde })
}

/// HTTP yanıtı: durum satırı + gövde (JSON ya da boş).
fn http_yaniti(kod: u16, neden: &str, govde: Option<&Value>) -> Vec<u8> {
    let g = govde.map(Value::to_string).unwrap_or_default();
    let tur = if govde.is_some() { "Content-Type: application/json\r\n" } else { "" };
    format!("HTTP/1.1 {kod} {neden}\r\n{tur}Content-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{g}", g.len()).into_bytes()
}

/// Saf yönlendirme: kimlik, kaynak, yöntem ve gövde denetimi; JSON-RPC işi `isle` ile yapılır (bildirimde `None`).
pub fn yonlendir(istek: &Http, jeton: &str, isle: impl FnOnce(&Value) -> Option<Value>) -> Vec<u8> {
    // Jeton doğrulanmadan hiçbir ayrıntı (yol, yöntem) sızdırılmaz.
    if istek.origin {
        return http_yaniti(403, "Forbidden", None);
    }
    if istek.jeton.as_deref() != Some(jeton) {
        return http_yaniti(401, "Unauthorized", None);
    }
    if istek.yol != "/mcp" {
        return http_yaniti(404, "Not Found", None);
    }
    if istek.yontem != "POST" {
        return http_yaniti(405, "Method Not Allowed", None);
    }
    match serde_json::from_slice::<Value>(&istek.govde) {
        Ok(v) if v.is_object() => match isle(&v) {
            Some(yanit) => http_yaniti(200, "OK", Some(&yanit)),
            None => http_yaniti(202, "Accepted", None),
        },
        _ => http_yaniti(400, "Bad Request", None),
    }
}

fn kisalt(metin: String) -> String {
    if metin.chars().count() > SONUC_EN_UZUN { metin.chars().take(SONUC_EN_UZUN).collect::<String>() + "…" } else { metin }
}

/// Çalışmış eylemin Zody'ye giden metni. Kart bekleyen eylem "yapıldı" diye söylenmez.
fn sonuc_metni(sonuc: Result<Outcome, String>) -> Result<String, String> {
    match sonuc? {
        Outcome::Done { result } => Ok(kisalt(if result.is_null() { "Tamam.".into() } else { result.to_string() })),
        Outcome::Pending { request } => Ok(format!(
            "Henüz yapılmadı: onay kartı kullanıcıya gösterildi ({}). Kullanıcı onaylarsa çalışır, 2 dakikada cevap yoksa iptal olur. Yapılmış gibi söyleme; kullanıcıya onay beklediğini söyle.",
            request.summary
        )),
        Outcome::Denied => Err("Kullanıcı reddetti.".into()),
    }
}

/// Bir JSON-RPC isteğini uygulamaya bağlar: seviye canlı okunur, çağrı onay hattına gider, dış içerik bayrağı işlenir.
fn rpc(app: &AppHandle, tur: &Mutex<Tur>, istek: &Value) -> Option<Value> {
    let seviye = access::erisim_seviyesi(app);
    let yanit = tools::isle(istek, seviye, &mut kilit(tur))?;
    Some(match yanit {
        Yanit::Hemen(v) => v,
        Yanit::Cagri { id, action, args } => {
            let sonuc = {
                let t = kilit(tur);
                tauri::async_runtime::block_on(actions::arac_calistir(app, &t, action, args))
            };
            // Dış içerik taşıyan bir şey okundu: kalan Güvenli eylemler bu turda kart ister.
            if sonuc.is_ok() && spec::dis_icerik_dondurur(action) {
                kilit(tur).dis_icerik = true;
            }
            tools::cagri_yaniti(&id, sonuc_metni(sonuc))
        }
    })
}

fn baglanti_isle(app: &AppHandle, mut akim: TcpStream, jeton: &str, tur: &Mutex<Tur>) {
    let _ = akim.set_read_timeout(Some(SOKET_SURESI));
    let _ = akim.set_write_timeout(Some(SOKET_SURESI));
    let Ok(okuma) = akim.try_clone() else { return };
    let yanit = match http_oku(okuma) {
        Ok(istek) => yonlendir(&istek, jeton, |v| rpc(app, tur, v)),
        Err(_) => http_yaniti(400, "Bad Request", None),
    };
    let _ = akim.write_all(&yanit);
    let _ = akim.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn istek(yontem: &str, yol: &str, jeton: Option<&str>, origin: bool, govde: &str) -> Http {
        Http { yontem: yontem.into(), yol: yol.into(), jeton: jeton.map(str::to_owned), origin, govde: govde.as_bytes().to_vec() }
    }

    fn kod(y: &[u8]) -> u16 {
        String::from_utf8_lossy(y).split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    #[test]
    fn jetonsuz_ve_tarayicidan_gelen_reddedilir() {
        let ok = |_: &Value| Some(json!({"ok": true}));
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", None, false, "{}"), "j", ok)), 401);
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("yanlis"), false, "{}"), "j", ok)), 401);
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("j"), true, "{}"), "j", ok)), 403, "Origin başlığı = tarayıcı");
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("j"), false, "{}"), "j", ok)), 200);
    }

    #[test]
    fn yol_yontem_ve_govde_denetimi() {
        let ok = |_: &Value| Some(json!({}));
        assert_eq!(kod(&yonlendir(&istek("POST", "/baska", Some("j"), false, "{}"), "j", ok)), 404);
        assert_eq!(kod(&yonlendir(&istek("GET", "/mcp", Some("j"), false, ""), "j", ok)), 405);
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("j"), false, "bozuk"), "j", ok)), 400);
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("j"), false, "[1]"), "j", ok)), 400, "toplu istek desteklenmez");
        // Bildirim (yanıtsız) 202.
        assert_eq!(kod(&yonlendir(&istek("POST", "/mcp", Some("j"), false, r#"{"method":"notifications/initialized"}"#), "j", |_| None)), 202);
    }

    #[test]
    fn jetonsuz_istek_isleyiciyi_hic_calistirmaz() {
        let y = yonlendir(&istek("POST", "/mcp", None, false, "{}"), "j", |_| panic!("kapı atlandı"));
        assert_eq!(kod(&y), 401);
    }

    #[test]
    fn http_okuma() {
        let ham = "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:1\r\nAuthorization: Bearer abc\r\nContent-Length: 2\r\n\r\n{}";
        let h = http_oku(ham.as_bytes()).unwrap();
        assert_eq!((h.yontem.as_str(), h.yol.as_str(), h.jeton.as_deref(), h.origin, h.govde.as_slice()), ("POST", "/mcp", Some("abc"), false, b"{}".as_slice()));
        let tarayici = "POST /mcp HTTP/1.1\r\nOrigin: https://kotu.example\r\nContent-Length: 0\r\n\r\n";
        assert!(http_oku(tarayici.as_bytes()).unwrap().origin);
        assert!(http_oku("POST /mcp HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n".as_bytes()).is_err());
        assert!(http_oku("POST /mcp HTTP/1.1\r\nContent-Length: 10\r\n\r\nkisa".as_bytes()).is_err());
        assert!(http_oku("".as_bytes()).is_err());
    }

    #[test]
    fn yapilandirma_jetonu_basliga_koyar() {
        let b = Baglanti { url: "http://127.0.0.1:5/mcp".into(), jeton: "gizli".into(), seviye: Erisim::Dusuk };
        let v: Value = serde_json::from_str(&b.yapilandirma()).unwrap();
        assert_eq!(v["mcpServers"]["zodyvel"]["url"], "http://127.0.0.1:5/mcp");
        assert_eq!(v["mcpServers"]["zodyvel"]["headers"]["Authorization"], "Bearer gizli");
        assert_eq!(v["mcpServers"]["zodyvel"]["type"], "http");
    }

    #[test]
    fn jetonlar_rastgele_ve_uzun() {
        let (a, b) = (rastgele_jeton(), rastgele_jeton());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn kart_bekleyen_eylem_yapildi_denmez() {
        let p = spec::Pending { id: 1, action: "app.close", risk: spec::Risk::Confirm, summary: "Uygulamayı kapat".into(), source: "zody".into(), istek: String::new(), created_ms: 0, args: Default::default() };
        let m = sonuc_metni(Ok(Outcome::Pending { request: p })).unwrap();
        assert!(m.contains("Henüz yapılmadı") && m.contains("Uygulamayı kapat"));
        assert!(sonuc_metni(Ok(Outcome::Denied)).is_err());
        assert_eq!(sonuc_metni(Ok(Outcome::Done { result: Value::Null })).unwrap(), "Tamam.");
        assert!(sonuc_metni(Ok(Outcome::Done { result: json!("x".repeat(9000)) })).unwrap().chars().count() <= SONUC_EN_UZUN + 1);
    }
}
