//! Eşli cihaz kaydı ve eylem imzası. Kayıt tek bir gizli blob'dur (masaüstünde Windows Kimlik Bilgisi
//! Yöneticisi, testte bellek); anahtarlar günlüğe/hata metnine girmez.
use crate::pairing::hex_decode;
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const MAX_DEVICES: usize = 5;
/// 30 gün hiç bağlanmayan cihaz yeniden eşleştirme ister.
pub const REPAIR_AFTER_SECS: u64 = 30 * 24 * 3600;

#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Device {
    pub id: String,
    pub name: String,
    /// Noise kalıcı açık anahtarı (hex).
    pub pubkey: String,
    /// Eylem imza anahtarı: ECDSA P-256, SEC1 (hex). Telefonda Keystore'da, biyometriye bağlı.
    pub apub: String,
    pub paired_at: u64,
    pub last_seen: u64,
    /// O9: Key Attestation doğrulandı mı (donanım anahtarı + kullanıcı doğrulaması). O9'dan önce eşlenmiş
    /// kayıtlarda alan yoktur → `false` (korumasız; yeniden eşleşince doğrulanır).
    #[serde(default)]
    pub hw: bool,
    /// "strongbox" | "tee" | "" (korumasız).
    #[serde(default)]
    pub hw_level: String,
    /// Korumasızsa kısa neden (içerik/anahtar yok). G4: bilinen nedenler kayıtta kısa kodla ("#3") tutulur
    /// (Kimlik Bilgisi Yöneticisi 2560 bayt sınırı); eski kayıtlardaki düz metin aynen okunur.
    #[serde(default, serialize_with = "why_ser", deserialize_with = "why_de")]
    pub hw_why: String,
    /// G4: kanıt zincirindeki ara sertifikaların seri özetleri (Google iptal listesine karşı yeniden denetlenir).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rv: Vec<String>,
    /// G4: eşleşmede iptal listesi yoktu; zincir geçerliydi, karar liste gelince verilir.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rp: bool,
}

/// Korumasızlık nedenleri (kayıtta sıra numarasıyla saklanır). SIRA DEĞİŞTİRİLMEZ, yalnız sona eklenir.
pub const WHY_TEXTS: &[&str] = &[
    "Kanıt zinciri yok (telefon desteklemiyor ya da emülatör).",
    "Kanıt zinciri uzunluğu geçersiz.",
    "Kanıt zinciri okunamadı.",
    "Kanıt sertifikası okunamadı.",
    "Kanıt zincirinde imza doğrulanamadı.",
    "Kanıt zinciri Google köküne bağlanmıyor.",
    "İmza anahtarı geçersiz.",
    "Kanıtlanan anahtar beklenen türde değil.",
    "Kanıtlanan anahtar telefonun anahtarıyla eşleşmiyor.",
    "Kanıt uzantısı yok.",
    "Kanıt uzantısı okunamadı.",
    "Kanıt bu eşleştirmeye ait değil.",
    "Anahtar donanımda değil (yazılım Keystore).",
    "Anahtar kullanıcı doğrulaması (parmak izi/PIN) istemiyor.",
    "Önyükleme bilgisi yok.",
    "Telefonun önyüklemesi doğrulanmamış (kilidi açık ya da değiştirilmiş sistem).",
    "Kanıt sertifikası Google tarafından iptal edilmiş.",
    "İptal listesi alınamadı; internet olunca denetlenir.",
    // Yeni metinler yalnız sona eklenir (kayıttaki #sıra kodları değişmesin).
    "Kanıt zincirinde yetkisiz ara sertifika var.",
    "Anahtar telefonda üretilmemiş (dışarıdan aktarılmış).",
    "Kanıt bu uygulamaya ait değil.",
];
/// Bilinmeyen neden metni kayıtta en çok bu kadar karakter tutar.
pub const WHY_MAX: usize = 60;

fn why_ser<S: serde::Serializer>(why: &str, s: S) -> Result<S::Ok, S::Error> {
    match WHY_TEXTS.iter().position(|t| *t == why) {
        Some(i) => s.serialize_str(&format!("#{i}")),
        None => s.serialize_str(&why.chars().take(WHY_MAX).collect::<String>()),
    }
}

fn why_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let raw = String::deserialize(d)?;
    Ok(match raw.strip_prefix('#').and_then(|n| n.parse::<usize>().ok()).and_then(|i| WHY_TEXTS.get(i)) {
        Some(t) => (*t).to_owned(),
        None => raw,
    })
}

/// Korumasız cihazın yapamadığı eylemler için masaüstünün cevabı.
pub const UNPROTECTED_DENIED: &str = "Bu telefon donanım korumalı değil; bu eylem kapalı.";
impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device").field("id", &self.id).finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct VaultData {
    pub private: String,
    pub public: String,
    pub devices: Vec<Device>,
}
impl std::fmt::Debug for VaultData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultData(..)")
    }
}

/// Gizli blob deposu. Windows'ta Kimlik Bilgisi Yöneticisi; testte bellek.
pub trait Vault: Send + Sync {
    fn load(&self) -> Option<String>;
    /// "Kayıt yok" (`Ok(None)`) ile "okunamadı" (`Err`) ayrılır; okunamazsa anahtar yenilenmez.
    fn load_checked(&self) -> Result<Option<String>, String> {
        Ok(self.load())
    }
    fn store(&self, blob: &str) -> Result<(), String>;
    /// Kaydı siler; başarısızlık yüzeye çıkar (iptal sessizce başarısız olmasın).
    fn wipe(&self) -> Result<(), String>;
}

/// Cihaz adı (telefonun beyanı, doğrulanmamış): yalnız harf/rakam/boşluk/temel noktalama, en çok 24.
pub fn clean_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_alphanumeric() || *c == ' ' || ".-_'()".contains(*c)).take(24).collect();
    let n = n.trim().to_owned();
    if n.is_empty() { "Telefon".into() } else { n }
}

/// Eylemin imzalanan baytları: alan ayırıcı + el sıkışma özeti (oturuma bağlar) + sayaç + eylem metni.
pub fn signing_bytes(handshake_hash: &[u8], n: u64, action: &str) -> Vec<u8> {
    let mut v = b"zodyvel-act-v1".to_vec();
    v.extend_from_slice(handshake_hash);
    v.extend_from_slice(&n.to_be_bytes());
    v.extend_from_slice(action.as_bytes());
    v
}

pub fn valid_apub(apub_hex: &str) -> bool {
    hex_decode(apub_hex).and_then(|b| VerifyingKey::from_sec1_bytes(&b).ok()).is_some()
}

/// `sig`: DER kodlu ECDSA-SHA256 (Android `SHA256withECDSA` çıktısı).
pub fn verify_action(apub_hex: &str, handshake_hash: &[u8], n: u64, action: &str, sig_hex: &str) -> bool {
    let (Some(k), Some(s)) = (hex_decode(apub_hex), hex_decode(sig_hex)) else { return false };
    let (Ok(key), Ok(sig)) = (VerifyingKey::from_sec1_bytes(&k), Signature::from_der(&s)) else { return false };
    key.verify(&signing_bytes(handshake_hash, n, action), &sig).is_ok()
}

/// Kaydı siler ve DOĞRULAR: silme hata verirse ya da kayıt hâlâ okunuyorsa boş kayıtla üstüne yazar
/// (anahtar + cihaz listesi gider). İkisi de olmazsa hata döner; çağıran güvenli tarafta kalmalıdır.
pub fn wipe_verified(v: &dyn Vault) -> Result<(), String> {
    let first = v.wipe();
    if matches!(v.load_checked(), Ok(None)) {
        return Ok(());
    }
    let empty = serde_json::to_string(&VaultData::default()).map_err(|_| "Kayıt hatası.".to_string())?;
    v.store(&empty).map_err(|e| format!("Telefon kaydı silinemedi ({}; {e}). Windows Kimlik Bilgisi Yöneticisi'nden 'Zodyvel:phone-vault' kaydını elle sil.", first.err().unwrap_or_else(|| "hâlâ okunuyor".into())))?;
    match v.load_checked() {
        Ok(None) => Ok(()),
        Ok(Some(b)) if serde_json::from_str::<VaultData>(&b).is_ok_and(|d| d.devices.is_empty() && d.private.is_empty()) => Ok(()),
        _ => Err("Telefon kaydı doğrulanamadı; Kimlik Bilgisi Yöneticisi'nden 'Zodyvel:phone-vault' kaydını elle sil.".into()),
    }
}
