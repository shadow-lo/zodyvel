//! Eylem kaydının saf kısmı: tablo, argüman denetimi, onay kartı metni, çağrı planı ve bekleyen istek
//! kuyruğu. Tauri'ye ve Windows'a bağlı değil; böylece mantık her yerde sınanır.

use serde::Serialize;
use serde_json::{Map, Value, json};

/// Bir eylemin ne kadar dikkat istediği. Zody'nin onaysız çalıştırabildikleri `Read` ve `Safe`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    /// Yalnız okur.
    Read,
    /// Yerel, geri alınabilir, zararsız (müzik, ses düzeyi, zamanlayıcı). Onaysız çalışır.
    Safe,
    /// Veri siler, bağlantı keser ya da ajana iş verir. Her seferinde onay kartı.
    Confirm,
    /// Arayüzün kendi tesisatı ya da ajan onayı. Zody hiç çağıramaz; ses ve Zody asla onay vermez.
    Internal,
}

/// Faz 13 erişim seviyesi. Sıra önemli: `Kapali < Dusuk < Orta`. Seviyeyi yalnız kullanıcı arayüzden değiştirir.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Erisim {
    /// Yalnız konuşur, eylem yok.
    #[default]
    Kapali,
    /// Okuma + güvenli, geri alınabilir işler; onaysız.
    Dusuk,
    /// Düşük + kritik işler; her biri onay kartıyla.
    Orta,
}

impl Erisim {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "closed" => Some(Self::Kapali),
            "low" => Some(Self::Dusuk),
            "mid" => Some(Self::Orta),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kapali => "closed",
            Self::Dusuk => "low",
            Self::Orta => "mid",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    /// Kırpılır, boş olamaz.
    Text { max: usize },
    Choice { options: &'static [&'static str] },
    /// YYYY-AA-GG
    Date,
    /// SS:DD
    Time,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Arg {
    pub name: &'static str,
    /// Onay kartında görünen ad.
    pub label: &'static str,
    pub kind: Kind,
    pub optional: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// Kalıcı ad (ör. "music.control"); kurallar ve ileride model bu adı kullanır.
    pub name: &'static str,
    /// Arkasındaki Tauri komutu.
    pub command: &'static str,
    pub title: &'static str,
    pub risk: Risk,
    /// Bu eylemi görebilecek en düşük seviye; `None` = Internal, hiçbir seviyede Zody'ye açılmaz.
    /// Risk sınıfından türer (Read/Safe → Düşük, Confirm → Orta), böylece yeni eylem sütunsuz kalamaz.
    pub min_level: Option<Erisim>,
    pub args: &'static [Arg],
}

/// Risk sınıfının en düşük seviyesi.
pub const fn level_for(risk: Risk) -> Option<Erisim> {
    match risk {
        Risk::Read | Risk::Safe => Some(Erisim::Dusuk),
        Risk::Confirm => Some(Erisim::Orta),
        Risk::Internal => None,
    }
}

const fn arg(name: &'static str, label: &'static str, kind: Kind) -> Arg {
    Arg { name, label, kind, optional: false }
}

const fn opt(name: &'static str, label: &'static str, kind: Kind) -> Arg {
    Arg { name, label, kind, optional: true }
}

const fn act(name: &'static str, command: &'static str, title: &'static str, risk: Risk, args: &'static [Arg]) -> Action {
    Action { name, command, title, risk, min_level: level_for(risk), args }
}

/// Arayüzün kullandığı (ama Zody'nin çağıramadığı) komut.
const fn internal(command: &'static str, title: &'static str) -> Action {
    Action { name: command, command, title, risk: Risk::Internal, min_level: None, args: &[] }
}

const ID: Kind = Kind::Int { min: 0, max: i64::MAX };
const SESSION: Arg = arg("session", "Oturum", Kind::Text { max: 200 });
const TODAY_TEXT: Kind = Kind::Text { max: 120 };
const NOTE_TEXT: Kind = Kind::Text { max: 2000 };
const SOURCE: Arg = arg("id", "Kaynak", Kind::Choice { options: &["mail", "github"] });
const ITEM: Arg = arg("itemId", "Öğe", Kind::Text { max: 400 });

use Risk::{Confirm, Read, Safe};

/// Tek kayıt: `main.rs`'teki her komut burada en az bir kez geçer (test bunu denetler).
pub const ACTIONS: &[Action] = &[
    // Müzik
    act("music.read", "music_snapshot", "Çalan parçayı oku", Read, &[]),
    act("music.control", "music_control", "Müzik: oynat/duraklat, sonraki, önceki", Safe, &[arg("action", "Komut", Kind::Choice { options: &["toggle", "next", "prev"] })]),
    act("music.seek", "music_seek", "Parçada konuma git", Safe, &[arg("positionMs", "Konum (ms)", Kind::Int { min: 0, max: 86_400_000 })]),
    // Hızlı ayarlar
    act("controls.read", "controls_read", "Wi-Fi, Bluetooth, ses ve tema durumunu oku", Read, &[]),
    act("controls.volume", "controls_volume", "Ses düzeyi / sessiz", Safe, &[opt("level", "Düzey", Kind::Int { min: 0, max: 100 }), opt("muted", "Sessiz", Kind::Bool)]),
    act("controls.radio", "controls_radio", "Wi-Fi ya da Bluetooth aç/kapat", Confirm, &[arg("kind", "Radyo", Kind::Choice { options: &["wifi", "bluetooth"] }), arg("on", "Açık", Kind::Bool)]),
    act("controls.dark", "controls_dark", "Koyu tema aç/kapat", Safe, &[arg("on", "Koyu", Kind::Bool)]),
    act("controls.outputs", "controls_outputs", "Ses çıkışlarını listele", Read, &[]),
    act("controls.output", "controls_output", "Ses çıkışını değiştir", Safe, &[arg("id", "Aygıt", Kind::Text { max: 400 })]),
    act("controls.open", "controls_open", "Windows ayar sayfasını aç", Safe, &[arg("page", "Sayfa", Kind::Choice { options: &["wifi", "bluetooth", "sound", "nightlight", "notifications"] })]),
    act("system.task_manager", "open_task_manager", "Görev Yöneticisi'ni aç", Safe, &[]),
    act("power.status", "power_status", "Pil durumunu oku", Read, &[]),
    act("sound.quiet", "sound_quiet", "Rahatsız Etme açık mı", Read, &[]),
    // Bugün: takvim, yapılacaklar, zamanlayıcı
    act("today.read", "today_snapshot", "Takvim, yapılacaklar, Zodymetreler ve modları oku", Read, &[]),
    act("today.event_add", "today_apply", "Takvime etkinlik ekle", Confirm, &[arg("title", "Başlık", TODAY_TEXT), arg("date", "Tarih", Kind::Date), opt("time", "Saat", Kind::Time)]),
    act("today.event_remove", "today_apply", "Takvimden etkinlik sil", Confirm, &[arg("id", "Etkinlik", ID)]),
    act("today.todo_add", "today_apply", "Yapılacak ekle", Safe, &[arg("text", "Metin", TODAY_TEXT)]),
    act("today.todo_toggle", "today_apply", "Yapılacağı işaretle", Safe, &[arg("id", "Yapılacak", ID)]),
    act("today.todo_remove", "today_apply", "Yapılacağı sil", Confirm, &[arg("id", "Yapılacak", ID)]),
    act("today.timer_set", "today_apply", "Zodymetre süresini kur (ilk Zodymetre)", Safe, &[arg("minutes", "Dakika", Kind::Int { min: 1, max: 180 })]),
    act("today.timer_start", "today_apply", "Zodymetre'yi başlat", Safe, &[]),
    act("today.timer_pause", "today_apply", "Zodymetre'yi duraklat", Safe, &[]),
    act("today.timer_reset", "today_apply", "Zodymetre'yi sıfırla", Safe, &[]),
    act("today.focus_start", "today_apply", "Odak döngülü modu başlat", Safe, &[]),
    act("today.focus_stop", "today_apply", "Odak döngülü modu bitir", Safe, &[]),
    // Raf
    act("shelf.read", "shelf_snapshot", "Raftaki dosyaları listele", Read, &[]),
    act("shelf.remove", "shelf_remove", "Raftan kaldır (boşsa hepsini)", Confirm, &[opt("id", "Dosya", ID)]),
    act("shelf.reveal", "shelf_reveal", "Dosyayı Gezgin'de göster", Safe, &[arg("id", "Dosya", ID)]),
    // Ajanlar
    act("agents.read", "agents_snapshot", "Ajan oturumlarını oku", Read, &[]),
    act("agents.focus", "agents_focus", "Ajanın penceresini öne getir", Safe, &[SESSION]),
    act("agents.open", "agents_open", "Oturum klasörünü aç", Safe, &[SESSION, arg("target", "Nerede", Kind::Choice { options: &["folder", "code"] })]),
    act("agents.note", "agents_note", "Ajana not bırak", Confirm, &[SESSION, arg("text", "Not", NOTE_TEXT), arg("now", "Hemen", Kind::Bool)]),
    act("agents.note_remove", "agents_note_remove", "Sıradaki notu geri al", Safe, &[SESSION, arg("id", "Not", ID)]),
    act("agents.task", "agents_task", "Boştaki ajana görev ver", Confirm, &[SESSION, arg("text", "Görev", NOTE_TEXT)]),
    // Kaynaklar (Mail, GitHub). Satır eylemleri aynı komuta gider; `plan` eylem adını ekler.
    act("sources.read", "sources_snapshot", "Mail ve GitHub kaynaklarını oku", Read, &[]),
    act("sources.open", "source_action", "Kaynaktaki öğeyi tarayıcıda aç", Safe, &[SOURCE, ITEM]),
    act("sources.mark_read", "source_action", "Okundu yap", Safe, &[SOURCE, ITEM]),
    act("sources.preview", "source_preview", "İletinin önizlemesini oku", Safe, &[SOURCE, ITEM]),
    act("sources.refresh", "source_refresh", "Kaynağı yenile", Safe, &[SOURCE]),
    act("sources.rerun", "source_action", "Kırılan CI işini yeniden çalıştır", Confirm, &[SOURCE, ITEM]),
    act("sources.fix_with_claude", "source_action", "Kırılan CI'ı Claude'a düzelttir", Confirm, &[SOURCE, ITEM]),
    // Uygulama
    act("settings.read", "get_settings", "Ayarları oku", Read, &[]),
    act("settings.open", "open_settings", "Ayarlar penceresini aç", Safe, &[]),
    act("settings.data_folder", "open_data_folder", "Veri klasörünü aç", Safe, &[]),
    act("startup.read", "autostart_get", "Windows ile başlama durumunu oku", Read, &[]),
    act("startup.set", "autostart_set", "Windows ile başlamayı aç/kapat", Confirm, &[arg("enabled", "Açık", Kind::Bool)]),
    // Faz 13b. Düşük: tarayıcı ve Claude sohbeti (geri alınabilir, yalnız bir sayfa açar). Orta: kapsamlı işler, her biri kartlı.
    act("web.open", "web_open", "Tarayıcıda bağlantı aç", Safe, &[arg("url", "Adres", Kind::Text { max: 2000 })]),
    act("web.search", "web_search", "Tarayıcıda ara (ör. YouTube'da video)", Safe, &[arg("query", "Aranan", Kind::Text { max: 300 }), opt("engine", "Nerede", Kind::Choice { options: &["youtube", "google"] })]),
    act("claude.chat", "claude_chat", "Yeni Claude sohbeti aç (metin hazır yazılır, göndermeyi sen yaparsın)", Safe, &[arg("text", "Sohbet metni", Kind::Text { max: 1500 })]),
    act("app.open", "app_open", "Başlat menüsündeki uygulamayı aç", Confirm, &[arg("name", "Uygulama", Kind::Text { max: 80 })]),
    act("app.close", "app_close", "Uygulamayı kapat (pencerelerine kapatma isteği gider)", Confirm, &[arg("name", "Uygulama", Kind::Text { max: 80 })]),
    act("files.copy", "files_copy", "İzinli klasörde dosya kopyala", Confirm, &[arg("source", "Dosya", Kind::Text { max: 400 }), arg("targetDir", "Hedef klasör", Kind::Text { max: 400 })]),
    act("files.move", "files_move", "İzinli klasörde dosya taşı", Confirm, &[arg("source", "Dosya", Kind::Text { max: 400 }), arg("targetDir", "Hedef klasör", Kind::Text { max: 400 })]),
    act("window.type", "window_type", "Etkin pencereye metin yaz", Confirm, &[arg("text", "Metin", Kind::Text { max: 300 })]),
    act("mail.draft", "mail_draft", "Mail taslağı hazırla (gönderilmez)", Confirm, &[opt("to", "Kime", Kind::Text { max: 200 }), arg("subject", "Konu", Kind::Text { max: 200 }), arg("body", "Metin", Kind::Text { max: 800 })]),
    // Yalnız arayüz
    internal("brain_pick_folder", "Zody için izinli klasör seç"),
    internal("brain_status", "Zody asistanı durumu"),
    internal("brain_check_login", "Zody asistanı giriş denemesi"),
    internal("brain_ask", "Zody asistanına sor"),
    internal("brain_stop", "Zody asistanını durdur"),
    internal("access_get", "Erişim seviyesi durumu"),
    internal("access_set", "Erişim seviyesini değiştir (yalnız kullanıcı)"),
    internal("audit_recent", "Son eylemler (denetim kaydı)"),
    internal("audit_clear", "Denetim kaydını temizle"),
    internal("island_update", "Ada boyu ve konumu"),
    internal("island_typing", "Adada yazı alanı odağı"),
    internal("island_prewarm", "Ada ısıtma"),
    internal("island_pointer_inside", "İmleç adada mı"),
    internal("island_screens", "Ekran listesi"),
    internal("update_settings", "Ayarları yaz"),
    internal("update_check", "Güncelleme denetle"),
    internal("update_state", "Güncelleme durumu"),
    internal("update_download", "Güncellemeyi indir"),
    internal("update_install", "Güncellemeyi kur"),
    internal("update_dismiss", "Güncellemeyi reddet"),
    internal("shortcuts_status", "Kısayol durumu"),
    internal("shortcuts_arm", "Kısayol kaydı"),
    internal("shortcuts_pause", "Kısayolları duraklat"),
    internal("sound_play", "Uyarı sesi çal"),
    internal("sound_preview", "Ses önizleme"),
    internal("sound_upload", "Özel ses yükle"),
    // Uygulama kısayolları (sekme çubuğu): yalnız kullanıcının eklediği hedefler, kimliğiyle.
    act("apps.launch", "apps_launch", "Sekme çubuğundaki uygulama kısayolunu aç", Safe, &[arg("id", "Kısayol", Kind::Text { max: 40 })]),
    internal("apps_pick", "Kısayol için uygulama seç"),
    internal("modes_pick_app", "Mod için uygulama seç"),
    internal("apps_add", "Uygulama kısayolu ekle"),
    internal("apps_icon", "Kısayol simgesi"),
    internal("power_present", "Pil var mı"),
    internal("report_info", "Hata raporu bilgisi"),
    internal("report_debug", "Hata ayıklama günlüğü"),
    internal("report_mark", "Ölçüm damgası"),
    internal("system_watch", "Sistem ölçümü sıklığı"),
    internal("usage_summary", "Kullanım özeti"),
    internal("usage_session", "Oturum kullanımı"),
    internal("shelf_drag_out", "Raftan sürükle"),
    internal("shelf_target", "Bırakma hedefi AI oturumu"),
    internal("shelf_send", "Raftan AI oturumuna gönder"),
    internal("agents_decide", "Ajan iznine karar ver"),
    internal("phone_status", "Telefon köprüsü durumu"),
    internal("phone_pair_start", "Telefon eşleştirme QR'ı"),
    internal("phone_pair_confirm", "Telefon eşleştirmesini onayla"),
    internal("phone_pair_cancel", "Telefon eşleştirmesini iptal et"),
    internal("phone_revoke", "Eşli telefonu kaldır"),
    internal("phone_revoke_all", "Tüm eşli telefonları kaldır"),
    internal("phone_retry", "Telefon köprüsünü yeniden dene"),
    internal("agents_answer", "Ajanın sorusunu yanıtla"),
    internal("agents_pass", "Kararı terminale bırak"),
    internal("agents_step", "Adım ayrıntısı"),
    internal("agents_request_diff", "Değişiklik farkı"),
    internal("agents_demo", "Ajan örneği"),
    internal("agents_editor_present", "VS Code kurulu mu"),
    internal("connections_inspect", "Bağlantıları denetle"),
    internal("connections_prepare", "Bağlantı önizlemesi"),
    internal("connections_apply", "Bağlantıyı kur"),
    internal("statusline_inspect", "Durum satırı köprüsünü denetle"),
    internal("statusline_prepare", "Durum satırı önizlemesi"),
    internal("plan_limits", "Claude plan limitleri"),
    internal("list_recent_projects", "Son proje klasörleri"),
    internal("list_closed_sessions", "Kapanmış oturumlar"),
    internal("pick_folder", "Klasör seç"),
    internal("start_task", "Yeni görev başlat"),
    internal("resume_session", "Kapanmış oturuma devam et"),
    internal("task_stop", "Zodyvel'in başlattığı görevi durdur"),
    internal("source_connect", "Kaynağı bağla (gizli bilgi)"),
    internal("source_disconnect", "Kaynak bağlantısını kaldır"),
    internal("source_open_link", "Kurulum yardım bağlantısını aç"),
    internal("git_overview", "Yerel depoların durumu"),
    internal("git_commit", "Yerel depoda commit"),
    internal("git_push", "Yerel depoyu gönder (push)"),
    internal("git_discard", "Yerel değişiklikleri geri al"),
    internal("git_fetch", "Yerel depo için GitHub'ı denetle (fetch)"),
    internal("git_pull", "Yerel depoyu güncelle (pull)"),
    internal("git_open_folder", "Depo klasörünü aç"),
    internal("git_pick_folder", "Depo klasörü seç"),
    internal("git_desktop_repos", "GitHub Desktop depoları"),
];

pub fn find(name: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.name == name)
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && digits(0..4) && digits(5..7) && digits(8..10) && {
        let (m, d): (u32, u32) = (s[5..7].parse().unwrap_or(0), s[8..10].parse().unwrap_or(0));
        (1..=12).contains(&m) && (1..=31).contains(&d)
    }
}

fn valid_time(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5 && b[2] == b':' && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit()) && {
        let (h, m): (u32, u32) = (s[..2].parse().unwrap_or(99), s[3..].parse().unwrap_or(99));
        h < 24 && m < 60
    }
}

fn check(arg: &Arg, value: &Value) -> Result<Value, String> {
    let bad = || format!("{}: geçersiz değer.", arg.label);
    match arg.kind {
        Kind::Bool => value.as_bool().map(Value::from).ok_or_else(bad),
        Kind::Int { min, max } => value.as_i64().filter(|n| (min..=max).contains(n)).map(Value::from).ok_or_else(bad),
        Kind::Text { max } => {
            let text = value.as_str().map(str::trim).ok_or_else(bad)?;
            if text.is_empty() || text.chars().count() > max { Err(bad()) } else { Ok(text.into()) }
        }
        Kind::Choice { options } => value.as_str().filter(|s| options.contains(s)).map(Value::from).ok_or_else(bad),
        Kind::Date => value.as_str().filter(|s| valid_date(s)).map(Value::from).ok_or_else(bad),
        Kind::Time => value.as_str().filter(|s| valid_time(s)).map(Value::from).ok_or_else(bad),
    }
}

/// Argümanları şemaya göre denetler ve temizler. Bilinmeyen ad, eksik zorunlu argüman ya da yanlış tür
/// reddedilir; `null` verilen isteğe bağlı argüman yok sayılır.
pub fn validate(action: &Action, args: &Value) -> Result<Map<String, Value>, String> {
    let empty = Map::new();
    let given = match args {
        Value::Null => &empty,
        Value::Object(map) => map,
        _ => return Err("Argümanlar nesne olmalı.".into()),
    };
    if let Some(unknown) = given.keys().find(|k| !action.args.iter().any(|a| a.name == k.as_str())) {
        return Err(format!("Bilinmeyen argüman: {unknown}"));
    }
    let mut clean = Map::new();
    for arg in action.args {
        match given.get(arg.name) {
            None | Some(Value::Null) if arg.optional => {}
            None | Some(Value::Null) => return Err(format!("{} eksik.", arg.label)),
            Some(value) => {
                clean.insert(arg.name.into(), check(arg, value)?);
            }
        }
    }
    Ok(clean)
}

/// Onay kartındaki tek satır: "Takvime etkinlik ekle · Başlık: Toplantı, Tarih: 2026-10-21".
pub fn describe(action: &Action, args: &Map<String, Value>) -> String {
    let parts: Vec<String> = action
        .args
        .iter()
        .filter_map(|arg| {
            let value = match args.get(arg.name)? {
                Value::Bool(true) => "evet".to_owned(),
                Value::Bool(false) => "hayır".to_owned(),
                Value::String(s) => s.chars().take(300).collect(),
                other => other.to_string(),
            };
            Some(format!("{}: {value}", arg.label))
        })
        .collect();
    if parts.is_empty() { action.title.to_owned() } else { format!("{} · {}", action.title, parts.join(", ")) }
}

/// Denetlenmiş bir eylemin hangi komutla, hangi argümanla çağrılacağı. Komutun kendisi `mod.rs`'te.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// Komut argümanları olduğu gibi alır (adlar komutun parametreleriyle aynı).
    Direct { command: &'static str, args: Map<String, Value> },
    /// `today_apply`'ın `op` nesnesi (today::Op'un JSON biçimi).
    Today(Value),
}

/// Denetlenmiş argümanlardan çağrıyı kurar. `Internal` eylem burada da reddedilir.
pub fn plan(action: &Action, args: Map<String, Value>) -> Result<Call, String> {
    if action.risk == Risk::Internal {
        return Err("Bu eylem Zody'ye kapalı.".into());
    }
    if action.command == "source_action" {
        // "sources.mark_read" → source_action(action: "mark_read")
        let mut args = args;
        let verb = action.name.strip_prefix("sources.").ok_or("Bilinmeyen eylem.")?;
        args.insert("action".into(), json!(verb));
        return Ok(Call::Direct { command: action.command, args });
    }
    if action.command != "today_apply" {
        return Ok(Call::Direct { command: action.command, args });
    }
    let op = match action.name {
        "today.event_add" => "addEvent",
        "today.event_remove" => "removeEvent",
        "today.todo_add" => "addTodo",
        "today.todo_toggle" => "toggleTodo",
        "today.todo_remove" => "removeTodo",
        "today.timer_set" => "timerSet",
        "today.timer_start" => "timerStart",
        "today.timer_pause" => "timerPause",
        "today.timer_reset" => "timerReset",
        "today.focus_start" => "focusStart",
        "today.focus_stop" => "focusStop",
        _ => return Err("Bilinmeyen eylem.".into()),
    };
    let mut body = args;
    body.insert("op".into(), json!(op));
    Ok(Call::Today(Value::Object(body)))
}

/// Bu eylemin sonucu dış içerik taşır mı (mail, GitHub, ajan çıktısı): çağrıdan sonra turun dış içerik bayrağı kalkar
/// ve kalan Güvenli eylemler de onay kartına düşer (dış içerikteki "şunu yap" talimatı kendiliğinden çalışmasın).
pub fn dis_icerik_dondurur(action: &Action) -> bool {
    matches!(action.name, "sources.read" | "sources.preview" | "sources.refresh" | "agents.read")
}

/// Bekleyen istek bu kadar süre sonra düşer: kullanıcı görmediyse eski bir komut sonradan çalışmasın.
pub const PENDING_TTL_MS: u64 = 2 * 60_000;
/// Aynı anda en çok bu kadar istek bekler; fazlası en eskiyi düşürür.
pub const MAX_PENDING: usize = 8;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub id: u64,
    pub action: &'static str,
    pub risk: Risk,
    /// Onay kartı satırı.
    pub summary: String,
    /// İsteği yapan ("zody"; ileride "voice", "model").
    pub source: String,
    /// Kullanıcının isteğinin özeti (denetim kaydı için; arayüze gitmez).
    #[serde(skip)]
    pub istek: String,
    pub created_ms: u64,
    #[serde(skip)]
    pub args: Map<String, Value>,
}

#[derive(Default)]
pub struct Queue {
    items: Vec<Pending>,
    next_id: u64,
    /// Süresi dolup düşen istekler; çağıran `take_expired` ile alır ve denetim kaydına yazar.
    expired: Vec<Pending>,
}

impl Queue {
    fn expire(&mut self, now: u64) {
        let (live, dead): (Vec<Pending>, Vec<Pending>) =
            std::mem::take(&mut self.items).into_iter().partition(|p| now.saturating_sub(p.created_ms) < PENDING_TTL_MS);
        self.items = live;
        self.expired.extend(dead);
    }

    /// Süresi dolanlar (ve kuyruk dolunca düşen en eski istek): bir kez döner, sonra boşalır.
    pub fn take_expired(&mut self, now: u64) -> Vec<Pending> {
        self.expire(now);
        std::mem::take(&mut self.expired)
    }

    pub fn push(&mut self, action: &'static Action, args: Map<String, Value>, source: &str, istek: &str, now: u64) -> Pending {
        self.expire(now);
        if self.items.len() >= MAX_PENDING {
            let dropped = self.items.remove(0);
            self.expired.push(dropped);
        }
        self.next_id += 1;
        let pending = Pending {
            id: self.next_id,
            action: action.name,
            risk: action.risk,
            summary: describe(action, &args),
            source: source.to_owned(),
            istek: istek.to_owned(),
            created_ms: now,
            args,
        };
        self.items.push(pending.clone());
        pending
    }

    /// İsteği kuyruktan çıkarır (onay ya da ret). Süresi geçmişse yok sayılır.
    pub fn take(&mut self, id: u64, now: u64) -> Option<Pending> {
        self.expire(now);
        let index = self.items.iter().position(|p| p.id == id)?;
        Some(self.items.remove(index))
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn list(&mut self, now: u64) -> Vec<Pending> {
        self.expire(now);
        self.items.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `main.rs`'teki `generate_handler!` listesindeki komut adları (eylem merkezinin kendi komutları hariç).
    fn handler_commands() -> Vec<String> {
        let main = include_str!("../main.rs");
        let start = main.find("generate_handler![").expect("generate_handler! bulunamadı");
        let end = start + main[start..].find(']').unwrap();
        main[start + "generate_handler![".len()..end]
            .split(',')
            .filter_map(|path| path.trim().rsplit("::").next().map(str::to_owned))
            .filter(|name| !name.is_empty() && !name.starts_with("actions_"))
            .collect()
    }

    fn sample(kind: Kind) -> Value {
        match kind {
            Kind::Bool => json!(true),
            Kind::Int { min, .. } => json!(min.max(1)),
            Kind::Text { .. } => json!("  deneme  "),
            Kind::Choice { options } => json!(options[0]),
            Kind::Date => json!("2026-10-21"),
            Kind::Time => json!("10:30"),
        }
    }

    fn full_args(action: &Action) -> Value {
        Value::Object(action.args.iter().map(|a| (a.name.to_owned(), sample(a.kind))).collect())
    }

    #[test]
    fn every_command_is_registered() {
        let commands = handler_commands();
        assert!(commands.len() > 50, "komut listesi okunamadı: {commands:?}");
        for command in &commands {
            assert!(
                ACTIONS.iter().any(|a| a.command == command),
                "`{command}` eylem kaydında yok: src-tauri/src/actions/spec.rs ACTIONS'a ekleyin (Zody çağırmayacaksa internal(...))."
            );
        }
        for action in ACTIONS {
            assert!(commands.iter().any(|c| c == action.command), "`{}` main.rs'te kayıtlı değil", action.command);
        }
    }

    #[test]
    fn names_are_unique_and_tidy() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert!(ACTIONS[i + 1..].iter().all(|b| b.name != a.name), "iki kez: {}", a.name);
            assert!(a.name.bytes().all(|c| c.is_ascii_lowercase() || c == b'.' || c == b'_'), "{}", a.name);
            assert!(a.risk == Risk::Internal || a.name.contains('.'), "{}", a.name);
            assert!(!a.title.is_empty());
        }
    }

    #[test]
    fn every_open_action_plans() {
        for action in ACTIONS.iter().filter(|a| a.risk != Risk::Internal) {
            let args = validate(action, &full_args(action)).unwrap_or_else(|e| panic!("{}: {e}", action.name));
            let call = plan(action, args).unwrap_or_else(|e| panic!("{}: {e}", action.name));
            if let Call::Today(op) = call {
                assert!(op["op"].is_string(), "{}", action.name);
            }
        }
    }

    #[test]
    fn every_open_action_runs() {
        let runner = include_str!("mod.rs");
        for action in ACTIONS.iter().filter(|a| a.risk != Risk::Internal && a.command != "today_apply") {
            assert!(runner.contains(&format!("\"{}\" =>", action.command)), "`{}` actions/mod.rs run() içinde yok", action.command);
        }
    }

    #[test]
    fn source_rows_carry_their_verb() {
        let args = validate(find("sources.rerun").unwrap(), &json!({ "id": "github", "itemId": "run-7" })).unwrap();
        let Call::Direct { command, args } = plan(find("sources.rerun").unwrap(), args).unwrap() else { panic!() };
        assert_eq!((command, args["action"].as_str(), args["itemId"].as_str()), ("source_action", Some("rerun"), Some("run-7")));
        assert!(validate(find("sources.open").unwrap(), &json!({ "id": "telegram", "itemId": "1" })).is_err());
        assert_eq!(find("sources.fix_with_claude").unwrap().risk, Risk::Confirm);
        assert_eq!(find("source_connect").unwrap().risk, Risk::Internal);
    }

    #[test]
    fn every_action_has_a_level() {
        for action in ACTIONS {
            let want = match action.risk {
                Risk::Read | Risk::Safe => Some(Erisim::Dusuk),
                Risk::Confirm => Some(Erisim::Orta),
                Risk::Internal => None,
            };
            assert_eq!(action.min_level, want, "{}", action.name);
        }
        assert_eq!(find("music.control").unwrap().min_level, Some(Erisim::Dusuk));
        assert_eq!(find("music.read").unwrap().min_level, Some(Erisim::Dusuk));
        assert_eq!(find("controls.radio").unwrap().min_level, Some(Erisim::Orta));
        assert_eq!(find("agents.task").unwrap().min_level, Some(Erisim::Orta));
        for name in ["access_set", "access_get", "update_settings", "agents_decide", "audit_clear", "audit_recent"] {
            assert_eq!(find(name).expect(name).min_level, None, "{name}");
        }
    }

    #[test]
    fn levels_are_ordered_and_parse() {
        assert!(Erisim::Kapali < Erisim::Dusuk && Erisim::Dusuk < Erisim::Orta);
        for l in [Erisim::Kapali, Erisim::Dusuk, Erisim::Orta] {
            assert_eq!(Erisim::parse(l.as_str()), Some(l));
        }
        assert_eq!(Erisim::parse("root"), None);
        assert_eq!(Erisim::default(), Erisim::Kapali);
    }

    #[test]
    fn internal_actions_are_refused() {
        for action in ACTIONS.iter().filter(|a| a.risk == Risk::Internal) {
            assert!(plan(action, Map::new()).is_err(), "{}", action.name);
        }
        assert_eq!(find("agents_decide").unwrap().risk, Risk::Internal);
    }

    #[test]
    fn validate_cleans_and_rejects() {
        let add = find("today.event_add").unwrap();
        let clean = validate(add, &json!({ "title": "  Toplantı ", "date": "2026-10-21", "time": null })).unwrap();
        assert_eq!(clean, json!({ "title": "Toplantı", "date": "2026-10-21" }).as_object().unwrap().clone());
        assert!(validate(add, &json!({ "title": "x" })).unwrap_err().contains("Tarih"));
        assert!(validate(add, &json!({ "title": "x", "date": "2026-13-01" })).is_err());
        assert!(validate(add, &json!({ "title": "x", "date": "2026-10-21", "time": "24:00" })).is_err());
        assert!(validate(add, &json!({ "title": "   ", "date": "2026-10-21" })).is_err());
        assert!(validate(add, &json!({ "title": "x", "date": "2026-10-21", "extra": 1 })).unwrap_err().contains("extra"));
        assert!(validate(add, &json!([1])).is_err());

        let timer = find("today.timer_set").unwrap();
        assert!(validate(timer, &json!({ "minutes": 25 })).is_ok());
        assert!(validate(timer, &json!({ "minutes": 0 })).is_err());
        assert!(validate(timer, &json!({ "minutes": 181 })).is_err());
        assert!(validate(timer, &json!({ "minutes": "25" })).is_err());

        let music = find("music.control").unwrap();
        assert!(validate(music, &json!({ "action": "next" })).is_ok());
        assert!(validate(music, &json!({ "action": "stop" })).is_err());
        assert!(validate(find("music.read").unwrap(), &Value::Null).unwrap().is_empty());
        assert!(validate(find("controls.volume").unwrap(), &json!({ "level": 101 })).is_err());
    }

    #[test]
    fn today_plan_matches_op_shape() {
        let add = find("today.event_add").unwrap();
        let args = validate(add, &json!({ "title": "Toplantı", "date": "2026-10-21", "time": "10:00" })).unwrap();
        assert_eq!(plan(add, args).unwrap(), Call::Today(json!({ "op": "addEvent", "title": "Toplantı", "date": "2026-10-21", "time": "10:00" })));
        let start = find("today.timer_start").unwrap();
        assert_eq!(plan(start, Map::new()).unwrap(), Call::Today(json!({ "op": "timerStart" })));
    }

    #[test]
    fn describe_reads_like_a_card() {
        let add = find("today.event_add").unwrap();
        let args = validate(add, &json!({ "title": "Toplantı", "date": "2026-10-21" })).unwrap();
        assert_eq!(describe(add, &args), "Takvime etkinlik ekle · Başlık: Toplantı, Tarih: 2026-10-21");
        let radio = find("controls.radio").unwrap();
        let args = validate(radio, &json!({ "kind": "wifi", "on": false })).unwrap();
        assert_eq!(describe(radio, &args), "Wi-Fi ya da Bluetooth aç/kapat · Radyo: wifi, Açık: hayır");
        assert_eq!(describe(find("system.task_manager").unwrap(), &Map::new()), "Görev Yöneticisi'ni aç");
    }

    /// Faz 13b: tarayıcı ve Claude sohbeti Düşük'te onaysız; kapsamlı işler Orta'da ve her seferinde kartlı.
    #[test]
    fn faz_13b_eylemleri_sinifli() {
        for ad in ["web.open", "web.search", "claude.chat"] {
            let a = find(ad).expect(ad);
            assert_eq!((a.risk, a.min_level), (Risk::Safe, Some(Erisim::Dusuk)), "{ad}");
        }
        for ad in ["app.open", "app.close", "files.copy", "files.move", "window.type", "mail.draft"] {
            let a = find(ad).expect(ad);
            assert_eq!((a.risk, a.min_level), (Risk::Confirm, Some(Erisim::Orta)), "{ad}");
        }
        // "şu videoyu aç": argümanlar doğrulanır, motor seçilebilir.
        let ara = find("web.search").unwrap();
        assert!(validate(ara, &json!({ "query": "lofi", "engine": "youtube" })).is_ok());
        assert!(validate(ara, &json!({ "query": "lofi", "engine": "bing" })).is_err());
        assert!(validate(find("web.open").unwrap(), &json!({ "url": "https://youtu.be/x" })).is_ok());
        assert!(validate(find("mail.draft").unwrap(), &json!({ "subject": "k", "body": "g" })).is_ok(), "alıcı isteğe bağlı");
    }

    #[test]
    fn dis_icerik_donduren_eylemler() {
        for ad in ["sources.read", "sources.preview", "sources.refresh", "agents.read"] {
            assert!(dis_icerik_dondurur(find(ad).unwrap()), "{ad}");
        }
        for ad in ["music.read", "web.open", "today.read"] {
            assert!(!dis_icerik_dondurur(find(ad).unwrap()), "{ad}");
        }
    }

    #[test]
    fn suresi_dolan_istek_denetime_verilir() {
        let action = find("today.todo_remove").unwrap();
        let mut queue = Queue::default();
        let eski = queue.push(action, Map::new(), "zody", "i", 0);
        queue.push(action, Map::new(), "zody", "i", PENDING_TTL_MS);
        let dolan = queue.take_expired(PENDING_TTL_MS);
        assert_eq!(dolan.iter().map(|p| p.id).collect::<Vec<_>>(), vec![eski.id]);
        assert!(queue.take_expired(PENDING_TTL_MS).is_empty(), "bir kez döner");
        for i in 0..MAX_PENDING as u64 + 1 {
            queue.push(action, Map::new(), "zody", "i", PENDING_TTL_MS + i);
        }
        assert_eq!(queue.take_expired(PENDING_TTL_MS + 20).len(), 2, "kuyruk dolunca düşen en eski de kayda geçer");
    }

    #[test]
    fn queue_clear_drops_all_pending() {
        let mut queue = Queue::default();
        queue.push(find("today.todo_remove").unwrap(), Map::new(), "zody", "istek", 1_000);
        queue.push(find("shelf.remove").unwrap(), Map::new(), "zody", "istek", 1_000);
        queue.clear();
        assert!(queue.list(1_000).is_empty());
    }

    #[test]
    fn queue_takes_once_expires_and_caps() {
        let action = find("today.todo_remove").unwrap();
        let mut queue = Queue::default();
        let first = queue.push(action, Map::new(), "zody", "", 1_000);
        assert_eq!(queue.list(1_000).len(), 1);
        assert!(queue.take(first.id, 2_000).is_some());
        assert!(queue.take(first.id, 2_000).is_none(), "iki kez onaylanamaz");

        let old = queue.push(action, Map::new(), "zody", "", 0);
        assert!(queue.take(old.id, PENDING_TTL_MS).is_none(), "süresi geçen düşer");

        for i in 0..MAX_PENDING as u64 + 2 {
            queue.push(action, Map::new(), "zody", "", 10_000 + i);
        }
        let list = queue.list(10_000 + MAX_PENDING as u64 + 2);
        assert_eq!(list.len(), MAX_PENDING);
        assert!(list.windows(2).all(|w| w[0].id < w[1].id));
        assert!(list.iter().all(|p| p.id > 4), "en eskiler düştü");
    }
}
