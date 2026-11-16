//! Genel klavye kısayolları (`RegisterHotKey`).
//!
//! Kanca kurulmaz, tuş vuruşları izlenmez: sistem yalnız kayıtlı birleşime basıldığında iş
//! parçacığımızın kuyruğuna mesaj bırakır; arada iş parçacığı `GetMessageW` üzerinde uyur.
//! Onay kısayolları yalnız adada karar verilebilen bir onay kartı görünürken kaydedilir; diğer
//! zamanlarda o tuşlar öndeki uygulamaya gider. Aç/küçült, sustur, yazma sahnesi, onaya git ve terminale git
//! her zaman kayıtlıdır (kapalı özelliğin birleşimi `settings::active_shortcuts`ta boşaltılır).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_USER,
};

use crate::{debuglog, island, settings, sound};

/// Ayar değişti / ada onay kartını gösterdi: kayıtları yeniden hesapla.
const WM_SYNC: u32 = WM_APP + 1;
/// Kart yeni göründüyse bu süre dolmadan onay tuşu kabul edilmez (tam o an basılmış tuşla
/// görülmemiş bir isteğin onaylanmasını önler).
const ARM_GRACE: Duration = Duration::from_millis(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Toggle,
    Approve,
    Deny,
    Mute,
    Write,
    /// Bekleyen onay kartını adada aç.
    Approval,
    /// Dikkat isteyen oturumun penceresini öne getir (oturumu ada seçer).
    Terminal,
    /// Acil durdurma: Zody eylemlerini iptal eder.
    Stop,
}

const ACTIONS: [Action; 8] =
    [Action::Toggle, Action::Approve, Action::Deny, Action::Mute, Action::Write, Action::Approval, Action::Terminal, Action::Stop];

impl Action {
    fn id(self) -> i32 {
        self as i32 + 1
    }
    fn from_id(id: usize) -> Option<Self> {
        ACTIONS.get(id.checked_sub(1)?).copied()
    }
    fn name(self) -> &'static str {
        match self {
            Action::Toggle => "toggle",
            Action::Approve => "approve",
            Action::Deny => "deny",
            Action::Mute => "mute",
            Action::Write => "write",
            Action::Approval => "approval",
            Action::Terminal => "terminal",
            Action::Stop => "stop",
        }
    }
    /// Onay kartı beklemeden, her zaman kayıtlı olanlar.
    fn always(self) -> bool {
        matches!(self, Action::Toggle | Action::Mute | Action::Write | Action::Approval | Action::Terminal | Action::Stop)
    }
}

/// Tuş birleşimi: `MOD_*` bitleri ve sanal tuş kodu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Combo {
    mods: u32,
    vk: u32,
}

const NAMED_KEYS: [(u32, &str); 13] = [
    (0x08, "Backspace"),
    (0x0D, "Enter"),
    (0x20, "Space"),
    (0x21, "PageUp"),
    (0x22, "PageDown"),
    (0x23, "End"),
    (0x24, "Home"),
    (0x25, "Left"),
    (0x26, "Up"),
    (0x27, "Right"),
    (0x28, "Down"),
    (0x2D, "Insert"),
    (0x2E, "Delete"),
];

const MODIFIERS: [(u32, &str); 4] =
    [(MOD_CONTROL.0, "Ctrl"), (MOD_ALT.0, "Alt"), (MOD_SHIFT.0, "Shift"), (MOD_WIN.0, "Win")];

fn key_name(vk: u32) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6F)),
        _ => NAMED_KEYS.iter().find(|(code, _)| *code == vk).map(|(_, name)| (*name).to_string()),
    }
}

fn key_code(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(u32::from(bytes[0]));
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then_some(0x6F + n);
    }
    NAMED_KEYS.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(code, _)| *code)
}

impl Combo {
    /// "Ctrl+Alt+Z" biçimini okur. Ctrl, Alt veya Win olmadan (yalnız Shift ya da tek tuş) kabul edilmez:
    /// öyle bir kısayol yazı yazarken her uygulamadan tuş çalardı.
    pub fn parse(text: &str) -> Option<Combo> {
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts.pop()?;
        let mut mods = 0;
        for part in parts {
            let (bit, _) = MODIFIERS.iter().find(|(_, name)| name.eq_ignore_ascii_case(part))?;
            mods |= bit;
        }
        let vk = key_code(key)?;
        (mods & (MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0) != 0).then_some(Combo { mods, vk })
    }

    pub fn format(self) -> String {
        let mut parts: Vec<String> =
            MODIFIERS.iter().filter(|(bit, _)| self.mods & bit != 0).map(|(_, name)| (*name).to_string()).collect();
        parts.extend(key_name(self.vk));
        parts.join("+")
    }
}

/// Ayar dosyasındaki metni tek biçime getirir; geçersizse kısayol kapalı ("") sayılır.
pub fn normalize(text: &str) -> String {
    Combo::parse(text).map(Combo::format).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Off,
    Ok,
    /// Başka bir uygulama aynı birleşimi kaydetmiş.
    Conflict,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StatusSet {
    pub toggle: Status,
    pub approve: Status,
    pub deny: Status,
    pub mute: Status,
    pub write: Status,
    pub approval: Status,
    pub terminal: Status,
    pub stop: Status,
}

impl StatusSet {
    fn slot(&mut self, action: Action) -> &mut Status {
        match action {
            Action::Toggle => &mut self.toggle,
            Action::Approve => &mut self.approve,
            Action::Deny => &mut self.deny,
            Action::Mute => &mut self.mute,
            Action::Write => &mut self.write,
            Action::Approval => &mut self.approval,
            Action::Terminal => &mut self.terminal,
            Action::Stop => &mut self.stop,
        }
    }
}

#[derive(Default)]
struct Want {
    combos: [Option<Combo>; ACTIONS.len()],
    /// Adada görünen, karar verilebilir onay isteği ve göründüğü an.
    target: Option<(String, Instant)>,
    /// Ayarlar penceresi yeni kısayol kaydederken hepsi bırakılır (basılan tuşlar pencereye ulaşsın).
    paused: bool,
    /// Yalnız kart görünürken kaydedilenleri, ayar değişince bir kez dene: çakışma hemen görünsün.
    probe: bool,
}

pub struct Keys {
    app: AppHandle,
    thread: AtomicU32,
    want: Mutex<Want>,
    status: Mutex<StatusSet>,
}

impl Keys {
    fn want(&self) -> std::sync::MutexGuard<'_, Want> {
        self.want.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn wake(&self) {
        let thread = self.thread.load(Ordering::Acquire);
        if thread != 0 {
            let _ = unsafe { PostThreadMessageW(thread, WM_SYNC, WPARAM(0), LPARAM(0)) };
        }
    }

    fn configure(&self, shortcuts: &settings::Shortcuts) {
        {
            let mut want = self.want();
            want.combos = [
                &shortcuts.toggle,
                &shortcuts.approve,
                &shortcuts.deny,
                &shortcuts.mute,
                &shortcuts.write,
                &shortcuts.approval,
                &shortcuts.terminal,
                &shortcuts.stop,
            ]
            .map(|s| Combo::parse(s));
            want.probe = true;
        }
        self.wake();
    }

    pub fn set_paused(&self, paused: bool) {
        let changed = {
            let mut want = self.want();
            let changed = want.paused != paused;
            want.paused = paused;
            changed
        };
        if changed {
            self.wake();
        }
    }
}

pub fn init(app: &AppHandle, shortcuts: &settings::Shortcuts) {
    let keys = Arc::new(Keys {
        app: app.clone(),
        thread: AtomicU32::new(0),
        want: Mutex::new(Want::default()),
        status: Mutex::new(StatusSet::default()),
    });
    keys.configure(shortcuts);
    app.manage(keys.clone());
    thread::Builder::new().name("zodyvel-keys".into()).spawn(move || run(keys)).expect("kısayol iş parçacığı başlatılamadı");
}

pub fn configure(app: &AppHandle, shortcuts: &settings::Shortcuts) {
    if let Some(keys) = app.try_state::<Arc<Keys>>() {
        keys.configure(shortcuts);
    }
}

pub fn set_paused(app: &AppHandle, paused: bool) {
    if let Some(keys) = app.try_state::<Arc<Keys>>() {
        keys.set_paused(paused);
    }
}

fn run(keys: Arc<Keys>) {
    let mut msg = MSG::default();
    unsafe {
        // İş parçacığının mesaj kuyruğu ilk Peek/Get çağrısında oluşur; kimliği ondan sonra yayınlanır.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        keys.thread.store(GetCurrentThreadId(), Ordering::Release);
    }
    let mut registered: [Option<Combo>; ACTIONS.len()] = [None; ACTIONS.len()];
    sync(&keys, &mut registered);
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        match msg.message {
            WM_HOTKEY => {
                if let Some(action) = Action::from_id(msg.wParam.0) {
                    pressed(&keys, action);
                }
            }
            WM_SYNC => sync(&keys, &mut registered),
            _ => {}
        }
    }
}

fn register(action: Action, combo: Combo) -> bool {
    let mods = HOT_KEY_MODIFIERS(combo.mods | MOD_NOREPEAT.0);
    unsafe { RegisterHotKey(None, action.id(), mods, combo.vk) }.is_ok()
}

fn unregister(action: Action) {
    let _ = unsafe { UnregisterHotKey(None, action.id()) };
}

/// İstenen kayıtlarla mevcutları karşılaştırır; yalnız değişenler bırakılıp yeniden kaydedilir.
fn sync(keys: &Keys, registered: &mut [Option<Combo>; ACTIONS.len()]) {
    let (combos, armed, paused, probe) = {
        let mut want = keys.want();
        let probe = std::mem::take(&mut want.probe);
        (want.combos, want.target.is_some(), want.paused, probe)
    };
    let mut status = keys.status.lock().unwrap_or_else(|p| p.into_inner()).clone();
    for (index, action) in ACTIONS.into_iter().enumerate() {
        let combo = combos[index];
        let active = !paused && (action.always() || armed);
        let desired = combo.filter(|_| active);
        if registered[index] != desired {
            if registered[index].take().is_some() {
                unregister(action);
            }
            if let Some(c) = desired
                && register(action, c)
            {
                registered[index] = Some(c);
            }
        }
        let slot = status.slot(action);
        *slot = match (combo, desired) {
            (None, _) => Status::Off,
            (Some(_), Some(_)) => {
                if registered[index].is_some() {
                    Status::Ok
                } else {
                    Status::Conflict
                }
            }
            // Şu an kayıtlı olması gerekmiyor: ayar yeni değiştiyse bir kez dene, değilse son bilinen durum.
            (Some(c), None) if probe && !paused => {
                if register(action, c) {
                    unregister(action);
                    Status::Ok
                } else {
                    Status::Conflict
                }
            }
            (Some(_), None) => {
                if *slot == Status::Off {
                    Status::Ok
                } else {
                    *slot
                }
            }
        };
    }
    let mut current = keys.status.lock().unwrap_or_else(|p| p.into_inner());
    if *current != status {
        *current = status.clone();
        drop(current);
        let _ = keys.app.emit("shortcuts://status", &status);
    }
}

#[derive(Clone, Serialize)]
struct ShortcutMsg {
    action: &'static str,
    id: Option<String>,
}

fn pressed(keys: &Keys, action: Action) {
    let id = match action {
        Action::Toggle | Action::Write | Action::Approval | Action::Terminal => None,
        Action::Stop => {
            debuglog::line("kısayol: acil durdurma");
            crate::actions::access::emergency_stop(&keys.app);
            return;
        }
        Action::Mute => {
            debuglog::line("kısayol: mute");
            sound::toggle(&keys.app);
            return;
        }
        Action::Approve | Action::Deny => {
            let target = keys.want().target.clone();
            match target {
                Some((id, since)) if since.elapsed() >= ARM_GRACE => Some(id),
                _ => {
                    debuglog::line(format!("kısayol yok sayıldı: {}", action.name()));
                    return;
                }
            }
        }
    };
    debuglog::line(format!("kısayol: {}", action.name()));
    let _ = keys.app.emit_to(island::LABEL, "island://shortcut", ShortcutMsg { action: action.name(), id });
}

#[tauri::command]
pub fn shortcuts_status(keys: State<'_, Arc<Keys>>) -> StatusSet {
    keys.status.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Ada, karar verilebilir bir onay kartı gösterirken isteğin kimliğini, kart kalkınca `null` bildirir.
#[tauri::command]
pub fn shortcuts_arm(keys: State<'_, Arc<Keys>>, target: Option<String>) {
    let changed = {
        let mut want = keys.want();
        let was_armed = want.target.is_some();
        if want.target.as_ref().map(|(id, _)| id) != target.as_ref() {
            want.target = target.map(|id| (id, Instant::now()));
        }
        was_armed != want.target.is_some()
    };
    if changed {
        keys.wake();
    }
}

/// Ayarlar penceresi yeni bir birleşim kaydederken kısayolları geçici olarak bırakır.
#[tauri::command]
pub fn shortcuts_pause(keys: State<'_, Arc<Keys>>, paused: bool) {
    keys.set_paused(paused);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_in_fixed_order() {
        let combo = Combo::parse("alt + ctrl + z").expect("geçerli");
        assert_eq!(combo, Combo { mods: MOD_CONTROL.0 | MOD_ALT.0, vk: 0x5A });
        assert_eq!(combo.format(), "Ctrl+Alt+Z");
        assert_eq!(normalize("Ctrl+Alt+enter"), "Ctrl+Alt+Enter");
        assert_eq!(normalize("Win+Shift+F12"), "Shift+Win+F12");
        assert_eq!(normalize("Ctrl+7"), "Ctrl+7");
    }

    #[test]
    fn rejects_unsafe_or_unknown_combos() {
        for text in ["", "Z", "Shift+Z", "Ctrl+", "Ctrl+Alt+Ğ", "Hyper+Z", "Ctrl+F25", "Ctrl+F0", "Ctrl+Tab"] {
            assert_eq!(Combo::parse(text), None, "{text}");
        }
    }

    #[test]
    fn action_ids_round_trip() {
        for action in ACTIONS {
            assert_eq!(Action::from_id(action.id() as usize), Some(action));
        }
        assert_eq!(Action::from_id(0), None);
        assert_eq!(Action::from_id(ACTIONS.len() + 1), None);
    }
}
