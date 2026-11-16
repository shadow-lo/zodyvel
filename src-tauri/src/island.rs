//! Ada penceresi: yerleşim, ortak durum ve arayüzden gelen geometri bildirimi.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::Thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_19,
};
use windows::core::Interface;

use crate::display::{self, Mode};
use crate::pointer;
use crate::settings::IslandPrefs;
use crate::win::{self, Hwnd, Rect};

pub const LABEL: &str = "island";
/// Pencerenin mantıksal boyutu (src/island/levels.ts WINDOW ile aynı; tests/screen.test.ts denetler).
/// En büyük seviyeyi kapsar; pencere bölgesi adanın dışını keser.
pub const WINDOW_W: f64 = 740.0;
pub const WINDOW_H: f64 = 460.0;
/// Boştaki siyah şerit (src/island/levels.ts NOTCH ile aynı).
const NOTCH_W: f64 = 120.0;
const NOTCH_H: f64 = 6.0;

/// Ada boyutu yüzdesi (Ayarlar → Ada → Boyut). Pencere, pencere bölgesi ve bırakma penceresi bununla
/// çarpılır; arayüz aynı oranda WebView2 yakınlaştırmasıyla büyür. Arayüzün kendi ölçüleri (levels.ts)
/// değişmez: tek kaynak bu sayıdır.
static SIZE_PCT: AtomicU32 = AtomicU32::new(100);
/// WebView2'ye en son verilen yakınlaştırma (yüzde; 0: henüz verilmedi).
static ZOOM_PCT: AtomicU32 = AtomicU32::new(0);
/// Sayfa ilk geometri bildirimini yaptı (yakınlaştırma sayfa yüklendikten sonra bir kez daha verilir).
static PAGE_READY: AtomicBool = AtomicBool::new(false);
/// Ekran değişimi mesajı ana iş parçacığında gelir; imleç iş parçacığı yerleşimi yapsın diye uyandırılır.
static SHARED: OnceLock<Shared> = OnceLock::new();

pub fn size_factor() -> f64 {
    f64::from(SIZE_PCT.load(Ordering::Relaxed)) / 100.0
}

/// Mantıksal (arayüz) pikselden fiziksel piksele çarpan: ekran ölçeği × ada boyutu.
pub fn scale(hwnd: Hwnd) -> f64 {
    win::dpi_scale(hwnd) * size_factor()
}

/// Pencere içindeki ada dikdörtgeni (mantıksal piksel).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HitRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl HitRect {
    /// Pencereye göre fiziksel dikdörtgen.
    pub fn to_physical(self, scale: f64) -> Rect {
        let left = (self.x * scale).round() as i32;
        let top = (self.y * scale).round() as i32;
        Rect { left, top, right: left + (self.w * scale).round() as i32, bottom: top + (self.h * scale).round() as i32 }
    }
}

#[derive(Debug)]
pub struct IslandState {
    pub level: u8,
    pub level_since: Instant,
    pub hit: HitRect,
    pub user_hidden: bool,
    /// WebView2'den önbelleklerini bırakması istendi mi (ada hapta dinleniyor).
    pub memory_low: bool,
    pub hwnd: Option<Hwnd>,
    pub poller: Option<Thread>,
    /// Yazma kipindeyken, sonra odağın geri verileceği pencere.
    pub typing: Option<Option<Hwnd>>,
    /// Animasyon bitince uygulanacak son alan. Rust uygular: arayüzün zamanlayıcısı kısılsa ya da
    /// sayfa bir süre çizilmese de bölge büyük kalıp boş (beyaz görünen) bir alan bırakmaz.
    pub settle: Option<(Instant, HitRect)>,
    /// Hangi ekranda (Ayarlar → Ada) ve seçilen ekranın kimliği.
    pub screen: Mode,
    pub screen_id: String,
    /// Yeniden yerleşme bekliyor (ekran değişti ya da ayar değişti); imleç iş parçacığı yapar.
    pub relayout: bool,
}

impl Default for IslandState {
    fn default() -> Self {
        // Arayüz ilk bildirimini yapana kadar boştaki şerit boyutu varsayılır.
        let hit = HitRect { x: (WINDOW_W - NOTCH_W) / 2.0, y: 0.0, w: NOTCH_W, h: NOTCH_H };
        Self {
            level: 0,
            level_since: Instant::now(),
            hit,
            user_hidden: false,
            memory_low: false,
            hwnd: None,
            poller: None,
            typing: None,
            settle: None,
            screen: Mode::Primary,
            screen_id: String::new(),
            relayout: false,
        }
    }
}

/// Ada hapta dinlenirken WebView2'den GPU/doku önbelleklerini bırakmasını ister; açılınca normale döner.
/// Ölçüm: ada açılıp kapandıktan sonra WebView2 GPU süreci ~30 MB fazlasını tutmaya devam ediyordu.
pub fn set_memory_low(app: &AppHandle, low: bool) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let _ = window.with_webview(move |webview| unsafe {
        let Ok(core) = webview.controller().CoreWebView2() else { return };
        if let Ok(core) = core.cast::<ICoreWebView2_19>() {
            let level = if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL };
            let _ = core.SetMemoryUsageTargetLevel(level);
        }
    });
}

#[derive(Clone, Default)]
pub struct Shared(pub Arc<Mutex<IslandState>>);

impl Shared {
    pub fn with<R>(&self, f: impl FnOnce(&mut IslandState) -> R) -> R {
        let mut guard = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    /// İmleç iş parçacığını hemen uyandırır (seviye değişti, görünürlük değişti…).
    pub fn wake(&self) {
        if let Some(thread) = self.with(|s| s.poller.clone()) {
            thread.unpark();
        }
    }
}

/// Pencere bölgesini ada şekline göre ayarlar.
pub fn apply_region(hwnd: Hwnd, hit: HitRect) {
    let scale = scale(hwnd);
    // Şerit yalnız 6 px yüksek: alt köşeleri hafifçe yuvarlanır. Bildirimli çentik (seviye 0 ama
    // daha yüksek) ve açık ada için yarıçap arayüzdekiyle aynı.
    let radius = if hit.h <= 10.0 { 3.0 } else { (hit.h / 2.0).min(22.0) };
    win::set_region(hwnd, hit.to_physical(scale), (radius * scale).round() as i32);
}

/// Animasyon bitince geçilecek alan ve kaç ms sonra.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Settle {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub ms: u64,
}

fn valid_rect(x: f64, y: f64, w: f64, h: f64) -> bool {
    let valid = |v: f64, max: f64| v.is_finite() && (0.0..=max).contains(&v);
    valid(x, WINDOW_W) && valid(y, WINDOW_H) && valid(w, WINDOW_W) && valid(h, WINDOW_H)
}

/// Arayüz seviye değiştirdiğinde adanın animasyon boyunca kapladığı alanı ve (varsa) animasyon
/// bitince geçilecek son alanı bildirir. Son alanı imleç iş parçacığı zamanında uygular.
// Tauri komutu: argümanlar arayüzden adlarıyla gelir, yapıya toplamak çağrıyı değiştirirdi.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn island_update(
    app: AppHandle,
    shared: State<'_, Shared>,
    level: u8,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    settle: Option<Settle>,
) -> Result<(), String> {
    if !PAGE_READY.swap(true, Ordering::Relaxed) {
        // Yakınlaştırma sayfa yüklenmeden verildiyse yükleme onu sıfırlamış olabilir.
        ZOOM_PCT.store(0, Ordering::Relaxed);
        if let Some(window) = app.get_webview_window(LABEL) {
            sync_zoom(&window);
        }
    }
    if level > 2 || !valid_rect(x, y, w, h) || settle.is_some_and(|f| !valid_rect(f.x, f.y, f.w, f.h) || f.ms > 5_000) {
        return Err("Geçersiz ada geometrisi".into());
    }
    let hit = HitRect { x, y, w, h };
    let (hwnd, wake_memory) = shared.with(|s| {
        if s.level != level {
            s.level_since = Instant::now();
        }
        s.level = level;
        s.hit = hit;
        s.settle = settle.map(|f| (Instant::now() + Duration::from_millis(f.ms), HitRect { x: f.x, y: f.y, w: f.w, h: f.h }));
        let wake = level > 0 && s.memory_low;
        if wake {
            s.memory_low = false;
        }
        (s.hwnd, wake)
    });
    if wake_memory {
        set_memory_low(&app, false);
    }
    if let Some(hwnd) = hwnd {
        apply_region(hwnd, hit);
    }
    if level == 0 {
        set_typing(&app, &shared, false);
    }
    crate::shelf::catcher::level_changed(level);
    crate::osd::island_level(level);
    shared.wake();
    Ok(())
}

/// Yazma kipi: adadaki bir metin kutusuna yazılacakken ada geçici olarak odak alır; çıkınca
/// (ya da ada kapanınca) eski hâline döner ve odak önceki pencereye geri verilir.
fn set_typing(app: &AppHandle, shared: &Shared, on: bool) {
    let Some(hwnd) = shared.with(|s| s.hwnd) else { return };
    if on {
        let first = shared.with(|s| s.typing.is_none());
        if first {
            let previous = win::foreground().filter(|w| *w != hwnd);
            shared.with(|s| s.typing = Some(previous));
            win::set_activatable(hwnd, true);
        }
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.set_focus();
        }
    } else if let Some(previous) = shared.with(|s| s.typing.take()) {
        win::set_activatable(hwnd, false);
        if let Some(previous) = previous {
            win::set_foreground(previous);
        }
    }
}

/// İmleç şeride geldi (ada henüz açılmadı): hapta dinlenirken WebView2'ye bırakılan önbellekler,
/// üzerine gelme beklemesi sürerken geri istenir. Böylece ilk açılış karesi boş önbellekle başlamaz.
/// Sayaç da sıfırlanır: bekleme sırasında bakım turu belleği yeniden düşürmesin.
#[tauri::command]
pub fn island_prewarm(app: AppHandle, shared: State<'_, Shared>) {
    let wake = shared.with(|s| {
        s.level_since = Instant::now();
        std::mem::take(&mut s.memory_low)
    });
    if wake {
        set_memory_low(&app, false);
    }
}

/// İmleç gerçekten adanın (animasyon sürüyorsa varacağı) alanında mı. Ada büyürken imleç henüz küçük
/// olan öğeden çıkıp büyüyen alanda durursa WebView2 yeniden "girdi" olayı göndermez; arayüz kapatmadan
/// önce bunu sorar.
#[tauri::command]
pub fn island_pointer_inside(shared: State<'_, Shared>) -> bool {
    let (hwnd, target) = shared.with(|s| (s.hwnd, s.settle.map_or(s.hit, |(_, hit)| hit)));
    let Some(hwnd) = hwnd else { return false };
    match (win::cursor(), win::window_rect(hwnd)) {
        (Some((cx, cy)), Some(window)) => target.to_physical(scale(hwnd)).offset(window.left, window.top).inflate(2, 2).contains(cx, cy),
        _ => false,
    }
}

#[tauri::command]
pub fn island_typing(app: AppHandle, shared: State<'_, Shared>, on: bool) {
    set_typing(&app, &shared, on);
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let window = app.get_webview_window(LABEL).expect("ada penceresi yapılandırmada tanımlı");
    let hwnd = Hwnd::new(window.hwnd()?);
    let shared = app.state::<Shared>().inner().clone();
    let _ = SHARED.set(shared.clone());
    shared.with(|s| s.hwnd = Some(hwnd));
    configure(app, &crate::settings::current(app).island);
    relayout(app);
    // Görünürlüğü ilk kez tao'ya bildiriyoruz (odak almadan gösterir); sonrası doğrudan Win32.
    window.show()?;
    win::make_tool_window(hwnd);
    win::suppress_frame_paint(hwnd);
    win::watch_display_changes(hwnd, display_changed);
    // Başka "hep üstte" pencere öne gelince ada arkada kalmasın (Faz 7.4).
    win::keep_on_top(hwnd);
    crate::power::init(app, hwnd, crate::settings::current(app).power_wanted());
    pointer::spawn(app.clone(), hwnd, shared);
    crate::shelf::catcher::install(app, hwnd);
    let drops = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::DragDrop(drag) = event {
            crate::shelf::on_drag(&drops, drag);
        }
    });
    Ok(())
}

/// Ayarlardan: ada boyutu ve hangi ekranda. Değiştiyse yeniden yerleşme istenir.
pub fn configure(app: &AppHandle, prefs: &IslandPrefs) {
    let size = display::clamp_size(prefs.size);
    let mode = Mode::parse(&prefs.screen);
    let resized = SIZE_PCT.swap(size, Ordering::Relaxed) != size;
    let shared = app.state::<Shared>();
    let changed = shared.with(|s| {
        let changed = resized || s.screen != mode || s.screen_id != prefs.screen_id;
        s.screen = mode;
        s.screen_id.clone_from(&prefs.screen_id);
        s.relayout |= changed;
        changed
    });
    if changed {
        shared.wake();
    }
}

/// Ekran takıldı/çıkarıldı, çözünürlük ya da ölçek değişti (ada penceresinin Windows mesajı).
fn display_changed() {
    if let Some(shared) = SHARED.get() {
        shared.with(|s| s.relayout = true);
        shared.wake();
    }
}

/// Adayı seçilen ekranın üst ortasına yerleştirir; boyutu ekran ölçeği ve ada boyutuyla hesaplanır.
/// Yeri zaten doğruysa pencereye dokunmaz (kendi taşımamızın ölçek mesajı döngü kurmaz).
/// Adanın şimdi durduğu ekranı döndürür.
pub fn relayout(app: &AppHandle) -> Option<Rect> {
    let window = app.get_webview_window(LABEL)?;
    let shared = app.state::<Shared>();
    let (hwnd, mode, id, level) = shared.with(|s| {
        s.relayout = false;
        (s.hwnd, s.screen, s.screen_id.clone(), s.level)
    });
    let hwnd = hwnd?;
    let monitors = win::monitors();
    // İmleç kipinde açık ada yer değiştirmez: o an durduğu ekran seçilir.
    let point = match (mode, level) {
        (Mode::Cursor, 0) => win::cursor(),
        (Mode::Cursor, _) => win::window_rect(hwnd).map(|r| ((r.left + r.right) / 2, r.top)),
        _ => None,
    };
    let monitor = display::choose(&monitors, mode, &id, point)?;
    let target = display::placement(monitor, WINDOW_W, WINDOW_H, size_factor());
    if win::window_rect(hwnd) != Some(target) {
        let _ = move_to(&window, target);
    }
    sync_zoom(&window);
    apply_region(hwnd, shared.with(|s| s.hit));
    crate::shelf::catcher::relayout();
    Some(monitor.rect)
}

/// Başka ölçekli ekrana geçerken Windows pencereyi kendi önerdiği boyuta getirir; konum, boyut ve
/// yeniden konum sırasıyla son söz bizde kalır.
fn move_to(window: &WebviewWindow, r: Rect) -> tauri::Result<()> {
    let position = PhysicalPosition::new(r.left, r.top);
    window.set_position(position)?;
    window.set_size(PhysicalSize::new((r.right - r.left) as u32, (r.bottom - r.top) as u32))?;
    window.set_position(position)
}

/// Arayüzü ada boyutu kadar büyütür (WebView2 yakınlaştırması: yazılar keskin çizilir, yerleşim
/// arayüz pikseliyle aynı kalır).
fn sync_zoom(window: &WebviewWindow) {
    let size = SIZE_PCT.load(Ordering::Relaxed);
    if ZOOM_PCT.swap(size, Ordering::Relaxed) != size && window.set_zoom(f64::from(size) / 100.0).is_err() {
        ZOOM_PCT.store(0, Ordering::Relaxed);
    }
}

/// Ayarlar → Ada → "Hangi ekranda" listesi.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    pub id: String,
    pub label: String,
    pub primary: bool,
}

#[tauri::command]
pub fn island_screens() -> Vec<Screen> {
    let mut monitors = win::monitors();
    monitors.sort_by_key(|m| (m.rect.left, m.rect.top));
    monitors.iter().map(|m| Screen { id: m.id.clone(), label: display::label(m), primary: m.primary }).collect()
}

/// Tepsiden gizle/göster. Görünürlüğü imleç iş parçacığı uygular.
pub fn toggle_user_hidden(app: &AppHandle) -> bool {
    let shared = app.state::<Shared>();
    let hidden = shared.with(|s| {
        s.user_hidden = !s.user_hidden;
        s.user_hidden
    });
    shared.wake();
    hidden
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_rect_scales_with_dpi_and_island_size() {
        // %150 ekran × %130 ada: arayüzün bildirdiği alan aynı çarpanla fiziksele çevrilir.
        let hit = HitRect { x: 278.0, y: 0.0, w: 184.0, h: 32.0 };
        assert_eq!(hit.to_physical(1.5 * 1.3), Rect { left: 542, top: 0, right: 901, bottom: 62 });
        let window = display::placement(
            &display::Monitor { id: "a".into(), rect: Rect { left: 0, top: 0, right: 2560, bottom: 1440 }, scale: 1.5, primary: true },
            WINDOW_W,
            WINDOW_H,
            1.3,
        );
        let full = HitRect { x: 0.0, y: 0.0, w: WINDOW_W, h: WINDOW_H }.to_physical(1.5 * 1.3);
        assert_eq!((full.right, full.bottom), (window.right - window.left, window.bottom - window.top));
    }

    #[test]
    fn hit_rect_scales_with_dpi() {
        let hit = HitRect { x: 278.0, y: 0.0, w: 184.0, h: 32.0 };
        let r = hit.to_physical(1.5);
        assert_eq!(r, Rect { left: 417, top: 0, right: 693, bottom: 48 });
        assert!(r.contains(500, 10));
        assert!(!r.contains(500, 48));
    }
}
