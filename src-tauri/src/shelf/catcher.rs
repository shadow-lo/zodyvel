//! Adanın kendi bırakma hedefi: şeridin üstünde görünmez (alfa 1/255) küçük bir alt pencere.
//!
//! Neden: WebView2'nin pencereleri ayrı süreçte; Windows sürüklenen dosyayı imlecin altındaki en
//! derin pencereye sorar ve Tauri'nin kaydettiği hedefe hiç ulaşmayabilir (geliştirme bilgisayarında
//! ada sürüklemede hiç açılmadı). Bu pencere bizim süreçte ve şeridin en üstünde olduğundan hedef
//! kesin bizim. Boşta iş yapmaz: yalnız Windows mesajı gelince uyanır.
//!
//! Fare şeride gelince pencere gizlenir (fare olayları adaya geçer, üzerine gelme/tıklama aynen
//! çalışır); imleç şeritten çıkınca ya da ada kapanınca yeniden gösterilir. Sürükleme başlayınca
//! pencere adanın tamamını kaplar, bırakılınca ya da vazgeçilince şerit boyuna döner.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU8, Ordering};

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINTL, WPARAM};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, HWND_TOP, KillTimer, LWA_ALPHA, MA_NOACTIVATE, RegisterClassW, SW_HIDE, SW_SHOWNA,
    SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow,
    WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_TIMER, WNDCLASSW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED,
    WS_EX_NOACTIVATE,
};
use windows::core::{Ref, implement, w};

use super::list::Drag;
use crate::debuglog;
use crate::island::{self, HitRect, Shared};
use crate::win::{self, Hwnd, Rect};

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Alt pencere (0: kurulmadı); kurulduysa Tauri'nin sürükleme olayları yok sayılır.
static CATCHER: AtomicIsize = AtomicIsize::new(0);
static PARENT: AtomicIsize = AtomicIsize::new(0);
/// Dosya sürükleniyor (girişten bırakma/vazgeçmeye kadar).
static DRAGGING: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU8 = AtomicU8::new(0);
/// Fare şeritteyken gizlendi; imleç çıkınca yeniden gösterilecek.
static WAITING: AtomicBool = AtomicBool::new(false);

const WATCH_TIMER: usize = 1;
/// Gizliyken imlecin şeritten çıkıp çıkmadığına bakma aralığı (yalnız fare şeritteyken çalışır).
const WATCH_MS: u32 = 250;

pub fn installed() -> bool {
    CATCHER.load(Ordering::Relaxed) != 0
}

/// Sürüklemeyi bu pencere mi karşılıyor: kuruluysa ve ada kapalıyken ya da sürüklemeyi zaten yakalamışken.
/// Ada açıkken pencere gizlidir; dosya WebView2'ye bırakılır ve Tauri'nin olayı kullanılır (on_drag).
pub fn handles_drags() -> bool {
    installed() && (LEVEL.load(Ordering::Relaxed) == 0 || DRAGGING.load(Ordering::Relaxed))
}

/// Son bildirilen imleç yeri (ekran pikseli, x << 32 | y); aynı yer yeniden bildirilmez.
static LAST_POINT: AtomicI64 = AtomicI64::new(i64::MIN);

/// İmlecin yerini ada penceresine göre arayüz pikseline çevirip bildirir (oturum adını parlatmak için).
fn report(pt: &POINTL) {
    let packed = (i64::from(pt.x) << 32) | i64::from(pt.y as u32);
    if LAST_POINT.swap(packed, Ordering::Relaxed) == packed {
        return;
    }
    let (Some(app), Some(parent)) = (APP.get(), hwnd(&PARENT)) else { return };
    let Some(window) = win::window_rect(parent) else { return };
    let scale = island::scale(parent);
    super::over(app, f64::from(pt.x - window.left) / scale, f64::from(pt.y - window.top) / scale);
}

fn hwnd(slot: &AtomicIsize) -> Option<Hwnd> {
    match slot.load(Ordering::Relaxed) {
        0 => None,
        raw => Some(Hwnd::new(HWND(raw as *mut c_void))),
    }
}

/// Şerit (boştaki ada) pencere içinde, fiziksel piksel.
fn notch(parent: Hwnd) -> Rect {
    island::IslandState::default().hit.to_physical(island::scale(parent))
}

fn full(parent: Hwnd) -> Rect {
    let scale = island::scale(parent);
    HitRect { x: 0.0, y: 0.0, w: island::WINDOW_W, h: island::WINDOW_H }.to_physical(scale)
}

fn place(rect: Rect, show: bool) {
    let Some(catcher) = hwnd(&CATCHER) else { return };
    let flags = SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS | if show { SWP_SHOWWINDOW } else { Default::default() };
    unsafe {
        let _ = SetWindowPos(catcher.raw(), Some(HWND_TOP), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, flags);
    }
}

fn hide() {
    if let Some(catcher) = hwnd(&CATCHER) {
        unsafe {
            let _ = ShowWindow(catcher.raw(), SW_HIDE);
        }
    }
}

/// İmleç şeridin ya da adanın şu anki alanının (biraz genişletilmiş) üstünde mi.
///
/// Ada seviye 0'da kalırken de büyüyebilir (çentikteki hap ara boya ya da mini pencereye açılır).
/// Yalnız şeride bakılırsa, en üst satırdan girip büyüyen hapın içinde biraz aşağı inen imleç
/// "şeritten çıktı" sayılır: "stripLeave" hapı kapatır ve bu pencere yeniden hapın üstüne gelirdi.
fn cursor_on_notch(parent: Hwnd) -> bool {
    let (Some((x, y)), Some(window)) = (win::cursor(), win::window_rect(parent)) else { return false };
    let scale = island::scale(parent);
    let on = |r: Rect| r.offset(window.left, window.top).inflate(4, 4).contains(x, y);
    let current = APP.get().map(|app| app.state::<Shared>().with(|s| (s.hit, s.settle.map(|(_, hit)| hit))));
    on(notch(parent))
        || current.is_some_and(|(hit, target)| on(hit.to_physical(scale)) || target.is_some_and(|t| on(t.to_physical(scale))))
}

/// Fare şeride bu pencerenin üstünden girdi/çıktı: arayüze bildirilir. İmleç ekranın en üst satırına
/// (y=0) dayanınca daha yukarı gidemez; bu pencere gizlendikten sonra fare kımıldamadığı için WebView2
/// hiç fare olayı almaz ve ada açılmazdı. Üzerine gelmeyi bu bildirim başlatır.
fn notify_strip(kind: &'static str) {
    if let Some(app) = APP.get() {
        let _ = app.emit_to(island::LABEL, "island://pointer", serde_json::json!({ "kind": kind }));
    }
}

/// Fare şeritteyken gizlenir; imleç çıkınca (ve ada kapalıysa) geri gelir.
fn wait_for_cursor_to_leave(catcher: Hwnd) {
    hide();
    if !WAITING.swap(true, Ordering::Relaxed) {
        unsafe { SetTimer(Some(catcher.raw()), WATCH_TIMER, WATCH_MS, None) };
    }
}

fn watch_tick(catcher: Hwnd) {
    let Some(parent) = hwnd(&PARENT) else { return };
    let open = LEVEL.load(Ordering::Relaxed) > 0;
    if open || !cursor_on_notch(parent) {
        unsafe {
            let _ = KillTimer(Some(catcher.raw()), WATCH_TIMER);
        }
        WAITING.store(false, Ordering::Relaxed);
        if !open {
            place(notch(parent), true);
            notify_strip("stripLeave");
        }
    }
}

/// Ada seviyesi değişti (island_update). Açıkken fare olayları adanındır; kapanınca şerit yeniden dinlenir.
pub fn level_changed(level: u8) {
    let before = LEVEL.swap(level, Ordering::Relaxed);
    let (Some(catcher), Some(parent)) = (hwnd(&CATCHER), hwnd(&PARENT)) else { return };
    if DRAGGING.load(Ordering::Relaxed) || before == level {
        return;
    }
    if level > 0 {
        hide();
    } else if cursor_on_notch(parent) {
        wait_for_cursor_to_leave(catcher);
    } else {
        place(notch(parent), true);
    }
}

/// Ada yeniden yerleşti (başka ekran, ölçek ya da ada boyutu): şerit üstündeki pencere yeni boyuta geçer.
/// Sürükleme sürerken, ada açıkken ya da imleç şeritteyken dokunulmaz; oradan dönünce zaten yeniden yerleşir.
pub fn relayout() {
    let Some(parent) = hwnd(&PARENT) else { return };
    if DRAGGING.load(Ordering::Relaxed) || WAITING.load(Ordering::Relaxed) || LEVEL.load(Ordering::Relaxed) > 0 {
        return;
    }
    place(notch(parent), true);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE if !DRAGGING.load(Ordering::Relaxed) => {
            if !WAITING.load(Ordering::Relaxed) {
                notify_strip("stripEnter");
            }
            wait_for_cursor_to_leave(Hwnd::new(hwnd));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == WATCH_TIMER => {
            watch_tick(Hwnd::new(hwnd));
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Ana iş parçacığında (OLE orada başlatılmış) ada kurulurken bir kez çağrılır.
pub fn install(app: &AppHandle, parent: Hwnd) {
    let _ = APP.set(app.clone());
    let result = unsafe { create(parent) };
    match result {
        Ok(catcher) => {
            PARENT.store(parent.raw().0 as isize, Ordering::Relaxed);
            CATCHER.store(catcher.raw().0 as isize, Ordering::Relaxed);
            place(notch(parent), true);
            debuglog::line("shelf: bırakma penceresi hazır");
        }
        Err(error) => debuglog::line(format!("shelf: bırakma penceresi kurulamadı {error}")),
    }
}

unsafe fn create(parent: Hwnd) -> windows::core::Result<Hwnd> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("ZodyvelShelfDrop");
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        let rect = notch(parent);
        let catcher = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class,
            w!(""),
            WS_CHILD | WS_CLIPSIBLINGS,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(parent.raw()),
            None,
            Some(instance.into()),
            None,
        )?;
        // Neredeyse tam saydam: görünmez ama imleç ve sürükleme onu bulur (alfa 0 tıklamayı geçirebilir).
        SetLayeredWindowAttributes(catcher, COLORREF(0), 1, LWA_ALPHA)?;
        let target: IDropTarget = Target { valid: AtomicBool::new(false) }.into();
        RegisterDragDrop(catcher, &target)?;
        // Kayıt kendi başvurusunu tutar; pencere uygulama boyunca yaşar.
        std::mem::forget(target);
        let _ = ShowWindow(catcher, SW_SHOWNA);
        Ok(Hwnd::new(catcher))
    }
}

/// Bırakılan şeyin dosya yolları (dosya değilse None: metin, bağlantı).
fn paths(data: Ref<'_, IDataObject>) -> Option<Vec<PathBuf>> {
    let data = data.as_ref()?;
    let format = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    unsafe {
        let mut medium = data.GetData(&format).ok()?;
        let drop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(drop, u32::MAX, None);
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(drop, i, Some(&mut buf));
            out.push(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
        }
        ReleaseStgMedium(&mut medium);
        Some(out)
    }
}

fn deliver(drag: Drag) {
    if let Some(app) = APP.get() {
        super::handle(app, drag);
    }
}

#[implement(IDropTarget)]
struct Target {
    /// Girişte dosya vardı.
    valid: AtomicBool,
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, data: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let files = paths(data);
        let valid = files.as_ref().is_some_and(|f| !f.is_empty());
        self.valid.store(valid, Ordering::Relaxed);
        debuglog::line(format!("shelf: sürükleme girdi, {} dosya", files.as_ref().map_or(0, Vec::len)));
        unsafe { *effect = if valid { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if valid {
            DRAGGING.store(true, Ordering::Relaxed);
            if let Some(parent) = hwnd(&PARENT) {
                place(full(parent), true);
            }
            deliver(Drag::Enter(files.map_or(0, |f| f.len())));
            LAST_POINT.store(i64::MIN, Ordering::Relaxed);
            report(pt);
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let valid = self.valid.load(Ordering::Relaxed);
        unsafe { *effect = if valid { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if valid {
            report(pt);
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        debuglog::line("shelf: sürükleme çıktı");
        if self.valid.swap(false, Ordering::Relaxed) {
            finish();
            deliver(Drag::Leave);
        }
        Ok(())
    }

    fn Drop(&self, data: Ref<'_, IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let valid = self.valid.swap(false, Ordering::Relaxed);
        let files = paths(data).unwrap_or_default();
        unsafe { *effect = if valid && !files.is_empty() { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        if valid {
            finish();
            deliver(Drag::Drop(files.iter().map(|p| (p.to_string_lossy().into_owned(), p.is_dir())).collect()));
        }
        Ok(())
    }
}

/// Sürükleme bitti: şerit boyuna dön; ada açıksa fare olayları adaya kalsın diye gizlen.
fn finish() {
    DRAGGING.store(false, Ordering::Relaxed);
    let Some(parent) = hwnd(&PARENT) else { return };
    let open = APP.get().is_some_and(|app| app.state::<Shared>().with(|s| s.level) > 0);
    LEVEL.store(u8::from(open), Ordering::Relaxed);
    if open {
        hide();
        place(notch(parent), false);
    } else {
        place(notch(parent), true);
    }
}
