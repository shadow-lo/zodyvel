//! Bugün modülü: Zodyvel içindeki takvim (etkinlikler adadan eklenir), yapılacaklar, Zodymetreler ve Modlar.
//!
//! Veri ayarların yanında yerel bir dosyada (`today.json`); dış hesap ya da bağlantı yok.
//! Zodymetreler (en çok 3, adlı ve renkli zamanlayıcılar; K8) Rust'ta işler, ada kapalıyken de sürer:
//! iş parçacığı en yakın bitiş anına kadar uyur (yoklama yok); bitince arayüze haber verilir ve ayar
//! açıksa Windows'un hatırlatma sesi çalar. Modlar (`modes`, Faz 5a) aynı depoda ve aynı iş parçacığında:
//! mod süresi ve içindeki odak döngüsü (`focus`) de yalnız bitiş anlarında uyandırır.
//!
//! Bugün modülü kapalıyken (Ayarlar → Modüller) çalışan Zodymetreler duraklatılır (kalan süre korunur),
//! etkinlik hatırlatması ve ses yoktur. Modlar modülü kapalıyken açık mod biter ve mod komutları reddedilir.
//! İkisi de kapalıyken iş parçacığı hiç uyanmaz.

mod agenda;
mod focus;
mod modes;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::agents::now_ms;
use crate::{debuglog, sound};
pub use agenda::{PlanForm, Repeat};
pub use modes::{AppRef, ModeDef, ModeRun};
use modes::{MAX_MODES, default_modes};

const MAX_EVENTS: usize = 500;
const MAX_TODOS: usize = 200;
const MAX_TEXT: usize = 120;
const MAX_MINUTES: u32 = 180;
/// "+N dk" düğmesinin en çok ekleyebileceği süre.
const MAX_ADD_MINUTES: u32 = 60;
/// Aynı anda en çok Zodymetre (K8).
pub const MAX_METERS: usize = 3;
const METER_NAME_MAX: usize = 24;
/// Bugün dosyasının biçimi: 2 = çoklu Zodymetre + Modlar (Faz 5a). Eski dosya açılınca bir kez taşınır.
const VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    pub id: u64,
    /// Yerel tarih, "2026-10-14".
    pub date: String,
    /// Yerel saat, "14:00"; boşsa gün boyu.
    pub time: Option<String>,
    pub title: String,
    /// Ajanda (Faz 5b): süre (dk; 0 süresiz), renk, tekrar, haber ver ve kaç dakika önce. Eski dosyalarda yok.
    #[serde(default)]
    pub minutes: u32,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub repeat: Repeat,
    /// `repeat` "days" iken günler: bit 0 Pazartesi … bit 6 Pazar.
    #[serde(default)]
    pub days: u8,
    #[serde(default = "agenda::yes")]
    pub notify: bool,
    #[serde(default)]
    pub lead: u32,
    /// Telefon eşliyse hatırlatma telefonda da çalar (T2b'de telefona yalnız başlık/saat/tekrar/ön süre gider).
    #[serde(default = "agenda::yes")]
    pub phone: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

/// Zodymetre (K8): adlı, süreli, renkli zamanlayıcı; aynı anda en çok üç tane. Bugünkü tek zamanlayıcının
/// yerine geçti. Odak döngüsü Modlar'a taşındı (`modes`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Meter {
    pub id: u64,
    pub name: String,
    /// "#5b9cff"; boş: vurgu rengi.
    pub color: String,
    /// Dolunca ses (Ayarlar › Bildirimler › Sesler'deki zamanlayıcı sesi de açıksa).
    pub sound: bool,
    pub minutes: u32,
    /// Çalışıyorsa bitiş anı (Unix ms).
    pub ends_at: Option<u64>,
    /// Duraklatılmışken kalan süre; 0: doldu.
    pub left_ms: u64,
    /// Bugün kaç kez doldu ve toplam süresi (Bitiş'te "Bugün 3. tur · toplam 1 sa 15 dk"); `day` değişince sıfırlanır.
    pub day: String,
    pub rounds: u32,
    pub done_ms: u64,
}

impl Default for Meter {
    fn default() -> Self {
        Self::new(0, "Zodymetre".into(), 25, String::new(), true)
    }
}

impl Meter {
    fn new(id: u64, name: String, minutes: u32, color: String, sound: bool) -> Self {
        let left_ms = u64::from(minutes) * 60_000;
        Self { id, name, color, sound, minutes, ends_at: None, left_ms, day: String::new(), rounds: 0, done_ms: 0 }
    }
    fn full(&self) -> u64 {
        u64::from(self.minutes) * 60_000
    }
    fn due(&self, now: u64) -> bool {
        self.ends_at.is_some_and(|end| end <= now)
    }
    fn pause(&mut self, now: u64) {
        if let Some(end) = self.ends_at.take() {
            self.left_ms = end.saturating_sub(now);
        }
    }
}

/// Dolan Zodymetre: durur, bugünkü tur sayısı ve toplamı artar (`day` yerel tarih, "2026-11-10").
pub fn finish_meter(meter: &mut Meter, day: &str) -> MeterDone {
    meter.ends_at = None;
    meter.left_ms = 0;
    if meter.day != day {
        meter.day = day.to_owned();
        meter.rounds = 0;
        meter.done_ms = 0;
    }
    meter.rounds += 1;
    meter.done_ms += meter.full();
    MeterDone { id: meter.id, name: meter.name.clone(), color: meter.color.clone(), minutes: meter.minutes, rounds: meter.rounds, total_ms: meter.done_ms }
}

/// Arayüze giden "Zodymetre doldu" (`today://timer-done`): Bitiş bildirimi.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeterDone {
    pub id: u64,
    pub name: String,
    pub color: String,
    pub minutes: u32,
    pub rounds: u32,
    pub total_ms: u64,
}

/// Faz 5a öncesi tek zamanlayıcı: yalnız eski dosyayı okumak için (yazılmaz).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct LegacyTimer {
    pub minutes: u32,
    pub left_ms: u64,
}

/// Özelliği kapalı bir değişiklik reddedilir (kapalı özellik hiçbir şey başlatmaz).
pub fn guard(op: &Op, focus_on: bool) -> Result<(), String> {
    if matches!(op, Op::FocusStart) && !focus_on {
        return Err("Odak modu kapalı (Ayarlar → Özellikler).".into());
    }
    Ok(())
}

/// Değişiklik hangi modüle ait: Modlar ya da Bugün (Ayarlar → Modüller).
pub fn is_mode_op(op: &Op) -> bool {
    matches!(
        op,
        Op::ModeSave { .. }
            | Op::ModeRemove { .. }
            | Op::ModeStart { .. }
            | Op::ModePause
            | Op::ModeResume
            | Op::ModeAdd { .. }
            | Op::ModeStop
            | Op::ModeReopen
            | Op::FocusStart
            | Op::FocusStop
    )
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Today {
    pub version: u32,
    pub events: Vec<CalEvent>,
    pub todos: Vec<Todo>,
    pub meters: Vec<Meter>,
    pub modes: Vec<ModeDef>,
    /// Açık mod; yoksa None.
    pub mode: Option<ModeRun>,
    /// Son biten modun kapattığı uygulamalar (Mod Bitişi'ndeki "yeniden aç" düğmesi; açınca boşalır).
    pub reopen: Vec<AppRef>,
    pub next_id: u64,
    /// Eski dosyadaki tek zamanlayıcı (taşınınca silinir, yazılmaz).
    #[serde(skip_serializing)]
    pub timer: Option<LegacyTimer>,
}

impl Today {
    /// Eski ya da yeni (boş) dosyayı güncel biçime getirir: eski zamanlayıcı ilk Zodymetre olur, dört
    /// varsayılan mod eklenir. Değiştiyse `true`.
    pub fn migrate(&mut self) -> bool {
        let legacy = self.timer.take();
        if self.version >= VERSION {
            return false;
        }
        if self.meters.is_empty() {
            self.next_id += 1;
            let minutes = legacy.as_ref().map_or(25, |t| t.minutes.clamp(1, MAX_MINUTES));
            let mut meter = Meter::new(self.next_id, "Zodymetre".into(), minutes, String::new(), true);
            // Duraklatılmış eski zamanlayıcının kalanı korunur; çalışıyorduysa da duraklatılmış sayılır.
            if let Some(t) = legacy.filter(|t| t.left_ms > 0) {
                meter.left_ms = t.left_ms.min(meter.full());
            }
            self.meters.push(meter);
        }
        if self.modes.is_empty() {
            self.modes = default_modes();
        }
        self.version = VERSION;
        true
    }

    fn meter_mut(&mut self, id: Option<u64>) -> Result<&mut Meter, String> {
        match id {
            Some(id) => self.meters.iter_mut().find(|m| m.id == id).ok_or_else(|| "Zodymetre bulunamadı.".into()),
            None => {
                // Kimliksiz komut (Zody sohbeti, eylem kaydı): ilk Zodymetre; hiç yoksa biri oluşur.
                if self.meters.is_empty() {
                    self.next_id += 1;
                    self.meters.push(Meter { id: self.next_id, ..Meter::default() });
                }
                Ok(&mut self.meters[0])
            }
        }
    }

    pub fn mode_def(&self, id: &str) -> Option<&ModeDef> {
        self.modes.iter().find(|m| m.id == id)
    }

    /// Açık modun tanımı.
    pub fn active_mode(&self) -> Option<&ModeDef> {
        self.mode.as_ref().and_then(|r| self.mode_def(&r.id))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Op {
    /// Ajanda planı (eski `addEvent` biçimi de geçerli: yalnız tarih, saat, başlık).
    AddEvent(PlanForm),
    /// Planı baştan yazar (düzenleme formu ya da çizelgede sürükleme).
    EditEvent { id: u64, plan: PlanForm },
    RemoveEvent { id: u64 },
    /// "Döngü" / "Bugünü her güne uygula": o günün tekrarsız planları her gün tekrarlansın ya da bıraksın.
    LoopDay { date: String, on: bool },
    AddTodo { text: String },
    ToggleTodo { id: u64 },
    RemoveTodo { id: u64 },
    /// Zodymetre komutları. `id` yoksa ilk Zodymetre (Zody sohbeti ve eylem kaydı böyle çağırır).
    TimerSet {
        #[serde(default)]
        id: Option<u64>,
        minutes: u32,
    },
    TimerStart {
        #[serde(default)]
        id: Option<u64>,
    },
    TimerPause {
        #[serde(default)]
        id: Option<u64>,
    },
    TimerReset {
        #[serde(default)]
        id: Option<u64>,
    },
    /// Süre ekler (çentikteki "+5 dk"): çalışırken bitiş ileri, duraklatılmışken kalan artar; dolmuşsa
    /// yalnız eklenen süreyle hemen başlar. Toplam en çok MAX_MINUTES.
    TimerAdd {
        #[serde(default)]
        id: Option<u64>,
        minutes: u32,
    },
    MeterAdd { name: String, minutes: u32, color: String, sound: bool },
    MeterEdit { id: u64, name: String, minutes: u32, color: String, sound: bool },
    MeterRemove { id: u64 },
    /// Modlar: tanımı ekler ya da (aynı kimlikle) günceller. Kimlik boşsa yeni mod.
    ModeSave { mode: ModeDef },
    ModeRemove { id: String },
    /// `minutes` yoksa tanımdaki süre; 0 süresiz.
    ModeStart {
        id: String,
        #[serde(default)]
        minutes: Option<u32>,
    },
    ModePause,
    ModeResume,
    ModeAdd { minutes: u32 },
    ModeStop,
    /// Son biten modun kapattığı uygulamaları yeniden açar (yan etki komut katmanında).
    ModeReopen,
    /// Zody sohbeti "odak başlat": odak döngülü ilk mod (yoksa Çalışma) döngüyle başlar.
    FocusStart,
    /// Odak döngülü açık modu bitirir.
    FocusStop,
}

fn two_digits(s: &str) -> Option<u32> {
    (s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

fn valid_date(date: &str) -> bool {
    let parts: Vec<&str> = date.split('-').collect();
    let [year, month, day] = parts[..] else { return false };
    year.len() == 4
        && year.bytes().all(|b| b.is_ascii_digit())
        && two_digits(month).is_some_and(|m| (1..=12).contains(&m))
        && two_digits(day).is_some_and(|d| (1..=31).contains(&d))
}

fn valid_time(time: &str) -> bool {
    let Some((h, m)) = time.split_once(':') else { return false };
    two_digits(h).is_some_and(|h| h < 24) && two_digits(m).is_some_and(|m| m < 60)
}

fn clean_text(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Boş olamaz.".into());
    }
    if text.chars().count() > MAX_TEXT {
        return Err(format!("En çok {MAX_TEXT} karakter."));
    }
    Ok(text.to_owned())
}

fn valid_minutes(minutes: u32) -> Result<u32, String> {
    if (1..=MAX_MINUTES).contains(&minutes) { Ok(minutes) } else { Err("Geçersiz süre.".into()) }
}

fn meter_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() { "Zodymetre".into() } else { name.chars().take(METER_NAME_MAX).collect() }
}

fn meter_color(color: &str) -> String {
    let ok = color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if ok { color.to_owned() } else { String::new() }
}

fn sort_events(events: &mut [CalEvent]) {
    events.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
}

/// Ajanda komutu mu (Ajanda modülüne bağlı; diğerleri Bugün ya da Modlar).
pub fn is_agenda_op(op: &Op) -> bool {
    matches!(op, Op::AddEvent(_) | Op::EditEvent { .. } | Op::RemoveEvent { .. } | Op::LoopDay { .. })
}

/// Bir değişikliği uygular. Saf: zaman dışarıdan verilir (testli). `focus_on`: "Odak modu" özelliği.
#[cfg(test)]
pub fn apply(today: &mut Today, op: Op, now: u64) -> Result<(), String> {
    apply_with(today, op, now, true)
}

pub fn apply_with(today: &mut Today, op: Op, now: u64, focus_on: bool) -> Result<(), String> {
    match op {
        Op::AddEvent(plan) => {
            if today.events.len() >= MAX_EVENTS {
                return Err("Çok fazla plan var.".into());
            }
            let event = plan.into_event(today.next_id + 1)?;
            today.next_id += 1;
            today.events.push(event);
            sort_events(&mut today.events);
        }
        Op::EditEvent { id, plan } => {
            let event = plan.into_event(id)?;
            let slot = today.events.iter_mut().find(|e| e.id == id).ok_or("Plan bulunamadı.")?;
            *slot = event;
            sort_events(&mut today.events);
        }
        Op::RemoveEvent { id } => today.events.retain(|e| e.id != id),
        Op::LoopDay { date, on } => {
            if !valid_date(&date) {
                return Err("Geçersiz tarih.".into());
            }
            agenda::loop_day(&mut today.events, &date, on);
        }
        Op::AddTodo { text } => {
            if today.todos.len() >= MAX_TODOS {
                return Err("Çok fazla iş var.".into());
            }
            let text = clean_text(&text)?;
            today.next_id += 1;
            today.todos.push(Todo { id: today.next_id, text, done: false });
        }
        Op::ToggleTodo { id } => {
            if let Some(todo) = today.todos.iter_mut().find(|t| t.id == id) {
                todo.done = !todo.done;
            }
        }
        Op::RemoveTodo { id } => today.todos.retain(|t| t.id != id),
        Op::TimerSet { id, minutes } => {
            let minutes = valid_minutes(minutes)?;
            let meter = today.meter_mut(id)?;
            meter.minutes = minutes;
            meter.ends_at = None;
            meter.left_ms = meter.full();
        }
        Op::TimerStart { id } => {
            let meter = today.meter_mut(id)?;
            if meter.ends_at.is_none() {
                if meter.left_ms == 0 {
                    meter.left_ms = meter.full();
                }
                meter.ends_at = Some(now + meter.left_ms);
            }
        }
        Op::TimerPause { id } => today.meter_mut(id)?.pause(now),
        Op::TimerReset { id } => {
            let meter = today.meter_mut(id)?;
            meter.ends_at = None;
            meter.left_ms = meter.full();
        }
        Op::TimerAdd { id, minutes } => {
            if !(1..=MAX_ADD_MINUTES).contains(&minutes) {
                return Err("Geçersiz süre.".into());
            }
            let add = u64::from(minutes) * 60_000;
            let meter = today.meter_mut(id)?;
            if meter.ends_at.is_none() && meter.left_ms == 0 {
                meter.minutes = minutes;
                meter.ends_at = Some(now + add);
                meter.left_ms = add;
            } else {
                meter.minutes = (meter.minutes + minutes).min(MAX_MINUTES);
                match meter.ends_at.as_mut() {
                    Some(end) => *end += add,
                    None => meter.left_ms += add,
                }
            }
        }
        Op::MeterAdd { name, minutes, color, sound } => {
            if today.meters.len() >= MAX_METERS {
                return Err(format!("En çok {MAX_METERS} Zodymetre olur."));
            }
            let minutes = valid_minutes(minutes)?;
            today.next_id += 1;
            today.meters.push(Meter::new(today.next_id, meter_name(&name), minutes, meter_color(&color), sound));
        }
        Op::MeterEdit { id, name, minutes, color, sound } => {
            let minutes = valid_minutes(minutes)?;
            let meter = today.meter_mut(Some(id))?;
            meter.name = meter_name(&name);
            meter.color = meter_color(&color);
            meter.sound = sound;
            // Süre değişince baştan (durur); yalnız ad/renk değiştiyse çalışmaya devam eder.
            if meter.minutes != minutes {
                meter.minutes = minutes;
                meter.ends_at = None;
                meter.left_ms = meter.full();
            }
        }
        Op::MeterRemove { id } => today.meters.retain(|m| m.id != id),
        Op::ModeSave { mode } => {
            let mut mode = modes::sanitize(mode)?;
            match today.modes.iter_mut().find(|m| !mode.id.is_empty() && m.id == mode.id) {
                Some(existing) => *existing = mode,
                None => {
                    if today.modes.len() >= MAX_MODES {
                        return Err(format!("En çok {MAX_MODES} mod olur."));
                    }
                    today.next_id += 1;
                    mode.id = format!("m{}", today.next_id);
                    today.modes.push(mode);
                }
            }
        }
        Op::ModeRemove { id } => {
            if today.mode.as_ref().is_some_and(|r| r.id == id) {
                return Err("Açık mod silinemez; önce bitir.".into());
            }
            today.modes.retain(|m| m.id != id);
        }
        Op::ModeStart { id, minutes } => {
            if today.mode.is_some() {
                return Err("Zaten bir mod açık; önce onu bitir.".into());
            }
            let def = today.mode_def(&id).ok_or("Mod bulunamadı.")?;
            today.mode = Some(modes::start(def, minutes, now, focus_on));
        }
        Op::ModePause => modes::pause(today.mode.as_mut().ok_or("Açık mod yok.")?, now),
        Op::ModeResume => modes::resume(today.mode.as_mut().ok_or("Açık mod yok.")?, now),
        Op::ModeAdd { minutes } => {
            if !(1..=MAX_ADD_MINUTES).contains(&minutes) {
                return Err("Geçersiz süre.".into());
            }
            modes::add(today.mode.as_mut().ok_or("Açık mod yok.")?, minutes)?;
        }
        Op::ModeStop => {
            // Elle bitirilen mod da kapattıklarını yeniden açmayı sunar (Mod Bitişi komut katmanında).
            if let Some(run) = today.mode.take() {
                today.reopen = run.closed;
            }
        }
        Op::ModeReopen => today.reopen.clear(),
        Op::FocusStart => {
            if today.mode.is_some() {
                return Err("Zaten bir mod açık; önce onu bitir.".into());
            }
            let def = today.modes.iter().find(|m| m.focus).or_else(|| today.mode_def("work")).ok_or("Odak için mod yok.")?;
            let def = ModeDef { focus: true, ..def.clone() };
            today.mode = Some(modes::start(&def, None, now, true));
        }
        Op::FocusStop => {
            if today.mode.as_ref().is_some_and(|r| r.focus.is_some()) {
                today.mode = None;
            }
        }
    }
    Ok(())
}

/// Modül kapanırken: çalışan Zodymetreler kalan süreleriyle duraklatılır. Değiştiyse `true`.
pub fn freeze(today: &mut Today, now: u64) -> bool {
    let mut changed = false;
    for meter in today.meters.iter_mut().filter(|m| m.ends_at.is_some()) {
        meter.pause(now);
        changed = true;
    }
    changed
}

pub struct TodayHub {
    app: AppHandle,
    path: PathBuf,
    data: Mutex<Today>,
    worker: Mutex<Option<thread::Thread>>,
}

impl TodayHub {
    fn lock(&self) -> MutexGuard<'_, Today> {
        self.data.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, today: &Today) {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.path.parent() {
                fs::create_dir_all(dir)?;
            }
            // Önce geçici dosya, sonra yer değiştirme: yarım kalmış dosya oluşmaz.
            let tmp = self.path.with_extension("json.tmp");
            fs::write(&tmp, serde_json::to_vec_pretty(today)?)?;
            fs::rename(&tmp, &self.path)
        };
        if let Err(error) = write() {
            debuglog::line(format!("today: kaydedilemedi {error}"));
        }
    }

    fn wake(&self) {
        if let Some(worker) = self.worker.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            worker.unpark();
        }
    }
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let path = app.path().app_config_dir()?.join("today.json");
    let mut today = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Today>(&bytes).ok()).unwrap_or_default();
    let migrated = today.migrate();
    let now = now_ms();
    // Zodyvel kapalıyken dolmuş Zodymetre sessizce "doldu" olur (geç gelen ses anlamsız); süresi geçmiş
    // mod da sessizce biter (kapattığı uygulamalar sorulmaz).
    for meter in today.meters.iter_mut().filter(|m| m.due(now)) {
        meter.ends_at = None;
        meter.left_ms = 0;
    }
    if today.mode.as_ref().is_some_and(|r| modes::ended(r, now)) {
        today.mode = None;
    }
    let settings = crate::settings::current(app);
    // Modül kapalıyken başlıyorsa Zodymetreler arka planda dolmasın, mod açık kalmasın.
    if !settings.modules.today {
        freeze(&mut today, now);
    }
    if !settings.modules.modes {
        today.mode = None;
    }
    let hub = Arc::new(TodayHub { app: app.clone(), path, data: Mutex::new(today), worker: Mutex::new(None) });
    if migrated {
        hub.save(&hub.lock().clone());
    }
    app.manage(hub.clone());
    let worker_hub = hub.clone();
    let handle = thread::Builder::new().name("zodyvel-today".into()).spawn(move || run(worker_hub)).expect("bugün iş parçacığı");
    *hub.worker.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle.thread().clone());
    Ok(())
}

fn changed(app: &AppHandle, hub: &TodayHub, edit: impl FnOnce(&mut Today) -> bool) {
    let snapshot = {
        let mut today = hub.lock();
        if !edit(&mut today) {
            None
        } else {
            hub.save(&today);
            Some(today.clone())
        }
    };
    if let Some(snapshot) = snapshot {
        let _ = app.emit("today://changed", &snapshot);
        crate::phone::notify_agenda(app);
    }
}

/// Ayarlardan (Modüller → Bugün). Kapanınca Zodymetreler duraklar; iş parçacığı her durumda uyandırılır,
/// iki modül de kapalıysa süresiz uyur, açılınca hatırlatmaları yeniden hesaplar.
pub fn set_enabled(app: &AppHandle, on: bool) {
    let Some(hub) = app.try_state::<Arc<TodayHub>>() else { return };
    if !on {
        changed(app, &hub, |today| freeze(today, now_ms()));
    }
    hub.wake();
}

/// Ayarlardan (Modüller → Modlar). Kapanınca açık mod sessizce biter (Bitiş yok, uygulamalara dokunulmaz).
pub fn set_modes_enabled(app: &AppHandle, on: bool) {
    let Some(hub) = app.try_state::<Arc<TodayHub>>() else { return };
    if !on {
        changed(app, &hub, |today| today.mode.take().is_some());
    }
    hub.wake();
}

/// Açık mod Zody'yi susturuyor mu (sound.rs her seste sorar; kilit kısa).
pub fn quiet(app: &AppHandle) -> bool {
    let Some(hub) = app.try_state::<Arc<TodayHub>>() else { return false };
    let today = hub.lock();
    today.active_mode().is_some_and(|m| m.quiet)
}

fn local_day() -> String {
    let (y, mo, d, ..) = crate::win::local_time();
    format!("{y:04}-{mo:02}-{d:02}")
}

/// Modun dış dünyaya dokunan eylemleri. Komut ana iş parçacığında döndüğü için hepsi arka planda yapılır.
mod effects {
    use std::process::Command;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::{AppRef, ModeDef, TodayHub};
    use crate::debuglog;

    #[cfg(windows)]
    fn quiet_command(program: &str) -> Command {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    #[cfg(not(windows))]
    fn quiet_command(program: &str) -> Command {
        Command::new(program)
    }

    /// Süreç açık mı (`tasklist` süzgeciyle; yeni paket yok).
    fn running(exe: &str) -> bool {
        let out = quiet_command("tasklist").args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH", "/FO", "CSV"]).output();
        out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains(&format!("\"{}\"", exe.to_ascii_lowercase())))
    }

    /// Önce kibarca (pencereye kapan der), 2 sn sonra hâlâ açıksa (tepsiye saklananlar) zorla.
    fn close(exe: &str) {
        let _ = quiet_command("taskkill").args(["/IM", exe, "/T"]).output();
        thread::sleep(Duration::from_secs(2));
        if running(exe) {
            let _ = quiet_command("taskkill").args(["/F", "/IM", exe, "/T"]).output();
        }
    }

    fn open(app: &AppRef) {
        if !crate::win::shell_open(&app.path) {
            debuglog::line(format!("modlar: açılamadı {}", app.name));
        }
    }

    /// Mod başladı: müzik, uygulama kapat/aç. Kapatılan (o an açık olan) uygulamalar moda yazılır.
    pub fn on_start(hub: Arc<TodayHub>, def: ModeDef) {
        if !def.music {
            crate::media::set_playing(&hub.app, false);
        }
        thread::spawn(move || {
            let closed: Vec<AppRef> = def.close_apps.iter().filter(|a| running(a.exe())).cloned().collect();
            for app in &def.open_apps {
                open(app);
            }
            for app in &closed {
                close(app.exe());
            }
            if closed.is_empty() {
                return;
            }
            super::changed(&hub.app, &hub, |today| match today.mode.as_mut().filter(|r| r.id == def.id) {
                Some(run) => {
                    run.closed = closed;
                    true
                }
                None => false,
            });
        });
    }

    pub fn reopen(apps: Vec<AppRef>) {
        thread::spawn(move || apps.iter().for_each(open));
    }
}

/// Saati gelen etkinlik: arayüzde çentik bildirimi olur. `more`: aynı anda gelen diğerleri.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventDue {
    pub id: u64,
    pub title: String,
    pub time: String,
    pub more: usize,
    /// Ajanda: kaç dakika önce haber verildi (0: tam saatinde) ve planın rengi.
    pub lead: u32,
    pub color: String,
}

/// Bilgisayar uykudaydıysa ya da Zodyvel kapalıydıysa bundan daha geç kalan hatırlatma atlanır.
const LATE_LIMIT_MS: i64 = 10 * 60_000;
/// Saat elle ya da yaz saatiyle değişirse hatırlatma kaymasın: iş parçacığı en çok bu kadar uyur.
const MAX_SLEEP_MS: i64 = 15 * 60_000;

/// 1970-01-01'den bu yana gün (proleptik Gregoryen; H. Hinnant'ın `days_from_civil`'i).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Yerel takvimde bir an, 1970-01-01 00:00'dan bu yana ms. Saat dilimi yoktur: yalnız iki yerel anı
/// karşılaştırmak için (etkinlikler de yerel tarih ve saatle tutulur).
pub fn local_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32, ms: u32) -> i64 {
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    ((days * 24 + i64::from(hour)) * 60 + i64::from(minute)) * 60_000 + i64::from(second) * 1000 + i64::from(ms)
}

fn local_now() -> i64 {
    let (y, mo, d, h, mi, s, ms) = crate::win::local_time();
    local_ms(y, mo, d, h, mi, s, ms)
}

/// `(last, now]` aralığında haber anı gelen planlar (tekrarlarıyla, "x dk önce" payıyla); çok geç kalmış
/// olanlar atlanır. Haber vermeyen ve gün boyu planlar sayılmaz.
pub fn due_events(events: &[CalEvent], last: i64, now: i64) -> Vec<&CalEvent> {
    agenda::due(events, last, now, LATE_LIMIT_MS)
}

/// `now`'dan sonraki ilk haber anına kalan süre (ms).
pub fn next_event_in(events: &[CalEvent], now: i64) -> Option<i64> {
    agenda::next_in(events, now)
}

/// Zamanlayıcının bitişine ya da sıradaki etkinliğin saatine kadar uyur (yoklama yok); hiçbiri yoksa
/// MAX_SLEEP_MS. Her değişiklikte (etkinlik eklendi, zamanlayıcı başladı) uyandırılır.
fn run(hub: Arc<TodayHub>) {
    // Zodyvel açılmadan önce saati geçmiş etkinlikler için hatırlatma yağmaz.
    let mut reminded = local_now();
    loop {
        let now = now_ms();
        let local = local_now();
        // Ayar yalnız uyanınca okunur (bitiş anı, etkinlik saati ya da bir değişiklik).
        let settings = crate::settings::current(&hub.app);
        let (today_on, modes_on, agenda_on) = (settings.modules.today, settings.modules.modes, settings.modules.agenda);
        if !today_on && !modes_on && !agenda_on {
            // İkisi de kapalıyken hiçbir şey yapılmaz; açılınca o andan sonraki hatırlatmalar gelir.
            reminded = local;
            thread::park();
            continue;
        }
        // Saat geri alındıysa aralık yeni andan başlar.
        reminded = reminded.min(local);
        let mut meters_done: Vec<(MeterDone, bool)> = Vec::new();
        let mut steps: Vec<focus::FocusStep> = Vec::new();
        let mut mode_done: Option<modes::ModeDone> = None;
        let (due, waits, snapshot) = {
            let mut today = hub.lock();
            let mut dirty = false;
            // Açık mod "yalnız AI bildirimleri" diyorsa etkinlik hatırlatması (ve sesi) gelmez.
            let only_ai = modes_on && today.active_mode().is_some_and(|m| m.only_ai);
            let due: Vec<CalEvent> = if agenda_on && !only_ai {
                due_events(&today.events, reminded, local).into_iter().cloned().collect()
            } else {
                Vec::new()
            };
            let mut waits: Vec<i64> = Vec::new();
            if agenda_on
                && let Some(wait) = next_event_in(&today.events, local) {
                    waits.push(wait);
                }
            if today_on {
                let day = local_day();
                for meter in today.meters.iter_mut() {
                    if meter.due(now) {
                        meters_done.push((finish_meter(meter, &day), meter.sound));
                        dirty = true;
                    } else if let Some(end) = meter.ends_at {
                        waits.push(end.saturating_sub(now) as i64);
                    }
                }
            }
            if modes_on && today.mode.is_some() {
                let ended = today.mode.as_ref().is_some_and(|r| modes::ended(r, now));
                if ended {
                    let run = today.mode.take().unwrap_or_default();
                    mode_done = Some(modes::done(&run, today.mode_def(&run.id), now));
                    today.reopen = run.closed;
                    dirty = true;
                } else if let Some(run) = today.mode.as_mut() {
                    if let Some(step) = modes::advance_phase(run, now) {
                        steps.push(step);
                        dirty = true;
                    }
                    if let Some(at) = modes::next_wake(run) {
                        waits.push(at.saturating_sub(now) as i64);
                    }
                }
            }
            if dirty {
                hub.save(&today);
            }
            (due, waits, dirty.then(|| today.clone()))
        };
        reminded = local;
        if let Some(today) = snapshot {
            let _ = hub.app.emit("today://changed", &today);
            crate::phone::notify_agenda(&hub.app);
        }
        for (done, sound_on) in meters_done {
            let _ = hub.app.emit("today://timer-done", &done);
            if sound_on {
                sound::play(&hub.app, sound::Cue::Timer);
            }
        }
        for step in steps {
            let _ = hub.app.emit("today://focus", step);
            sound::play(&hub.app, sound::Cue::Timer);
        }
        if let Some(done) = mode_done {
            let _ = hub.app.emit("today://mode-done", &done);
            sound::play(&hub.app, sound::Cue::Timer);
        }
        if let Some(first) = due.first() {
            let message = EventDue {
                id: first.id,
                title: first.title.clone(),
                time: first.time.clone().unwrap_or_default(),
                more: due.len() - 1,
                lead: first.lead,
                color: first.color.clone(),
            };
            let _ = hub.app.emit("today://event-due", &message);
            sound::play(&hub.app, sound::Cue::Timer);
        }
        let wait = waits.into_iter().fold(MAX_SLEEP_MS, i64::min).max(0);
        thread::park_timeout(Duration::from_millis(wait as u64));
    }
}

/// Telefon köprüsü (T2b): bugünden başlayan 14 günün ajanda olayları (yalnız "Telefona da hatırlat" açık
/// planlar; Ajanda modülü kapalıysa boş). Telefona yalnız başlık, tarih, saat, süre ve ön süre gider.
pub fn phone_agenda(app: &AppHandle) -> zodyvel_phone_proto::proto::AgendaView {
    use zodyvel_phone_proto::proto::{AGENDA_DAYS, AgendaView};
    let (y, mo, d, ..) = crate::win::local_time();
    let from = days_from_civil(i64::from(y), i64::from(mo), i64::from(d));
    let Some(hub) = app.try_state::<Arc<TodayHub>>() else { return AgendaView::new(agenda::date_of(from), Vec::new()) };
    let items = if crate::settings::current(app).modules.agenda { agenda::phone_window(&hub.lock().events, from, AGENDA_DAYS) } else { Vec::new() };
    AgendaView::new(agenda::date_of(from), items)
}

#[tauri::command]
pub fn today_snapshot(hub: State<'_, Arc<TodayHub>>) -> Today {
    hub.lock().clone()
}

#[tauri::command]
pub fn today_apply(hub: State<'_, Arc<TodayHub>>, op: Op) -> Result<Today, String> {
    let settings = crate::settings::current(&hub.app);
    if is_mode_op(&op) {
        if !settings.modules.modes {
            return Err("Modlar modülü kapalı (Ayarlar → Modüller).".into());
        }
    } else if is_agenda_op(&op) {
        if !settings.modules.agenda {
            return Err("Ajanda modülü kapalı (Ayarlar → Modüller).".into());
        }
    } else if !settings.modules.today {
        return Err("Bugün modülü kapalı (Ayarlar → Modüller).".into());
    }
    let focus_on = settings.features.focus;
    guard(&op, focus_on)?;
    let starts = matches!(op, Op::ModeStart { .. } | Op::FocusStart);
    let stops = matches!(op, Op::ModeStop);
    let reopens = matches!(op, Op::ModeReopen);
    let (snapshot, started, stopped, reopen) = {
        let mut today = hub.lock();
        let before = today.mode.clone();
        let reopen = if reopens { today.reopen.clone() } else { Vec::new() };
        apply_with(&mut today, op, now_ms(), focus_on)?;
        hub.save(&today);
        let started = if starts { today.active_mode().cloned() } else { None };
        let stopped = before.filter(|_| stops).map(|run| modes::done(&run, today.mode_def(&run.id), now_ms()));
        (today.clone(), started, stopped, reopen)
    };
    hub.wake();
    let _ = hub.app.emit("today://changed", &snapshot);
    crate::phone::notify_agenda(&hub.app);
    if let Some(def) = started {
        effects::on_start(hub.inner().clone(), def);
    }
    // Elle bitirilen mod da Bitiş'i gösterir (kapattığı uygulamaları sormak için).
    if let Some(done) = stopped {
        let _ = hub.app.emit("today://mode-done", &done);
    }
    if !reopen.is_empty() {
        effects::reopen(reopen);
    }
    Ok(snapshot)
}

/// Modlar › uygulama ekle: dosya seçici (.exe; açılacaklarda .lnk de). Vazgeçilirse None.
#[tauri::command]
pub async fn modes_pick_app(app: AppHandle) -> Result<Option<AppRef>, String> {
    let picked = crate::apps::pick_program(&app, "Mod için uygulama seç").await?;
    Ok(picked.map(|path| AppRef { name: crate::apps::default_name(&path), path }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(json: &str) -> Op {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn ops_parse_from_ui_shape() {
        assert!(matches!(op(r#"{"op":"timerStart"}"#), Op::TimerStart { id: None }));
        assert!(matches!(op(r#"{"op":"timerStart","id":4}"#), Op::TimerStart { id: Some(4) }));
        assert!(matches!(op(r#"{"op":"addEvent","date":"2026-10-14","time":"14:00","title":"x"}"#), Op::AddEvent(_)));
        assert!(matches!(op(r#"{"op":"editEvent","id":2,"plan":{"date":"2026-10-14","time":"14:00","title":"x","minutes":30}}"#), Op::EditEvent { id: 2, .. }));
        assert!(matches!(op(r#"{"op":"loopDay","date":"2026-10-14","on":true}"#), Op::LoopDay { on: true, .. }));
        assert!(matches!(op(r#"{"op":"toggleTodo","id":3}"#), Op::ToggleTodo { id: 3 }));
    }

    #[test]
    fn events_are_validated_and_sorted() {
        let mut t = Today::default();
        apply(&mut t, op(r#"{"op":"addEvent","date":"2026-10-15","time":"09:00","title":" Toplantı "}"#), 0).unwrap();
        apply(&mut t, op(r#"{"op":"addEvent","date":"2026-10-14","time":null,"title":"Gün boyu"}"#), 0).unwrap();
        apply(&mut t, op(r#"{"op":"addEvent","date":"2026-10-15","time":"08:30","title":"Erken"}"#), 0).unwrap();
        let titles: Vec<&str> = t.events.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Gün boyu", "Erken", "Toplantı"]);
        for bad in [
            r#"{"op":"addEvent","date":"2026-13-01","time":null,"title":"x"}"#,
            r#"{"op":"addEvent","date":"26-10-01","time":null,"title":"x"}"#,
            r#"{"op":"addEvent","date":"2026-10-01","time":"24:00","title":"x"}"#,
            r#"{"op":"addEvent","date":"2026-10-01","time":"9:00","title":"x"}"#,
            r#"{"op":"addEvent","date":"2026-10-01","time":null,"title":"   "}"#,
        ] {
            assert!(apply(&mut t, op(bad), 0).is_err(), "{bad}");
        }
        let id = t.events[0].id;
        apply(&mut t, Op::RemoveEvent { id }, 0).unwrap();
        assert_eq!(t.events.len(), 2);
        // Düzenleme planı baştan yazar ve sırayı korur; döngü günü her güne çevirir.
        let id = t.events[1].id;
        apply(&mut t, op(&format!(r#"{{"op":"editEvent","id":{id},"plan":{{"date":"2026-10-15","time":"07:00","title":"Koşu","minutes":45,"repeat":"weekdays"}}}}"#)), 0).unwrap();
        assert_eq!((t.events[0].id, t.events[0].minutes, t.events[0].repeat), (id, 45, Repeat::Weekdays));
        assert!(apply(&mut t, op(r#"{"op":"editEvent","id":999,"plan":{"date":"2026-10-15","time":null,"title":"x"}}"#), 0).is_err());
        apply(&mut t, op(r#"{"op":"loopDay","date":"2026-10-15","on":true}"#), 0).unwrap();
        assert_eq!(t.events[1].repeat, Repeat::Daily);
    }

    #[test]
    fn local_time_counts_days_across_months() {
        assert_eq!(local_ms(1970, 1, 1, 0, 0, 0, 0), 0);
        assert_eq!(local_ms(2026, 10, 15, 0, 4, 0, 0) - local_ms(2026, 10, 14, 23, 59, 0, 0), 5 * 60_000);
        assert_eq!(local_ms(2026, 3, 1, 0, 0, 0, 0) - local_ms(2026, 2, 28, 0, 0, 0, 0), 86_400_000);
        assert_eq!(local_ms(2028, 3, 1, 0, 0, 0, 0) - local_ms(2028, 2, 28, 0, 0, 0, 0), 2 * 86_400_000);
    }

    #[test]
    fn events_are_reminded_once_at_their_minute() {
        let ev = |id, date: &str, time: Option<&str>| CalEvent {
            id,
            date: date.into(),
            time: time.map(Into::into),
            title: "x".into(),
            minutes: 0,
            color: String::new(),
            repeat: Repeat::None,
            days: 0,
            notify: true,
            lead: 0,
            phone: true,
        };
        let events = vec![ev(1, "2026-10-15", Some("00:04")), ev(2, "2026-10-15", None), ev(3, "2026-10-15", Some("09:30"))];
        let at = |h, m, s| local_ms(2026, 10, 15, h, m, s, 0);
        // Saat gelmeden yok; gelince bir kez; sonraki aralıkta tekrar yok.
        assert!(due_events(&events, at(0, 0, 0), at(0, 3, 59)).is_empty());
        let due: Vec<u64> = due_events(&events, at(0, 3, 59), at(0, 4, 0)).iter().map(|e| e.id).collect();
        assert_eq!(due, [1]);
        assert!(due_events(&events, at(0, 4, 0), at(0, 5, 0)).is_empty());
        // Uykudan 10 dakikadan geç dönülürse sessiz.
        assert!(due_events(&events, at(0, 0, 0), at(0, 15, 0)).is_empty());
        // Sıradaki saat: gün boyu etkinlik sayılmaz.
        assert_eq!(next_event_in(&events, at(0, 3, 0)), Some(60_000));
        assert_eq!(next_event_in(&events, at(0, 4, 0)), Some(at(9, 30, 0) - at(0, 4, 0)));
        assert_eq!(next_event_in(&events, at(10, 0, 0)), None);
    }

    #[test]
    fn todos_add_toggle_remove() {
        let mut t = Today::default();
        apply(&mut t, Op::AddTodo { text: "Market".into() }, 0).unwrap();
        let id = t.todos[0].id;
        apply(&mut t, Op::ToggleTodo { id }, 0).unwrap();
        assert!(t.todos[0].done);
        assert!(apply(&mut t, Op::AddTodo { text: "x".repeat(121) }, 0).is_err());
        apply(&mut t, Op::RemoveTodo { id }, 0).unwrap();
        assert!(t.todos.is_empty());
    }

    fn meters(t: &Today) -> Vec<(u64, Option<u64>, u64)> {
        t.meters.iter().map(|m| (m.id, m.ends_at, m.left_ms)).collect()
    }

    #[test]
    fn timer_runs_pauses_and_finishes() {
        let mut t = Today::default();
        // Kimliksiz komut ilk Zodymetre'ye gider; hiç yoksa biri oluşur.
        apply(&mut t, op(r#"{"op":"timerSet","minutes":1}"#), 0).unwrap();
        assert_eq!(t.meters.len(), 1);
        apply(&mut t, op(r#"{"op":"timerStart"}"#), 1_000).unwrap();
        let id = t.meters[0].id;
        assert_eq!(meters(&t), [(id, Some(61_000), 60_000)]);
        apply(&mut t, Op::TimerPause { id: Some(id) }, 21_000).unwrap();
        assert_eq!(meters(&t), [(id, None, 40_000)]);
        apply(&mut t, Op::TimerStart { id: None }, 100_000).unwrap();
        assert!(!t.meters[0].due(139_999) && t.meters[0].due(140_000));
        let done = finish_meter(&mut t.meters[0], "2026-11-10");
        assert_eq!((t.meters[0].left_ms, done.rounds, done.total_ms), (0, 1, 60_000));
        // Dolmuş Zodymetre yeniden başlatılınca tam süreyle başlar.
        apply(&mut t, Op::TimerStart { id: Some(id) }, 200_000).unwrap();
        assert_eq!(t.meters[0].ends_at, Some(260_000));
        apply(&mut t, Op::TimerReset { id: Some(id) }, 210_000).unwrap();
        assert_eq!(meters(&t), [(id, None, 60_000)]);
        assert!(apply(&mut t, Op::TimerSet { id: None, minutes: 0 }, 0).is_err());
        assert!(apply(&mut t, Op::TimerStart { id: Some(999) }, 0).is_err());
    }

    #[test]
    fn up_to_three_named_meters_run_side_by_side() {
        let mut t = Today::default();
        let add = |t: &mut Today, name: &str, minutes| {
            apply(t, Op::MeterAdd { name: name.into(), minutes, color: "#2ec4b6".into(), sound: true }, 0)
        };
        add(&mut t, " Okuma ", 25).unwrap();
        add(&mut t, "", 10).unwrap();
        add(&mut t, "Çay", 4).unwrap();
        assert!(add(&mut t, "Dördüncü", 5).is_err(), "en çok üç");
        let names: Vec<&str> = t.meters.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["Okuma", "Zodymetre", "Çay"]);
        let [a, b, _] = [t.meters[0].id, t.meters[1].id, t.meters[2].id];
        apply(&mut t, Op::TimerStart { id: Some(a) }, 0).unwrap();
        apply(&mut t, Op::TimerStart { id: Some(b) }, 60_000).unwrap();
        assert_eq!((t.meters[0].ends_at, t.meters[1].ends_at, t.meters[2].ends_at), (Some(25 * 60_000), Some(11 * 60_000), None));
        // Ad/renk değişince sürer; süre değişince baştan ve durur. Geçersiz renk vurguya döner.
        apply(&mut t, Op::MeterEdit { id: a, name: "Kitap".into(), minutes: 25, color: "mavi".into(), sound: false }, 0).unwrap();
        assert_eq!((t.meters[0].name.as_str(), t.meters[0].color.as_str(), t.meters[0].ends_at), ("Kitap", "", Some(25 * 60_000)));
        apply(&mut t, Op::MeterEdit { id: a, name: "Kitap".into(), minutes: 30, color: String::new(), sound: false }, 0).unwrap();
        assert_eq!((t.meters[0].ends_at, t.meters[0].left_ms), (None, 30 * 60_000));
        apply(&mut t, Op::MeterRemove { id: b }, 0).unwrap();
        assert_eq!(t.meters.len(), 2);
    }

    #[test]
    fn rounds_count_per_day() {
        let mut m = Meter { minutes: 25, ..Meter::default() };
        finish_meter(&mut m, "2026-11-10");
        let done = finish_meter(&mut m, "2026-11-10");
        assert_eq!((done.rounds, done.total_ms), (2, 50 * 60_000));
        let next_day = finish_meter(&mut m, "2026-11-11");
        assert_eq!((next_day.rounds, next_day.total_ms), (1, 25 * 60_000));
    }

    #[test]
    fn timer_add_extends_running_paused_and_finished() {
        let mut t = Today::default();
        apply(&mut t, Op::TimerSet { id: None, minutes: 25 }, 0).unwrap();
        let id = Some(t.meters[0].id);
        apply(&mut t, Op::TimerStart { id }, 0).unwrap();
        apply(&mut t, Op::TimerAdd { id, minutes: 5 }, 1_000).unwrap();
        assert_eq!((t.meters[0].minutes, t.meters[0].ends_at), (30, Some(30 * 60_000)));
        apply(&mut t, Op::TimerPause { id }, 60_000).unwrap();
        apply(&mut t, Op::TimerAdd { id, minutes: 5 }, 60_000).unwrap();
        assert_eq!((t.meters[0].ends_at, t.meters[0].left_ms), (None, 34 * 60_000));
        // Dolmuş Zodymetre yalnız eklenen süreyle hemen başlar.
        finish_meter(&mut t.meters[0], "2026-11-10");
        apply(&mut t, Op::TimerAdd { id, minutes: 5 }, 100_000).unwrap();
        assert_eq!((t.meters[0].minutes, t.meters[0].ends_at, t.meters[0].left_ms), (5, Some(400_000), 5 * 60_000));
        assert!(apply(&mut t, Op::TimerAdd { id, minutes: 0 }, 0).is_err());
        assert!(apply(&mut t, Op::TimerAdd { id, minutes: 61 }, 0).is_err());
        t.meters[0].minutes = MAX_MINUTES;
        apply(&mut t, Op::TimerAdd { id, minutes: 5 }, 0).unwrap();
        assert_eq!(t.meters[0].minutes, MAX_MINUTES, "toplam süre sınırı aşılmaz");
    }

    #[test]
    fn turning_the_module_off_pauses_running_meters() {
        let mut t = Today::default();
        apply(&mut t, Op::TimerSet { id: None, minutes: 1 }, 0).unwrap();
        assert!(!freeze(&mut t, 0), "duran Zodymetre'ye dokunulmaz");
        apply(&mut t, Op::TimerStart { id: None }, 1_000).unwrap();
        assert!(freeze(&mut t, 31_000));
        assert_eq!((t.meters[0].ends_at, t.meters[0].left_ms), (None, 30_000));
        assert!(!t.meters[0].due(u64::MAX), "kapalıyken dolmaz");
    }

    #[test]
    fn old_file_migrates_once_to_meters_and_default_modes() {
        // Bu fazdan önceki today.json: tek zamanlayıcı, duraklatılmış.
        let mut old: Today = serde_json::from_str(r#"{"timer":{"minutes":10,"endsAt":null,"leftMs":240000,"focus":null},"nextId":7}"#).unwrap();
        assert!(old.migrate());
        assert_eq!(old.meters.len(), 1);
        assert_eq!((old.meters[0].id, old.meters[0].minutes, old.meters[0].left_ms, old.meters[0].name.as_str()), (8, 10, 240_000, "Zodymetre"));
        assert_eq!(old.modes.len(), 4);
        assert!(!old.migrate(), "ikinci kez dokunulmaz");
        // Kaydedilen dosyada eski alan yok; geri okununca aynısı.
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("\"timer\""));
        let back: Today = serde_json::from_str(&json).unwrap();
        assert_eq!(back, old);
        // Boş (yeni) dosya: 25 dk'lık bir Zodymetre ve dört mod. Kullanıcı hepsini silerse geri gelmez.
        let mut fresh = Today::default();
        assert!(fresh.migrate());
        assert_eq!((fresh.meters[0].minutes, fresh.modes.len()), (25, 4));
        fresh.meters.clear();
        assert!(!fresh.migrate());
        assert!(fresh.meters.is_empty());
    }

    fn seeded() -> Today {
        let mut t = Today::default();
        t.migrate();
        t
    }

    #[test]
    fn modes_start_pause_add_and_stop_with_reopen() {
        let mut t = seeded();
        assert!(matches!(op(r#"{"op":"modeStart","id":"study"}"#), Op::ModeStart { minutes: None, .. }));
        apply(&mut t, op(r#"{"op":"modeStart","id":"study","minutes":25}"#), 0).unwrap();
        assert_eq!(t.mode.as_ref().map(|r| (r.id.as_str(), r.minutes)), Some(("study", 25)));
        assert_eq!(t.active_mode().map(|m| m.quiet), Some(true));
        assert!(apply(&mut t, op(r#"{"op":"modeStart","id":"work"}"#), 0).is_err(), "aynı anda bir mod");
        assert!(apply(&mut t, Op::ModeRemove { id: "study".into() }, 0).is_err(), "açık mod silinmez");
        apply(&mut t, Op::ModePause, 60_000).unwrap();
        apply(&mut t, Op::ModeAdd { minutes: 5 }, 60_000).unwrap();
        assert_eq!(t.mode.as_ref().map(|r| (r.left_ms, r.paused)), Some((29 * 60_000, true)));
        apply(&mut t, Op::ModeResume, 120_000).unwrap();
        t.mode.as_mut().unwrap().closed = vec![AppRef { name: "Discord".into(), path: r"C:\D\Discord.exe".into() }];
        apply(&mut t, op(r#"{"op":"modeStop"}"#), 130_000).unwrap();
        assert!(t.mode.is_none());
        assert_eq!(t.reopen.len(), 1, "bitince kapattıkları sorulur");
        apply(&mut t, op(r#"{"op":"modeReopen"}"#), 0).unwrap();
        assert!(t.reopen.is_empty());
        assert!(apply(&mut t, Op::ModePause, 0).is_err());
        assert!(apply(&mut t, op(r#"{"op":"modeStart","id":"yok"}"#), 0).is_err());
    }

    #[test]
    fn modes_are_saved_cleaned_and_removed() {
        let mut t = seeded();
        let new = ModeDef { id: String::new(), name: " Gece okuması ".into(), minutes: 40, ..ModeDef::default() };
        apply(&mut t, Op::ModeSave { mode: new }, 0).unwrap();
        let added = t.modes.last().unwrap().clone();
        assert!(added.id.starts_with('m'));
        assert_eq!(added.name, "Gece okuması");
        apply(&mut t, Op::ModeSave { mode: ModeDef { minutes: 0, ..added.clone() } }, 0).unwrap();
        assert_eq!((t.modes.len(), t.modes.last().unwrap().minutes), (5, 0), "aynı kimlik günceller");
        assert!(apply(&mut t, Op::ModeSave { mode: ModeDef { name: " ".into(), ..added.clone() } }, 0).is_err());
        apply(&mut t, Op::ModeRemove { id: added.id }, 0).unwrap();
        assert_eq!(t.modes.len(), 4);
        // Kimliği boş tanım arayüzden de gelir.
        let from_ui: Op = op(r##"{"op":"modeSave","mode":{"name":"Spor","icon":"leaf","color":"#4fcf86","minutes":30}}"##);
        apply(&mut t, from_ui, 0).unwrap();
        assert_eq!(t.modes.last().map(|m| (m.icon.as_str(), m.music)), Some(("leaf", true)));
    }

    #[test]
    fn focus_moved_into_modes() {
        // Odak modu özelliği kapalı: "odak başlat" reddedilir, modlar döngüsüz başlar.
        assert!(guard(&Op::FocusStart, false).is_err());
        assert!(guard(&Op::TimerStart { id: None }, false).is_ok() && guard(&Op::FocusStop, false).is_ok());
        let mut t = seeded();
        apply_with(&mut t, op(r#"{"op":"modeStart","id":"work"}"#), 0, false).unwrap();
        assert_eq!(t.mode.as_ref().unwrap().focus, None);
        apply(&mut t, Op::ModeStop, 0).unwrap();
        // "Odak başlat": odak döngülü ilk mod (Çalışma) döngüyle; "bitir" yalnız döngülü modu kapatır.
        apply(&mut t, op(r#"{"op":"focusStart"}"#), 0).unwrap();
        assert_eq!(t.mode.as_ref().map(|r| (r.id.as_str(), r.focus.is_some())), Some(("work", true)));
        apply(&mut t, Op::FocusStop, 0).unwrap();
        assert!(t.mode.is_none());
        apply(&mut t, op(r#"{"op":"modeStart","id":"game"}"#), 0).unwrap();
        apply(&mut t, Op::FocusStop, 0).unwrap();
        assert!(t.mode.is_some(), "döngüsüz moda dokunmaz");
        assert!(is_mode_op(&Op::FocusStart) && !is_mode_op(&Op::TimerStart { id: None }));
    }
}
