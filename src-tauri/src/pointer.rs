//! Adanın tek arka plan iş parçacığı.
//!
//! Boştayken (seviye 0) imleç hiç yoklanmaz: üzerine gelme pencere bölgesi sayesinde arayüzün
//! kendi fare olaylarıyla algılanır. İş parçacığı 2 saniyede bir adanın ekranında tam ekran uygulama
//! olup olmadığına (imleç kipinde imlecin hangi ekranda olduğuna da) bakıp uyur. Ekran takılması,
//! çözünürlük ve ölçek değişimi Windows mesajıyla gelir (island::display_changed) ve iş parçacığını uyandırır. Ada açıkken (odak almadığı için klavye ve "dışarı tıklama" olayı gelmez)
//! dışarı tıklama ve Esc, düğme durumu sorgulanarak yakalanır. Kanca (hook) kurulmaz.

use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::display::Mode;
use crate::island::{self, Shared};
use crate::win::{self, Hwnd, Rect};

const OPEN_TICK: Duration = Duration::from_millis(30);
const MAINTENANCE: Duration = Duration::from_secs(2);
const REST_BEFORE_TRIM: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Serialize)]
struct PointerMsg {
    kind: &'static str,
}

#[derive(Clone, Copy, Serialize)]
struct VisibilityMsg {
    visible: bool,
}

pub fn spawn(app: AppHandle, hwnd: Hwnd, shared: Shared) {
    let tracker = Tracker::new(app, hwnd, shared.clone());
    let handle = thread::Builder::new()
        .name("zodyvel-pointer".into())
        .spawn(move || tracker.run())
        .expect("imleç iş parçacığı başlatılamadı");
    shared.with(|s| s.poller = Some(handle.thread().clone()));
}

struct Tracker {
    app: AppHandle,
    hwnd: Hwnd,
    shared: Shared,
    visible: bool,
    fullscreen: bool,
    /// Adanın durduğu ekran (fiziksel piksel).
    monitor: Option<Rect>,
    mouse_was_down: bool,
    esc_was_down: bool,
    /// Bir önceki adımda ada açıktı. Açılış anında zaten basılı olan tuş (ör. adaya dosya
    /// sürüklenirken) "dışarı tıklama" sayılmaz.
    was_open: bool,
    last_maintenance: Instant,
}

impl Tracker {
    fn new(app: AppHandle, hwnd: Hwnd, shared: Shared) -> Self {
        Self {
            app,
            hwnd,
            shared,
            visible: true,
            fullscreen: false,
            monitor: win::monitor_of(hwnd),
            mouse_was_down: false,
            esc_was_down: false,
            was_open: false,
            last_maintenance: Instant::now(),
        }
    }

    fn run(mut self) {
        loop {
            let wait = self.tick().min(self.settle());
            thread::park_timeout(wait);
        }
    }

    /// Animasyon bittiyse son alanı uygular; değilse bitişine kalan süreyi döndürür.
    fn settle(&self) -> Duration {
        let now = Instant::now();
        let due = self.shared.with(|s| match s.settle {
            Some((at, hit)) if at <= now => {
                s.settle = None;
                s.hit = hit;
                Ok(hit)
            }
            Some((at, _)) => Err(at - now),
            None => Err(Duration::MAX),
        });
        match due {
            Ok(hit) => {
                island::apply_region(self.hwnd, hit);
                Duration::MAX
            }
            Err(wait) => wait,
        }
    }

    /// Bir sonraki uyanmaya kadar beklenecek süreyi döndürür.
    fn tick(&mut self) -> Duration {
        if self.shared.with(|s| s.relayout) {
            self.monitor = island::relayout(&self.app).or(self.monitor);
        }
        if self.last_maintenance.elapsed() >= MAINTENANCE {
            self.maintenance();
        }
        let (level, hit, user_hidden) = self.shared.with(|s| (s.level, s.hit, s.user_hidden));

        let should_show = !user_hidden && !self.fullscreen;
        if should_show != self.visible {
            self.set_visible(should_show);
        }
        if !self.visible || level == 0 {
            self.mouse_was_down = false;
            self.esc_was_down = false;
            self.was_open = false;
            self.trim_memory_when_resting();
            return MAINTENANCE.saturating_sub(self.last_maintenance.elapsed());
        }

        if let (Some((cx, cy)), Some(window)) = (win::cursor(), win::window_rect(self.hwnd)) {
            let island = hit.to_physical(island::scale(self.hwnd)).offset(window.left, window.top);
            let inside = island.inflate(2, 2).contains(cx, cy);
            let mouse_down = win::left_button_down();
            if !self.was_open {
                self.was_open = true;
                self.mouse_was_down = mouse_down;
            }
            if mouse_down && !self.mouse_was_down && !inside {
                self.emit("outside");
            }
            self.mouse_was_down = mouse_down;
        }
        let esc_down = win::escape_down();
        if esc_down && !self.esc_was_down {
            self.emit("escape");
        }
        self.esc_was_down = esc_down;
        OPEN_TICK
    }

    /// Ada en az `REST_BEFORE_TRIM` boyunca hapta kaldıysa WebView2'den önbelleklerini bırakmasını iste.
    fn trim_memory_when_resting(&self) {
        let trim = self.shared.with(|s| {
            let resting = s.level == 0 && !s.memory_low && s.level_since.elapsed() >= REST_BEFORE_TRIM;
            if resting {
                s.memory_low = true;
            }
            resting
        });
        if trim {
            island::set_memory_low(&self.app, true);
        }
    }

    fn maintenance(&mut self) {
        self.last_maintenance = Instant::now();
        // İmleç kipi: ada kapalıyken imleç başka ekrana geçtiyse ada oraya taşınır.
        let (mode, level) = self.shared.with(|s| (s.screen, s.level));
        if mode == Mode::Cursor
            && level == 0
            && let (Some((x, y)), Some(monitor)) = (win::cursor(), self.monitor)
            && !monitor.contains(x, y)
        {
            self.monitor = island::relayout(&self.app).or(self.monitor);
        }
        self.fullscreen = self.monitor.is_some_and(|monitor| win::fullscreen_on(monitor, self.hwnd));
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        crate::osd::island_visible(visible);
        if visible {
            win::show_without_focus(self.hwnd);
        } else {
            win::hide(self.hwnd);
        }
        let _ = self.app.emit_to(island::LABEL, "island://visibility", VisibilityMsg { visible });
    }

    fn emit(&self, kind: &'static str) {
        let _ = self.app.emit_to(island::LABEL, "island://pointer", PointerMsg { kind });
    }
}
