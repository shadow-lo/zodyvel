//! Sekme çubuğunun sağındaki uygulama kısayolları (Faz 2b).
//!
//! Kullanıcı en çok `MAX_APPS` kısayol ekler: bir .exe/.lnk dosyası (adadaki "+" ile dosya seçici) ya da
//! http(s) adresi (Ayarlar › Ada). Liste ayarlarda (`Settings::apps`) durur; tıklayınca hedef kabuk
//! üzerinden açılır. Yalnız ayarlarda kayıtlı hedefler açılır: arayüz ya da Zody keyfî bir yol
//! başlatamaz, yalnız kısayolun kimliğini verir.
//!
//! Simge: dosyanın kendi simgesi bir kez çıkarılıp PNG'ye çevrilir, ayar klasöründe `icons/` altında
//! önbelleğe yazılır (dosya değişince yenisi). Adres kısayolunda ağdan favicon çekilmez; arayüz
//! harfli bir kutucuk çizer. Boşta hiçbir iş yapılmaz: simge yalnız ada ilk kez isteyince okunur.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::settings;

/// Aynı anda en çok bu kadar kısayol (src/lib/types.ts MAX_APPS ile aynı).
pub const MAX_APPS: usize = 4;
/// Simgenin piksel boyu: çubukta 22–26 px, yüksek DPI'da keskin kalsın diye 64.
const ICON_PX: i32 = 64;
const NAME_MAX: usize = 40;
const TARGET_MAX: usize = 1024;
/// Kabuğun açabildiği dosya türleri.
const EXTENSIONS: [&str; 4] = ["exe", "lnk", "url", "appref-ms"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppShortcut {
    pub id: String,
    pub name: String,
    /// .exe/.lnk yolu ya da http(s) adresi.
    pub target: String,
}

pub fn is_url(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.starts_with('/')) && !target.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Mutlak bir Windows yolu (C:\… ya da \\sunucu\…) ve açılabilir bir uzantı.
pub fn is_program(target: &str) -> bool {
    let b = target.as_bytes();
    let absolute = (b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')) || target.starts_with("\\\\");
    let ext = target.rsplit(['\\', '/']).next().and_then(|name| name.rsplit_once('.')).map(|(_, e)| e);
    absolute && ext.is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e))) && !target.chars().any(char::is_control)
}

/// Ad verilmezse: adreste alan adı (www. olmadan), dosyada uzantısız dosya adı.
pub fn default_name(target: &str) -> String {
    let name = if is_url(target) {
        let host = target.split_once("://").map_or(target, |(_, r)| r).split(['/', '?', '#', ':']).next().unwrap_or_default();
        host.strip_prefix("www.").unwrap_or(host).to_owned()
    } else {
        let file = target.rsplit(['\\', '/']).next().unwrap_or(target);
        file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_owned()
    };
    name.chars().take(NAME_MAX).collect()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn new_id() -> String {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default());
    format!("app-{:012x}", h.finish() & 0xffff_ffff_ffff)
}

/// Ayar dosyasından gelen listeyi temizler: geçersiz hedef düşer, ad kısalır, kimlik tekil olur, aynı hedef
/// iki kez yer almaz, en çok `MAX_APPS`.
pub fn sanitize(list: Vec<AppShortcut>) -> Vec<AppShortcut> {
    let mut out: Vec<AppShortcut> = Vec::new();
    for mut app in list {
        app.target = app.target.trim().to_owned();
        if app.target.len() > TARGET_MAX || !(is_url(&app.target) || is_program(&app.target)) {
            continue;
        }
        if out.iter().any(|o| o.target.eq_ignore_ascii_case(&app.target)) {
            continue;
        }
        app.name = app.name.trim().chars().take(NAME_MAX).collect();
        if app.name.is_empty() {
            app.name = default_name(&app.target);
        }
        if !valid_id(&app.id) || out.iter().any(|o| o.id == app.id) {
            app.id = format!("app-{}", out.len() + 1);
            while out.iter().any(|o| o.id == app.id) {
                app.id.push('x');
            }
        }
        out.push(app);
        if out.len() == MAX_APPS {
            break;
        }
    }
    out
}

fn find(app: &AppHandle, id: &str) -> Result<AppShortcut, String> {
    settings::current(app).apps.into_iter().find(|a| a.id == id).ok_or_else(|| "Kısayol bulunamadı.".into())
}

/// Listeye ekler; eklenen kısayolu (temizlenmiş haliyle) döndürür.
fn add(app: &AppHandle, target: String, name: String) -> Result<AppShortcut, String> {
    let target = target.trim().to_owned();
    if !is_url(&target) && !is_program(&target) {
        return Err("Yalnız .exe/.lnk dosyası ya da http(s) adresi eklenebilir.".into());
    }
    let current = settings::current(app).apps;
    if current.iter().any(|a| a.target.eq_ignore_ascii_case(&target)) {
        return Err("Bu kısayol zaten ekli.".into());
    }
    if current.len() >= MAX_APPS {
        return Err(format!("En çok {MAX_APPS} kısayol eklenebilir."));
    }
    let id = new_id();
    let item = AppShortcut { id: id.clone(), name, target };
    let next = settings::modify(app, |s| s.apps.push(item))?;
    next.apps.into_iter().find(|a| a.id == id).ok_or_else(|| "Kısayol eklenemedi.".into())
}

/// Adadaki "+": dosya seçici açılır (.exe, .lnk…); seçilen uygulama listeye eklenir. Vazgeçilirse None.
#[tauri::command]
pub async fn apps_pick(app: AppHandle) -> Result<Option<AppShortcut>, String> {
    if settings::current(&app).apps.len() >= MAX_APPS {
        return Err(format!("En çok {MAX_APPS} kısayol eklenebilir."));
    }
    let owner = app.get_webview_window("island").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    let picked = tauri::async_runtime::spawn_blocking(move || pick_file(owner, "Kısayol için uygulama seç")).await.map_err(|e| e.to_string())?;
    match picked {
        Some(path) => add(&app, path, String::new()).map(Some),
        None => Ok(None),
    }
}

/// Modlar › uygulama ekle: aynı seçici, ama kısayol listesine eklenmez; seçilen yol döner.
pub async fn pick_program(app: &AppHandle, title: &'static str) -> Result<Option<String>, String> {
    let owner = app.get_webview_window("island").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    tauri::async_runtime::spawn_blocking(move || pick_file(owner, title)).await.map_err(|e| e.to_string())
}

/// Ayarlar › Ada: adres (ya da elle yazılan yol) ekler.
#[tauri::command]
pub fn apps_add(app: AppHandle, target: String, name: String) -> Result<AppShortcut, String> {
    add(&app, target, name)
}

/// Kısayolu açar (kimliğiyle; hedef ayarlardan okunur).
#[tauri::command]
pub fn apps_launch(app: AppHandle, id: String) -> Result<(), String> {
    let item = find(&app, &id)?;
    if !is_url(&item.target) && !Path::new(&item.target).exists() {
        return Err(format!("{} bulunamadı; taşınmış ya da kaldırılmış olabilir.", item.name));
    }
    if crate::win::shell_open(&item.target) { Ok(()) } else { Err(format!("{} açılamadı.", item.name)) }
}

/// Kısayolun simgesi (data:image/png). Adres kısayolunda ya da simge çıkarılamazsa None.
#[tauri::command]
pub async fn apps_icon(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let item = find(&app, &id)?;
    if is_url(&item.target) {
        return Ok(None);
    }
    let dir = settings::data_dir(&app).map(|d| d.join("icons"));
    tauri::async_runtime::spawn_blocking(move || icon_url(&item.target, dir.as_deref())).await.map_err(|e| e.to_string())
}

/// Bellekte (hedef + değişme anı → data URL) ve diskte önbellek.
fn icon_url(target: &str, dir: Option<&Path>) -> Option<String> {
    use base64::Engine;
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let modified = std::fs::metadata(target).and_then(|m| m.modified()).ok()?;
    let stamp = modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default();
    let key = format!("{:016x}-{stamp}", fnv(&target.to_lowercase()));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(url) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return Some(url.clone());
    }
    let file: Option<PathBuf> = dir.map(|d| d.join(format!("{key}.png")));
    let png = match file.as_ref().and_then(|f| std::fs::read(f).ok()) {
        Some(bytes) => bytes,
        None => {
            let rgba = extract_rgba(target, ICON_PX)?;
            let bytes = png_rgba(ICON_PX as u32, ICON_PX as u32, &rgba);
            if let (Some(dir), Some(file)) = (dir, &file) {
                // Önbellek yazılamazsa simge yine gösterilir, bir dahaki sefere yeniden çıkarılır.
                let _ = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(file, &bytes));
            }
            bytes
        }
    };
    let url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png));
    cache.lock().unwrap_or_else(|p| p.into_inner()).insert(key, url.clone());
    Some(url)
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

// --- PNG (sıkıştırmasız, yeni paket yok) -------------------------------------------------------------

/// RGBA pikselleri PNG'ye çevirir. Deflate "saklanmış" bloklarla: 64×64 simge ~16 KB, sıkıştırmaya değmez.
pub fn png_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks(row).take(height as usize) {
        raw.push(0); // satır süzgeci: yok
        raw.extend_from_slice(line);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(0xffff).collect();
    if blocks.is_empty() {
        z.extend([1, 0, 0, 0xff, 0xff]);
    }
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend(adler32(&raw).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(width.to_be_bytes());
    ihdr.extend(height.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]); // 8 bit, RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in bytes {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// GDI'nin BGRA (önceden çarpılmış alfa) çıktısını düz RGBA'ya çevirir. Eski simgelerde alfa hiç yoksa
/// (hepsi 0) boyanan pikseller opak sayılır.
pub fn bgra_to_rgba(mut px: Vec<u8>) -> Vec<u8> {
    let has_alpha = px.chunks(4).any(|p| p[3] != 0);
    for p in px.chunks_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = if p[0] | p[1] | p[2] != 0 { 255 } else { 0 };
        } else if p[3] != 0 && p[3] != 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    px
}

// --- Win32 -----------------------------------------------------------------------------------------

/// Dosyanın simgesini `px` boyunda RGBA olarak okur: önce dosyanın kendi büyük simgesi, olmazsa kabuğun
/// gösterdiği simge (.lnk ve .url dahil).
fn extract_rgba(path: &str, px: i32) -> Option<Vec<u8>> {
    use std::ffi::c_void;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows::Win32::UI::Shell::{SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
    use windows::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};
    use windows::core::HSTRING;

    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let wide = HSTRING::from(path);
        let mut icon = HICON::default();
        let _ = SHDefExtractIconW(&wide, 0, 0, Some(&mut icon), None, px as u32);
        if icon.is_invalid() {
            let mut info = SHFILEINFOW::default();
            let found = SHGetFileInfoW(&wide, FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
            if found != 0 {
                icon = info.hIcon;
            }
        }
        let pixels = (!icon.is_invalid()).then(|| {
            let dc = CreateCompatibleDC(None);
            let header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: px,
                    biHeight: -px, // yukarıdan aşağı
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let out = CreateDIBSection(Some(dc), &header, DIB_RGB_COLORS, &mut bits, None, 0).ok().and_then(|bmp| {
                let old = SelectObject(dc, bmp.into());
                let drawn = DrawIconEx(dc, 0, 0, icon, px, px, 0, None, DI_NORMAL).is_ok();
                let copy = (drawn && !bits.is_null()).then(|| std::slice::from_raw_parts(bits as *const u8, (px * px * 4) as usize).to_vec());
                SelectObject(dc, old);
                let _ = DeleteObject(bmp.into());
                copy
            });
            let _ = DeleteDC(dc);
            let _ = DestroyIcon(icon);
            out
        });
        if init {
            CoUninitialize();
        }
        pixels.flatten().map(bgra_to_rgba)
    }
}

/// Uygulama seçici (.exe, .lnk, .url). Kısayol dosyası çözülmeden kendisi seçilir: simgesi ve adı onun.
fn pick_file(owner: Option<isize>, title: &str) -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize};
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NODEREFERENCELINKS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    use windows::core::{HSTRING, w};
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let picked = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let options = dialog.GetOptions().ok()?;
            dialog.SetOptions(options | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_NODEREFERENCELINKS).ok()?;
            let filters = [COMDLG_FILTERSPEC { pszName: w!("Uygulamalar"), pszSpec: w!("*.exe;*.lnk;*.url;*.appref-ms") }];
            let _ = dialog.SetFileTypes(&filters);
            let _ = dialog.SetTitle(&HSTRING::from(title));
            let owner = owner.map(|h| HWND(h as *mut c_void));
            dialog.Show(owner).ok()?;
            let item = dialog.GetResult().ok()?;
            let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path = name.to_string().ok();
            CoTaskMemFree(Some(name.0 as *const c_void));
            path
        })();
        if init {
            CoUninitialize();
        }
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, name: &str, target: &str) -> AppShortcut {
        AppShortcut { id: id.into(), name: name.into(), target: target.into() }
    }

    #[test]
    fn targets_are_checked() {
        assert!(is_program(r"C:\Program Files\Code\Code.exe"));
        assert!(is_program(r"C:\Users\a\Desktop\Discord.LNK"));
        assert!(is_program(r"\\nas\apps\tool.exe"));
        assert!(!is_program(r"Code.exe"), "göreli yol");
        assert!(!is_program(r"C:\Windows\System32\evil.bat"), "betik değil");
        assert!(is_url("https://mail.google.com/mail/u/0/"));
        assert!(!is_url("https://"));
        assert!(!is_url("javascript:alert(1)"));
        assert!(!is_url("https://a b.com"));
    }

    #[test]
    fn names_default_from_target() {
        assert_eq!(default_name("https://www.youtube.com/feed"), "youtube.com");
        assert_eq!(default_name(r"C:\Apps\Visual Studio Code.exe"), "Visual Studio Code");
    }

    #[test]
    fn sanitize_drops_bad_dupes_and_extras() {
        let list = vec![
            app("a", "  Kod  ", r"C:\Apps\code.exe"),
            app("a", "", r"C:\Apps\discord.lnk"),
            app("b", "Kopya", r"c:\apps\CODE.exe"),
            app("c", "Kötü", "ftp://x"),
            app("BAD ID", "", "https://github.com"),
            app("e", "", "https://mail.google.com"),
            app("f", "", "https://youtube.com"),
        ];
        let out = sanitize(list);
        assert_eq!(out.len(), MAX_APPS);
        assert_eq!(out[0].name, "Kod");
        assert_eq!(out[1].name, "discord");
        assert_ne!(out[1].id, "a", "kimlik tekil");
        assert!(out.iter().all(|a| valid_id(&a.id)));
        assert_eq!(out[2].name, "github.com");
        let mut ids: Vec<_> = out.iter().map(|a| a.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), MAX_APPS);
    }

    #[test]
    fn png_is_well_formed() {
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        let png = png_rgba(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn colors_unpremultiply() {
        assert_eq!(bgra_to_rgba(vec![10, 20, 30, 0, 0, 0, 0, 0]), vec![30, 20, 10, 255, 0, 0, 0, 0], "alfasız eski simge");
        assert_eq!(bgra_to_rgba(vec![0, 0, 128, 128]), vec![255, 0, 0, 128], "yarı saydam kırmızı");
    }
}
