//! Ada için gereken küçük Win32 çağrıları. Hepsi ucuz, engellemeyen sistem çağrılarıdır.
//!
//! Not: Pencere stili ve görünürlük için tao'nun kendi bayraklarına dokunan Tauri API'leri
//! (set_ignore_cursor_events, hide/show) ilk gösterimden sonra kullanılmaz; tao stili baştan
//! hesaplayıp bizim bayraklarımızı silerdi. Tıklama-geçirme pencere bölgesiyle yapılır.

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HGDIOBJ, HMONITOR,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow, RGN_OR, SetWindowRgn,
};
use windows::Win32::Media::Audio::{
    PlaySoundW, SND_ALIAS, SND_ASYNC, SND_FILENAME, SND_FLAGS, SND_MEMORY, SND_NODEFAULT, SND_SYSTEM,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON};
use windows::Win32::UI::Shell::{
    DefSubclassProc, FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, QUNS_PRESENTATION_MODE,
    QUNS_QUIET_TIME, SHFILEOPSTRUCTW, SHFileOperationW, SHQueryUserNotificationState, SetWindowSubclass, ShellExecuteW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW, GetCursorPos, GetForegroundWindow, GetShellWindow, GetWindowLongPtrW,
    GetWindowRect, HWND_TOPMOST, IsIconic, IsWindow, IsWindowVisible, IsZoomed, MONITORINFOF_PRIMARY, SW_HIDE, SW_RESTORE,
    SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCACTIVATE, WM_NCPAINT,
    WS_CAPTION, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, PCWSTR, s, w};

use crate::display;

/// Pencere tutamacı; iş parçacıkları arasında taşınabilmesi için sayı olarak saklanır.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hwnd(isize);

impl Hwnd {
    pub fn new(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }
    pub fn raw(self) -> HWND {
        HWND(self.0 as *mut _)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    pub fn inflate(&self, dx: i32, dy: i32) -> Rect {
        Rect { left: self.left - dx, top: self.top - dy, right: self.right + dx, bottom: self.bottom + dy }
    }
    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect { left: self.left + dx, top: self.top + dy, right: self.right + dx, bottom: self.bottom + dy }
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
    }
}

pub fn cursor() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

pub fn window_rect(hwnd: Hwnd) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd.raw(), &mut r) }.ok().map(|_| r.into())
}

pub fn dpi_scale(hwnd: Hwnd) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd.raw()) };
    if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 }
}

fn key_down(vk: u16) -> bool {
    (unsafe { GetAsyncKeyState(i32::from(vk)) } as u16 & 0x8000) != 0
}

pub fn left_button_down() -> bool {
    key_down(VK_LBUTTON.0)
}

pub fn escape_down() -> bool {
    key_down(VK_ESCAPE.0)
}

/// Adanın ekranında tam ekran bir uygulama (oyun, video, sunum, F11) önde mi; öyleyse ada gizlenir.
/// Başka ekrandaki tam ekran uygulama adayı gizlemez. Windows'un sunum modu (Sunum ayarları) her
/// ekranda geçerlidir.
pub fn fullscreen_on(monitor: Rect, own: Hwnd) -> bool {
    if matches!(unsafe { SHQueryUserNotificationState() }, Ok(state) if state == QUNS_PRESENTATION_MODE) {
        return true;
    }
    unsafe {
        let front = GetForegroundWindow();
        if front.is_invalid() {
            return false;
        }
        let root = GetAncestor(front, GA_ROOT);
        let root = if root.is_invalid() { front } else { root };
        if root == own.raw() || root == GetShellWindow() || is_desktop(root) {
            return false;
        }
        let Some(rect) = window_rect(Hwnd::new(root)) else { return false };
        let style = GetWindowLongPtrW(root, GWL_STYLE) as u32;
        let captioned = style & WS_CAPTION.0 == WS_CAPTION.0;
        display::covers(rect, monitor, IsZoomed(root).as_bool() && captioned)
    }
}

/// Masaüstü (simgelerin durduğu pencere) ekranı kaplar ama tam ekran uygulama değildir.
fn is_desktop(hwnd: HWND) -> bool {
    let mut buf = [0u16; 16];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) }.max(0) as usize;
    let class = String::from_utf16_lossy(&buf[..len]);
    class == "WorkerW" || class == "Progman"
}

/// Bağlı ekranlar (fiziksel piksel, Windows ölçeğiyle). Takılan/çıkarılan ekranı hemen görür.
pub fn monitors() -> Vec<display::Monitor> {
    unsafe extern "system" fn each(monitor: HMONITOR, _hdc: HDC, _clip: *mut RECT, data: LPARAM) -> BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<display::Monitor>) };
        if let Some(found) = unsafe { describe_monitor(monitor) } {
            list.push(found);
        }
        BOOL(1)
    }
    let mut list: Vec<display::Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&raw mut list as isize));
    }
    list
}

unsafe fn describe_monitor(monitor: HMONITOR) -> Option<display::Monitor> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    let scale = match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } {
        Ok(()) if dpi_x > 0 => f64::from(dpi_x) / 96.0,
        _ => 1.0,
    };
    let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
    Some(display::Monitor {
        id: String::from_utf16_lossy(&info.szDevice[..len]),
        rect: info.monitorInfo.rcMonitor.into(),
        scale,
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

/// Pencerenin bulunduğu ekranın tam alanı (fiziksel piksel).
pub fn monitor_of(hwnd: Hwnd) -> Option<Rect> {
    unsafe {
        let monitor = MonitorFromWindow(hwnd.raw(), MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(monitor, &mut info).as_bool().then(|| info.rcMonitor.into())
    }
}

/// Ekran takıldı/çıkarıldı, çözünürlük ya da ölçek değişti: `changed` çağrılır (Windows mesajıyla,
/// yoklama yok). Mesajları pencerenin iş parçacığı alır; `changed` kısa olmalı.
pub fn watch_display_changes(hwnd: Hwnd, changed: fn()) {
    static CHANGED: OnceLock<fn()> = OnceLock::new();
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if (msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED)
            && let Some(changed) = CHANGED.get()
        {
            changed();
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }
    if CHANGED.set(changed).is_ok() {
        unsafe {
            let _ = SetWindowSubclass(hwnd.raw(), Some(proc), 3, 0);
        }
    }
}

/// Alt+Tab ve görev çubuğunda görünmez; tıklanınca etkinleşmez.
pub fn make_tool_window(hwnd: Hwnd) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd.raw(), GWL_EXSTYLE);
        let style = (style | WS_EX_NOACTIVATE.0 as isize | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd.raw(), GWL_EXSTYLE, style);
    }
}

/// Yazma kipi: ada normalde tıklanınca etkinleşmez (öndeki pencerenin odağı bozulmaz). Bir metin
/// kutusuna yazılacağı zaman geçici olarak etkinleşebilir olur; iş bitince eski hâline döner.
pub fn set_activatable(hwnd: Hwnd, on: bool) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd.raw(), GWL_EXSTYLE);
        let flag = WS_EX_NOACTIVATE.0 as isize;
        SetWindowLongPtrW(hwnd.raw(), GWL_EXSTYLE, if on { style & !flag } else { style | flag });
    }
}

pub fn foreground() -> Option<Hwnd> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then(|| Hwnd::new(hwnd))
}

/// Pencere hâlâ varsa öne getirir (yazma kipinden çıkarken eski pencereye dönüş).
pub fn set_foreground(hwnd: Hwnd) {
    unsafe {
        if IsWindow(Some(hwnd.raw())).as_bool() {
            let _ = SetForegroundWindow(hwnd.raw());
        }
    }
}

/// Bir ajan oturumunun penceresini (terminal, düzenleyici) öne getirir; simge durumundaysa açar.
/// Tutamak köprüden gelir: yalnız hâlâ var olan, görünür bir pencereye uygulanır. Kullanıcı adaya
/// az önce tıkladığı için Windows öne getirmeye izin verir.
pub fn bring_to_front(raw: u64) -> bool {
    let hwnd = HWND(raw as usize as *mut _);
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Pencereyi yalnız ada şekline indirger: üst köşeler düz, alt köşeler yuvarlak.
/// Bölgenin dışı pencereye ait sayılmaz; tıklamalar doğrudan alttaki uygulamaya gider.
/// `r` pencerenin sol üst köşesine göredir (fiziksel piksel).
pub fn set_region(hwnd: Hwnd, r: Rect, radius: i32) {
    unsafe {
        let full = CreateRoundRectRgn(r.left, r.top, r.right + 1, r.bottom + 1, radius * 2, radius * 2);
        let top = CreateRectRgn(r.left, r.top, r.right, (r.bottom - radius).max(r.top));
        let _ = CombineRgn(Some(full), Some(full), Some(top), RGN_OR);
        let _ = DeleteObject(HGDIOBJ(top.0));
        // Başarılı olursa bölgenin sahibi sistemdir; silinmez.
        if SetWindowRgn(hwnd.raw(), Some(full), true) == 0 {
            let _ = DeleteObject(HGDIOBJ(full.0));
        }
    }
}

pub fn show_without_focus(hwnd: Hwnd) {
    unsafe {
        let _ = ShowWindow(hwnd.raw(), SW_SHOWNOACTIVATE);
    }
    raise_topmost(hwnd);
}

/// En üst katmanın en önüne yeniden geçer (odak almadan, yer/boyut değişmeden). Başka bir "hep üstte"
/// pencere (Görev Yöneticisi, kayıt araçları…) öne gelince ada onun arkasında kalmasın.
pub fn raise_topmost(hwnd: Hwnd) {
    unsafe {
        if IsWindowVisible(hwnd.raw()).as_bool() {
            let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
            let _ = SetWindowPos(hwnd.raw(), Some(HWND_TOPMOST), 0, 0, 0, 0, flags);
        }
    }
}

static TOPMOST: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Ön plan penceresi değişince (Windows olayı; yoklama yok) adanın en üst sırası yenilenir. Ana iş
/// parçacığında (mesaj döngüsü olan) bir kez çağrılır. Zodyvel'in kendi pencereleri olay üretmez.
pub fn keep_on_top(hwnd: Hwnd) {
    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
        let raw = TOPMOST.load(std::sync::atomic::Ordering::Relaxed);
        if raw != 0 {
            raise_topmost(Hwnd::new(HWND(raw as *mut c_void)));
        }
    }
    if TOPMOST.swap(hwnd.raw().0 as isize, std::sync::atomic::Ordering::Relaxed) != 0 {
        return;
    }
    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    unsafe { SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0, flags) };
}

pub fn hide(hwnd: Hwnd) {
    unsafe {
        let _ = ShowWindow(hwnd.raw(), SW_HIDE);
    }
}

/// Seslerin ortak bayrakları: beklemeden döner; sistem sesi düzeyiyle ("Sistem sesleri" kaydırıcısı)
/// çalar; ses bulunamazsa varsayılan "ding" de çalmaz.
const SOUND_FLAGS: SND_FLAGS = SND_FLAGS(SND_ASYNC.0 | SND_NODEFAULT.0 | SND_SYSTEM.0);

/// Windows'un kendi bildirim sesi (kullanıcının ses düzeni). Düzende ses yoksa hiçbir şey çalmaz.
pub fn play_alias(alias: &str) {
    let alias = HSTRING::from(alias);
    unsafe {
        let _ = PlaySoundW(&alias, None, SND_ALIAS | SOUND_FLAGS);
    }
}

/// Bellekteki bir WAV dosyası (gömülü Zody sesleri). Veri `'static` olmalı: çalma arka planda sürer.
pub fn play_wav(bytes: &'static [u8]) {
    unsafe {
        let _ = PlaySoundW(PCWSTR(bytes.as_ptr().cast()), None, SND_MEMORY | SOUND_FLAGS);
    }
}

/// Diskteki bir WAV dosyası (kullanıcının yüklediği ses).
pub fn play_file(path: &std::path::Path) {
    let path = HSTRING::from(path);
    unsafe {
        let _ = PlaySoundW(&path, None, SND_FILENAME | SOUND_FLAGS);
    }
}

/// Windows "Rahatsız etme" açık mı (Windows 10'da Odak yardımı; elle, zamanlamayla ya da otomatik
/// kurallarla açılmış olabilir). Sunum modu ve kurulumdan sonraki sessiz saat de sessiz sayılır.
pub fn do_not_disturb() -> bool {
    if quiet_hours_profile().is_some_and(|profile| profile != 0) {
        return true;
    }
    matches!(unsafe { SHQueryUserNotificationState() }, Ok(state) if state == QUNS_PRESENTATION_MODE || state == QUNS_QUIET_TIME)
}

type QueryWnfStateData =
    unsafe extern "system" fn(*const u64, *const c_void, *const c_void, *mut u32, *mut c_void, *mut u32) -> i32;

/// Rahatsız etme için belgelenmiş bir API yok (`SHQueryUserNotificationState` onu görmez; eski hata buydu).
/// Windows kabuğunun kendisi durumu WNF'ye yayınlar: `WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED`
/// (0 kapalı, 1 yalnız öncelikli, 2 yalnız alarmlar). ntdll'den okunur; okunamazsa `None`.
fn quiet_hours_profile() -> Option<u32> {
    static QUERY: OnceLock<Option<QueryWnfStateData>> = OnceLock::new();
    const WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED: u64 = 0x0D83_063E_A3BF_1C75;
    let query = (*QUERY.get_or_init(|| unsafe {
        let ntdll = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        let function = GetProcAddress(ntdll, s!("NtQueryWnfStateData"))?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryWnfStateData>(function))
    }))?;
    let mut stamp = 0u32;
    let mut profile = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        query(
            &WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED,
            std::ptr::null(),
            std::ptr::null(),
            &mut stamp,
            (&raw mut profile).cast(),
            &mut size,
        )
    };
    // Hiç yayınlanmamışsa boyut 0 döner: kapalı demektir.
    (status >= 0).then_some(if size >= 4 { profile } else { 0 })
}

/// Kabuk üzerinden açar; Görev Yöneticisi gibi yükseltme isteyen uygulamalar için gerekli.
pub fn shell_open(file: &str) -> bool {
    let file = HSTRING::from(file);
    let result = unsafe { ShellExecuteW(None, w!("open"), &file, None, None, SW_SHOWNORMAL) };
    result.0 as isize > 32
}

/// Dosyayı Geri Dönüşüm Kutusu'na taşır (Gezgin'deki "Sil" gibi, geri getirilebilir). Kutuya sığmayacaksa
/// Windows kalıcı silmeden önce sorar. Taşındıysa true.
pub fn recycle(file: &std::path::Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    // Çift NUL ile biten yol listesi.
    let mut from: Vec<u16> = file.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    let flags = FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0 | FOF_NOERRORUI.0 | FOF_WANTNUKEWARNING.0;
    let mut op = SHFILEOPSTRUCTW { wFunc: FO_DELETE, pFrom: PCWSTR(from.as_ptr()), fFlags: flags as u16, ..Default::default() };
    let result = unsafe { SHFileOperationW(&mut op) };
    result == 0 && !op.fAnyOperationsAborted.as_bool() && file.symlink_metadata().is_err()
}

/// Yerel saat (saat dilimi ve yaz saati uygulanmış): yıl, ay, gün, saat, dakika, saniye, milisaniye.
pub fn local_time() -> (i32, u32, u32, u32, u32, u32, u32) {
    let t = unsafe { GetLocalTime() };
    (
        i32::from(t.wYear),
        u32::from(t.wMonth),
        u32::from(t.wDay),
        u32::from(t.wHour),
        u32::from(t.wMinute),
        u32::from(t.wSecond),
        u32::from(t.wMilliseconds),
    )
}


/// Windows, pencerenin etkinlik durumu değişince (ör. adadan Ayarlar açılınca) `WM_NCACTIVATE`
/// ile eski tip başlık çubuğunu boyar. Kenarlıksız ada penceresinde bu çubuk istemci alanının
/// üstüne, açık mavi bir şerit olarak düşüyordu: ada küçülürken pencere bölgesinin içinde
/// görünüyordu. Ada penceresinde çerçeve boyaması engellenir; başka davranış değişmez.
pub fn suppress_frame_paint(hwnd: Hwnd) {
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        match msg {
            // TRUE: etkinlik değişimi olur, çerçeve boyanmaz.
            WM_NCACTIVATE => LRESULT(1),
            WM_NCPAINT => LRESULT(0),
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd.raw(), Some(proc), 1, 0);
    }
}

/// Windows klasör seçicisi ("Başka klasör…", "Klasör ekle"). `owner`: çağıran pencerenin tutamacı (seçici onun
/// önünde açılır), `title`: seçicinin başlığı. Seçilen klasörün yolu; vazgeçilirse ya da seçici açılamazsa
/// `None`. Engelleyen çağrı: arka planda çağrılır.
pub fn pick_folder(owner: Option<isize>, title: &str) -> Option<String> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize};
    use windows::Win32::UI::Shell::{FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let picked = (|| {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let options = dialog.GetOptions().ok()?;
            dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
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
