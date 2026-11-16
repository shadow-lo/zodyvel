//! Gizli bilgiler (Gmail uygulama şifresi, GitHub tokenı) Windows Kimlik Bilgisi Yöneticisi'nde durur:
//! kullanıcının Windows hesabına bağlı, Windows'un kendi şifrelemesiyle (DPAPI). Ayar dosyasına, kayda
//! ya da arayüze asla yazılmaz; arayüz yalnız "var/yok" görür. Yalnız `sources::model`'deki anahtarlar
//! (`FIELDS`, `MAIL_KEYS`) kullanılır.
//!
//! Kayıtlar Denetim Masası → Kimlik Bilgisi Yöneticisi → Windows Kimlik Bilgileri'nde "Zodyvel:<anahtar>"
//! adıyla görünür; kullanıcı oradan da silebilir.

use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
};
use windows::core::{HSTRING, PWSTR};

const SERVICE: &str = "Zodyvel";

/// Telefon köprüsünün tek gizli kaydı (Noise anahtarı + eşli cihazlar); yalnız `phone` modülü kullanır.
pub const PHONE_VAULT: &str = "phone-vault";

fn target(key: &str) -> Result<HSTRING, String> {
    if key != PHONE_VAULT && !crate::sources::model::known_key(key) {
        return Err("İzinsiz anahtar.".into());
    }
    Ok(HSTRING::from(format!("{SERVICE}:{key}")))
}

/// Yazar (varsa üstüne). Hata metninde değer geçmez.
pub fn write(key: &str, value: &str) -> Result<(), String> {
    let target = target(key)?;
    let user = HSTRING::from(SERVICE);
    let mut blob = value.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_ptr() as *mut _),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_ptr() as *mut _),
        ..Default::default()
    };
    let result = unsafe { CredWriteW(&credential, 0) };
    blob.fill(0);
    result.map_err(|e| format!("Bilgi Windows'a kaydedilemedi ({:#x}).", e.code().0))
}

/// Okur; yoksa `None`. Okunamayan kayıt da `None` sayılır (telefon kaydı için `read_checked`).
pub fn read(key: &str) -> Option<String> {
    read_checked(key).ok().flatten()
}

/// "Kayıt yok" (`Ok(None)`) ile "okunamadı" (`Err`) ayrılır.
pub fn read_checked(key: &str) -> Result<Option<String>, String> {
    let target = target(key)?;
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    match unsafe { CredReadW(&target, CRED_TYPE_GENERIC, None, &mut credential) } {
        Ok(()) => {}
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
        Err(e) => return Err(format!("Bilgi Windows'tan okunamadı ({:#x}).", e.code().0)),
    }
    let value = unsafe {
        let c = &*credential;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        let value = String::from_utf8(bytes.to_vec()).ok();
        CredFree(credential as *const _);
        value
    };
    match value {
        Some(v) if v.is_empty() => Ok(None),
        Some(v) => Ok(Some(v)),
        None => Err("Kayıt okunamadı (bozuk).".into()),
    }
}

pub fn exists(key: &str) -> bool {
    read(key).is_some()
}

/// Siler; zaten yoksa da başarılı.
pub fn delete(key: &str) -> Result<(), String> {
    let target = target(key)?;
    match unsafe { CredDeleteW(&target, CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
        Err(e) => Err(format!("Bilgi Windows'tan silinemedi ({:#x}).", e.code().0)),
    }
}
