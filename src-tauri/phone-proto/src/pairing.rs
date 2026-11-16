//! QR yükü, tek kullanımlık jeton ve 6 haneli doğrulama kodu.
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

pub const QR_TTL: Duration = Duration::from_secs(120);

/// QR'ın içeriği. Jeton gizlidir; günlüğe/ekrana basılmaz.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QrPayload {
    pub v: u8,
    /// "ip:port"
    pub addr: String,
    /// Tek kullanımlık jeton (hex, 32 bayt).
    pub token: String,
    /// Bilgisayarın Noise açık anahtarı parmak izi (hex).
    pub fp: String,
}

impl fmt::Debug for QrPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QrPayload").field("addr", &self.addr).finish_non_exhaustive()
    }
}

impl QrPayload {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("serde")
    }
    pub fn from_json(s: &str) -> Result<Self, String> {
        let p: Self = serde_json::from_str(s).map_err(|_| "QR okunamadı".to_string())?;
        if p.v != 1 || hex_decode(&p.token).map(|t| t.len()) != Some(32) {
            return Err("QR sürümü/jetonu geçersiz".into());
        }
        Ok(p)
    }
    pub fn token_bytes(&self) -> Vec<u8> {
        hex_decode(&self.token).unwrap_or_default()
    }
}

/// Masaüstü tarafında açık eşleştirme penceresi: tek jeton, 2 dk, tek kullanım.
pub struct PairingWindow {
    token: [u8; 32],
    opened: Instant,
    used: bool,
}

impl PairingWindow {
    pub fn open() -> Self {
        let mut token = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut token);
        Self { token, opened: Instant::now(), used: false }
    }
    pub fn token(&self) -> &[u8; 32] {
        &self.token
    }
    pub fn used(&self) -> bool {
        self.used
    }
    pub fn expired(&self) -> bool {
        self.opened.elapsed() > QR_TTL
    }
    /// Jetonu tüketmeden verir (el sıkışma için); yanmış/süresi dolmuşsa None.
    /// Pencere yalnız el sıkışma BAŞARILI olunca `take()` ile yanar (jetonsuz bağlantı QR'ı yakamaz).
    pub fn peek(&self) -> Option<[u8; 32]> {
        (!self.used && !self.expired()).then_some(self.token)
    }
    /// Pencereyi tüketir; ikinci kez ya da süre dolunca None.
    pub fn take(&mut self) -> Option<[u8; 32]> {
        if self.used || self.expired() {
            return None;
        }
        self.used = true;
        Some(self.token)
    }
    pub fn qr(&self, addr: &str, desktop_pub: &[u8]) -> QrPayload {
        QrPayload { v: 1, addr: addr.into(), token: hex(&self.token), fp: fingerprint(desktop_pub) }
    }
}

/// Açık anahtar parmak izi. Prototipte anahtarın tamamı (hex, 64 karakter): kısalmaz, çakışmaz.
/// Üretimde `blake2` ile kısaltılıp telefonda 6 haneli kodla birlikte gösterilebilir.
pub fn fingerprint(pubkey: &[u8]) -> String {
    hex(pubkey)
}

/// El sıkışma özetinden iki tarafta aynı çıkan 6 haneli kod.
pub fn confirm_code(handshake_hash: &[u8]) -> String {
    let mut n: u64 = 0;
    for b in handshake_hash.iter().take(8) {
        n = (n << 8) | *b as u64;
    }
    format!("{:06}", n % 1_000_000)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}
