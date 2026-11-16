//! Modlar (Faz 5a, K4): Çalışma, Ders, Uyku, Oyun ve kullanıcının kendi modları. Bir mod başlayınca
//! seçili eylemler bir kez yapılır: Zodyvel bildirimlerini yalnız AI'ya süzmek, müziği duraklatmak,
//! seçili uygulamaları kapatmak ya da açmak, Zody'nin sesini kısmak; isteğe bağlı 25/5 odak döngüsü
//! modun içinde döner. Site engelleme ve Windows Rahatsız Etme yok (yönetici izni ister, K4).
//!
//! Mod da Bugün deposundadır (today.json) ve aynı iş parçacığı bitiş anına kadar uyur (yoklama yok).
//! Bu dosya saf: zaman dışarıdan verilir (testli). Uygulama kapatma/açma `today.rs` `effects`'te.

use serde::{Deserialize, Serialize};

use super::focus::{self, Focus, FocusStep};

pub const MAX_MODES: usize = 8;
/// Bir modun kapatacağı ya da açacağı en çok uygulama (her biri için).
pub const MAX_MODE_APPS: usize = 4;
/// Süreli modun en uzun süresi (10 sa); 0 süresiz demektir.
pub const MAX_MODE_MIN: u32 = 600;
const NAME_MAX: usize = 24;
pub const ICONS: [&str; 6] = ["brief", "book", "moon", "game", "star", "leaf"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppRef {
    pub name: String,
    /// Tam yol (.exe; açılacaklarda .lnk/.url da olur).
    pub path: String,
}

impl AppRef {
    /// Kapatmak için süreç adı: "C:\…\Discord.exe" → "Discord.exe".
    pub fn exe(&self) -> &str {
        self.path.rsplit(['\\', '/']).next().unwrap_or(&self.path)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModeDef {
    pub id: String,
    pub name: String,
    pub icon: String,
    /// "#5b9cff"
    pub color: String,
    /// 0: süresiz.
    pub minutes: u32,
    /// Mod açıkken çentikte yalnız AI bildirimleri (onay, soru, bitti) ve kendi Zodymetre/mod Bitişleri.
    pub only_ai: bool,
    /// true: müziğe dokunulmaz. false: başlarken çalan müzik duraklatılır.
    pub music: bool,
    /// Mod açıkken Zody'nin sesleri kısılır.
    pub quiet: bool,
    /// Modun içinde 25/5 odak döngüsü ("Odak modu" özelliği açıksa).
    pub focus: bool,
    pub close_apps: Vec<AppRef>,
    pub open_apps: Vec<AppRef>,
}

impl Default for ModeDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Yeni mod".into(),
            icon: "star".into(),
            color: "#8b7cff".into(),
            minutes: 50,
            only_ai: false,
            music: true,
            quiet: false,
            focus: false,
            close_apps: Vec::new(),
            open_apps: Vec::new(),
        }
    }
}

fn def(id: &str, name: &str, icon: &str, color: &str, minutes: u32) -> ModeDef {
    ModeDef { id: id.into(), name: name.into(), icon: icon.into(), color: color.into(), minutes, ..ModeDef::default() }
}

/// İlk açılışta gelen dört mod (eskiz paket 2 §4).
pub fn default_modes() -> Vec<ModeDef> {
    vec![
        ModeDef { only_ai: true, focus: true, ..def("work", "Çalışma", "brief", "#5b9cff", 50) },
        ModeDef { only_ai: true, music: false, quiet: true, ..def("study", "Ders", "book", "#2ec4b6", 90) },
        ModeDef { only_ai: true, music: false, quiet: true, ..def("sleep", "Uyku", "moon", "#9d7bff", 480) },
        ModeDef { quiet: true, ..def("game", "Oyun", "game", "#ff7a59", 0) },
    ]
}

fn valid_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn clean_apps(list: Vec<AppRef>, exe_only: bool) -> Vec<AppRef> {
    let mut out: Vec<AppRef> = Vec::new();
    for mut a in list {
        a.path = a.path.trim().to_owned();
        let ok = crate::apps::is_program(&a.path) && (!exe_only || a.exe().to_ascii_lowercase().ends_with(".exe"));
        if !ok || out.iter().any(|x| x.path.eq_ignore_ascii_case(&a.path)) {
            continue;
        }
        let name = a.name.trim();
        a.name = if name.is_empty() { crate::apps::default_name(&a.path) } else { name.chars().take(NAME_MAX).collect() };
        out.push(a);
        if out.len() >= MAX_MODE_APPS {
            break;
        }
    }
    out
}

/// Arayüzden gelen tanımı temizler. Ad boşsa hata; tanımadığımız simge/renk varsayılana döner;
/// kapatılacaklar yalnız .exe (süreç adı gerekir), açılacaklar .exe/.lnk/.url.
pub fn sanitize(mut d: ModeDef) -> Result<ModeDef, String> {
    let name = d.name.trim();
    if name.is_empty() {
        return Err("Modun adı boş olamaz.".into());
    }
    d.name = name.chars().take(NAME_MAX).collect();
    if !ICONS.contains(&d.icon.as_str()) {
        d.icon = "star".into();
    }
    if !valid_color(&d.color) {
        d.color = "#8b7cff".into();
    }
    d.minutes = d.minutes.min(MAX_MODE_MIN);
    d.close_apps = clean_apps(d.close_apps, true);
    d.open_apps = clean_apps(d.open_apps, false);
    Ok(d)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ModeRun {
    pub id: String,
    pub started_at: u64,
    /// Seçilen süre; 0: süresiz.
    pub minutes: u32,
    /// Süreli ve çalışıyorsa bitiş anı.
    pub ends_at: Option<u64>,
    /// Süreli ve duraklatılmışken kalan.
    pub left_ms: u64,
    pub paused: bool,
    /// Odak döngüsü açıksa aşama; süresiz modda da döner (uzun moladan sonra baştan).
    pub focus: Option<Focus>,
    pub phase_ends_at: Option<u64>,
    pub phase_left_ms: u64,
    /// Bu modun kapattığı (başlarken açık olan) uygulamalar; bitince "yeniden açılsın mı?" diye sorulur.
    pub closed: Vec<AppRef>,
}

/// Mod başlar. `minutes` None ise tanımdaki süre. Odak döngüsü tanımda açıksa ve özellik açıksa.
pub fn start(d: &ModeDef, minutes: Option<u32>, now: u64, focus_on: bool) -> ModeRun {
    let minutes = minutes.unwrap_or(d.minutes).min(MAX_MODE_MIN);
    let left_ms = u64::from(minutes) * 60_000;
    let focus = (d.focus && focus_on).then(Focus::default);
    let phase_ms = focus.map_or(0, |f| u64::from(f.phase.minutes()) * 60_000);
    ModeRun {
        id: d.id.clone(),
        started_at: now,
        minutes,
        ends_at: (minutes > 0).then_some(now + left_ms),
        left_ms,
        paused: false,
        focus,
        phase_ends_at: focus.map(|_| now + phase_ms),
        phase_left_ms: phase_ms,
        closed: Vec::new(),
    }
}

pub fn pause(run: &mut ModeRun, now: u64) {
    if run.paused {
        return;
    }
    run.paused = true;
    if let Some(end) = run.ends_at.take() {
        run.left_ms = end.saturating_sub(now);
    }
    if let Some(end) = run.phase_ends_at.take() {
        run.phase_left_ms = end.saturating_sub(now);
    }
}

pub fn resume(run: &mut ModeRun, now: u64) {
    if !run.paused {
        return;
    }
    run.paused = false;
    if run.minutes > 0 {
        run.ends_at = Some(now + run.left_ms);
    }
    if run.focus.is_some() {
        run.phase_ends_at = Some(now + run.phase_left_ms);
    }
}

/// "+N dk": yalnız süreli modda. Toplam en çok MAX_MODE_MIN.
pub fn add(run: &mut ModeRun, minutes: u32) -> Result<(), String> {
    if run.minutes == 0 {
        return Err("Süresiz modda süre eklenmez.".into());
    }
    let room = MAX_MODE_MIN.saturating_sub(run.minutes);
    let minutes = minutes.min(room);
    let add = u64::from(minutes) * 60_000;
    run.minutes += minutes;
    match run.ends_at.as_mut() {
        Some(end) => *end += add,
        None => run.left_ms += add,
    }
    Ok(())
}

/// Kalan süre (ms); süresizde None. (Arayüz kendisi hesaplar; testlerde.)
#[cfg(test)]
pub fn left(run: &ModeRun, now: u64) -> Option<u64> {
    (run.minutes > 0).then(|| run.ends_at.map_or(run.left_ms, |end| end.saturating_sub(now)))
}

/// Modun kendisi doldu mu.
pub fn ended(run: &ModeRun, now: u64) -> bool {
    run.ends_at.is_some_and(|end| end <= now)
}

/// Odak aşaması dolduysa sıradakine geçer (döngü mod sürdükçe baştan döner) ve adımı döndürür.
pub fn advance_phase(run: &mut ModeRun, now: u64) -> Option<FocusStep> {
    let current = run.focus?;
    if run.phase_ends_at.is_none_or(|end| end > now) {
        return None;
    }
    let next = focus::next(current).unwrap_or_default();
    let ms = u64::from(next.phase.minutes()) * 60_000;
    run.focus = Some(next);
    run.phase_ends_at = Some(now + ms);
    run.phase_left_ms = ms;
    Some(FocusStep { ended: current.phase, next: Some(next) })
}

/// İş parçacığının uyanacağı en yakın an.
pub fn next_wake(run: &ModeRun) -> Option<u64> {
    [run.ends_at, run.phase_ends_at].into_iter().flatten().min()
}

/// Biten modun özeti (`today://mode-done`): Mod Bitişi bildirimi.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModeDone {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    /// Geçen süre (dk).
    pub minutes: u32,
    /// Bu modun kapattığı uygulamaların adları (yeniden açma düğmesi).
    pub reopen: Vec<String>,
}

pub fn done(run: &ModeRun, d: Option<&ModeDef>, now: u64) -> ModeDone {
    let minutes = if run.minutes > 0 && ended(run, now) { run.minutes } else { (now.saturating_sub(run.started_at) / 60_000) as u32 };
    ModeDone {
        id: run.id.clone(),
        name: d.map_or_else(|| "Mod".into(), |d| d.name.clone()),
        icon: d.map_or_else(|| "star".into(), |d| d.icon.clone()),
        color: d.map_or_else(|| "#8b7cff".into(), |d| d.color.clone()),
        minutes,
        reopen: run.closed.iter().map(|a| a.name.clone()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::today::focus::Phase;

    #[test]
    fn defaults_are_the_four_sketched_modes_and_clean() {
        let modes = default_modes();
        let ids: Vec<&str> = modes.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["work", "study", "sleep", "game"]);
        for m in modes {
            assert_eq!(sanitize(m.clone()).unwrap(), m, "varsayılanlar temizlikten değişmeden geçer");
        }
    }

    #[test]
    fn sanitize_rejects_empty_name_and_fixes_the_rest() {
        assert!(sanitize(ModeDef { name: "  ".into(), ..ModeDef::default() }).is_err());
        let app = |p: &str| AppRef { name: String::new(), path: p.into() };
        let d = sanitize(ModeDef {
            name: " Gece ".into(),
            icon: "rocket".into(),
            color: "blue".into(),
            minutes: 9999,
            close_apps: vec![app(r"C:\Apps\Discord.exe"), app(r"C:\Apps\Steam.lnk"), app("notepad.exe"), app(r"c:\apps\discord.exe")],
            open_apps: vec![app(r"C:\Users\x\Desktop\Code.lnk")],
            ..ModeDef::default()
        })
        .unwrap();
        assert_eq!((d.name.as_str(), d.icon.as_str(), d.color.as_str(), d.minutes), ("Gece", "star", "#8b7cff", MAX_MODE_MIN));
        // Kapatılacaklar yalnız tam yollu .exe, aynısı bir kez; ad dosyadan.
        assert_eq!(d.close_apps, vec![AppRef { name: "Discord".into(), path: r"C:\Apps\Discord.exe".into() }]);
        assert_eq!(d.close_apps[0].exe(), "Discord.exe");
        assert_eq!(d.open_apps.len(), 1);
    }

    #[test]
    fn timed_mode_pauses_resumes_extends_and_ends() {
        let d = def("work", "Çalışma", "brief", "#5b9cff", 50);
        let mut r = start(&d, None, 1_000, true);
        assert_eq!((r.minutes, r.ends_at, r.focus), (50, Some(1_000 + 50 * 60_000), None));
        pause(&mut r, 61_000);
        assert_eq!((r.ends_at, r.left_ms, r.paused), (None, 49 * 60_000, true));
        assert_eq!(left(&r, u64::MAX), Some(49 * 60_000), "duraklatılmışken azalmaz");
        resume(&mut r, 100_000);
        assert_eq!(r.ends_at, Some(100_000 + 49 * 60_000));
        add(&mut r, 5).unwrap();
        assert_eq!((r.minutes, r.ends_at), (55, Some(100_000 + 54 * 60_000)));
        let end = r.ends_at.unwrap();
        assert!(!ended(&r, end - 1) && ended(&r, end));
        let summary = done(&r, Some(&d), end);
        assert_eq!((summary.minutes, summary.name.as_str()), (55, "Çalışma"));
        // Süre seçilerek başlatma, sınır.
        assert_eq!(start(&d, Some(25), 0, true).minutes, 25);
        let mut long = start(&d, Some(MAX_MODE_MIN - 2), 0, true);
        add(&mut long, 60).unwrap();
        assert_eq!(long.minutes, MAX_MODE_MIN);
    }

    #[test]
    fn endless_mode_never_ends_and_takes_no_extra_time() {
        let d = def("game", "Oyun", "game", "#ff7a59", 0);
        let mut r = start(&d, None, 0, true);
        assert_eq!((r.ends_at, left(&r, 10)), (None, None));
        assert!(!ended(&r, u64::MAX));
        assert!(add(&mut r, 5).is_err());
        assert_eq!(next_wake(&r), None, "süresiz ve döngüsüz mod iş parçacığını uyandırmaz");
        assert_eq!(done(&r, Some(&d), 30 * 60_000).minutes, 30);
    }

    #[test]
    fn focus_cycle_runs_inside_the_mode_and_wraps() {
        let d = ModeDef { focus: true, ..def("work", "Çalışma", "brief", "#5b9cff", 0) };
        assert_eq!(start(&d, None, 0, false).focus, None, "özellik kapalıysa döngü yok");
        let mut r = start(&d, None, 0, true);
        assert_eq!((r.focus, r.phase_ends_at), (Some(Focus::default()), Some(25 * 60_000)));
        assert_eq!(advance_phase(&mut r, 25 * 60_000 - 1), None);
        let step = advance_phase(&mut r, 25 * 60_000).unwrap();
        assert_eq!((step.ended, step.next.map(|f| f.phase)), (Phase::Work, Some(Phase::Break)));
        // Duraklatınca aşama da durur.
        pause(&mut r, 26 * 60_000);
        assert_eq!((r.phase_ends_at, r.phase_left_ms), (None, 4 * 60_000));
        resume(&mut r, 40 * 60_000);
        assert_eq!(r.phase_ends_at, Some(44 * 60_000));
        // Uzun moladan sonra baştan.
        r.focus = Some(Focus { phase: Phase::LongBreak, round: 4 });
        let at = r.phase_ends_at.unwrap();
        let step = advance_phase(&mut r, at).unwrap();
        assert_eq!(step.next, Some(Focus::default()));
        assert_eq!(next_wake(&r), r.phase_ends_at);
    }
}
