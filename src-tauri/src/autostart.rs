//! Windows ile başlat: kullanıcının `Run` kaydına tek bir değer. Ek süreç, görev zamanlayıcı veya
//! yönetici izni gerekmez. Görev Yöneticisi → Başlangıç uygulamaları'ndaki kapatma da okunur
//! (`StartupApproved`), böylece iki yerdeki anahtar birbiriyle çelişmez.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_ROUTINE_FLAGS, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

const RUN: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const APPROVED: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
const NAME: PCWSTR = w!("Zodyvel");

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Autostart {
    /// Oturum açılınca bir Zodyvel başlayacak mı.
    pub enabled: bool,
    /// Kayıt var ama Görev Yöneticisi'nden kapatılmış.
    pub disabled_by_system: bool,
    /// Kayıt başka bir konumdaki Zodyvel'i gösteriyorsa onun yolu.
    pub other_path: Option<String>,
    /// Geliştirme derlemesi tek başına çalışmaz (arayüzü geliştirme sunucusundan alır); orada kapalı.
    pub available: bool,
}

fn read(sub: PCWSTR, flags: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(HKEY_CURRENT_USER, sub, NAME, flags, None, None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let status = RegGetValueW(HKEY_CURRENT_USER, sub, NAME, flags, None, Some(buf.as_mut_ptr().cast()), Some(&mut size));
        (status == ERROR_SUCCESS).then(|| {
            buf.truncate(size as usize);
            buf
        })
    }
}

fn read_string(sub: PCWSTR) -> Option<String> {
    let bytes = read(sub, RRF_RT_REG_SZ)?;
    let wide: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
    Some(String::from_utf16_lossy(&wide))
}

/// Komut satırının ilk parçası (tırnaklı veya tırnaksız yol).
fn command_path(command: &str) -> &str {
    let command = command.trim();
    match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(rest),
        None => command.split(' ').next().unwrap_or(command),
    }
}

fn same_path(a: &str, b: &Path) -> bool {
    a.eq_ignore_ascii_case(&b.to_string_lossy())
}

/// `StartupApproved` değerinin ilk baytı tekse kullanıcı başlangıçta çalışmayı kapatmış demektir.
fn disabled_flag(bytes: &[u8]) -> bool {
    bytes.first().is_some_and(|b| b & 1 == 1)
}

fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

pub fn status() -> Autostart {
    let available = !cfg!(debug_assertions);
    let Some(command) = read_string(RUN) else {
        return Autostart { enabled: false, disabled_by_system: false, other_path: None, available };
    };
    let disabled_by_system = read(APPROVED, RRF_RT_REG_BINARY).is_some_and(|b| disabled_flag(&b));
    let path = command_path(&command);
    let here = current_exe().is_some_and(|exe| same_path(path, &exe));
    Autostart {
        enabled: !disabled_by_system,
        disabled_by_system,
        other_path: (!here).then(|| path.to_string()),
        available,
    }
}

fn delete(sub: PCWSTR) {
    let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, sub, NAME) };
}

fn set(enabled: bool) -> Result<(), String> {
    if !enabled {
        delete(RUN);
        delete(APPROVED);
        return Ok(());
    }
    if cfg!(debug_assertions) {
        return Err("Geliştirme sürümünde kullanılamaz; kurulu Zodyvel'den aç.".into());
    }
    register(&current_exe().ok_or("Uygulamanın konumu okunamadı.")?)
}

fn register(exe: &Path) -> Result<(), String> {
    let command = HSTRING::from(format!("\"{}\"", exe.display()));
    let bytes = (command.len() + 1) * 2;
    let status = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(command.as_ptr().cast()), bytes as u32)
    };
    if status != ERROR_SUCCESS {
        return Err(format!("Başlangıç kaydı yazılamadı (hata {}).", status.0));
    }
    // Görev Yöneticisi'nden önceden kapatılmışsa o işaret de kalkar; yoksa kayıt yok sayılırdı.
    delete(APPROVED);
    Ok(())
}

/// Ayarlar penceresi ve tepsi menüsü aynı yoldan geçer; ikisi de yeni durumu alır.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<Autostart, String> {
    let result = set(enabled);
    let now = status();
    crate::tray::sync_autostart(app, &now);
    let _ = app.emit("autostart://changed", &now);
    result.map(|_| now)
}

#[tauri::command]
pub fn autostart_get() -> Autostart {
    status()
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<Autostart, String> {
    apply(&app, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_and_plain_paths() {
        assert_eq!(command_path(r#""C:\Program Files\Zodyvel\zodyvel.exe" --x"#), r"C:\Program Files\Zodyvel\zodyvel.exe");
        assert_eq!(command_path(r"C:\Apps\zodyvel.exe --x"), r"C:\Apps\zodyvel.exe");
        assert!(same_path(r"c:\apps\ZODYVEL.exe", Path::new(r"C:\Apps\zodyvel.exe")));
    }

    /// Gerçek kayıt defteri: `cargo test -- --ignored autostart`. Kendi yazdığını hemen siler;
    /// kullanıcının mevcut bir kaydı varsa hiçbir şey yapmaz.
    #[test]
    #[ignore]
    fn writes_reads_and_removes_real_run_value() {
        if read_string(RUN).is_some() {
            return;
        }
        let exe = current_exe().unwrap();
        register(&exe).expect("yazılabilmeli");
        let on = status();
        let written = read_string(RUN);
        set(false).expect("silinebilmeli");
        assert!(on.enabled && on.other_path.is_none(), "{on:?}");
        assert_eq!(written, Some(format!("\"{}\"", exe.display())));
        assert!(!status().enabled);
        assert_eq!(read_string(RUN), None);
    }

    #[test]
    fn task_manager_disable_flag() {
        assert!(!disabled_flag(&[2, 0, 0, 0]));
        assert!(!disabled_flag(&[6, 0]));
        assert!(disabled_flag(&[3, 0, 0, 0]));
        assert!(disabled_flag(&[7]));
        assert!(!disabled_flag(&[]));
    }
}
