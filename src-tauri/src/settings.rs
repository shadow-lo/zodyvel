//! Kalıcı ayarlar ve ayarlar penceresi. Pencere yalnız istendiğinde oluşturulur, kapanınca yok edilir.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::webview::Color;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::sound::{self, Cue};
use crate::{shortcuts, tray};

pub const WINDOW_LABEL: &str = "settings";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Modules {
    pub agents: bool,
    pub music: bool,
    pub system: bool,
    pub today: bool,
    /// Modlar sekmesi (Faz 5a): Çalışma, Ders, Uyku, Oyun ve kullanıcının modları.
    pub modes: bool,
    /// Ajanda sekmesi (Faz 5b): saat saat planlar, tekrar, Gün/Ay görünümü ve plan hatırlatmaları.
    pub agenda: bool,
    /// Telefon köprüsü (Plan 3 · T2): varsayılan KAPALI ve arayüzde gizli; kapalıyken soket/iş parçacığı/RAM yok.
    pub phone: bool,
    /// Zody asistanı (Faz 12, Deneysel): varsayılan KAPALI; kapalıyken süreç ve bellek yok.
    pub brain: bool,
}

impl Default for Modules {
    fn default() -> Self {
        Self { agents: true, music: true, system: true, today: true, modes: true, agenda: true, phone: false, brain: false }
    }
}

/// Zody asistanı (Faz 12): model seçimi. "auto" (kısa istek Haiku, uzun iş Sonnet), "fast" ya da "long".
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct BrainPrefs {
    pub model: String,
    /// Erişim seviyesi (Faz 13): "closed" | "low" | "mid". Yalnız `access_set` değiştirir; `update_settings` korur.
    pub access: String,
    /// Dosya kopyalama/taşıma eylemlerinin çalışabildiği klasörler (Faz 13b). Boşsa dosya eylemleri hata verir.
    /// Yalnız Ayarlar sayfası yazar (`update_settings` Zody'ye kapalı).
    pub allowed_folders: Vec<String>,
}

impl Default for BrainPrefs {
    fn default() -> Self {
        Self { model: "auto".into(), access: "closed".into(), allowed_folders: Vec::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct IslandPrefs {
    pub hover_delay_ms: u64,
    pub reduce_motion: bool,
    /// Tüm seslerin ana anahtarı (sustur kısayolu ve tepsi bunu değiştirir); ayrıntı `Sounds`'ta.
    pub sound: bool,
    /// Bir ajanın turu bitince/hata verince 5 sn'lik çentik bildirimi.
    pub flash: bool,
    /// Üzerine gelince açılacak bölüm: "auto" (müzik çalıyorsa oynatıcı), "ai" ya da "last" (en son
    /// bırakılan bölüm).
    pub hover: String,
    /// Üzerine gelince mini açılmadan önce maskot kafasını çıkarıp selam verir (peek).
    pub peek: bool,
    /// Açılış: "hover" (üzerine gelince mini) ya da "click" (üzerine gelince yalnız Zody, tıkla mini,
    /// bir tık daha ayrıntı).
    pub reveal: String,
    /// Uygulama açılınca kısa selam.
    pub greet: bool,
    /// Tanıtım turu görüldü. Eski ayar dosyalarında yok: güncellemeden sonra tur bir kez görünür.
    pub welcomed: bool,
    /// Küçük görünümde simge çubuğunun boş yerine tıklayınca ayrıntı açılır.
    pub bar_open: bool,
    /// Hangi ekranda: "primary" (birincil), "fixed" (`screen_id` ekranı; yoksa birincil) ya da "cursor".
    pub screen: String,
    /// Seçilen ekranın Windows aygıt adı (`\\.\DISPLAY2`).
    pub screen_id: String,
    /// Ada boyutu yüzdesi, %85–%130 (adım 5).
    pub size: u32,
    /// Ses tuşlarında Zody göstergesi (Faz 10, osd.rs): açıkken Windows'un ses penceresi çıkmaz.
    pub volume_osd: bool,
    /// En son açılışta görülen sürüm ("0.7.1"); güncelleme sonrası bir kereliğe "güncellendi" notu için. Boş: ilk açılış.
    pub last_seen_version: String,
}

impl Default for IslandPrefs {
    fn default() -> Self {
        Self {
            hover_delay_ms: 140,
            reduce_motion: false,
            sound: true,
            flash: true,
            hover: "auto".into(),
            peek: true,
            reveal: "hover".into(),
            greet: true,
            welcomed: false,
            bar_open: true,
            screen: "primary".into(),
            screen_id: String::new(),
            size: 100,
            volume_osd: false,
            last_seen_version: String::new(),
        }
    }
}

/// Bildirim sesleri: hangi set, hangi olaylar ve kullanıcının kendi dosyaları.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Sounds {
    /// "zody" (kendi setimiz, uygulamaya gömülü) ya da "windows" (kullanıcının Windows ses düzeni).
    pub pack: String,
    pub attention: bool,
    pub done: bool,
    pub error: bool,
    pub timer: bool,
    /// Zodyvel açılınca kısa selam.
    pub hello: bool,
    /// Windows "Rahatsız etme" açıkken hiç ses çalınmaz.
    pub respect_quiet: bool,
    /// Kullanıcının yüklediği sesler: olay adı → özgün dosya adı (yalnız gösterim için). Dosyanın
    /// kendisi ayar klasöründe `sounds/<olay>.wav`; varsa setin yerine o çalar.
    pub custom: BTreeMap<String, String>,
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            pack: "zody".into(),
            attention: true,
            done: true,
            error: true,
            timer: true,
            hello: false,
            respect_quiet: true,
            custom: BTreeMap::new(),
        }
    }
}

impl Sounds {
    pub fn enabled(&self, cue: Cue) -> bool {
        match cue {
            Cue::Attention => self.attention,
            Cue::Done => self.done,
            Cue::Error => self.error,
            Cue::Timer => self.timer,
            Cue::Hello => self.hello,
        }
    }
}

const PACKS: [&str; 2] = ["zody", "windows"];

/// Küçük, tek tek açılıp kapanan özellikler (modüllerin içindeki eklemeler). Kural: kapalı özellik veri
/// okumaz, olay dinlemez, arayüzde yer kaplamaz. Bildirim yoran şeyler varsayılan kapalı.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Features {
    /// Onay/soru ya da yanıt `wait_alert_min` dakikadır bekliyorsa bir kez hatırlatır.
    pub wait_alert: bool,
    pub wait_alert_min: u32,
    /// "Bitti" bildiriminde tur süresi ve değişen dosya sayısı.
    pub done_summary: bool,
    /// Bilgisayar ekranında pil (yalnız pili olan bilgisayarda görünür).
    pub battery: bool,
    /// Şarja takılınca/çıkınca ve pil azalınca çentik bildirimi.
    pub battery_alerts: bool,
    /// Yeni görünümde müzik kartı kapağın baskın rengini alır (kapak değişince bir kez hesaplanır).
    pub cover_tint: bool,
    /// Zamanlayıcıda odak döngüsü: 25 dk odak, 5 dk mola; dördüncü odaktan sonra 15 dk uzun mola.
    pub focus: bool,
    /// Claude plan modunda planını sunduğunda planı adada okuyup onaylama kartı.
    pub plan_review: bool,
    /// Claude kullanım ve tahmini maliyet özeti (yerel kayıtlardan; yalnız panel açıkken okunur).
    pub usage: bool,
    /// Pazartesi geçen haftanın özetini çentikte bir kez gösterir.
    pub weekly_recap: bool,
    /// Adaya bırakılan dosyalar rafta durur, oradan başka uygulamaya sürüklenir (yalnız yolları tutulur).
    pub shelf: bool,
    /// AI ayrıntısından çalışan Claude oturumuna not: tur biterken ya da bir sonraki araç çağrısında iletilir.
    pub agent_notes: bool,
    /// AI ayrıntısında oturumun modeli ve tahmini bağlam doluluğu (oturumun kendi kaydından, yalnız ayrıntı açıkken).
    pub agent_context: bool,
    /// Aynı komut art arda hata verince "döngüde olabilir" uyarısı.
    pub loop_alert: bool,
    /// Mini adada yazma sahnesi (✎ ve kısayol): açık oturuma not/görev; bugünkü "Ajana not" buraya taşınır.
    pub write_scene: bool,
    /// Yazma sahnesinden yeni Claude Code görevi başlatma ve kapanmış oturuma devam (`--resume`).
    pub new_task: bool,
    /// Onay/soru kartında "Sonra ⌄": kart cevaplanmadan katlanır, simge çubuğunda hap kalır.
    pub fold_card: bool,
    /// AI ayrıntısında Claude plan limiti hapı (5 saatlik ve haftalık; Claude Code durum satırı köprüsünden).
    pub plan_limit: bool,
    /// "Onaya git" ve "Terminale git" kısayolları (Kısayollar'dan değişir). Yalnız Yeni görünümde kaydedilir.
    pub jump_keys: bool,
    /// Rafta dosyaya "Claude'a sor": yazma sahnesi o dosya ekli açılır (yazma sahnesi açıkken).
    pub shelf_ask: bool,
    /// Kaynaklar (ana anahtar): AI sekmesi Yeni görünümde "Kaynaklar" olur; Mail ve GitHub gibi AI dışı
    /// kaynaklar adada. Kapalıyken hiçbir kaynak ağa çıkmaz.
    pub sources: bool,
    /// Kaynak: Gmail (IMAP, uygulama şifresi Windows Kimlik Bilgisi Yöneticisi'nde).
    pub source_mail: bool,
    /// Kaynak: GitHub (klasik token Windows Kimlik Bilgisi Yöneticisi'nde; PR, CI, bildirimler).
    pub source_github: bool,
    /// GitHub kaynağında "Depolarım": bilgisayardaki depolarda commitlenmemiş değişiklikler, adadan commit ve
    /// push (onaylı). Yalnız istenince `git status` çalışır; yoklama yok.
    pub local_git: bool,
    /// Ada açılırken en son etkinlik olan bölüm (son ajan değişikliği, yeni mail/CI, çalan şarkı,
    /// dolan zamanlayıcı) seçilir; bekleyen onay her zaman önce gelir.
    pub recent_tab: bool,
    /// Üst çubukta Zody sekmesi: Zody'ye yazarak komut (kural tabanlı, eylem kaydından; model yok). Yalnız Yeni görünümde.
    pub zody_chat: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            wait_alert: false,
            wait_alert_min: 10,
            done_summary: true,
            battery: true,
            battery_alerts: true,
            cover_tint: true,
            focus: true,
            plan_review: true,
            usage: true,
            weekly_recap: false,
            shelf: true,
            agent_notes: true,
            agent_context: true,
            loop_alert: true,
            write_scene: true,
            new_task: true,
            fold_card: true,
            plan_limit: false,
            jump_keys: true,
            shelf_ask: true,
            sources: false,
            source_mail: false,
            source_github: false,
            local_git: false,
            recent_tab: false,
            zody_chat: true,
        }
    }
}

const WAIT_ALERT_MIN: [u32; 3] = [5, 10, 20];

/// Yerel Git'in bakacağı klasörler (Ayarlar › Kaynaklar › GitHub). Klasör depo ise kendisi, değilse 2 kat
/// altındaki depolar. Yalnız yol tutulur.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct GitPrefs {
    pub folders: Vec<String>,
}

/// En çok bu kadar klasör.
const MAX_GIT_FOLDERS: usize = 20;

/// Tema (vurgu rengi), ada zemini ve görünüm; değerler src/lib/theme.ts ile aynı.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub theme: String,
    pub surface: String,
    /// Faz 2a'dan (K2) beri tek görünüm var: her zaman "new". Eski dosyalarla uyum için alan duruyor.
    pub look: String,
    /// Eski görünüm sorusu kalktı; her zaman true.
    pub look_asked: bool,
    /// Düzen (Faz 2b): "zody" (üzerine gelince mini AI) ya da "clock" (Saat ana ekranı). Sekmeler aynı.
    pub layout: String,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { theme: "gece".into(), surface: "siyah".into(), look: "new".into(), look_asked: true, layout: "zody".into() }
    }
}


/// Saat ana ekranının kartları (Faz 3); kimlikler src/lib/clock-cards.ts ile aynı.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ClockPrefs {
    /// Seçilen kartlar, gösterim sırasıyla (en çok MAX_CLOCK_CARDS, tekrar yok).
    pub cards: Vec<String>,
    /// Büyük saat görünsün mü; kapalıysa kartlar bütün genişliği kullanır.
    pub show_time: bool,
    /// Not kartının metni (en çok MAX_NOTE karakter).
    pub note: String,
}

impl Default for ClockPrefs {
    fn default() -> Self {
        Self { cards: ["music", "ai", "agenda", "shelf"].map(String::from).to_vec(), show_time: true, note: String::new() }
    }
}

pub const CLOCK_CARDS: [&str; 8] = ["music", "meter", "ai", "agenda", "mail", "shelf", "note", "pc"];
pub const MAX_CLOCK_CARDS: usize = 6;
pub const MAX_NOTE: usize = 280;

impl ClockPrefs {
    fn sanitize(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        for card in self.cards.drain(..) {
            if CLOCK_CARDS.contains(&card.as_str()) && !seen.contains(&card) && seen.len() < MAX_CLOCK_CARDS {
                seen.push(card);
            }
        }
        self.cards = seen;
        self.note = self.note.trim().chars().take(MAX_NOTE).collect();
    }
}

/// Canlı çentik (Faz 4, Ayarlar › Bildirimler › Çentik); alanlar src/lib/types.ts NotchPrefs ile aynı.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct NotchPrefs {
    /// Süren etkinlikler (Zodymetre, AI çalışıyor, müzik) ada kapalıyken çentikte kalıcı görünür.
    pub live: bool,
    /// Fare gelince: "peek" ara boy, "open" ilgili sekme, "click" yalnız tıklayınca, "mini" mini pencere (Faz 9).
    pub hover: String,
    pub timer: bool,
    /// Açık mod (Faz 5a).
    pub mode: bool,
    pub ai: bool,
    pub music: bool,
    /// "Mini pencere" kipinde hangi haplar mini pencere açar (Faz 9); kapalı olan ara boyda kalır.
    pub mini_ai: bool,
    pub mini_music: bool,
    pub mini_timer: bool,
}

impl Default for NotchPrefs {
    fn default() -> Self {
        Self { live: true, hover: "peek".into(), timer: true, mode: true, ai: true, music: true, mini_ai: true, mini_music: true, mini_timer: true }
    }
}

impl NotchPrefs {
    fn sanitize(&mut self) {
        if !matches!(self.hover.as_str(), "peek" | "open" | "click" | "mini") {
            self.hover = Self::default().hover;
        }
    }
}

const THEMES: [&str; 6] = ["gece", "okyanus", "gunbatimi", "orman", "gul", "grafit"];
const SURFACES: [&str; 3] = ["siyah", "grafit", "lacivert"];

/// Genel kısayollar ("Ctrl+Alt+Z" biçiminde; boş: kapalı).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Shortcuts {
    pub toggle: String,
    pub approve: String,
    pub deny: String,
    /// Tüm sesleri kapat/aç (her zaman kayıtlı).
    pub mute: String,
    /// Yazma sahnesini aç (her zaman kayıtlı; Özellikler → Yazma sahnesi kapalıyken kaydedilmez).
    pub write: String,
    /// Bekleyen onay kartını aç (katlanmışsa açar). Özellikler → Onaya/terminale git kısayolları.
    pub approval: String,
    /// Dikkat isteyen oturumun penceresini öne getir. Aynı özellik anahtarına bağlı.
    pub terminal: String,
    /// Acil durdurma (her zaman kayıtlı): bekleyen kartları siler, beyin isteğini iptal eder, seviyeyi bu oturum için Kapalı yapar.
    pub stop: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+Z".into(),
            approve: "Ctrl+Alt+Enter".into(),
            deny: "Ctrl+Alt+Backspace".into(),
            mute: "Ctrl+Alt+M".into(),
            write: "Ctrl+Alt+Y".into(),
            approval: "Ctrl+Alt+O".into(),
            terminal: "Ctrl+Alt+T".into(),
            stop: "Ctrl+Alt+X".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub modules: Modules,
    pub island: IslandPrefs,
    pub sounds: Sounds,
    pub appearance: Appearance,
    pub shortcuts: Shortcuts,
    pub features: Features,
    pub git: GitPrefs,
    /// Sekme çubuğundaki uygulama kısayolları (apps.rs).
    pub apps: Vec<crate::apps::AppShortcut>,
    /// Saat ana ekranı: kartlar, büyük saat, not (Faz 3).
    pub clock: ClockPrefs,
    /// Canlı çentik: kalıcılık, fare davranışı, hangi etkinlikler (Faz 4).
    pub notch: NotchPrefs,
    /// Zody asistanı ayarları (Faz 12).
    pub brain: BrainPrefs,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            modules: Modules::default(),
            island: IslandPrefs::default(),
            sounds: Sounds::default(),
            appearance: Appearance::default(),
            shortcuts: Shortcuts::default(),
            features: Features::default(),
            git: GitPrefs::default(),
            apps: Vec::new(),
            clock: ClockPrefs::default(),
            notch: NotchPrefs::default(),
            brain: BrainPrefs::default(),
        }
    }
}

impl Settings {
    /// Hiç ayar dosyası yokken (yeni kurulum). Görünüm tek (K2), varsayılanla aynı.
    fn fresh() -> Self {
        Self::default()
    }

    /// Pil okunacak mı: Bilgisayar ekranındaki pil (modül açıksa) ya da pil bildirimleri.
    pub fn power_wanted(&self) -> bool {
        (self.modules.system && self.features.battery) || self.features.battery_alerts
    }

    /// Kaydedilecek kısayollar: kapalı özelliğin kısayolu (yazma sahnesi, onaya/terminale git) tuş çalmaz.
    pub fn active_shortcuts(&self) -> Shortcuts {
        let mut keys = self.shortcuts.clone();
        if !self.features.write_scene {
            keys.write.clear();
        }
        if !self.features.jump_keys {
            keys.approval.clear();
            keys.terminal.clear();
        }
        keys
    }

    fn sanitized(mut self) -> Self {
        self.version = 1;
        self.island.hover_delay_ms = self.island.hover_delay_ms.clamp(60, 800);
        if !matches!(self.island.hover.as_str(), "auto" | "ai" | "last") {
            self.island.hover = "auto".into();
        }
        if !matches!(self.island.reveal.as_str(), "hover" | "click") {
            self.island.reveal = "hover".into();
        }
        if !crate::display::Mode::NAMES.contains(&self.island.screen.as_str()) {
            self.island.screen = "primary".into();
        }
        self.island.screen_id = self.island.screen_id.trim().chars().take(64).collect();
        self.island.last_seen_version = self.island.last_seen_version.trim().chars().take(32).collect();
        self.island.size = crate::display::clamp_size(self.island.size);
        if !PACKS.contains(&self.sounds.pack.as_str()) {
            self.sounds.pack = Sounds::default().pack;
        }
        self.sounds.custom.retain(|cue, _| Cue::parse(cue).is_some());
        for name in self.sounds.custom.values_mut() {
            *name = name.trim().chars().take(80).collect();
        }
        let defaults = Appearance::default();
        if !THEMES.contains(&self.appearance.theme.as_str()) {
            self.appearance.theme = defaults.theme;
        }
        if !SURFACES.contains(&self.appearance.surface.as_str()) {
            self.appearance.surface = defaults.surface;
        }
        // Eski "Klasik" görünüm kalktı (K2): kim ne seçmiş olursa olsun tek görünüm.
        self.appearance.look = defaults.look;
        self.appearance.look_asked = true;
        if !matches!(self.appearance.layout.as_str(), "zody" | "clock") {
            self.appearance.layout = defaults.layout;
        }
        self.apps = crate::apps::sanitize(std::mem::take(&mut self.apps));
        self.clock.sanitize();
        self.notch.sanitize();
        if !matches!(self.brain.model.as_str(), "auto" | "fast" | "long") {
            self.brain.model = "auto".into();
        }
        if crate::actions::spec::Erisim::parse(&self.brain.access).is_none() {
            self.brain.access = "closed".into();
        }
        self.brain.allowed_folders = crate::actions::ops::izinli_klasorleri_ayikla(std::mem::take(&mut self.brain.allowed_folders));
        if !WAIT_ALERT_MIN.contains(&self.features.wait_alert_min) {
            self.features.wait_alert_min = Features::default().wait_alert_min;
        }
        let mut seen = std::collections::BTreeSet::new();
        self.git.folders = std::mem::take(&mut self.git.folders)
            .iter()
            .filter_map(|f| crate::localgit::core::clean_folder(f))
            .filter(|f| seen.insert(f.to_lowercase()))
            .take(MAX_GIT_FOLDERS)
            .collect();
        let keys = &mut self.shortcuts;
        keys.toggle = shortcuts::normalize(&keys.toggle);
        keys.approve = shortcuts::normalize(&keys.approve);
        keys.deny = shortcuts::normalize(&keys.deny);
        keys.mute = shortcuts::normalize(&keys.mute);
        keys.write = shortcuts::normalize(&keys.write);
        keys.approval = shortcuts::normalize(&keys.approval);
        keys.terminal = shortcuts::normalize(&keys.terminal);
        keys.stop = shortcuts::normalize(&keys.stop);
        // Aynı birleşim iki işe atanamaz; sonraki kapatılır.
        if !keys.approve.is_empty() && keys.approve == keys.toggle {
            keys.approve.clear();
        }
        if !keys.deny.is_empty() && (keys.deny == keys.toggle || keys.deny == keys.approve) {
            keys.deny.clear();
        }
        if !keys.mute.is_empty() && [&keys.toggle, &keys.approve, &keys.deny].contains(&&keys.mute) {
            keys.mute.clear();
        }
        if !keys.write.is_empty() && [&keys.toggle, &keys.approve, &keys.deny, &keys.mute].contains(&&keys.write) {
            keys.write.clear();
        }
        if !keys.approval.is_empty() && [&keys.toggle, &keys.approve, &keys.deny, &keys.mute, &keys.write].contains(&&keys.approval) {
            keys.approval.clear();
        }
        if !keys.terminal.is_empty()
            && [&keys.toggle, &keys.approve, &keys.deny, &keys.mute, &keys.write, &keys.approval].contains(&&keys.terminal)
        {
            keys.terminal.clear();
        }
if !keys.stop.is_empty()            && [&keys.toggle, &keys.approve, &keys.deny, &keys.mute, &keys.write, &keys.approval, &keys.terminal].contains(&&keys.stop)        {            keys.stop.clear();        }
        self
    }
}

pub struct Store {
    path: PathBuf,
    current: Mutex<Settings>,
}

pub fn init(app: &AppHandle) -> tauri::Result<Settings> {
    let path = app.path().app_config_dir()?.join("settings.json");
    // Dosya yoksa yeni kurulum; varsa ama okunamıyorsa eski kullanıcının görünümü değişmesin diye Klasik.
    let current = match fs::read(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::fresh(),
        read => read.ok().and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok()).unwrap_or_default(),
    }
    .sanitized();
    app.manage(Store { path, current: Mutex::new(current.clone()) });
    Ok(current)
}

fn save(path: &PathBuf, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    // Önce geçici dosyaya yaz, sonra yer değiştir: yarım kalmış ayar dosyası oluşmaz.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(&tmp, path)
}

/// Geçerli ayarlar (Rust'ta karar verenler için: sesler).
pub fn current(app: &AppHandle) -> Settings {
    app.try_state::<Store>().map(|store| store.current.lock().unwrap_or_else(|p| p.into_inner()).clone()).unwrap_or_default()
}

/// Ayar klasörü (kullanıcının yüklediği sesler burada durur).
pub fn data_dir(app: &AppHandle) -> Option<PathBuf> {
    app.try_state::<Store>().and_then(|store| store.path.parent().map(PathBuf::from))
}

/// Geçerli ayarları değiştirip kaydeder ve her yere yayar (arayüz dışından: sustur kısayolu, tepsi, ses yükleme).
pub fn modify(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let mut next = current(app);
    change(&mut next);
    apply(app, next)
}

/// Sustur: tüm seslerin ana anahtarını çevirir; yeni durumu döndürür.
pub fn toggle_sound(app: &AppHandle) -> Result<bool, String> {
    modify(app, |s| s.island.sound = !s.island.sound).map(|s| s.island.sound)
}

#[tauri::command]
pub fn get_settings(store: State<'_, Store>) -> Settings {
    store.current.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

#[tauri::command]
pub fn update_settings(app: AppHandle, mut next: Settings) -> Result<Settings, String> {
    // Erişim seviyesini yalnız `access_set` değiştirir: arayüzün genel ayar yazımı (ya da bayat bir kopya) onu ezemez.
    next.brain.access = current(&app).brain.access;
    apply(&app, next)
}

fn apply(app: &AppHandle, next: Settings) -> Result<Settings, String> {
    let store = app.try_state::<Store>().ok_or("Ayarlar hazır değil.")?;
    let next = next.sanitized();
    // Yüklenen ses kaldırıldıysa dosyası da silinir.
    let dropped: Vec<Cue> = {
        let current = store.current.lock().unwrap_or_else(|p| p.into_inner());
        sound::CUES.into_iter().filter(|c| current.sounds.custom.contains_key(c.name()) && !next.sounds.custom.contains_key(c.name())).collect()
    };
    save(&store.path, &next).map_err(|e| format!("Ayarlar kaydedilemedi: {e}"))?;
    *store.current.lock().unwrap_or_else(|p| p.into_inner()) = next.clone();
    for cue in dropped {
        sound::remove_custom(app, cue);
    }
    tray::sync_sound(app, next.island.sound);
    // Modül kapalıyken köprü olayları hiç işlenmez; araçlar kendi akışında devam eder.
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::agents::AgentHub>>() {
        hub.set_enabled(next.modules.agents);
    }
    if let Some(hub) = app.try_state::<std::sync::Arc<crate::media::MediaHub>>() {
        crate::media::set_enabled(&hub, next.modules.music);
    }
    shortcuts::configure(app, &next.active_shortcuts());
    crate::island::configure(app, &next.island);
    crate::osd::configure(app, next.island.volume_osd);
    crate::power::set_enabled(app, next.power_wanted());
    crate::today::set_enabled(app, next.modules.today);
    crate::phone::set_enabled(app, next.modules.phone);
    crate::brain::set_enabled(app, next.modules.brain);
    crate::today::set_modes_enabled(app, next.modules.modes);
    if !next.features.usage {
        crate::usage::forget(app);
    }
    crate::sources::configure(app, &next);
    let _ = app.emit("settings://changed", &next);
    Ok(next)
}

/// Hakkında → "Klasörü aç": ayarlar ve Bugün verisinin durduğu klasör.
#[tauri::command]
pub fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if crate::win::shell_open(&dir.to_string_lossy()) { Ok(()) } else { Err("Klasör açılamadı.".into()) }
}

/// Async olmalı: Windows'ta eşzamanlı komut ana iş parçacığında çalışır ve orada pencere kurmak
/// WebView2'yi kilitler, uygulama donup kapanır (Tauri'nin belgelediği durum). Tepsi ve tek örnek
/// yolları olay döngüsünden çağırdığı için onlarda sorun yok.
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    open_window(&app);
}

/// WebView2 bayrakları; ada penceresindekiyle (tauri.conf.json additionalBrowserArgs) aynı olmalı, yoksa
/// ikinci pencerenin WebView2 ortamı kurulamaz. İlk üçü Tauri'nin varsayılanı. CalculateNativeWinOcclusion
/// kapalı: Chromium başka bir pencere (ör. bu ayarlar penceresi) adanın pencere dikdörtgenini örtünce adayı
/// "görünmüyor" sayıp çizimi ve zamanlayıcıları kısıyordu; ada bölgesi büyük kalıp boş bir şerit kalıyordu.
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion";

pub fn open_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("settings.html".into()))
        .title("Zodyvel · Ayarlar")
        .inner_size(900.0, 640.0)
        .min_inner_size(760.0, 520.0)
        .background_color(Color(16, 17, 25, 255))
        .additional_browser_args(BROWSER_ARGS)
        .center()
        .build();
    match built {
        Ok(window) => {
            // Kısayol kaydı sırasında pencere kapanırsa kısayollar bırakılmış kalmasın.
            let app = app.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::Destroyed = event {
                    shortcuts::set_paused(&app, false);
                }
            });
        }
        Err(error) => eprintln!("Ayarlar penceresi açılamadı: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_file_gets_default_shortcuts() {
        let old: Settings = serde_json::from_str(r#"{"version":1,"modules":{"agents":true,"music":false,"system":true}}"#).unwrap();
        assert!(!old.modules.music);
        assert!(old.modules.modes, "Modlar modülü eski dosyada varsayılan açık");
        assert!(old.modules.agenda, "Ajanda modülü eski dosyada varsayılan açık");
        assert_eq!(old.shortcuts, Shortcuts::default());
    }

    #[test]
    fn shortcuts_are_normalized_and_deduplicated() {
        let shortcuts = Shortcuts {
            toggle: "alt+ctrl+z".into(),
            approve: "Ctrl+Alt+Z".into(),
            deny: "Shift+X".into(),
            mute: "ctrl+alt+z".into(),
            write: "Alt+Ctrl+z".into(),
            approval: "ctrl+alt+o".into(),
            terminal: "Alt+Ctrl+O".into(),
            stop: "Ctrl+alt+Z".into(),
        };
        let s = Settings { shortcuts, ..Settings::default() }.sanitized();
        assert_eq!(
            s.shortcuts,
            Shortcuts {
                toggle: "Ctrl+Alt+Z".into(),
                approve: String::new(),
                deny: String::new(),
                mute: String::new(),
                write: String::new(),
                approval: "Ctrl+Alt+O".into(),
                terminal: String::new(),
                stop: String::new(),
            }
        );
    }


    #[test]
    fn access_defaults_closed_and_bad_values_fall_back() {
        let old: Settings = serde_json::from_str(r#"{"version":1,"brain":{"model":"fast"}}"#).unwrap();
        assert_eq!(old.brain.access, "closed");
        let mut s = Settings::default();
        s.brain.access = "root".into();
        assert_eq!(s.sanitized().brain.access, "closed");
        let mut s = Settings::default();
        s.brain.access = "mid".into();
        assert_eq!(s.sanitized().brain.access, "mid");
        assert_eq!(Shortcuts::default().stop, "Ctrl+Alt+X");
    }
    #[test]
    fn notch_prefs_default_and_sanitize() {
        let old: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert_eq!(old.notch, NotchPrefs::default(), "eski dosya varsayılan çentikle açılır");
        let odd = Settings { notch: NotchPrefs { hover: "wheel".into(), live: false, ..NotchPrefs::default() }, ..Settings::default() }.sanitized();
        assert_eq!(odd.notch.hover, "peek", "bilinmeyen fare davranışı varsayılana döner");
        assert!(!odd.notch.live);
        let mini = Settings { notch: NotchPrefs { hover: "mini".into(), ..NotchPrefs::default() }, ..Settings::default() }.sanitized();
        assert_eq!(mini.notch.hover, "mini", "mini pencere kipi geçerli (Faz 9)");
        let old_notch: NotchPrefs = serde_json::from_str(r#"{"live":true,"hover":"peek"}"#).unwrap();
        assert!(old_notch.mini_ai && old_notch.mini_music && old_notch.mini_timer, "eski dosyada mini anahtarları açık gelir");
    }

    #[test]
    fn clock_prefs_default_and_sanitize() {
        let old: Settings = serde_json::from_str(r#"{"version":1}"#).unwrap();
        assert_eq!(old.clock, ClockPrefs::default(), "eski dosya varsayılan kartlarla açılır");
        assert!(old.clock.show_time);
        let odd = Settings {
            clock: ClockPrefs {
                cards: ["ai", "ai", "hava", "music", "meter", "mail", "note", "pc", "shelf"].map(String::from).to_vec(),
                show_time: false,
                note: format!("  {}  ", "a".repeat(400)),
            },
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(odd.clock.cards, ["ai", "music", "meter", "mail", "note", "pc"], "bilinmeyen ve tekrar atılır, en çok 6");
        assert!(!odd.clock.show_time);
        assert_eq!(odd.clock.note.chars().count(), MAX_NOTE);
        let empty = Settings { clock: ClockPrefs { cards: vec![], ..ClockPrefs::default() }, ..Settings::default() }.sanitized();
        assert!(empty.clock.cards.is_empty(), "hiç kart seçmemek geçerli");
    }

    #[test]
    fn appearance_and_new_island_prefs_default_and_sanitize() {
        let old: Settings = serde_json::from_str(r#"{"version":1,"island":{"hoverDelayMs":200}}"#).unwrap();
        assert_eq!(old.appearance, Appearance::default());
        assert!(old.island.flash && old.island.sound);
        assert_eq!(old.island.hover, "auto");
        assert!(old.island.peek && old.island.greet && !old.island.welcomed);
        assert_eq!(old.island.reveal, "hover");
        assert!(old.island.bar_open);
        let odd = Settings {
            appearance: Appearance { theme: "neon".into(), surface: "lacivert".into(), ..Appearance::default() },
            island: IslandPrefs { hover: "music".into(), reveal: "double".into(), ..IslandPrefs::default() },
            ..Settings::default()
        }
        .sanitized();
        assert_eq!((odd.appearance.theme.as_str(), odd.appearance.surface.as_str(), odd.island.hover.as_str()), ("gece", "lacivert", "auto"));
        assert_eq!(odd.island.reveal, "hover");
    }

    #[test]
    fn features_and_look_default_for_old_files_and_sanitize() {
        // Bu alanlar eklenmeden önceki dosya: tek görünüm (K2), soru yok; özellikler varsayılanla açılır.
        let old: Settings = serde_json::from_str(r#"{"version":1,"appearance":{"theme":"gul","surface":"siyah"}}"#).unwrap();
        assert_eq!(old.appearance.look, "new");
        assert!(old.appearance.look_asked, "görünüm sorusu kalktı");
        assert_eq!(old.appearance.theme, "gul");
        assert_eq!(old.features, Features::default());
        assert!(!old.features.wait_alert, "bildirim yoran uyarı varsayılan kapalı");
        assert!(old.features.done_summary && old.features.battery && old.features.battery_alerts);
        assert!(old.features.cover_tint && old.features.focus && old.features.shelf, "Dalga C özellikleri eski dosyada açık gelir");
        let odd = Settings {
            appearance: Appearance { look: "classic".into(), look_asked: false, ..Appearance::default() },
            features: Features { wait_alert_min: 7, ..Features::default() },
            ..Settings::default()
        }
        .sanitized();
        assert_eq!((odd.appearance.look.as_str(), odd.appearance.look_asked), ("new", true), "eski Klasik seçimi tek görünüme taşınır");
        assert_eq!(odd.features.wait_alert_min, 10);
        let fresh = Settings::fresh().sanitized();
        assert_eq!((fresh.appearance.look.as_str(), fresh.appearance.look_asked), ("new", true), "yeni kurulum Yeni görünümle, sorusuz");
        assert_eq!(old.appearance.layout, "zody", "eski dosya Zody düzeninde açılır");
        assert!(old.apps.is_empty());
        let odd = Settings { appearance: Appearance { layout: "grid".into(), ..Appearance::default() }, ..Settings::default() }.sanitized();
        assert_eq!(odd.appearance.layout, "zody");
    }

    #[test]
    fn plan_review_and_usage_default_for_old_files() {
        // Dalga B'den önceki dosya: plan kartı ve kullanım özeti açık, Pazartesi özeti kapalı gelir.
        let old: Settings = serde_json::from_str(r#"{"version":1,"features":{"waitAlert":true}}"#).unwrap();
        assert!(old.features.wait_alert);
        assert!(old.features.plan_review && old.features.usage);
        assert!(!old.features.weekly_recap, "bildirim yoran özet varsayılan kapalı");
        assert!(old.features.write_scene && old.features.new_task && old.features.fold_card, "Dalga 1 özellikleri eski dosyada açık gelir");
        assert_eq!(old.shortcuts.write, "Ctrl+Alt+Y");
        let off = Settings { features: Features { write_scene: false, ..Features::default() }, ..Settings::default() };
        assert_eq!(off.active_shortcuts().write, "");
    }

    #[test]
    fn jump_keys_default_on_and_registered_unless_off() {
        // Kısayollar/raftan sor eklenmeden önceki dosya: ikisi açık, varsayılan birleşimler gelir.
        let old: Settings = serde_json::from_str(r#"{"version":1,"shortcuts":{"toggle":"Ctrl+Alt+Z"}}"#).unwrap();
        assert!(old.features.jump_keys && old.features.shelf_ask);
        assert_eq!((old.shortcuts.approval.as_str(), old.shortcuts.terminal.as_str()), ("Ctrl+Alt+O", "Ctrl+Alt+T"));
        // Açıkken kaydedilir; özellik kapalıyken hiç kaydedilmez.
        let new = Settings::default();
        assert_eq!((new.active_shortcuts().approval.as_str(), new.active_shortcuts().terminal.as_str()), ("Ctrl+Alt+O", "Ctrl+Alt+T"));
        let off = Settings { features: Features { jump_keys: false, ..Features::default() }, ..new };
        assert_eq!((off.active_shortcuts().approval.as_str(), off.active_shortcuts().terminal.as_str()), ("", ""));
    }

    #[test]
    fn sources_default_off_for_old_files() {
        // Kaynaklar eklenmeden önceki dosya: ağa çıkan hiçbir şey kendiliğinden açılmaz.
        let old: Settings = serde_json::from_str(r#"{"version":1,"features":{"shelf":true}}"#).unwrap();
        assert!(!old.features.sources && !old.features.source_mail && !old.features.source_github);
        let on: Settings = serde_json::from_str(r#"{"features":{"sources":true,"sourceMail":true,"sourceGithub":true}}"#).unwrap();
        assert!(on.features.sources && on.features.source_mail && on.features.source_github);
    }

    #[test]
    fn island_screen_and_size_default_and_sanitize() {
        // Dalga D'den önceki dosya: birincil ekran, %100.
        let old: Settings = serde_json::from_str(r#"{"version":1,"island":{"hoverDelayMs":200}}"#).unwrap();
        assert_eq!((old.island.screen.as_str(), old.island.screen_id.as_str(), old.island.size), ("primary", "", 100));
        let odd = Settings {
            island: IslandPrefs { screen: "tv".into(), screen_id: format!(" {} ", "x".repeat(90)), size: 400, ..IslandPrefs::default() },
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(odd.island.screen, "primary");
        assert_eq!(odd.island.screen_id.len(), 64);
        assert_eq!(odd.island.size, 130);
        let fixed: Settings = serde_json::from_str(r#"{"island":{"screen":"fixed","screenId":"\\\\.\\DISPLAY2","size":85}}"#).unwrap();
        let fixed = fixed.sanitized();
        assert_eq!((fixed.island.screen.as_str(), fixed.island.screen_id.as_str(), fixed.island.size), ("fixed", r"\\.\DISPLAY2", 85));
    }

    #[test]
    fn sounds_default_for_old_files_and_sanitize() {
        let old: Settings = serde_json::from_str(r#"{"version":1,"island":{"sound":false}}"#).unwrap();
        assert!(!old.island.sound);
        assert_eq!(old.sounds, Sounds::default());
        assert_eq!(old.shortcuts.mute, "Ctrl+Alt+M");
        let mut sounds = Sounds { pack: "mp3".into(), ..Sounds::default() };
        sounds.custom.insert("timer".into(), "  zil.wav ".into());
        sounds.custom.insert("../x".into(), "kötü.wav".into());
        let s = Settings { sounds, ..Settings::default() }.sanitized();
        assert_eq!(s.sounds.pack, "zody");
        assert_eq!(s.sounds.custom.into_iter().collect::<Vec<_>>(), vec![("timer".to_string(), "zil.wav".to_string())]);
    }
}
