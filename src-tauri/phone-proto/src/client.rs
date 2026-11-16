//! Telefon (istemci) tarafı: eşleştirme ve normal bağlantı (T4-a). Android uygulaması ve testler kullanır.
//! Masaüstü sunucusuyla (`server.rs`) aynı iletileri konuşur; anahtar ve içerik günlüğe yazılmaz.
use crate::devices::signing_bytes;
use crate::pairing::{hex, hex_decode, QrPayload};
use crate::proto::MAX_INCOMING;
use crate::{MODE_NORMAL, MODE_PAIR};
use crate::session::{connect, Channel, Keypair, Trust};
use serde_json::{json, Value};
use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
/// Masaüstü onayı + telefon onayı için en çok bekleme (sunucu tarafıyla uyumlu).
const PAIR_WAIT: Duration = Duration::from_secs(125);

/// Eylem imzalayıcı: Android'de Keystore anahtarı (BiometricPrompt ile), testte bellekteki anahtar.
/// Çıktı DER kodlu ECDSA-SHA256 imzasıdır.
pub trait Signer {
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, String>;
}

fn dial(addr: &str, mode: u8, io_timeout: Duration) -> Result<TcpStream, String> {
    let sa: SocketAddr = addr.trim().to_socket_addrs().map_err(|_| "Adres geçersiz.".to_string())?.next().ok_or("Adres geçersiz.")?;
    let mut s = TcpStream::connect_timeout(&sa, CONNECT_TIMEOUT).map_err(|_| "Bilgisayara ulaşılamadı.".to_string())?;
    let _ = s.set_nodelay(true);
    s.set_read_timeout(Some(io_timeout)).map_err(|_| "Bağlantı kurulamadı.")?;
    s.set_write_timeout(Some(Duration::from_secs(5))).map_err(|_| "Bağlantı kurulamadı.")?;
    s.write_all(&[mode]).map_err(|_| "Bilgisayara ulaşılamadı.")?;
    Ok(s)
}

fn send_json(ch: &mut Channel<TcpStream>, v: Value) -> Result<(), String> {
    ch.send(v.to_string().as_bytes()).map_err(|_| "Bağlantı koptu.".to_string())
}
fn recv_json(ch: &mut Channel<TcpStream>) -> Result<Value, String> {
    let b = ch.recv().map_err(|_| "Bağlantı koptu.".to_string())?;
    if b.len() > MAX_INCOMING * 8 {
        return Err("İleti çok büyük.".into());
    }
    serde_json::from_slice(&b).map_err(|_| "İleti okunamadı.".to_string())
}

/// Eşleştirmenin ilk yarısı bitti: ekranda gösterilecek 6 haneli kod hazır.
pub struct PairingSession {
    ch: Channel<TcpStream>,
    pub confirm: String,
    /// QR'daki parmak izine (masaüstü Noise anahtarı) sabitlenen anahtar; eşleşince saklanır.
    pub desktop_pub: Vec<u8>,
    pub addr: String,
}

pub struct Paired {
    pub id: String,
    pub desktop_pub: Vec<u8>,
    pub addr: String,
    /// O9: masaüstü kanıt zincirini doğruladı mı (eski masaüstü söylemezse `None`).
    pub hw: Option<bool>,
    /// "strongbox" | "tee" | "".
    pub level: String,
}

/// QR'ı okuyup el sıkışır ve "hello" yollar. `att`: Keystore anahtar kanıtı zinciri (hex DER sertifikalar; boş olabilir).
/// Anahtar zaten varsa; kanıt bu oturuma bağlanmadığı için masaüstünde "korumasız" sayılır. Uygulama `start_pairing_with` kullanır.
pub fn start_pairing(qr: &QrPayload, me: &Keypair, name: &str, apub_hex: &str, att: &[String]) -> Result<PairingSession, String> {
    start_pairing_with(qr, me, name, |_| Ok((apub_hex.to_owned(), att.to_vec())))
}

/// O9: önce el sıkışır, sonra imza anahtarını `make_key(el_sıkışma_özeti)` ile üretir. Özet Key Attestation
/// sınaması (challenge) olur: masaüstü zincirin bu eşleştirmeye ait olduğunu böyle doğrular.
/// `make_key` → (apub hex, kanıt zinciri hex DER, yaprak önce).
pub fn start_pairing_with(qr: &QrPayload, me: &Keypair, name: &str, make_key: impl FnOnce(&[u8]) -> Result<(String, Vec<String>), String>) -> Result<PairingSession, String> {
    let fp = hex_decode(&qr.fp).ok_or("QR geçersiz.")?;
    let stream = dial(&qr.addr, MODE_PAIR, PAIR_WAIT)?;
    let mut ch = connect(stream, me, &qr.token_bytes(), Trust::PinnedKey(&fp)).map_err(|_| "Eşleştirme başarısız (QR süresi dolmuş ya da bu bilgisayar değil).".to_string())?;
    let (apub_hex, att) = make_key(&ch.handshake_hash)?;
    let mut hello = json!({"t": "hello", "name": name, "apub": apub_hex});
    if !att.is_empty() {
        hello["att"] = json!(att);
    }
    send_json(&mut ch, hello)?;
    Ok(PairingSession { confirm: ch.confirm.clone(), ch, desktop_pub: fp, addr: qr.addr.clone() })
}

impl PairingSession {
    /// Kullanıcı kodu masaüstüne yazdıktan sonra: `same=true` → eşleşmeyi tamamla, `false` → vazgeç.
    pub fn finish(mut self, same: bool) -> Result<Paired, String> {
        send_json(&mut self.ch, json!({"t": "pc", "ok": same}))?;
        if !same {
            return Err("Eşleştirmeden vazgeçildi.".into());
        }
        let m = recv_json(&mut self.ch)?;
        match m.get("t").and_then(Value::as_str) {
            Some("paired") => Ok(Paired {
                id: m.get("id").and_then(Value::as_str).unwrap_or_default().to_owned(),
                desktop_pub: self.desktop_pub,
                addr: self.addr,
                hw: m.get("hw").and_then(Value::as_bool),
                level: m.get("level").and_then(Value::as_str).unwrap_or_default().to_owned(),
            }),
            Some("bye") => Err(match m.get("why").and_then(Value::as_str) {
                Some("declined") => "Bilgisayar eşleştirmeyi reddetti ya da kod yanlış.",
                Some("full") => "Bilgisayarda eşli cihaz sınırı dolu.",
                _ => "Eşleştirme tamamlanamadı.",
            }
            .into()),
            _ => Err("Eşleştirme tamamlanamadı.".into()),
        }
    }
}

/// Normal (eşli) bağlantı; masaüstü önce `{"t":"device","hw":..}` (O9), sonra durumu yollar.
pub struct Link {
    ch: Channel<TcpStream>,
    ctl: TcpStream,
    n: u64,
}

pub fn connect_normal(addr: &str, me: &Keypair, desktop_pub: &[u8]) -> Result<Link, String> {
    let stream = dial(addr, MODE_NORMAL, Duration::from_secs(5))?;
    let ctl = stream.try_clone().map_err(|_| "Bağlantı kurulamadı.".to_string())?;
    let ch = connect(stream, me, b"", Trust::PinnedKey(desktop_pub)).map_err(|_| "Bu bilgisayarla eşli değilsin ya da bağlantı reddedildi.".to_string())?;
    Ok(Link { ch, ctl, n: 0 })
}

impl Link {
    /// En çok `wait` bekler; ileti yoksa `Ok(None)`. Yarım çerçevede zaman aşımı olmaz (peek ile beklenir).
    pub fn recv(&mut self, wait: Duration) -> Result<Option<Value>, String> {
        let _ = self.ctl.set_read_timeout(Some(wait.max(Duration::from_millis(1))));
        let mut one = [0u8; 1];
        match self.ctl.peek(&mut one) {
            Ok(0) => return Err("Bağlantı koptu.".into()),
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
            Err(_) => return Err("Bağlantı koptu.".into()),
        }
        let _ = self.ctl.set_read_timeout(Some(Duration::from_secs(5)));
        recv_json(&mut self.ch).map(Some)
    }

    pub fn ping(&mut self) -> Result<(), String> {
        send_json(&mut self.ch, json!({"t": "ping"}))
    }
    pub fn request(&mut self, what: &str) -> Result<(), String> {
        // "agenda" | "folders"
        send_json(&mut self.ch, json!({"t": what}))
    }

    /// Eylemi imzalayıp yollar; dönen sayı `ack` ile eşleşir. İmza için kullanıcı kimlik doğrulaması `signer` içinde olur.
    pub fn act(&mut self, action: &Value, signer: &dyn Signer) -> Result<u64, String> {
        self.n += 1;
        let a = action.to_string();
        let sig = signer.sign(&signing_bytes(&self.ch.handshake_hash, self.n, &a))?;
        send_json(&mut self.ch, json!({"t": "act", "n": self.n, "a": a, "sig": hex(&sig)}))?;
        Ok(self.n)
    }
}
