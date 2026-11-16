//! Müzik modülü: Windows'un ortak medya kontrolleri (GSMTC). Spotify, tarayıcıdaki YouTube,
//! Medya Oynatıcı ve medya kontrollerini destekleyen her uygulama.
//!
//! Tamamen olay tabanlı: yoklama yapılmaz. Bir MTA iş parçacığı yöneticiye bağlanıp uyur;
//! Windows şarkı/oynatma/zaman değişikliklerini kendi olaylarıyla bildirir. Kapak resmi yalnız
//! şarkı değiştiğinde okunur ve arayüze bir kez gönderilir. Modül kapatılınca tüm abonelikler bırakılır.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

use crate::debuglog;

const MAX_ART_BYTES: u32 = 1024 * 1024;
/// Windows dosya zamanı (1601) ile Unix zamanı (1970) arasındaki fark, 100 ns biriminde.
const FILETIME_UNIX_OFFSET: i64 = 116_444_736_000_000_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub app: String,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// Konumun ölçüldüğü an (Unix ms). Arayüz çalarken konumu buradan ileri sayar.
    pub updated_at: u64,
    pub art_id: Option<String>,
    pub can_toggle: bool,
    pub can_next: bool,
    pub can_prev: bool,
    /// Uygulama konuma gitmeye izin veriyor mu (Chrome/Edge medya oturumu evet, bazı uygulamalar hayır).
    pub can_seek: bool,
    pub demo: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicUpdate {
    pub now: Option<NowPlaying>,
    /// Yalnız kapak değiştiğinde dolu: `data:` adresi.
    pub art: Option<String>,
}

#[derive(Default)]
struct Inner {
    manager: Option<SessionManager>,
    manager_tokens: Vec<i64>,
    session: Option<Session>,
    session_tokens: [i64; 3],
    now: Option<NowPlaying>,
    art_key: Option<String>,
    art: Option<(String, String)>,
    /// Bu parça için kapak okuma denemesi sayısı (kapak gecikirse olay/zamanlayıcıyla yeniden denenir).
    art_tries: u8,
    /// Önceki parçanın kapağı: yeni parçada aynısı gelirse tarayıcı henüz güncellememiştir.
    prev_art_url: Option<String>,
    /// Parça değişince kalan kapak doğrulama sayısı (3 → 1, 3, 6 sn sonra yeniden oku).
    verify_left: u8,
}

/// Kapak hazır olmadığında en fazla bu kadar yeniden deneme yapılır (1 sn, 2 sn, 4 sn, 8 sn).
const MAX_ART_TRIES: u8 = 4;

/// Parça değişince kapak bu kadar kez (1, 3, 6 sn sonra) doğrulanır: Chrome kapağı başlıktan sonra, çoğu kez olaysız günceller.
const VERIFY_COUNT: u8 = 3;
const VERIFY_DELAYS: [u64; 3] = [1, 3, 6];

pub struct MediaHub {
    app: AppHandle,
    inner: Mutex<Inner>,
    /// Aynı anda gelen olaylarda yenileme tek tek yapılır.
    refreshing: Mutex<()>,
    enabled: AtomicBool,
    generation: Mutex<u64>,
    worker: Mutex<Option<thread::Thread>>,
    /// Bekleyen gecikmeli kapak denemesi var mı (en fazla biri).
    retry_pending: AtomicBool,
}

impl MediaHub {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn snapshot(&self) -> MusicUpdate {
        let inner = self.lock();
        MusicUpdate { now: inner.now.clone(), art: inner.art.as_ref().map(|(_, url)| url.clone()) }
    }
}

pub fn init(app: &AppHandle, enabled: bool) {
    let hub = Arc::new(MediaHub {
        app: app.clone(),
        inner: Mutex::new(Inner::default()),
        refreshing: Mutex::new(()),
        enabled: AtomicBool::new(false),
        generation: Mutex::new(0),
        worker: Mutex::new(None),
        retry_pending: AtomicBool::new(false),
    });
    app.manage(hub.clone());
    set_enabled(&hub, enabled);
}

pub fn set_enabled(hub: &Arc<MediaHub>, enabled: bool) {
    if hub.enabled.swap(enabled, Ordering::SeqCst) == enabled {
        return;
    }
    let generation = {
        let mut g = hub.generation.lock().unwrap_or_else(|p| p.into_inner());
        *g += 1;
        *g
    };
    if enabled {
        let worker_hub = hub.clone();
        if let Ok(handle) = thread::Builder::new().name("zodyvel-media".into()).spawn(move || run(worker_hub, generation)) {
            *hub.worker.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle.thread().clone());
        }
    } else {
        {
            let _guard = hub.refreshing.lock().unwrap_or_else(|p| p.into_inner());
            detach_all(hub);
        }
        if let Some(worker) = hub.worker.lock().unwrap_or_else(|p| p.into_inner()).take() {
            worker.unpark();
        }
        let _ = hub.app.emit("music://now", MusicUpdate { now: None, art: None });
    }
}

fn current_generation(hub: &MediaHub) -> u64 {
    *hub.generation.lock().unwrap_or_else(|p| p.into_inner())
}

fn run(hub: Arc<MediaHub>, generation: u64) {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    if let Err(error) = attach_manager(&hub) {
        debuglog::line(format!("media: yönetici açılamadı {error:?}"));
        return;
    }
    refresh(&hub);
    // MTA'yı ayakta tutmak için iş parçacığı yaşar ama çalışmaz; modül kapanınca çıkar.
    while hub.enabled.load(Ordering::SeqCst) && current_generation(&hub) == generation {
        thread::park_timeout(Duration::from_secs(3600));
    }
}

fn attach_manager(hub: &Arc<MediaHub>) -> windows::core::Result<()> {
    let manager = SessionManager::RequestAsync()?.join()?;
    let weak = Arc::downgrade(hub);
    let on_current = TypedEventHandler::new(move |_, _| {
        if let Some(hub) = weak.upgrade() {
            refresh(&hub);
        }
        Ok(())
    });
    let weak = Arc::downgrade(hub);
    let on_sessions = TypedEventHandler::new(move |_, _| {
        if let Some(hub) = weak.upgrade() {
            refresh(&hub);
        }
        Ok(())
    });
    let tokens = vec![manager.CurrentSessionChanged(&on_current)?, manager.SessionsChanged(&on_sessions)?];
    let mut inner = hub.lock();
    inner.manager = Some(manager);
    inner.manager_tokens = tokens;
    Ok(())
}

fn detach_session(inner: &mut Inner) {
    if let Some(session) = inner.session.take() {
        let [media, playback, timeline] = inner.session_tokens;
        let _ = session.RemoveMediaPropertiesChanged(media);
        let _ = session.RemovePlaybackInfoChanged(playback);
        let _ = session.RemoveTimelinePropertiesChanged(timeline);
    }
}

fn detach_all(hub: &MediaHub) {
    let mut inner = hub.lock();
    detach_session(&mut inner);
    if let Some(manager) = inner.manager.take()
        && let [current, sessions] = inner.manager_tokens[..]
    {
        let _ = manager.RemoveCurrentSessionChanged(current);
        let _ = manager.RemoveSessionsChanged(sessions);
    }
    inner.manager_tokens.clear();
    inner.now = None;
    inner.art = None;
    inner.art_key = None;
    inner.verify_left = 0;
}

fn is_playing(session: &Session) -> bool {
    session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == PlaybackStatus::Playing)
}

/// Gösterilecek oturum: çalan varsa o (Windows'un "geçerli" oturumu öncelikli), yoksa geçerli oturum.
fn pick(manager: &SessionManager) -> Option<Session> {
    let current = manager.GetCurrentSession().ok();
    if current.as_ref().is_some_and(is_playing) {
        return current;
    }
    if let Ok(sessions) = manager.GetSessions() {
        for i in 0..sessions.Size().unwrap_or(0) {
            if let Ok(session) = sessions.GetAt(i)
                && is_playing(&session)
            {
                return Some(session);
            }
        }
    }
    current
}

/// Nesne kimliği: aynı tarayıcının iki sekmesi aynı uygulama kimliğini taşır ama ayrı oturumdur.
fn same_session(a: &Session, b: &Session) -> bool {
    a == b
}

fn refresh(hub: &Arc<MediaHub>) {
    refresh_with(hub, false);
}

/// `force_art`: oturum kapağını değiştirdi (MediaPropertiesChanged); kapak yeniden okunur, farklıysa güncellenir.
fn refresh_with(hub: &Arc<MediaHub>, force_art: bool) {
    let _guard = hub.refreshing.lock().unwrap_or_else(|p| p.into_inner());
    if !hub.enabled.load(Ordering::SeqCst) {
        return;
    }
    let Some(manager) = hub.lock().manager.clone() else { return };
    let chosen = pick(&manager);

    // Oturum değiştiyse eski aboneliği bırak, yenisine abone ol.
    let needs_attach = {
        let inner = hub.lock();
        match (&inner.session, &chosen) {
            (Some(old), Some(new)) => !same_session(old, new),
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        }
    };
    if needs_attach {
        let mut inner = hub.lock();
        detach_session(&mut inner);
        if let Some(session) = &chosen
            && let Ok(tokens) = subscribe(hub, session)
        {
            inner.session = Some(session.clone());
            inner.session_tokens = tokens;
        }
    }

    let now = chosen.as_ref().and_then(|s| read(s).ok());
    let art_key = now.as_ref().map(|n| format!("{}\u{1f}{}\u{1f}{}", n.app, n.title, n.artist));
    let mut art_update = None;
    let key_changed = art_key != hub.lock().art_key;
    let (art_missing, tries) = {
        let inner = hub.lock();
        (inner.art.is_none(), inner.art_tries)
    };
    // Kapak yalnız parça değişince ya da önceki deneme boş kaldıysa okunur (olay tabanlı; sınırlı deneme).
    if key_changed || (art_key.is_some() && art_missing && tries < MAX_ART_TRIES) {
        let attempt = if key_changed { 0 } else { tries + 1 };
        let read_url = match (&chosen, &art_key) {
            (Some(session), Some(_)) => read_art(session),
            _ => None,
        };
        let mut inner = hub.lock();
        if key_changed {
            inner.verify_left = VERIFY_COUNT;
            inner.prev_art_url = inner.art.as_ref().map(|(_, url)| url.clone());
            inner.art = None;
        }
        // Tarayıcılar başlığı kapaktan önce günceller: önceki parçanın kapağı geldiyse henüz hazır değildir.
        let stale = read_url.is_some() && read_url == inner.prev_art_url && attempt < MAX_ART_TRIES;
        let art = if stale { None } else { read_url }.zip(art_key.as_ref()).map(|(url, key)| (art_id(key, &url), url));
        inner.art_key = art_key.clone();
        inner.art_tries = attempt;
        art_update = art.as_ref().map(|(_, url)| url.clone());
        inner.art = art;
    }
    if force_art && !key_changed && art_update.is_none() && art_key.is_some() {
        // Tarayıcı kapağı başlıktan sonra gönderir; sonradan gelen doğru kapak eskisinin yerine geçer.
        if let Some(url) = chosen.as_ref().and_then(read_art) {
            let mut inner = hub.lock();
            if inner.art.as_ref().map(|(_, u)| u) != Some(&url) {
                let id = art_id(art_key.as_deref().unwrap_or(""), &url);
                inner.art = Some((id, url.clone()));
                art_update = Some(url);
            }
        }
    }
    let retry_needed = {
        let inner = hub.lock();
        art_key.is_some() && ((inner.art.is_none() && inner.art_tries < MAX_ART_TRIES) || inner.verify_left > 0)
    };
    if retry_needed {
        schedule_art_retry(hub);
    }
    let now = now.map(|mut n| {
        n.art_id = hub.lock().art.as_ref().map(|(id, _)| id.clone());
        n
    });

    let changed = {
        let mut inner = hub.lock();
        let changed = inner.now != now || art_update.is_some();
        inner.now = now.clone();
        changed
    };
    if changed {
        let _ = hub.app.emit("music://now", MusicUpdate { now, art: art_update });
    }
}

/// Kapak gelmediyse (olay da gelmeyebilir) birkaç kez, artan aralıkla bir kez daha dener. Döngü değil.
fn schedule_art_retry(hub: &Arc<MediaHub>) {
    if hub.retry_pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let delay = {
        let inner = hub.lock();
        match inner.verify_left {
            0 => Duration::from_secs(1 << inner.art_tries.min(3)),
            n => Duration::from_secs(VERIFY_DELAYS[(VERIFY_COUNT - n.min(VERIFY_COUNT)) as usize]),
        }
    };
    let weak = Arc::downgrade(hub);
    let spawned = thread::Builder::new().name("zodyvel-art".into()).spawn(move || {
        thread::sleep(delay);
        if let Some(hub) = weak.upgrade() {
            hub.retry_pending.store(false, Ordering::SeqCst);
            {
                let mut inner = hub.lock();
                inner.verify_left = inner.verify_left.saturating_sub(1);
            }
            refresh_with(&hub, true);
        }
    });
    if spawned.is_err() {
        hub.retry_pending.store(false, Ordering::SeqCst);
    }
}

fn subscribe(hub: &Arc<MediaHub>, session: &Session) -> windows::core::Result<[i64; 3]> {
    let handler = |force: bool| {
        let weak = Arc::downgrade(hub);
        move || {
            if let Some(hub) = weak.upgrade() {
                refresh_with(&hub, force);
            }
        }
    };
    let (a, b, c) = (handler(true), handler(false), handler(false));
    Ok([
        session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            a();
            Ok(())
        }))?,
        session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            b();
            Ok(())
        }))?,
        session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            c();
            Ok(())
        }))?,
    ])
}

fn ticks_to_ms(ticks: i64) -> u64 {
    (ticks.max(0) / 10_000) as u64
}

fn read(session: &Session) -> windows::core::Result<NowPlaying> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let info = session.GetPlaybackInfo()?;
    let controls = info.Controls()?;
    let timeline = session.GetTimelineProperties()?;
    let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
    let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
    let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);
    let updated = timeline.LastUpdatedTime().map(|t| t.UniversalTime).unwrap_or(0);
    let updated_at = if updated > FILETIME_UNIX_OFFSET { ((updated - FILETIME_UNIX_OFFSET) / 10_000) as u64 } else { crate::agents::now_ms() };
    let title = props.Title()?.to_string();
    let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
    Ok(NowPlaying {
        title: if title.is_empty() { "Bilinmeyen parça".into() } else { title },
        artist,
        app: app_name(&session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default()),
        playing: info.PlaybackStatus()? == PlaybackStatus::Playing,
        position_ms: ticks_to_ms(position - start),
        duration_ms: ticks_to_ms(end - start),
        updated_at,
        art_id: None,
        can_toggle: controls.IsPlayPauseToggleEnabled().unwrap_or(false)
            || controls.IsPlayEnabled().unwrap_or(false)
            || controls.IsPauseEnabled().unwrap_or(false),
        can_next: controls.IsNextEnabled().unwrap_or(false),
        can_prev: controls.IsPreviousEnabled().unwrap_or(false),
        can_seek: end > start && controls.IsPlaybackPositionEnabled().unwrap_or(false),
        demo: false,
    })
}

fn read_art(session: &Session) -> Option<String> {
    let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
    let size = u32::try_from(stream.Size().ok()?).ok().filter(|s| *s > 0 && *s <= MAX_ART_BYTES)?;
    let content_type = stream.ContentType().map(|c| c.to_string()).unwrap_or_default();
    let mime = if content_type.starts_with("image/") { content_type } else { "image/jpeg".into() };
    let buffer = Buffer::Create(size).ok()?;
    let filled = stream.ReadAsync(&buffer, size, InputStreamOptions::None).ok()?.join().ok()?;
    let reader = DataReader::FromBuffer(&filled).ok()?;
    let mut bytes = vec![0u8; filled.Length().ok()? as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn art_id(key: &str, url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    url.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Uygulama kimliğinden okunur ad: "Spotify.exe" → Spotify, "Chrome" → Chrome…
pub fn app_name(aumid: &str) -> String {
    let lower = aumid.to_ascii_lowercase();
    let known = [
        ("spotify", "Spotify"),
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("opera", "Opera"),
        ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"),
        ("brave", "Brave"),
        ("vivaldi", "Vivaldi"),
        ("zunemusic", "Medya Oynatıcı"),
        ("zunevideo", "Filmler ve TV"),
        ("vlc", "VLC"),
        ("applemusic", "Apple Music"),
        ("deezer", "Deezer"),
        ("tidal", "TIDAL"),
        ("youtube", "YouTube"),
    ];
    if let Some((_, name)) = known.iter().find(|(needle, _)| lower.contains(needle)) {
        return (*name).to_owned();
    }
    let base = aumid.rsplit(['\\', '!']).next().unwrap_or(aumid);
    let base = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    if base.is_empty() { "Medya".into() } else { base.to_owned() }
}

#[tauri::command]
pub fn music_snapshot(hub: State<'_, Arc<MediaHub>>) -> MusicUpdate {
    hub.snapshot()
}

/// Oynat/duraklat, sonraki, önceki. WinRT çağrısı arka planda yapılır; arayüz beklemez.
#[tauri::command]
pub fn music_control(hub: State<'_, Arc<MediaHub>>, action: String) -> Result<(), String> {
    let session = hub.lock().session.clone().ok_or("Çalan bir oturum yok.")?;
    if !matches!(action.as_str(), "toggle" | "next" | "prev") {
        return Err("Geçersiz komut.".into());
    }
    thread::spawn(move || {
        let result = match action.as_str() {
            "toggle" => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
            "next" => session.TrySkipNextAsync().and_then(|op| op.join()),
            _ => session.TrySkipPreviousAsync().and_then(|op| op.join()),
        };
        if let Err(error) = result {
            debuglog::line(format!("media: komut başarısız {error:?}"));
        }
    });
    Ok(())
}

/// Modlar: başlarken çalan müziği duraklatır (`play` false) ya da sürdürür. Oturum yoksa bir şey yapılmaz;
/// modül kapalıyken hub zaten oturum tutmaz.
pub fn set_playing(app: &AppHandle, play: bool) {
    let Some(hub) = app.try_state::<Arc<MediaHub>>() else { return };
    let Some(session) = hub.lock().session.clone() else { return };
    thread::spawn(move || {
        let result = if play { session.TryPlayAsync().and_then(|op| op.join()) } else { session.TryPauseAsync().and_then(|op| op.join()) };
        if let Err(error) = result {
            debuglog::line(format!("media: mod komutu başarısız {error:?}"));
        }
    });
}

/// Parçanın başından itibaren `position_ms`'e gider. Windows konumu zaman çizelgesinin başlangıcına
/// göre 100 ns biriminde ister; süre dışındaki değerler sınırlanır.
#[tauri::command]
pub fn music_seek(hub: State<'_, Arc<MediaHub>>, position_ms: u64) -> Result<(), String> {
    let session = hub.lock().session.clone().ok_or("Çalan bir oturum yok.")?;
    thread::spawn(move || {
        let result = (|| {
            let timeline = session.GetTimelineProperties()?;
            let start = timeline.StartTime()?.Duration;
            let end = timeline.EndTime()?.Duration;
            session.TryChangePlaybackPositionAsync(seek_target(start, end, position_ms))?.join()
        })();
        if !matches!(result, Ok(true)) {
            debuglog::line(format!("media: sarma başarısız {result:?}"));
        }
    });
    Ok(())
}

fn seek_target(start: i64, end: i64, position_ms: u64) -> i64 {
    let offset = i64::try_from(position_ms).unwrap_or(i64::MAX).saturating_mul(10_000);
    start.saturating_add(offset.min((end - start).max(0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_app_names() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("Chrome"), "Chrome");
        assert_eq!(app_name("MSEdge"), "Edge");
        assert_eq!(app_name("308046B0AF4A39CB"), "Firefox");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "Medya Oynatıcı");
        assert_eq!(app_name("C:\\Apps\\foobar2000.exe"), "foobar2000");
        assert_eq!(app_name(""), "Medya");
    }

    /// Bu bilgisayardaki medya oturumunu okur; oturum yoksa da geçer. Şarkı adı yazdırılmaz.
    #[test]
    fn reads_current_media_session_if_any() {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let manager = SessionManager::RequestAsync().unwrap().join().unwrap();
        match pick(&manager) {
            Some(session) => {
                let now = read(&session).unwrap();
                println!(
                    "oturum: {} çalıyor={} süre={} sn başlık_var={} kapak_var={}",
                    now.app,
                    now.playing,
                    now.duration_ms / 1000,
                    !now.title.is_empty(),
                    read_art(&session).is_some()
                );
            }
            None => println!("oturum: yok"),
        }
    }

    #[test]
    fn ticks_convert_to_ms() {
        assert_eq!(ticks_to_ms(2_130_000_000), 213_000);
        assert_eq!(ticks_to_ms(-5), 0);
    }

    /// Gerçek oturumda sarma: bulunduğu konuma "sarar" (kullanıcının yeri değişmez), Windows'un kabul ettiğini doğrular.
    #[test]
    #[ignore]
    fn seeks_real_session_in_place() {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let manager = SessionManager::RequestAsync().unwrap().join().unwrap();
        let Some(session) = pick(&manager) else { return println!("oturum: yok") };
        let now = read(&session).unwrap();
        let timeline = session.GetTimelineProperties().unwrap();
        let (start, end) = (timeline.StartTime().unwrap().Duration, timeline.EndTime().unwrap().Duration);
        let accepted = session.TryChangePlaybackPositionAsync(seek_target(start, end, now.position_ms)).unwrap().join().unwrap();
        println!("oturum: {} sarılabilir={} kabul={accepted}", now.app, now.can_seek);
        assert_eq!(accepted, now.can_seek);
    }

    #[test]
    fn seek_target_is_relative_to_start_and_clamped() {
        assert_eq!(seek_target(0, 2_130_000_000, 74_000), 740_000_000);
        assert_eq!(seek_target(50_000_000, 2_180_000_000, 0), 50_000_000);
        assert_eq!(seek_target(0, 2_130_000_000, 999_000), 2_130_000_000);
        assert_eq!(seek_target(0, 0, 10_000), 0);
        assert_eq!(seek_target(0, 100, u64::MAX), 100);
    }
}
