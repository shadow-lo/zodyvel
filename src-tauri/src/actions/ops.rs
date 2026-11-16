//! Faz 13b eylemleri: tarayıcıda bağlantı/arama, yeni Claude sohbeti, uygulama açma/kapatma, izinli klasörde
//! dosya kopyalama/taşıma, etkin pencereye yazma ve mail taslağı. Eylem kaydı (`spec.rs`) sınıflandırır, onay
//! merkezi (`mod.rs`) kapıdan geçirir; bu dosya yalnız işi yapar. Üst kısım saf (sınanır), alt kısım Windows'a bağlı.
//!
//! Asla yapmaz (burada da zorlanır): mail **göndermek** (yalnız `mailto:` taslağı açılır), dosya **silmek** (kopya ya da
//! taşıma; hedefte aynı ad varsa ezmez, yeni ad alır), zorla kapatmak (yalnız `WM_CLOSE`, süreç öldürülmez),
//! kabuk/şifre yöneticisi/güvenlik uygulamalarına yazmak, Zodyvel'in kendisini kapatmak.

use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::{apps, settings, win};

/// Adrese konan metin üst sınırı ve adres üst sınırı.
const ADRES_EN_UZUN: usize = 2000;
/// `mailto:` adresinin üst sınırı (ShellExecute ve bazı istemciler ~2000'den sonra keser).
const MAILTO_EN_UZUN: usize = 1900;

// ───────────────────────────── saf kısım ─────────────────────────────

/// RFC 3986'ya göre yüzde kodlama (UTF-8; yalnız ayrılmamış karakterler olduğu gibi kalır).
pub fn yuzde(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Denetim karakteri (satır sonu, sekme dahil) var mı. Yazılacak ya da adrese girecek metinde istenmez.
pub fn denetim_var(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// Tarayıcıda açılabilecek adres: yalnız http(s), boşluksuz, kullanıcı-bilgisiz (`user@host` aldatmacası yok).
pub fn adres_gecerli(url: &str) -> bool {
    if url.chars().count() > ADRES_EN_UZUN || !apps::is_url(url) {
        return false;
    }
    let authority = url.split_once("://").map_or("", |(_, r)| r).split(['/', '?', '#']).next().unwrap_or("");
    !authority.contains('@')
}

/// "şu videoyu aç" gibi isteklerin adresi. `motor`: "youtube" | "google".
pub fn arama_adresi(motor: &str, sorgu: &str) -> Result<String, String> {
    if denetim_var(sorgu) {
        return Err("Arama metninde denetim karakteri olamaz.".into());
    }
    let q = yuzde(sorgu.trim());
    match motor {
        "youtube" => Ok(format!("https://www.youtube.com/results?search_query={q}")),
        "google" => Ok(format!("https://www.google.com/search?q={q}")),
        _ => Err("Bilinmeyen arama motoru.".into()),
    }
}

/// Yeni Claude sohbeti: claude.ai'yi sohbet metni hazır yazılı açar; gönder tuşuna kullanıcı basar.
pub fn claude_sohbet_adresi(metin: &str) -> Result<String, String> {
    if metin.chars().any(|c| c.is_control() && c != '\n') {
        return Err("Sohbet metninde denetim karakteri olamaz.".into());
    }
    Ok(format!("https://claude.ai/new?q={}", yuzde(metin.trim())))
}

/// Mail taslağı (`mailto:`). Gönderilmez: varsayılan mail uygulaması taslak olarak açar.
pub fn mailto_adresi(kime: Option<&str>, konu: &str, govde: &str) -> Result<String, String> {
    if denetim_var(konu) {
        return Err("Konuda denetim karakteri olamaz.".into());
    }
    if govde.chars().any(|c| c.is_control() && c != '\n') {
        return Err("Gövdede denetim karakteri olamaz.".into());
    }
    let alici = match kime {
        None => String::new(),
        Some(k) => {
            let k = k.trim();
            let (yerel, alan) = k.split_once('@').ok_or("Alıcı adresi geçersiz.")?;
            let temiz = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'));
            if !temiz(yerel) || !temiz(alan) || !alan.contains('.') {
                return Err("Alıcı adresi geçersiz.".into());
            }
            k.to_owned()
        }
    };
    // Gövdedeki satır sonları mailto'da CRLF (%0D%0A) olmalı.
    let govde = govde.replace("\r\n", "\n").replace('\n', "\r\n");
    let url = format!("mailto:{alici}?subject={}&body={}", yuzde(konu.trim()), yuzde(&govde));
    if url.len() > MAILTO_EN_UZUN { Err("Taslak çok uzun; kısaltılmalı.".into()) } else { Ok(url) }
}

/// `yol` (kanonik), izinli klasörlerden (kanonik) birinin içinde mi. Bileşen bileşen karşılaştırılır; `C:\izin2`,
/// `C:\izin` içinde sayılmaz.
pub fn yol_izinli(izinli: &[PathBuf], yol: &Path) -> bool {
    izinli.iter().any(|k| yol.starts_with(k))
}

/// Hedef klasörde çakışmayan ad: `dosya.txt` → `dosya (kopya).txt` → `dosya (kopya 2).txt`. Var olanı hiç ezmez.
pub fn bos_ad(ad: &str, var_mi: impl Fn(&str) -> bool) -> String {
    if !var_mi(ad) {
        return ad.to_owned();
    }
    let (govde, uzanti) = match ad.rsplit_once('.') {
        Some((g, u)) if !g.is_empty() => (g, format!(".{u}")),
        _ => (ad, String::new()),
    };
    for n in 1..1000 {
        let aday = if n == 1 { format!("{govde} (kopya){uzanti}") } else { format!("{govde} (kopya {n}){uzanti}") };
        if !var_mi(&aday) {
            return aday;
        }
    }
    format!("{govde} (kopya {}){uzanti}", 1000)
}

fn kucuk(s: &str) -> String {
    s.trim().to_lowercase()
}

/// "Discord", "discord.exe", "Spotify" gibi bir adı Başlat menüsü kısayolları arasında bulur: önce tam eşleşme, sonra
/// tek bir kısmi eşleşme. Birden çok kısmi eşleşme belirsizdir; seçmek yerine reddedilir.
pub fn uygulama_bul<'a>(liste: &'a [(String, PathBuf)], istenen: &str) -> Result<&'a PathBuf, String> {
    let ist = kucuk(istenen.trim_end_matches(".exe").trim_end_matches(".lnk"));
    if ist.chars().count() < 2 {
        return Err("Uygulama adı çok kısa.".into());
    }
    if let Some((_, yol)) = liste.iter().find(|(ad, _)| kucuk(ad) == ist) {
        return Ok(yol);
    }
    let kismi: Vec<&(String, PathBuf)> = liste.iter().filter(|(ad, _)| kucuk(ad).contains(&ist)).collect();
    match kismi.as_slice() {
        [] => Err(format!("\"{istenen}\" Başlat menüsünde bulunamadı.")),
        [(_, yol)] => Ok(yol),
        coklu => {
            let adlar: Vec<&str> = coklu.iter().take(4).map(|(a, _)| a.as_str()).collect();
            Err(format!("\"{istenen}\" birden çok uygulamayla eşleşti ({}); tam adı söyle.", adlar.join(", ")))
        }
    }
}

/// Zodyvel'in kapatmadığı işlemler (sistem, kendisi, kabuk).
pub fn kapatma_yasak(exe: &str) -> bool {
    matches!(
        kucuk(exe).as_str(),
        "zodyvel" | "explorer" | "winlogon" | "csrss" | "dwm" | "lsass" | "services" | "smss" | "wininit" | "svchost" | "securityhealthsystray" | "taskmgr"
    )
}

/// Etkin pencereye yazılmayan işlemler: kabuklar (satır sonu ve komut riski), şifre yöneticileri, güvenlik ve
/// kimlik doğrulama pencereleri, kayıt defteri, Zodyvel'in kendisi.
pub fn yazma_yasak(exe: &str) -> bool {
    let e = kucuk(exe);
    matches!(
        e.as_str(),
        "zodyvel" | "cmd" | "powershell" | "pwsh" | "windowsterminal" | "wt" | "conhost" | "bash" | "wsl" | "mintty" | "regedit" | "mmc"
            | "consent" | "logonui" | "credentialuibroker" | "securityhealthui" | "winlogon" | "lockapp" | "taskmgr"
    ) || ["keepass", "bitwarden", "1password", "lastpass", "dashlane", "enpass", "nordpass", "roboform", "keeper"].iter().any(|p| e.contains(p))
}

/// İzinli klasör olabilir mi: mutlak sürücü yolu, sürücü kökü değil, sistem klasörü değil. (Var olup olmadığına bakılmaz:
/// ayar dosyası yüklenirken disk gezilmez; eylem anında kanonik yolla yeniden denetlenir.)
pub fn klasor_uygun(yol: &str) -> bool {
    let b = yol.as_bytes();
    if yol.chars().count() > 400 || denetim_var(yol) || b.len() < 4 || !b[0].is_ascii_alphabetic() || b[1] != b':' || (b[2] != b'\\' && b[2] != b'/') {
        return false;
    }
    let norm = yol.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let Some(govde) = norm.get(3..) else { return false };
    if govde.is_empty() || govde.split('\\').any(|p| p == ".." || p == "." || p.is_empty()) {
        return false;
    }
    let ilk = govde.split('\\').next().unwrap_or("");
    // Sistem ve program klasörleri, kullanıcılar kökü: kopyalama/taşıma buralarda anlamsız ve tehlikeli.
    !(matches!(ilk, "windows" | "program files" | "program files (x86)" | "programdata" | "$recycle.bin" | "system volume information") || govde == "users")
}

/// Ayar dosyasındaki izinli klasör listesini temizler: uygun olmayanları atar, tekrarları kaldırır, en çok 8.
pub fn izinli_klasorleri_ayikla(liste: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in liste {
        let k = k.trim().trim_end_matches(['\\', '/']).to_owned();
        if klasor_uygun(&format!("{k}\\")) && !out.iter().any(|o| o.eq_ignore_ascii_case(&k)) && out.len() < 8 {
            out.push(k);
        }
    }
    out
}

/// Ayarlar sayfası için klasör seçici (izinli klasör eklemek). Yalnız arayüz; Zody göremez.
#[tauri::command]
pub async fn brain_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri::Manager;
    let owner = app.get_webview_window("settings").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize);
    tauri::async_runtime::spawn_blocking(move || win::pick_folder(owner, "Zody'nin dosya işleyebileceği klasör")).await.map_err(|e| e.to_string())
}

/// Yazılacak metin: denetim karakteri yok (Enter, sekme, kaçış dizisi ile komut çalıştırılamaz).
pub fn yazi_gecerli(metin: &str) -> bool {
    !metin.is_empty() && !denetim_var(metin)
}

// ───────────────────────────── Windows kısmı ─────────────────────────────

fn ac(adres: &str) -> Result<(), String> {
    if win::shell_open(adres) { Ok(()) } else { Err("Açılamadı.".into()) }
}

fn izinli_klasorler(app: &AppHandle) -> Vec<PathBuf> {
    settings::current(app).brain.allowed_folders.iter().filter_map(|k| Path::new(k).canonicalize().ok()).collect()
}

/// Kaynak dosya + hedef klasör, ikisi de izinli klasörde (kanonik yollarla; `..` ve bağlantı kaçışı geçmez).
fn dosya_yollari(app: &AppHandle, kaynak: &str, hedef: &str) -> Result<(PathBuf, PathBuf), String> {
    let izinli = izinli_klasorler(app);
    if izinli.is_empty() {
        return Err("İzinli klasör yok (Ayarlar › Zody asistanı › İzinli klasörler).".into());
    }
    let kaynak = Path::new(kaynak).canonicalize().map_err(|_| "Kaynak dosya bulunamadı.".to_owned())?;
    let hedef = Path::new(hedef).canonicalize().map_err(|_| "Hedef klasör bulunamadı.".to_owned())?;
    if !kaynak.is_file() {
        return Err("Kaynak bir dosya olmalı (klasör kopyalanmaz).".into());
    }
    if !hedef.is_dir() {
        return Err("Hedef bir klasör olmalı.".into());
    }
    if !yol_izinli(&izinli, &kaynak) || !yol_izinli(&izinli, &hedef) {
        return Err("Kaynak ve hedef izinli klasörlerde olmalı.".into());
    }
    Ok((kaynak, hedef))
}

fn hedef_yolu(kaynak: &Path, hedef: &Path) -> Result<PathBuf, String> {
    let ad = kaynak.file_name().and_then(|n| n.to_str()).ok_or("Dosya adı okunamadı.")?;
    Ok(hedef.join(bos_ad(ad, |a| hedef.join(a).exists())))
}

/// Tarayıcıda adres aç.
#[tauri::command]
pub fn web_open(url: String) -> Result<(), String> {
    if !adres_gecerli(&url) {
        return Err("Geçerli bir http(s) adresi değil.".into());
    }
    ac(&url)
}

/// Tarayıcıda arama aç ("şu videoyu aç" → YouTube araması).
#[tauri::command]
pub fn web_search(query: String, engine: Option<String>) -> Result<(), String> {
    ac(&arama_adresi(engine.as_deref().unwrap_or("google"), &query)?)
}

/// Yeni Claude sohbeti (metin hazır, göndermeyi kullanıcı yapar).
#[tauri::command]
pub fn claude_chat(text: String) -> Result<(), String> {
    ac(&claude_sohbet_adresi(&text)?)
}

/// Başlat menüsü kısayolları (ad, yol): kullanıcı ve ortak klasör, en çok üç düzey. Yalnız istek anında taranır.
fn baslat_menusu() -> Vec<(String, PathBuf)> {
    fn tara(dir: &Path, derinlik: u8, out: &mut Vec<(String, PathBuf)>) {
        let Ok(girdiler) = std::fs::read_dir(dir) else { return };
        for g in girdiler.flatten() {
            let yol = g.path();
            if yol.is_dir() {
                if derinlik < 3 {
                    tara(&yol, derinlik + 1, out);
                }
            } else if yol.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
                && let Some(ad) = yol.file_stem().and_then(|s| s.to_str())
            {
                out.push((ad.to_owned(), yol.clone()));
            }
        }
    }
    let mut out = Vec::new();
    for (degisken, ek) in [("APPDATA", r"Microsoft\Windows\Start Menu\Programs"), ("ProgramData", r"Microsoft\Windows\Start Menu\Programs")] {
        if let Some(kok) = std::env::var_os(degisken) {
            tara(&Path::new(&kok).join(ek), 0, &mut out);
        }
    }
    out
}

/// Başlat menüsündeki bir uygulamayı adıyla aç (yalnız Başlat menüsü kısayolları; rastgele yol çalıştırılmaz).
#[tauri::command]
pub fn app_open(name: String) -> Result<String, String> {
    let liste = baslat_menusu();
    let yol = uygulama_bul(&liste, &name)?;
    ac(&yol.to_string_lossy())?;
    Ok(yol.file_stem().and_then(|s| s.to_str()).unwrap_or(&name).to_owned())
}

/// Açık bir uygulamanın pencerelerini nazikçe kapatır (`WM_CLOSE`: kaydedilmemiş iş için uygulama kendisi sorar).
#[tauri::command]
pub fn app_close(name: String) -> Result<usize, String> {
    let ist = kucuk(name.trim().trim_end_matches(".exe"));
    if ist.chars().count() < 2 {
        return Err("Uygulama adı çok kısa.".into());
    }
    if kapatma_yasak(&ist) {
        return Err("Bu uygulamayı Zody kapatamaz.".into());
    }
    let n = pencere::kapat(&ist);
    if n == 0 { Err(format!("\"{name}\" için açık pencere bulunamadı.")) } else { Ok(n) }
}

/// İzinli klasörde dosya kopyala (hedefte aynı ad varsa yeni ad alır; hiçbir şey ezilmez).
#[tauri::command]
pub fn files_copy(app: AppHandle, source: String, target_dir: String) -> Result<String, String> {
    let (kaynak, hedef) = dosya_yollari(&app, &source, &target_dir)?;
    let yol = hedef_yolu(&kaynak, &hedef)?;
    std::fs::copy(&kaynak, &yol).map_err(|e| format!("Kopyalanamadı: {e}"))?;
    Ok(yol.to_string_lossy().into_owned())
}

/// İzinli klasörde dosya taşı. Aynı sürücüde yeniden adlandırma; başka sürücüde kopya + boyut doğrulaması + kaynağı kaldırma.
#[tauri::command]
pub fn files_move(app: AppHandle, source: String, target_dir: String) -> Result<String, String> {
    let (kaynak, hedef) = dosya_yollari(&app, &source, &target_dir)?;
    let yol = hedef_yolu(&kaynak, &hedef)?;
    if std::fs::rename(&kaynak, &yol).is_err() {
        let boyut = std::fs::copy(&kaynak, &yol).map_err(|e| format!("Taşınamadı: {e}"))?;
        if std::fs::metadata(&kaynak).map(|m| m.len()).ok() != Some(boyut) {
            return Err("Taşıma doğrulanamadı; kaynak yerinde bırakıldı.".into());
        }
        std::fs::remove_file(&kaynak).map_err(|e| format!("Kopyalandı ama kaynak kaldırılamadı: {e}"))?;
    }
    Ok(yol.to_string_lossy().into_owned())
}

/// Etkin pencereye metin yaz (Unicode tuş vuruşları; Enter/sekme yok, kabuk ve şifre yöneticisi pencereleri hariç).
#[tauri::command]
pub fn window_type(text: String) -> Result<String, String> {
    if !yazi_gecerli(&text) || text.chars().count() > 1000 {
        return Err("Metin boş, çok uzun ya da denetim karakteri içeriyor.".into());
    }
    let exe = pencere::etkin_exe().ok_or("Etkin pencere bulunamadı.")?;
    if exe.eq_ignore_ascii_case("zodyvel") {
        return Err("Etkin pencere Zodyvel; yazılacak başka bir pencere seç, sonra onayla (ör. Ctrl+Alt+Enter).".into());
    }
    if yazma_yasak(&exe) {
        return Err(format!("{exe} penceresine yazılmaz."));
    }
    pencere::yaz(&text)?;
    Ok(exe)
}

/// Mail taslağı aç (gönderilmez).
#[tauri::command]
pub fn mail_draft(to: Option<String>, subject: String, body: String) -> Result<(), String> {
    ac(&mailto_adresi(to.as_deref(), &subject, &body)?)
}

mod pencere {
    use std::mem::size_of;

    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
    };
    use windows::core::{BOOL, PWSTR};

    /// Sürecin exe adı (uzantısız, küçük harf olmayabilir).
    fn exe_adi(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
            let _ = CloseHandle(h);
            if !ok {
                return None;
            }
            let yol = String::from_utf16_lossy(&buf[..n as usize]);
            let ad = yol.rsplit(['\\', '/']).next()?;
            Some(ad.strip_suffix(".exe").or_else(|| ad.strip_suffix(".EXE")).unwrap_or(ad).to_owned())
        }
    }

    fn pid_of(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    }

    pub fn etkin_exe() -> Option<String> {
        let h = unsafe { GetForegroundWindow() };
        if h.is_invalid() {
            return None;
        }
        exe_adi(pid_of(h))
    }

    struct Ara {
        istenen: String,
        pencereler: Vec<HWND>,
        kendi: u32,
    }

    unsafe extern "system" fn say(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ara = unsafe { &mut *(lparam.0 as *mut Ara) };
        unsafe {
            // Yalnız görünür, sahipsiz (üst düzey), başlıklı pencereler.
            if !IsWindowVisible(hwnd).as_bool() || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                return BOOL(1);
            }
            let uzun = GetWindowTextLengthW(hwnd);
            if uzun <= 0 {
                return BOOL(1);
            }
            let pid = pid_of(hwnd);
            if pid == ara.kendi {
                return BOOL(1);
            }
            let mut baslik = vec![0u16; uzun as usize + 1];
            let n = GetWindowTextW(hwnd, &mut baslik);
            let baslik = String::from_utf16_lossy(&baslik[..n.max(0) as usize]).to_lowercase();
            let exe = exe_adi(pid).unwrap_or_default().to_lowercase();
            if exe.is_empty() || super::kapatma_yasak(&exe) {
                return BOOL(1);
            }
            if exe == ara.istenen || baslik.contains(&ara.istenen) {
                ara.pencereler.push(hwnd);
            }
        }
        BOOL(1)
    }

    /// Eşleşen pencerelere `WM_CLOSE` gönderir; kaç pencereye gönderildiği döner.
    pub fn kapat(istenen: &str) -> usize {
        let mut ara = Ara { istenen: istenen.to_owned(), pencereler: Vec::new(), kendi: std::process::id() };
        unsafe {
            let _ = EnumWindows(Some(say), LPARAM(&mut ara as *mut Ara as isize));
        }
        ara.pencereler.iter().filter(|&&h| unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() }).count()
    }

    /// Unicode tuş vuruşları (aktif pencereye). Her UTF-16 birimi için bas + bırak.
    pub fn yaz(metin: &str) -> Result<(), String> {
        let mut girdiler: Vec<INPUT> = Vec::with_capacity(metin.len() * 2);
        for birim in metin.encode_utf16() {
            for bayrak in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                girdiler.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: birim, dwFlags: bayrak, time: 0, dwExtraInfo: 0 } },
                });
            }
        }
        let gonderilen = unsafe { SendInput(&girdiler, size_of::<INPUT>() as i32) } as usize;
        if gonderilen == girdiler.len() { Ok(()) } else { Err("Metin eksik yazıldı (pencere girişi engelledi).".into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuzde_kodlama() {
        assert_eq!(yuzde("a b&c=ç"), "a%20b%26c%3D%C3%A7");
        assert_eq!(yuzde("AZaz09-_.~"), "AZaz09-_.~");
    }

    #[test]
    fn arama_ve_sohbet_adresleri() {
        assert_eq!(arama_adresi("youtube", "lofi hip hop").unwrap(), "https://www.youtube.com/results?search_query=lofi%20hip%20hop");
        assert_eq!(arama_adresi("google", "a&b").unwrap(), "https://www.google.com/search?q=a%26b");
        assert!(arama_adresi("bing", "x").is_err());
        assert!(arama_adresi("google", "x\ny").is_err());
        assert_eq!(claude_sohbet_adresi("Merhaba\nDünya").unwrap(), "https://claude.ai/new?q=Merhaba%0AD%C3%BCnya");
        assert!(claude_sohbet_adresi("x\u{7}").is_err());
        // Adres ayrıştırıcıdan kaçamaz: ek parametre ya da fragman girişi kodlanır.
        assert!(!claude_sohbet_adresi("a&model=x#y").unwrap().contains(['#', ' ']));
        assert_eq!(claude_sohbet_adresi("a&b").unwrap(), "https://claude.ai/new?q=a%26b");
    }

    #[test]
    fn adres_denetimi() {
        assert!(adres_gecerli("https://youtu.be/abc?t=5"));
        for kotu in ["javascript:alert(1)", "file:///C:/x", "ftp://x.com", "https://", "https://a b.com", "https://google.com@evil.com/x", "C:\\x.exe", ""] {
            assert!(!adres_gecerli(kotu), "{kotu}");
        }
        assert!(!adres_gecerli(&format!("https://a.com/{}", "x".repeat(2000))));
    }

    #[test]
    fn mail_taslagi_gonderilmez_ve_temizdir() {
        let u = mailto_adresi(Some("ali@ornek.com"), "Merhaba dünya", "Selam\nNaber").unwrap();
        assert!(u.starts_with("mailto:ali@ornek.com?subject=Merhaba%20d%C3%BCnya&body=Selam%0D%0ANaber"), "{u}");
        assert!(mailto_adresi(None, "Konu", "g").unwrap().starts_with("mailto:?subject="));
        for kotu in ["ali", "a@b", "a li@b.com", "a@b.com,c@d.com", "a@b.com?cc=x", "@b.com"] {
            assert!(mailto_adresi(Some(kotu), "k", "g").is_err(), "{kotu}");
        }
        // Başlık enjeksiyonu: satır sonu konuya girmez.
        assert!(mailto_adresi(None, "k\r\nBcc: x@y.com", "g").is_err());
        assert!(mailto_adresi(None, "k", &"x".repeat(2000)).is_err());
    }

    #[test]
    fn izinli_yol_bilesen_bilesen() {
        let izin = vec![PathBuf::from(r"C:\izin"), PathBuf::from(r"D:\proje\alt")];
        assert!(yol_izinli(&izin, Path::new(r"C:\izin")));
        assert!(yol_izinli(&izin, Path::new(r"C:\izin\a\b.txt")));
        assert!(yol_izinli(&izin, Path::new(r"D:\proje\alt\x")));
        assert!(!yol_izinli(&izin, Path::new(r"C:\izin2\a.txt")), "önek eşleşmesi izin değil");
        assert!(!yol_izinli(&izin, Path::new(r"C:\baska")));
        assert!(!yol_izinli(&izin, Path::new(r"D:\proje")));
        assert!(!yol_izinli(&[], Path::new(r"C:\izin")), "izinli klasör yoksa hiçbir yol izinli değil");
    }

    #[test]
    fn dosya_ezilmez() {
        let var = |a: &str| ["r.txt", "r (kopya).txt", "x"].contains(&a);
        assert_eq!(bos_ad("yeni.txt", var), "yeni.txt");
        assert_eq!(bos_ad("r.txt", var), "r (kopya 2).txt");
        assert_eq!(bos_ad("x", var), "x (kopya)");
    }

    #[test]
    fn uygulama_eslesmesi() {
        let l: Vec<(String, PathBuf)> = ["Discord", "Spotify", "Visual Studio Code", "Visual Studio Installer", "Notepad"]
            .iter()
            .map(|a| (a.to_string(), PathBuf::from(format!(r"C:\sm\{a}.lnk"))))
            .collect();
        assert_eq!(uygulama_bul(&l, "discord").unwrap(), &PathBuf::from(r"C:\sm\Discord.lnk"));
        assert_eq!(uygulama_bul(&l, "Spotify.exe").unwrap(), &PathBuf::from(r"C:\sm\Spotify.lnk"));
        assert_eq!(uygulama_bul(&l, "studio code").unwrap(), &PathBuf::from(r"C:\sm\Visual Studio Code.lnk"));
        assert!(uygulama_bul(&l, "visual studio").unwrap_err().contains("birden çok"));
        assert!(uygulama_bul(&l, "yok böyle").is_err());
        assert!(uygulama_bul(&l, "d").is_err());
    }

    #[test]
    fn izinli_klasor_uygunlugu() {
        for iyi in [r"C:\Users\Ben\Belgeler\Zody", "D:/proje", r"E:\a\b\"] {
            assert!(klasor_uygun(iyi), "{iyi}");
        }
        for kotu in [r"C:\", "C:", "D:/", r"C:\Windows", r"C:\Windows\System32", r"C:\Program Files\x", r"C:\Users", "relative\\x", r"\\sunucu\paylasim", r"C:\a\..\b", "C:\\a\nb", ""] {
            assert!(!klasor_uygun(kotu), "{kotu:?}");
        }
        let l = izinli_klasorleri_ayikla(vec![r"C:\Benim".into(), r"c:\benim\".into(), r"C:\Windows".into(), "x".into(), r"D:\Baska".into()]);
        assert_eq!(l, vec![r"C:\Benim".to_owned(), r"D:\Baska".to_owned()]);
        let cok: Vec<String> = (0..20).map(|i| format!(r"C:\k{i}")).collect();
        assert_eq!(izinli_klasorleri_ayikla(cok).len(), 8);
    }

    #[test]
    fn yasak_listeleri() {
        for e in ["explorer", "Zodyvel", "winlogon", "TaskMgr"] {
            assert!(kapatma_yasak(e), "{e}");
        }
        assert!(!kapatma_yasak("discord"));
        for e in ["cmd", "PowerShell", "WindowsTerminal", "KeePassXC", "Bitwarden", "1Password", "Zodyvel", "consent", "regedit"] {
            assert!(yazma_yasak(e), "{e}");
        }
        for e in ["notepad", "chrome", "Code", "WINWORD"] {
            assert!(!yazma_yasak(e), "{e}");
        }
        assert!(yazi_gecerli("merhaba dünya") && !yazi_gecerli("a\nb") && !yazi_gecerli("a\tb") && !yazi_gecerli(""));
    }
}
