//! Zodyvel telefon köprüsü · T1 mimari denemesi (Plan 3, bölüm 2.2 ve 2.5).
//!
//! Sunucu yok: telefon ile masaüstü aynı yerel ağda doğrudan konuşur.
//! - Kanal: Noise_XX_25519_ChaChaPoly_BLAKE2s (`snow`), her taşıma mesajında
//!   Noise'un kendi sayacı var; tekrar oynatılan / sırası bozulan mesaj reddedilir.
//! - Eşleştirme: QR tek kullanımlık jeton taşır; jeton Noise "prologue"una girer,
//!   yani jetonu bilmeyen biri el sıkışmayı bitiremez.
//! - 6 haneli doğrulama kodu el sıkışma özetinden (handshake hash) türetilir;
//!   iki ekranda aynıysa araya girilmemiştir.
//! - Hiçbir yere anahtar, jeton ya da içerik yazılmaz (bkz. `Debug` uygulamaları).

#[cfg(feature = "bridge")]
pub mod attest;
pub mod client;
pub mod devices;
#[cfg(feature = "bridge")]
pub mod discovery;
#[cfg(feature = "bridge")]
pub mod net;
pub mod pairing;
pub mod proto;
#[cfg(feature = "bridge")]
pub mod server;
pub mod session;

pub const NOISE_PARAMS: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
/// Tek çerçevenin (şifreli) en büyük boyutu; Noise sınırı 65535.
pub const MAX_FRAME: usize = 65535;
/// Bağlantının ilk baytı: eşleştirme ya da normal bağlantı.
pub const MODE_PAIR: u8 = 1;
pub const MODE_NORMAL: u8 = 2;
