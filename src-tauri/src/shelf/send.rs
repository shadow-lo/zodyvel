//! Raf → AI (Faz 6, K5): rafa ya da adaya sürüklenen dosyayı açık bir AI oturumuna gönderir.
//!
//! Claude Code etkileşimli bir oturuma dışarıdan dosya eklemeye izin vermez. Bu yüzden iki yol var:
//! - Zodyvel'in başlattığı, süreci açık görev (`claude -p`, girdi stream-json): dosya gerçekten eklenir.
//!   Resimler resim bloğu olarak gider, diğer dosyalar (ve büyük resimler) yollarıyla; Claude onları okur.
//! - Diğer oturumlar (terminal, VS Code, Claude masaüstü, Codex): yollar panoya kopyalanır ve oturumun
//!   penceresi öne getirilir; kullanıcı yapıştırır.
//!
//! Boşta iş yok: yalnız bırakma anında çalışır.

use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};

use crate::agents::AgentHub;
use crate::debuglog;

/// Bir seferde gönderilen en çok dosya.
pub const MAX_FILES: usize = 10;
/// Resim bloğu olarak gönderilen en büyük dosya (bayt). Base64 ile ~5 MB'a çıkar; üstü yoluyla gider.
pub const MAX_IMAGE: u64 = 3_750_000;

/// Gönderimin sonucu (arayüzdeki kısa not).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    pub session: String,
    /// "attached": göreve eklendi · "copied": yol panoya kopyalandı.
    pub how: &'static str,
    /// Oturumun penceresi öne geldi (yalnız "copied").
    pub focused: bool,
    pub count: usize,
    /// İlk dosyanın adı (not için).
    pub name: String,
    /// Gönderilemediyse nedeni.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Resim bloğu olabilecek dosyanın türü (Claude'un kabul ettikleri); değilse `None`.
pub fn image_type(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Panoya yazılacak metin: yollar boşlukla ayrılır, boşluk içereni tırnağa alınır (terminale yapıştırılınca
/// tek yol sayılsın).
pub fn clip_text(paths: &[String]) -> String {
    paths.iter().map(|p| if p.contains(' ') { format!("\"{p}\"") } else { p.clone() }).collect::<Vec<_>>().join(" ")
}

/// Göreve giden mesajın içeriği: önce yolları sayan metin, sonra okunabilen resimler. `read` resmin
/// baytlarını verir (boyut sınırı içindeyse); okunamayan resim yalnız yoluyla kalır.
pub fn content(paths: &[String], read: impl Fn(&str) -> Option<Vec<u8>>) -> Value {
    let list = paths.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n");
    let intro = if paths.len() == 1 { "Zodyvel rafından bir dosya ekledim:" } else { "Zodyvel rafından dosyalar ekledim:" };
    let mut blocks = vec![json!({ "type": "text", "text": format!("{intro}\n{list}") })];
    for path in paths {
        let Some(media) = image_type(path) else { continue };
        let Some(bytes) = read(path) else { continue };
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        blocks.push(json!({ "type": "image", "source": { "type": "base64", "media_type": media, "data": data } }));
    }
    Value::Array(blocks)
}

fn read_image(path: &str) -> Option<Vec<u8>> {
    let size = std::fs::metadata(path).ok()?.len();
    if size == 0 || size > MAX_IMAGE {
        return None;
    }
    std::fs::read(path).ok()
}

fn name_of(paths: &[String]) -> String {
    paths.first().map(|p| super::list::file_name(p)).unwrap_or_default()
}

/// Dosyaları oturuma gönderir. Hata da `Sent` içinde döner (arayüz notu tek biçimde gösterir).
pub fn deliver(app: &AppHandle, session: &str, paths: Vec<String>) -> Sent {
    let paths: Vec<String> = paths.into_iter().filter(|p| Path::new(p).exists()).take(MAX_FILES).collect();
    let mut sent = Sent { session: session.to_owned(), how: "copied", focused: false, count: paths.len(), name: name_of(&paths), error: None };
    let result = route(app, session, &paths, &mut sent);
    if let Err(error) = result {
        debuglog::line(format!("shelf: oturuma gönderilemedi {error}"));
        sent.error = Some(error);
    }
    sent
}

fn route(app: &AppHandle, session: &str, paths: &[String], sent: &mut Sent) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Dosya bulunamadı.".into());
    }
    let agents = app.state::<Arc<AgentHub>>();
    if !agents.enabled() {
        return Err("AI modülü kapalı.".into());
    }
    let (live, window) = agents.drop_route(session).ok_or("Bu oturum artık yok.")?;
    if live {
        let tool_id = session.strip_prefix("claude:").unwrap_or(session);
        crate::tasks::send_content(app, tool_id, content(paths, read_image))?;
        sent.how = "attached";
        debuglog::line(format!("shelf: {} dosya göreve eklendi", paths.len()));
        return Ok(());
    }
    clipboard::set_text(&clip_text(paths))?;
    sent.focused = window.is_some_and(crate::win::bring_to_front);
    debuglog::line(format!("shelf: {} yol panoya, pencere öne {}", paths.len(), sent.focused));
    Ok(())
}

mod clipboard {
    use windows::Win32::Foundation::{E_FAIL, GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    /// Standart pano biçimi: UTF-16 metin.
    const CF_UNICODETEXT: u32 = 13;

    /// Metni panoya yazar. Pano o an başka bir uygulamada açıksa kısa aralıklarla birkaç kez denenir.
    pub fn set_text(text: &str) -> Result<(), String> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut opened = false;
        for _ in 0..5 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        if !opened {
            return Err("Pano açılamadı.".into());
        }
        let result = unsafe { put(&wide) };
        unsafe {
            let _ = CloseClipboard();
        }
        result.map_err(|_| "Yol panoya kopyalanamadı.".to_string())
    }

    unsafe fn put(wide: &[u16]) -> windows::core::Result<()> {
        unsafe {
            EmptyClipboard()?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
            let target = GlobalLock(memory) as *mut u16;
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(E_FAIL.into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
            let _ = GlobalUnlock(memory);
            // Başarılıysa bellek artık panonun; başarısızsa bizde kalır ve bırakılır.
            if let Err(error) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(error);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_known_by_extension() {
        assert_eq!(image_type(r"C:\a\Ekran Görüntüsü.PNG"), Some("image/png"));
        assert_eq!(image_type("x.jpeg"), Some("image/jpeg"));
        assert_eq!(image_type("x.webp"), Some("image/webp"));
        assert_eq!(image_type("rapor.pdf"), None);
        assert_eq!(image_type("uzantısız"), None);
    }

    #[test]
    fn clipboard_text_quotes_paths_with_spaces() {
        let paths = vec![r"C:\a\b.txt".to_string(), r"C:\Yeni klasör\ş ç.png".to_string()];
        assert_eq!(clip_text(&paths), r#"C:\a\b.txt "C:\Yeni klasör\ş ç.png""#);
    }

    #[test]
    fn content_lists_paths_and_adds_readable_images() {
        let paths = vec![r"C:\a\rapor.pdf".to_string(), r"C:\a\ekran.png".to_string(), r"C:\a\büyük.jpg".to_string()];
        let blocks = content(&paths, |p| p.ends_with("ekran.png").then(|| vec![1, 2, 3]));
        let blocks = blocks.as_array().unwrap();
        assert_eq!(blocks.len(), 2, "okunamayan (büyük) resim yalnız yoluyla kalır");
        let text = blocks[0]["text"].as_str().unwrap();
        assert!(text.starts_with("Zodyvel rafından dosyalar ekledim:"));
        assert!(text.contains(r"- C:\a\rapor.pdf") && text.contains(r"- C:\a\büyük.jpg"));
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
        assert_eq!(blocks[1]["source"]["data"], "AQID");
        let one = content(&paths[..1], |_| None);
        assert!(one[0]["text"].as_str().unwrap().starts_with("Zodyvel rafından bir dosya ekledim:"));
    }
}
