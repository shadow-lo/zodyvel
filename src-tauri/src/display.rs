//! Ada hangi ekranda ve nerede durur: saf hesaplar (Win32 çağrısı yok, testli). Ekran listesini
//! `win::monitors` okur; burada yalnız seçim, yerleşim ve tam ekran kararı verilir.

use crate::win::Rect;

/// Bir ekran (fiziksel piksel). `id` Windows'un aygıt adıdır (`\\.\DISPLAY2`); ayarlarda saklanır.
#[derive(Clone, Debug, PartialEq)]
pub struct Monitor {
    pub id: String,
    pub rect: Rect,
    /// Windows ölçeği (1.0 = %100, 1.5 = %150).
    pub scale: f64,
    pub primary: bool,
}

/// Ayarlar → Ada → "Hangi ekranda".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Primary,
    /// Seçilen ekran; çıkarılırsa ada birincile geçer, takılınca geri döner.
    Fixed,
    /// İmlecin bulunduğu ekran (ada kapalıyken izlenir; açık ada yer değiştirmez).
    Cursor,
}

impl Mode {
    pub const NAMES: [&'static str; 3] = ["primary", "fixed", "cursor"];

    pub fn parse(name: &str) -> Mode {
        match name {
            "fixed" => Mode::Fixed,
            "cursor" => Mode::Cursor,
            _ => Mode::Primary,
        }
    }
}

/// Ada boyutu yüzdesi: Ayarlar → Ada → "Boyut" (src/settings/pages/IslandPage.svelte ile aynı aralık).
pub const SIZE_MIN: u32 = 85;
pub const SIZE_MAX: u32 = 130;
pub const SIZE_STEP: u32 = 5;

/// Yüzdeyi aralığa ve adıma oturtur (elle düzenlenmiş ayar dosyası için).
pub fn clamp_size(percent: u32) -> u32 {
    let stepped = (percent + SIZE_STEP / 2) / SIZE_STEP * SIZE_STEP;
    stepped.clamp(SIZE_MIN, SIZE_MAX)
}

/// Adanın duracağı ekran. Bulunamazsa (seçilen ekran çıkarıldı, imleç ekranlar arası boşlukta)
/// birincil; birincil işaretli değilse ilk ekran.
pub fn choose<'a>(monitors: &'a [Monitor], mode: Mode, fixed: &str, cursor: Option<(i32, i32)>) -> Option<&'a Monitor> {
    let primary = monitors.iter().find(|m| m.primary).or_else(|| monitors.first());
    let wanted = match mode {
        Mode::Primary => None,
        Mode::Fixed => monitors.iter().find(|m| m.id == fixed),
        Mode::Cursor => cursor.and_then(|(x, y)| monitors.iter().find(|m| m.rect.contains(x, y))),
    };
    wanted.or(primary)
}

/// Ada penceresinin fiziksel dikdörtgeni: ekranın üst ortası. Boyut mantıksal pencere × ekran
/// ölçeği × ada boyutu; pencere bölgesi de aynı çarpanla hesaplanır (`island::scale`).
pub fn placement(monitor: &Monitor, logical_w: f64, logical_h: f64, size: f64) -> Rect {
    let w = (logical_w * monitor.scale * size).round() as i32;
    let h = (logical_h * monitor.scale * size).round() as i32;
    let left = monitor.rect.left + (monitor.rect.right - monitor.rect.left - w) / 2;
    let top = monitor.rect.top;
    Rect { left, top, right: left + w, bottom: top + h }
}

/// Öndeki pencere adanın ekranını tamamen kaplıyor mu (oyun, video, sunum, F11). Kenarlıklı ve
/// büyütülmüş pencere sayılmaz: görev çubuğu olmayan ikinci ekranda büyütülen pencere de ekranı
/// (birkaç piksel taşarak) kaplar ama tam ekran değildir.
pub fn covers(window: Rect, monitor: Rect, maximized_with_caption: bool) -> bool {
    !maximized_with_caption
        && window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

/// Ayarlardaki liste için ad: "Ekran 2 · 2560×1440 · %150".
pub fn label(monitor: &Monitor) -> String {
    let number = monitor.id.trim_start_matches(|c: char| !c.is_ascii_digit());
    let name = if number.is_empty() { "Ekran".to_string() } else { format!("Ekran {number}") };
    let w = monitor.rect.right - monitor.rect.left;
    let h = monitor.rect.bottom - monitor.rect.top;
    format!("{name} · {w}×{h} · %{}", (monitor.scale * 100.0).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: &str, left: i32, top: i32, w: i32, h: i32, scale: f64, primary: bool) -> Monitor {
        Monitor { id: id.into(), rect: Rect { left, top, right: left + w, bottom: top + h }, scale, primary }
    }

    fn setup() -> Vec<Monitor> {
        // İkinci ekran solda (eksi koordinat), %150 ölçekli.
        vec![monitor(r"\\.\DISPLAY1", 0, 0, 1920, 1080, 1.0, true), monitor(r"\\.\DISPLAY2", -2560, -200, 2560, 1440, 1.5, false)]
    }

    #[test]
    fn primary_mode_and_fallbacks() {
        let m = setup();
        assert_eq!(choose(&m, Mode::Primary, "", None).unwrap().id, r"\\.\DISPLAY1");
        assert_eq!(choose(&m, Mode::Fixed, r"\\.\DISPLAY2", None).unwrap().id, r"\\.\DISPLAY2");
        // Seçilen ekran çıkarıldı: birincile dön.
        assert_eq!(choose(&m, Mode::Fixed, r"\\.\DISPLAY7", None).unwrap().id, r"\\.\DISPLAY1");
        // Birincil işaretli değilse ilk ekran; hiç ekran yoksa hiçbiri.
        let unmarked = vec![monitor("a", 0, 0, 800, 600, 1.0, false), monitor("b", 800, 0, 800, 600, 1.0, false)];
        assert_eq!(choose(&unmarked, Mode::Primary, "", None).unwrap().id, "a");
        assert!(choose(&[], Mode::Cursor, "", Some((0, 0))).is_none());
    }

    #[test]
    fn cursor_mode_follows_the_pointer() {
        let m = setup();
        assert_eq!(choose(&m, Mode::Cursor, "", Some((-100, 500))).unwrap().id, r"\\.\DISPLAY2");
        assert_eq!(choose(&m, Mode::Cursor, "", Some((100, 500))).unwrap().id, r"\\.\DISPLAY1");
        // Ekranlar arası boşluk ya da imleç okunamadı: birincil.
        assert_eq!(choose(&m, Mode::Cursor, "", Some((-100, 1300))).unwrap().id, r"\\.\DISPLAY1");
        assert_eq!(choose(&m, Mode::Cursor, "", None).unwrap().id, r"\\.\DISPLAY1");
    }

    #[test]
    fn placement_centers_on_top_edge_with_scale_and_size() {
        let m = setup();
        assert_eq!(placement(&m[0], 740.0, 460.0, 1.0), Rect { left: 590, top: 0, right: 1330, bottom: 460 });
        // %150 ekran, %130 ada: 740 × 1,95 = 1443.
        let r = placement(&m[1], 740.0, 460.0, 1.3);
        assert_eq!((r.right - r.left, r.bottom - r.top), (1443, 897));
        assert_eq!(r.top, -200);
        assert_eq!(r.left, -2560 + (2560 - 1443) / 2);
        let small = placement(&m[0], 740.0, 460.0, 0.85);
        assert_eq!((small.right - small.left, small.bottom - small.top), (629, 391));
    }

    #[test]
    fn fullscreen_needs_full_cover_without_caption() {
        let screen = Rect { left: -2560, top: -200, right: 0, bottom: 1240 };
        assert!(covers(screen, screen, false));
        // Oyunlar çoğu zaman birkaç piksel taşar.
        assert!(covers(screen.inflate(8, 8), screen, false));
        // Görev çubuğu olmayan ekranda büyütülmüş, başlıklı pencere.
        assert!(!covers(screen.inflate(8, 8), screen, true));
        // Büyük ama tam kaplamayan pencere.
        assert!(!covers(Rect { bottom: 1200, ..screen }, screen, false));
    }

    #[test]
    fn size_is_clamped_to_steps() {
        assert_eq!(clamp_size(100), 100);
        assert_eq!(clamp_size(50), SIZE_MIN);
        assert_eq!(clamp_size(500), SIZE_MAX);
        assert_eq!(clamp_size(112), 110);
        assert_eq!(clamp_size(113), 115);
    }

    #[test]
    fn labels_and_modes() {
        assert_eq!(label(&setup()[1]), "Ekran 2 · 2560×1440 · %150");
        assert_eq!(Mode::parse("cursor"), Mode::Cursor);
        assert_eq!(Mode::parse("başka"), Mode::Primary);
        assert!(Mode::NAMES.iter().all(|n| *n == "primary" || Mode::parse(n) != Mode::Primary));
    }
}
