//! Hakkında → "Sorun bildir": sürüm, Windows ve ekran bilgisi ile ayıklanmış debug.log sonu. Panoya kopyalamayı
//! arayüz yapar; burada yalnız veri toplanır. Kişisel veri `redact` ile ayıklanır (yollar, kullanıcı ve bilgisayar adı).

use std::fmt::Write;

use serde::Serialize;
use tauri::AppHandle;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::{PCWSTR, w};

use crate::{debuglog, redact};

/// Rapora giren debug.log satırı sayısı.
const LOG_LINES: usize = 200;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub version: String,
    pub windows: String,
    /// Bağlı ekranlar, birincil başta: "1920×1080 · %125 (birincil)".
    pub screens: Vec<String>,
    /// Ayrıntılı kayıt açık mı (açıksa dosya henüz yazılmamış olabilir).
    pub debug_on: bool,
    /// debug.log'un ayıklanmış son satırları; dosya yoksa None.
    pub log: Option<String>,
}

fn reg_string(name: PCWSTR) -> Option<String> {
    const KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(HKEY_LOCAL_MACHINE, KEY, name, RRF_RT_REG_SZ, None, None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        let status = RegGetValueW(HKEY_LOCAL_MACHINE, KEY, name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size));
        (status == ERROR_SUCCESS).then(|| {
            let wide: Vec<u16> = buf.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
            String::from_utf16_lossy(&wide)
        })
    }
}

fn reg_dword(name: PCWSTR) -> Option<u32> {
    const KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        let status = RegGetValueW(HKEY_LOCAL_MACHINE, KEY, name, RRF_RT_REG_DWORD, None, Some((&raw mut value).cast()), Some(&mut size));
        (status == ERROR_SUCCESS).then_some(value)
    }
}

/// "Windows 11 Home 24H2 (derleme 26100.2894)"; kayıt defteri okunamazsa "bilinmiyor".
fn windows_version() -> String {
    let build: u32 = reg_string(w!("CurrentBuildNumber")).and_then(|b| b.parse().ok()).unwrap_or(0);
    let mut name = reg_string(w!("ProductName")).unwrap_or_else(|| "Windows".into());
    // Windows 11 da kayıtta "Windows 10" yazar; ayırt eden derleme numarasıdır.
    if build >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    let mut out = name;
    if let Some(display) = reg_string(w!("DisplayVersion")) {
        let _ = write!(out, " {display}");
    }
    if build > 0 {
        let _ = write!(out, " (derleme {build}");
        if let Some(ubr) = reg_dword(w!("UBR")) {
            let _ = write!(out, ".{ubr}");
        }
        out.push(')');
    }
    out
}

fn screens(app: &AppHandle) -> Vec<String> {
    let primary = app.primary_monitor().ok().flatten().map(|m| *m.position());
    let mut list: Vec<(bool, String)> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let is_primary = primary.is_some_and(|p| p == *m.position());
            let size = m.size();
            let text = format!("{}×{} · %{}{}", size.width, size.height, (m.scale_factor() * 100.0).round(), if is_primary { " (birincil)" } else { "" });
            (is_primary, text)
        })
        .collect();
    list.sort_by_key(|(is_primary, _)| !is_primary);
    list.into_iter().map(|(_, text)| text).collect()
}

#[tauri::command]
pub fn report_info(app: AppHandle) -> Report {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let machine = std::env::var("COMPUTERNAME").unwrap_or_default();
    let log = debuglog::tail(LOG_LINES).map(|text| redact::redact(&text, &[&user, &machine]));
    Report {
        version: app.package_info().version.to_string(),
        windows: windows_version(),
        screens: screens(&app),
        debug_on: debuglog::enabled(),
        log,
    }
}

/// Ayrıntılı kaydı aç/kapat. Yeniden başlatma gerekmez; kayıt bir sonraki olaydan itibaren yazılır.
#[tauri::command]
pub fn report_debug(on: bool) -> Result<(), String> {
    debuglog::set_enabled(on).map_err(|e| e.to_string())
}

/// Arayüzden ölçüm damgası (Faz 7.2: "Bitti" çentikte çizildi). Yalnız kayıt açıkken ve yalnız kısa,
/// içerik taşımayan etiketler (harf, rakam, `_:-`) yazılır.
#[tauri::command]
pub fn report_mark(label: String, at: Option<u64>) {
    if !debuglog::enabled() || label.len() > 48 || !label.chars().all(|c| c.is_ascii_alphanumeric() || "_:-".contains(c)) {
        return;
    }
    let lag = at.map(|t| crate::agents::now_ms().saturating_sub(t));
    debuglog::line(format!("olcum: arayuz {label}{}", lag.map_or(String::new(), |l| format!(" (yayından {l} ms sonra)"))));
}
