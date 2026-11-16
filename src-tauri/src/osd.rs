//! Zody ses göstergesi (Faz 10; md/yeni-plan-2.md, eskiz design/eskiz-paket3.html ekran 4).
//!
//! Ayar (Ayarlar › Ada › "Ses tuşlarında Zody göstergesi", varsayılan kapalı) kapalıyken burada hiçbir şey
//! çalışmaz: kanca kurulmaz, iş parçacığı açılmaz, ses aygıtına bildirim kaydı yapılmaz; her şey Windows'ta.
//! Açıkken iki iş parçacığı:
//! - Kanca (`zodyvel-volume-keys`): düşük düzeyli klavye kancası (`WH_KEYBOARD_LL`) yalnız üç ses tuşuna bakar
//!   (artır, azalt, sessiz). Tuşu Windows'a iletmez, böylece Windows'un kendi ses penceresi çıkmaz. Kanca hiçbir
//!   işi beklemez: tuşu kanala bırakıp hemen döner (yavaş kancayı Windows sessizce kaldırır). Diğer tuşlara
//!   dokunulmaz, kaydedilmez.
//!   Ada gizliyken (tam ekran uygulama, kullanıcı gizledi) ya da açıkken tuş olduğu gibi Windows'a gider:
//!   gösterge çentikte çıkamaz, Windows'unki görünsün.
//! - Ses (`zodyvel-volume`): Core Audio ile varsayılan çıkışın düzeyini 2'şer değiştirir (Windows gibi çift
//!   sayılara oturur), sessizi çevirir ve adaya `volume://osd` gönderir. Başka yoldan değişen ses (fare tekerleği,
//!   başka uygulama, Windows'un kaydırıcısı) `IAudioEndpointVolumeCallback` ile gelir, aynı olay gider.
//!   Varsayılan çıkış değişince (`IMMNotificationClient`) kayıt yeni aygıta taşınır.
//!
//! Bazı klavyeler ses tuşunu uygulama komutu (HID) olarak gönderir; o zaman kanca tuşu görmez, Windows'un
//! penceresi de çıkar ama ses değiştiği için bizim gösterge de açılır.

use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{LPARAM, LRESULT, PROPERTYKEY, WPARAM};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN, WM_USER,
};
use windows::core::{GUID, PCWSTR, implement};

use crate::{debuglog, island};

/// Kendi değişikliğimizin olay bağlamı: aygıt bildirimi geri geldiğinde tanınır (gösterge zaten gönderildi).
const OURS: GUID = GUID::from_u128(0x7a3c_51e2_9d4b_4f06_8b1e_2c5d_0f9a_6e31);

/// Adaya giden olay (src/lib/types.ts VolumeOsd ile aynı).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Osd {
    /// 0–100
    pub level: u8,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Up,
    Down,
    Mute,
}

enum Cmd {
    Key(Key),
    /// Varsayılan çıkış değişti: ses bildirimi yeni aygıta taşınır.
    Rebind,
    Stop,
}

struct Running {
    hook_thread: u32,
    tx: Sender<Cmd>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);
/// Ada gizli (tam ekran, kullanıcı gizledi) ya da açık: tuşlar Windows'a gider.
static HIDDEN: AtomicBool = AtomicBool::new(false);
static OPEN: AtomicBool = AtomicBool::new(false);
/// Sessiz tuşu basılı: tekrar eden basılar (tuşu basılı tutmak) sessizi açıp kapatmasın.
static MUTE_HELD: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Kanca geri çağrısı kanca iş parçacığında çalışır: kanal oraya aittir, kilit gerekmez.
    static KEYS: RefCell<Option<Sender<Cmd>>> = const { RefCell::new(None) };
}

/// Ayar değişince (ve açılışta): açıksa kanca ve ses iş parçacığı kurulur, kapalıysa ikisi de kapanır.
pub fn configure(app: &AppHandle, on: bool) {
    let mut running = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
    match (on, running.is_some()) {
        (true, false) => *running = start(app.clone()),
        (false, true) => {
            if let Some(r) = running.take() {
                stop(r);
            }
        }
        _ => {}
    }
}

/// İmleç iş parçacığından: ada görünür mü (tam ekran uygulamada ya da kullanıcı gizleyince gizli).
pub fn island_visible(visible: bool) {
    HIDDEN.store(!visible, Ordering::Relaxed);
}

/// Arayüzün geometri bildiriminden: ada açık mı (seviye 1–2).
pub fn island_level(level: u8) {
    OPEN.store(level > 0, Ordering::Relaxed);
}

fn start(app: AppHandle) -> Option<Running> {
    let (tx, rx) = mpsc::channel();
    let watch = tx.clone();
    if let Err(e) = thread::Builder::new().name("zodyvel-volume".into()).spawn(move || audio_loop(app, rx, watch)) {
        debuglog::line(format!("osd: ses iş parçacığı açılamadı: {e}"));
        return None;
    }
    let (ready_tx, ready_rx) = mpsc::channel();
    let keys = tx.clone();
    let spawned = thread::Builder::new().name("zodyvel-volume-keys".into()).spawn(move || hook_loop(keys, ready_tx));
    let hook_thread = match spawned.ok().and_then(|_| ready_rx.recv().ok().flatten()) {
        Some(id) => id,
        None => {
            debuglog::line("osd: klavye kancası kurulamadı; ses tuşları Windows'ta kalır");
            let _ = tx.send(Cmd::Stop);
            return None;
        }
    };
    Some(Running { hook_thread, tx })
}

fn stop(running: Running) {
    unsafe {
        let _ = PostThreadMessageW(running.hook_thread, WM_QUIT, WPARAM(0), LPARAM(0));
    }
    let _ = running.tx.send(Cmd::Stop);
}

fn hook_loop(keys: Sender<Cmd>, ready: Sender<Option<u32>>) {
    KEYS.with(|k| *k.borrow_mut() = Some(keys));
    unsafe {
        let mut msg = MSG::default();
        // İş parçacığının mesaj kuyruğu oluşsun: kapatma mesajı (WM_QUIT) kaybolmaz.
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
        let module = GetModuleHandleW(None).ok().map(Into::into);
        let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), module, 0) else {
            let _ = ready.send(None);
            return;
        };
        let _ = ready.send(Some(GetCurrentThreadId()));
        // Kanca geri çağrıları bu döngüde gelir; arada iş parçacığı uyur.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
        let _ = UnhookWindowsHookEx(hook);
    }
}

fn key_of(vk: u32) -> Option<Key> {
    match vk {
        v if v == u32::from(VK_VOLUME_UP.0) => Some(Key::Up),
        v if v == u32::from(VK_VOLUME_DOWN.0) => Some(Key::Down),
        v if v == u32::from(VK_VOLUME_MUTE.0) => Some(Key::Mute),
        _ => None,
    }
}

/// Basış işlenecek mi: sessiz tuşunda yalnız ilk basış (basılı tutunca tekrar etmez); ses tuşlarında her
/// tekrar (basılı tutunca çubuk akıcı dolar).
fn accept(key: Key, down: bool) -> bool {
    if key != Key::Mute {
        return down;
    }
    if !down {
        MUTE_HELD.store(false, Ordering::Relaxed);
        return false;
    }
    !MUTE_HELD.swap(true, Ordering::Relaxed)
}

unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if let Some(key) = key_of(info.vkCode) {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let take = accept(key, down);
            if !HIDDEN.load(Ordering::Relaxed) && !OPEN.load(Ordering::Relaxed) {
                if take {
                    KEYS.with(|k| {
                        if let Some(tx) = k.borrow().as_ref() {
                            let _ = tx.send(Cmd::Key(key));
                        }
                    });
                }
                // Windows'a iletilmez: kendi ses penceresi çıkmaz.
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn audio_loop(app: AppHandle, rx: Receiver<Cmd>, watch: Sender<Cmd>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mut audio = Audio::new(app, watch);
    'outer: while let Ok(first) = rx.recv() {
        // Basılı tutunca biriken basılar tek seferde işlenir, gösterge bir kez gönderilir.
        let mut cmd = first;
        let mut pressed = false;
        loop {
            match cmd {
                Cmd::Key(key) => {
                    audio.press(key);
                    pressed = true;
                }
                Cmd::Rebind => audio.rebind(),
                Cmd::Stop => break 'outer,
            }
            cmd = match rx.try_recv() {
                Ok(next) => next,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
        }
        if pressed {
            audio.emit();
        }
    }
    drop(audio);
    unsafe { CoUninitialize() };
}

struct Audio {
    app: AppHandle,
    enumerator: Option<IMMDeviceEnumerator>,
    client: Option<IMMNotificationClient>,
    callback: IAudioEndpointVolumeCallback,
    endpoint: Option<IAudioEndpointVolume>,
}

impl Audio {
    fn new(app: AppHandle, watch: Sender<Cmd>) -> Self {
        let enumerator: Option<IMMDeviceEnumerator> = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() };
        let client = enumerator.as_ref().and_then(|e| {
            let client: IMMNotificationClient = DefaultWatch { watch }.into();
            unsafe { e.RegisterEndpointNotificationCallback(&client).ok()? };
            Some(client)
        });
        let callback: IAudioEndpointVolumeCallback = Changes { app: app.clone() }.into();
        let mut audio = Self { app, enumerator, client, callback, endpoint: None };
        audio.rebind();
        audio
    }

    fn rebind(&mut self) {
        self.unbind();
        let Some(enumerator) = &self.enumerator else { return };
        let endpoint = unsafe {
            enumerator.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|device| device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None))
        };
        let Ok(endpoint) = endpoint else { return };
        unsafe {
            let _ = endpoint.RegisterControlChangeNotify(&self.callback);
        }
        self.endpoint = Some(endpoint);
    }

    fn unbind(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            unsafe {
                let _ = endpoint.UnregisterControlChangeNotify(&self.callback);
            }
        }
    }

    fn press(&mut self, key: Key) {
        if self.endpoint.is_none() || self.apply(key).is_err() {
            // Aygıt çıkarılmış ya da değişmiş olabilir: yeniden bağlanıp bir kez daha denenir.
            self.rebind();
            let _ = self.apply(key);
        }
    }

    fn apply(&self, key: Key) -> windows::core::Result<()> {
        let Some(endpoint) = &self.endpoint else { return Ok(()) };
        unsafe {
            let level = endpoint.GetMasterVolumeLevelScalar()?;
            let muted = endpoint.GetMute()?.as_bool();
            match key {
                Key::Mute => endpoint.SetMute(!muted, &OURS)?,
                Key::Up | Key::Down => {
                    endpoint.SetMasterVolumeLevelScalar(stepped(level, key == Key::Up), &OURS)?;
                    // Windows gibi: sessizken ses tuşu sesi açar.
                    if muted {
                        endpoint.SetMute(false, &OURS)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn emit(&self) {
        let Some(endpoint) = &self.endpoint else { return };
        let read = unsafe { endpoint.GetMasterVolumeLevelScalar().and_then(|level| Ok((level, endpoint.GetMute()?.as_bool()))) };
        if let Ok((level, muted)) = read {
            send(&self.app, level, muted);
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.unbind();
        if let (Some(enumerator), Some(client)) = (&self.enumerator, self.client.take()) {
            unsafe {
                let _ = enumerator.UnregisterEndpointNotificationCallback(&client);
            }
        }
    }
}

fn send(app: &AppHandle, level: f32, muted: bool) {
    let _ = app.emit_to(island::LABEL, "volume://osd", Osd { level: percent(level), muted });
}

fn percent(level: f32) -> u8 {
    (level * 100.0).round().clamp(0.0, 100.0) as u8
}

/// Bir adım: 2'şer, Windows gibi çift sayılara oturur (37 → 38 ya da 36), 0–100 arasında kalır.
fn stepped(level: f32, up: bool) -> f32 {
    let now = i32::from(percent(level));
    let next = if up { (now / 2 + 1) * 2 } else { ((now + 1) / 2 - 1) * 2 };
    next.clamp(0, 100) as f32 / 100.0
}

/// Başka yoldan değişen ses (fare tekerleği, başka uygulama, Windows'un kaydırıcısı).
#[implement(IAudioEndpointVolumeCallback)]
struct Changes {
    app: AppHandle,
}

impl IAudioEndpointVolumeCallback_Impl for Changes_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let Some(data) = (unsafe { data.as_ref() }) else { return Ok(()) };
        // Kendi değişikliğimiz: ses iş parçacığı göstergeyi zaten gönderiyor.
        if data.guidEventContext != OURS {
            send(&self.app, data.fMasterVolume, data.bMuted.as_bool());
        }
        Ok(())
    }
}

/// Varsayılan çıkış aygıtı değişince ses bildirimi yeni aygıta taşınır.
#[implement(IMMNotificationClient)]
struct DefaultWatch {
    watch: Sender<Cmd>,
}

impl IMMNotificationClient_Impl for DefaultWatch_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            let _ = self.watch.send(Cmd::Rebind);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_by_two_on_even_numbers() {
        let up = |p: u8| percent(stepped(f32::from(p) / 100.0, true));
        let down = |p: u8| percent(stepped(f32::from(p) / 100.0, false));
        assert_eq!((up(36), up(37), up(99), up(100)), (38, 38, 100, 100));
        assert_eq!((down(36), down(37), down(1), down(0)), (34, 36, 0, 0));
    }

    #[test]
    fn only_volume_keys_are_caught() {
        assert_eq!(key_of(0xAF), Some(Key::Up));
        assert_eq!(key_of(0xAE), Some(Key::Down));
        assert_eq!(key_of(0xAD), Some(Key::Mute));
        assert_eq!(key_of(0xB3), None); // oynat/duraklat
        assert_eq!(key_of(u32::from(b'A')), None);
    }

    #[test]
    fn held_mute_toggles_once() {
        MUTE_HELD.store(false, Ordering::Relaxed);
        assert!(accept(Key::Mute, true));
        assert!(!accept(Key::Mute, true));
        assert!(!accept(Key::Mute, false));
        assert!(accept(Key::Mute, true));
        assert!(accept(Key::Up, true) && accept(Key::Up, true));
        assert!(!accept(Key::Up, false));
    }
}
