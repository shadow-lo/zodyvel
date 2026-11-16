//! Canlı bağlantı (T4-b, T4-c): eşli bilgisayara bağlı kalır, durum değişikliklerini bildirime çevirir; her AI eylemini
//! (onay/ret, soru cevabı, not, görev, yeni görev, durdur) her seferinde `Platform::sign` (BiometricPrompt + Keystore
//! imzası) ile yollar. Uygulama içi eylemler masaüstünün `ack`'ını bekler (imzadan sonra en çok `ACK_WAIT`).
//!
//! Kurallar:
//! - Yalnız eşleşme varsa, kullanıcı kapatmadıysa ve `live_start` çağrıldıysa tek bir iş parçacığı çalışır.
//!   Durdurulunca iş parçacığı biter, bellek bırakılır (yeniden bağlanma beklemesi dahil hiçbir şey çalışmaz).
//! - Bağlantı yokken üstel geri çekilme (5 sn → 60 sn; 1 saatten uzun bağlantısızlıkta 5 dk); bilgisayar "ağ Özel
//!   değil / kilitli" derse 30 sn'den başlayıp 5 dk'ya dek üstel. Bekleme ancak bağlantı `STABLE_AFTER` boyunca
//!   ayakta kalıp durum iletisi geldiyse başa döner (T-O1: bağlanıp hemen kopan sunucu tam el sıkışmaya boğulmaz).
//!   mDNS araması da üstel seyrekleşir (60 sn → 30 dk). 24 saat hiç bağlanamazsa deneme durur ve ön plan hizmeti
//!   kapanır; uygulama açılınca yeniden başlar.
//! - T-O2: bağlıyken yoklama yok. Bir bekçi iş parçacığı soketi (ping aralığından uzun süre) bekler, ileti gelince
//!   canlı iş parçacığına komut kanalından haber verir; canlı iş parçacığı yalnız bu kanalda uyur (komut, ileti,
//!   ping ya da süre dolumu gelince uyanır).
//! - Kalıcı bitişte (kaldırıldı / yeniden eşleş / vazgeçildi) ön plan hizmeti de kapanır.
//! - Bildirime YALNIZ oturum adı ve olay türü gider; istem, kod, komut çıktısı, adım metni gitmez.
//! - Ayrıntı (onay özeti) yalnız `Platform::sign`'ın sistem kimlik doğrulama penceresinde ve 2 dk ayrıntı
//!   penceresi açıkken uygulama içinde görünür.
use super::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use serde_json::Map;
use zodyvel_phone_proto::proto::{Action, RESTRICT_UNPROTECTED};
use std::io::Write as _;
use std::net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use zeroize::Zeroize;
use zodyvel_phone_proto::devices::signing_bytes;
use zodyvel_phone_proto::proto::{clip, MAX_INCOMING};
use zodyvel_phone_proto::session::{Channel, Trust};
use zodyvel_phone_proto::MODE_NORMAL;

/// Bağlanma ve tek çerçeve okuma/yazma için en çok bekleme (`client::Link` ile aynı).
const CONNECT_WAIT: Duration = Duration::from_secs(4);
const IO_WAIT: Duration = Duration::from_secs(5);
/// İmza bitip ileti yollandıktan sonra masaüstünün `ack`'ı için en çok bekleme.
const ACK_WAIT: Duration = Duration::from_secs(10);
/// Klasör/Ajanda isteğinin cevabı için en çok bekleme.
const ASK_WAIT: Duration = Duration::from_secs(5);
/// Çağıranın güvenlik sınırı (parmak izi penceresi dahil): bu sürede hiçbir cevap yoksa vazgeçer.
const CALLER_WAIT: Duration = Duration::from_secs(150);

pub const ERR_OFFLINE: &str = "Bilgisayara bağlı değilsin.";
pub const ERR_NO_ANSWER: &str = "Bilgisayar cevap vermedi.";
pub const ERR_DROPPED: &str = "Bağlantı koptu.";
pub const ERR_CANCELED: &str = "İptal edildi.";
const PING_EVERY: Duration = Duration::from_secs(20);
/// Bu kadar süre hiçbir şey gelmezse bağlantı ölü sayılır (bilgisayar ping'e pong yollar).
const DEAD_AFTER: Duration = Duration::from_secs(60);
const RETRY_MIN: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// T-O1: bağlantı bu kadar ayakta kalıp durum iletisi geldiyse geri çekilme başa döner.
pub const STABLE_AFTER: Duration = Duration::from_secs(60);
/// T-O1: bilgisayar "kilitli / ağ Özel değil" derse bekleme buradan başlar, `RETRY_SLOW`'a dek ikiye katlanır.
const RETRY_LOCKED: Duration = Duration::from_secs(30);
/// T-Y2: bu kadar süredir bağlantı yoksa denemeler seyrekleşir.
const SLOW_AFTER: Duration = Duration::from_secs(3600);
const RETRY_SLOW: Duration = Duration::from_secs(5 * 60);
/// T-Y2: bu kadar süre hiç bağlanamazsa deneme tamamen durur (uygulama açılınca yeniden başlar).
pub const GIVE_UP_AFTER: Duration = Duration::from_secs(24 * 3600);
/// T-Y2: mDNS arama aralığı her başarısız aramada ikiye katlanır, en çok bu kadar.
pub const SEARCH_MAX: Duration = Duration::from_secs(30 * 60);
/// T-Y2: bilgisayar art arda bu kadar kez "eşli değilsin" derse telefon kaldırılmış sayılır.
pub const UNPAIRED_LIMIT: u32 = 3;

/// Platforma (Android bildirimleri) giden olaylar. İçerik taşımaz.
#[derive(Clone, Debug, PartialEq)]
pub enum Notice {
    /// Ön plan hizmeti bildiriminin kısa metni ("Bağlı · 1 onay bekliyor").
    Service(String),
    /// Yeni onay isteği. `kind`: permission | question | plan. `session`: oturum adı.
    Approval { id: String, session: String, kind: String },
    /// Onay artık geçerli değil (cevaplandı, süresi doldu, başka yerden verildi).
    Gone { id: String },
    Finished { session: String },
    Failed { session: String },
    /// İçeriksiz kısa bilgi ("Onay gönderilemedi.").
    Info(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QuestionInfo {
    pub q: String,
    pub options: Vec<String>,
    pub multi: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ApprovalInfo {
    pub id: String,
    /// Oturumun ADI (bildirim ve liste için).
    pub session: String,
    /// Oturum kimliği (ekran onayı oturuma bununla bağlar).
    pub session_id: String,
    pub kind: String,
    /// Yalnız ayrıntı penceresi açıkken dolu.
    pub action: Option<String>,
    /// Soru formu; yalnız ayrıntı penceresi açıkken dolu.
    pub questions: Option<Vec<QuestionInfo>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StepInfo {
    pub kind: String,
    pub label: String,
    pub target: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SessionInfo {
    pub id: String,
    pub tool: String,
    pub name: String,
    pub folder: String,
    /// running | waiting_input | waiting_approval | done | error
    pub status: String,
    /// Unix saniye.
    pub started: Option<i64>,
    /// Aşağıdakiler yalnız ayrıntı penceresi (2 dk) açıkken dolu.
    pub step: Option<String>,
    pub steps: Option<Vec<StepInfo>>,
    pub reply: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct LiveState {
    /// Kullanıcı arka planda bağlı kalmayı kapatmadı.
    pub enabled: bool,
    pub running: bool,
    /// "off" | "connecting" | "searching" | "connected" | "retry" | "repair" | "revoked"
    pub link: String,
    pub detail: String,
    pub approvals: Vec<ApprovalInfo>,
    /// Ad, klasör, durum, süre her zaman; adım/cevap yalnız ayrıntı penceresinde.
    pub sessions: Vec<SessionInfo>,
    /// Ayrıntı penceresinin bitişi (ms, Unix); kapalıysa `None`.
    pub unlocked_until: Option<u64>,
    /// Son durum iletisinin geldiği an (ms, Unix).
    pub last_connected: Option<u64>,
    /// O9: masaüstünün kararı (donanım kanıtı doğrulandı mı); bilinmiyorsa `None`.
    pub hw: Option<bool>,
    /// "strongbox" | "tee" | "".
    pub hw_level: Option<String>,
    /// G3: Ajanda hatırlatıcıları açık mı (varsayılan açık).
    pub reminders: bool,
    /// G3: kurulu hatırlatma sayısı (son eşitleme); hiç eşitlenmediyse `None`.
    pub reminder_count: Option<u32>,
    /// G3: telefon açılınca bağlan (varsayılan kapalı).
    pub autostart: bool,
}

type Reply<T> = Sender<Result<T, String>>;

pub(crate) enum Cmd {
    /// Onay/ret. `reply` yoksa (bildirim eylemi) sonuç bildirimle söylenir.
    Decide { id: String, allow: bool, reply: Option<Reply<()>> },
    /// İmzalı eylem (cevap, not, görev, yeni görev, durdur).
    Act { action: Value, prompt: Prompt, reply: Reply<()> },
    /// İmzasız salt okunur istek: "folders" | "agenda".
    Ask { what: &'static str, reply: Reply<Value> },
    Stop,
    /// T-O2: bekçiden: `gen` numaralı bağlantıda okunacak veri var (`open`) ya da bağlantı kapandı. Eski bağlantının
    /// geç kalan haberi `gen` ile ayıklanır.
    Wire { gen: u64, open: bool },
}

pub(crate) struct LiveHandle {
    tx: Sender<Cmd>,
    join: std::thread::JoinHandle<()>,
}

#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) link: String,
    pub(crate) detail: String,
    /// Ham onaylar (eylem metni dahil, yalnız bellekte).
    pub(crate) approvals: Vec<ApprovalInfo>,
    /// Ham oturumlar (adım ve cevap dahil, yalnız bellekte).
    pub(crate) sessions: Vec<SessionInfo>,
    pub(crate) running: bool,
    pub(crate) last_connected: Option<u64>,
    pub(crate) hw: Option<bool>,
    pub(crate) hw_level: Option<String>,
    /// Son alınan ajanda (G3'te AlarmManager da kullanır); yalnız bellekte.
    pub(crate) agenda: Option<Value>,
    /// Son alınan klasör listesi (yeni görev parmak izi metnindeki ad için).
    pub(crate) folders: Vec<(String, String)>,
    /// G3: Android'e son gönderilen hatırlatma listesinin özeti (aynısı tekrar gönderilmez).
    pub(crate) remind_sig: Option<u64>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl Core {
    fn off_marker(&self) -> PathBuf {
        self.dir.join("live.off")
    }

    pub fn live_enabled(&self) -> bool {
        !self.off_marker().exists()
    }

    pub fn live_state(&self) -> LiveState {
        let until = self.unlocked_until();
        let open = until.is_some();
        let saved_hw = self.load().and_then(|s| s.hw);
        let s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
        LiveState {
            enabled: self.live_enabled(),
            running: s.running,
            link: if s.link.is_empty() { "off".into() } else { s.link.clone() },
            detail: s.detail.clone(),
            approvals: s
                .approvals
                .iter()
                .map(|a| ApprovalInfo { action: a.action.clone().filter(|_| open), questions: a.questions.clone().filter(|_| open), ..a.clone() })
                .collect(),
            sessions: s
                .sessions
                .iter()
                .map(|x| SessionInfo { step: x.step.clone().filter(|_| open), steps: x.steps.clone().filter(|_| open), reply: x.reply.clone().filter(|_| open), ..x.clone() })
                .collect(),
            unlocked_until: until,
            last_connected: s.last_connected,
            hw: s.hw.or(saved_hw),
            hw_level: s.hw_level.clone(),
            reminders: self.reminders_enabled(),
            reminder_count: self.reminder_count(),
            autostart: self.autostart(),
        }
    }

    // ---- G3 · Ajanda hatırlatıcıları ve açılışta bağlanma ----

    fn flag(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }
    fn set_flag(&self, name: &str, on: bool) -> Result<(), String> {
        if on {
            std::fs::create_dir_all(&self.dir).map_err(|_| "Ayar yazılamadı.".to_string())?;
            std::fs::write(self.flag(name), b"1").map_err(|_| "Ayar yazılamadı.".to_string())
        } else {
            match std::fs::remove_file(self.flag(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err("Ayar yazılamadı.".into()),
                _ => Ok(()),
            }
        }
    }

    /// Hatırlatıcılar açık mı (kapalıysa `remind.off` işareti var).
    pub fn reminders_enabled(&self) -> bool {
        !self.flag("remind.off").exists()
    }

    /// Kurulu hatırlatma sayısı (yalnız sayı diske yazılır; başlık/saat Android'in şifreli kaydında).
    pub fn reminder_count(&self) -> Option<u32> {
        std::fs::read_to_string(self.flag("remind.n")).ok()?.trim().parse().ok()
    }

    /// Kullanıcı ayarı. Kapatınca tüm alarmlar, şifreli kayıt ve bellekteki ajanda silinir (T-O5: kapalıyken ajanda
    /// tutulmaz); açınca bağlıysa bilgisayardan taze ajanda istenir ve gelince kurulur.
    pub fn reminders_set_enabled(&self, on: bool) -> Result<(), String> {
        {
            // T-O4: ayar yazımı ve alarmların silinmesi, canlı iş parçacığının eşitlemesiyle aynı kilit altında.
            let _r = self.remind.lock().unwrap_or_else(|e| e.into_inner());
            self.set_flag("remind.off", !on)?;
            if !on {
                self.platform.reminders(&[]);
                let _ = std::fs::remove_file(self.flag("remind.n"));
                let mut s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
                s.remind_sig = None;
                s.agenda = None;
            }
        }
        if on {
            // Bağlıysa ajanda aboneliği başlar; cevabı bekleyen yok (kanal hemen bırakılır). `remind` kilidi burada
            // tutulmaz: `live` kilidiyle sıra ters dönmesin.
            let (tx, _rx) = channel();
            let _ = self.send_cmd(Cmd::Ask { what: "agenda", reply: tx });
        }
        Ok(())
    }

    /// Ajanda geldi: hatırlatıcılar açıksa saklanır ve alarm listesi eşitlenir; kapalıysa hiç tutulmaz (T-O5).
    pub(crate) fn take_agenda(&self, agenda: &Value) {
        let _r = self.remind.lock().unwrap_or_else(|e| e.into_inner());
        if !self.reminders_enabled() {
            return;
        }
        self.live_shared.lock().unwrap_or_else(|e| e.into_inner()).agenda = Some(agenda.clone());
        self.sync_locked(agenda);
    }

    /// Alarm listesini çıkarır ve DEĞİŞTİYSE Android'e baştan kurdurur. `remind` kilidi tutulurken çağrılır.
    fn sync_locked(&self, agenda: &Value) {
        if !self.reminders_enabled() || self.load().is_none() {
            return;
        }
        let list = crate::remind::plan(agenda);
        let sig = crate::remind::signature(&list);
        {
            let mut s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.remind_sig == Some(sig) {
                return;
            }
            s.remind_sig = Some(sig);
        }
        self.platform.reminders(&list);
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.flag("remind.n"), list.len().to_string());
        self.platform.changed();
    }

    /// G4: bilgisayar eşleşmeyi geçersiz saydı (`bye` revoked/repair, Noise ile doğrulanmış kanaldan). Alarmlar ve
    /// şifreli liste silinir, açılışta bağlanma kapanır, bellekteki ajanda bırakılır. Eşleşme kaydı kalır (uygulama
    /// "kaldırıldı" der, kullanıcı yeniden eşleşir ya da eşleşmeyi siler); hatırlatıcı ayarı (`remind.off`) değişmez.
    pub(crate) fn forget_remote(&self) {
        {
            // T-O4: aynı anda gelen eşitleme alarmları yeniden kuramaz.
            let _r = self.remind.lock().unwrap_or_else(|e| e.into_inner());
            self.platform.reminders(&[]);
            for f in ["remind.n", "boot.on"] {
                let _ = std::fs::remove_file(self.flag(f));
            }
            let mut s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            s.remind_sig = None;
            s.agenda = None;
        }
        self.platform.changed();
    }

    /// Telefon açılınca bağlan (Android açılış alıcısı `boot.on` işaretine bakar).
    pub fn autostart(&self) -> bool {
        self.flag("boot.on").exists()
    }
    pub fn autostart_set(&self, on: bool) -> Result<(), String> {
        if on && self.load().is_none() {
            return Err("Henüz eşleşmedin.".into());
        }
        self.set_flag("boot.on", on)
    }

    fn set_link(&self, link: &str, detail: &str) {
        let changed = {
            let mut s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            let changed = s.link != link || s.detail != detail;
            s.link = link.into();
            s.detail = detail.into();
            if link != "connected" {
                s.approvals.clear();
            }
            changed
        };
        if changed {
            self.platform.changed();
        }
    }

    /// Açılışta / eşleşince çağrılır: eşleşme varsa ve kullanıcı kapatmadıysa canlı bağlantıyı başlatır.
    pub fn live_start(self: &Arc<Self>) -> Result<(), String> {
        // T-D2: denetimler de kilit altında; `unpair` / kapatma ile araya girip silinmiş eşleşmeyle başlamaz.
        let mut h = self.live.lock().unwrap_or_else(|e| e.into_inner());
        if self.load().is_none() {
            return Err("Henüz eşleşmedin.".into());
        }
        if !self.live_enabled() {
            return Ok(());
        }
        if let Some(cur) = h.as_ref() {
            if !cur.join.is_finished() {
                return Ok(());
            }
        }
        let (tx, rx) = channel();
        let wake = tx.clone();
        let me = self.clone();
        self.live_shared.lock().unwrap_or_else(|e| e.into_inner()).running = true;
        let join = std::thread::Builder::new()
            .name("zv-live".into())
            .spawn(move || {
                // Hizmeti çağıranı bekletmeden, canlı iş parçacığının içinden aç.
                me.platform.service(true);
                let stopped = run(&me, rx, wake);
                me.live_shared.lock().unwrap_or_else(|e| e.into_inner()).running = false;
                // T-Y1: kalıcı bitişte de hizmet kapanır; durdurmada `live_stop` kapatır.
                if !stopped {
                    me.platform.service(false);
                }
                me.platform.changed();
            })
            .map_err(|_| "Bağlantı başlatılamadı.".to_string())?;
        *h = Some(LiveHandle { tx, join });
        drop(h);
        self.platform.changed();
        Ok(())
    }

    /// Canlı bağlantıyı ve ön plan hizmetini durdurur; iş parçacığının bitmesini bekler.
    pub fn live_stop(&self) {
        let mut h = self.live.lock().unwrap_or_else(|e| e.into_inner());
        self.stop_locked(&mut h);
    }

    /// T-D2: `live` kilidi tutulurken durdurur; aradaki `live_start` yeni iş parçacığı açamaz ve bitişteki "kapalı"
    /// durumu yeni başlayanın üstüne yazılmaz. Canlı iş parçacığı `live` kilidini hiç almaz (kilitlenme olmaz).
    pub(crate) fn stop_locked(&self, h: &mut Option<LiveHandle>) {
        if let Some(h) = h.take() {
            let _ = h.tx.send(Cmd::Stop);
            let _ = h.join.join();
        }
        self.live_shared.lock().unwrap_or_else(|e| e.into_inner()).running = false;
        self.platform.service(false);
        self.set_link("off", "");
    }

    /// Kullanıcı ayarı: arka planda bağlı kal / kalma. Kapatınca her şey durur.
    pub fn live_set_enabled(self: &Arc<Self>, on: bool) -> Result<(), String> {
        if on {
            let _ = std::fs::remove_file(self.off_marker());
            self.live_start()
        } else {
            let mut h = self.live.lock().unwrap_or_else(|e| e.into_inner());
            std::fs::create_dir_all(&self.dir).map_err(|_| "Ayar yazılamadı.".to_string())?;
            std::fs::write(self.off_marker(), b"1").map_err(|_| "Ayar yazılamadı.".to_string())?;
            self.stop_locked(&mut h);
            Ok(())
        }
    }

    fn send_cmd(&self, c: Cmd) -> Result<(), String> {
        let h = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let Some(h) = h.as_ref() else { return Err(ERR_OFFLINE.into()) };
        h.tx.send(c).map_err(|_| ERR_OFFLINE.to_string())
    }

    fn wait<T>(rx: Receiver<Result<T, String>>) -> Result<T, String> {
        match rx.recv_timeout(CALLER_WAIT) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(ERR_NO_ANSWER.into()),
            Err(RecvTimeoutError::Disconnected) => Err(ERR_DROPPED.into()),
        }
    }

    /// Onay/ret isteği (bildirim eylemi). Kimlik doğrulama canlı iş parçacığında yapılır; sonuç bildirimle gelir.
    pub fn decide(&self, id: &str, allow: bool) -> Result<(), String> {
        self.send_cmd(Cmd::Decide { id: id.to_owned(), allow, reply: None })
    }

    /// Uygulama içi onay/ret: imza + masaüstünün `ack`'ı beklenir.
    pub fn decide_wait(&self, id: &str, allow: bool) -> Result<(), String> {
        let (tx, rx) = channel();
        self.send_cmd(Cmd::Decide { id: id.to_owned(), allow, reply: Some(tx) })?;
        Self::wait(rx)
    }

    fn session_name(&self, session: &str) -> String {
        let s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
        s.sessions.iter().find(|x| x.id == session).map(|x| x.name.clone()).unwrap_or_else(|| "Oturum".into())
    }

    /// İmzalı eylemi doğrular, gönderir ve `ack`'ı bekler. Korumasız cihazda kapalı eylem parmak izi istemeden reddedilir.
    fn act_wait(&self, action: Value, title: &str, subtitle: String) -> Result<(), String> {
        let parsed: Action = serde_json::from_value(action.clone()).map_err(|_| "Geçersiz eylem.".to_string())?;
        parsed.validate()?;
        let hw = self.live_shared.lock().unwrap_or_else(|e| e.into_inner()).hw;
        if RESTRICT_UNPROTECTED && hw == Some(false) && !parsed.allowed_unprotected() {
            return Err(zodyvel_phone_proto::devices::UNPROTECTED_DENIED.into());
        }
        let (tx, rx) = channel();
        self.send_cmd(Cmd::Act { action, prompt: Prompt { title: title.into(), subtitle }, reply: tx })?;
        Self::wait(rx)
    }

    /// Soru formu cevabı: anahtar = soru metni, değer = seçenek(ler) (", " ile) ya da serbest metin.
    pub fn answer(&self, id: &str, answers: Map<String, Value>) -> Result<(), String> {
        let name = {
            let s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            s.approvals.iter().find(|a| a.id == id).map(|a| a.session.clone()).unwrap_or_else(|| "Oturum".into())
        };
        self.act_wait(json!({"k": "answer", "id": id, "answers": answers}), "Zodyvel · Cevap gönder", name)
    }
    /// Çalışan oturuma not; `now=false` → tur bitince iletilir.
    pub fn note(&self, session: &str, text: &str, now: bool) -> Result<(), String> {
        let sub = format!("{} · {}", self.session_name(session), clip(text.trim(), 60));
        self.act_wait(json!({"k": "note", "session": session, "text": text, "now": now}), "Zodyvel · Not gönder", sub)
    }
    /// Bitmiş/boştaki oturuma yeni iş.
    pub fn task(&self, session: &str, text: &str) -> Result<(), String> {
        let sub = format!("{} · {}", self.session_name(session), clip(text.trim(), 60));
        self.act_wait(json!({"k": "task", "session": session, "text": text}), "Zodyvel · Görev ver", sub)
    }
    /// Yeni görev: `folder` = `folders()`'tan gelen opak kimlik.
    pub fn new_task(&self, folder: &str, prompt: &str) -> Result<(), String> {
        let fname = {
            let s = self.live_shared.lock().unwrap_or_else(|e| e.into_inner());
            s.folders.iter().find(|f| f.0 == folder).map(|f| f.1.clone()).unwrap_or_else(|| "Klasör".into())
        };
        let sub = format!("{fname} · {}", clip(prompt.trim(), 60));
        self.act_wait(json!({"k": "new_task", "folder": folder, "prompt": prompt}), "Zodyvel · Yeni görev", sub)
    }
    /// Oturumu durdurur.
    pub fn stop_session(&self, session: &str) -> Result<(), String> {
        let sub = self.session_name(session);
        self.act_wait(json!({"k": "stop", "session": session}), "Zodyvel · Durdur", sub)
    }

    /// Yeni görev klasörleri (imzasız; bağlıyken masaüstünden istenir).
    pub fn folders(&self) -> Result<Value, String> {
        let (tx, rx) = channel();
        self.send_cmd(Cmd::Ask { what: "folders", reply: tx })?;
        Self::wait(rx)
    }

    /// Ajanda (14 gün). Bağlıyken istenir ve bellekte saklanır; bağlı değilse son kopya.
    pub fn agenda(&self) -> Result<Value, String> {
        let (tx, rx) = channel();
        let live = self.send_cmd(Cmd::Ask { what: "agenda", reply: tx }).and_then(|_| Self::wait(rx));
        match live {
            Ok(v) => Ok(v),
            Err(e) => self.live_shared.lock().unwrap_or_else(|p| p.into_inner()).agenda.clone().ok_or(e),
        }
    }
}

/// İmzalayıcı: sistem doğrulaması iptal/hata ile mi, yoksa ağ aşamasında mı başarısız olduğu ayırt edilir.
struct Recording<'a> {
    inner: &'a dyn Signer,
    err: std::cell::RefCell<Option<String>>,
}
impl Signer for Recording<'_> {
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, String> {
        self.inner.sign(msg).inspect_err(|e| *self.err.borrow_mut() = Some(e.clone()))
    }
}

/// `ack` bekleyen eylem.
struct Pending {
    /// Onay kimliği (bildirim silme/uyarı için); onay dışı eylemde `None`.
    approval: Option<String>,
    reply: Option<Reply<()>>,
    deadline: Instant,
}

#[derive(Default)]
struct Mem {
    /// Bildirimi verilmiş onay kimlikleri (yeniden bağlanmada tekrar bildirilmez).
    notified: HashSet<String>,
    /// Son bilinen oturum durumları ("done"/"error" geçişini yakalamak için).
    status: HashMap<String, String>,
    current: Vec<ApprovalInfo>,
    /// Yollanmış, `ack` bekleyen eylemler: n → bekleyen.
    acks: HashMap<u64, Pending>,
    /// Cevap bekleyen istekler (folders/agenda).
    asks: Vec<(&'static str, Instant, Reply<Value>)>,
    last_text: String,
}

impl Mem {
    /// Bağlantı bitti: bekleyen çağıranlar "Bağlantı koptu." alır (gönderici düşünce).
    fn drop_waiters(&mut self) {
        for (_, p) in self.acks.drain() {
            if let Some(r) = p.reply {
                let _ = r.send(Err(ERR_DROPPED.into()));
            }
        }
        for (_, _, r) in self.asks.drain(..) {
            let _ = r.send(Err(ERR_DROPPED.into()));
        }
    }
    /// En yakın bekleme süresi dolumu (T-O2: canlı iş parçacığı en geç o an uyanır).
    fn next_deadline(&self) -> Option<Instant> {
        self.acks.values().map(|p| p.deadline).chain(self.asks.iter().map(|a| a.1)).min()
    }
    /// Süresi dolan bekleyenler.
    fn expire(&mut self) {
        let now = Instant::now();
        let late: Vec<u64> = self.acks.iter().filter(|(_, p)| p.deadline <= now).map(|(n, _)| *n).collect();
        for n in late {
            if let Some(Pending { reply: Some(r), .. }) = self.acks.remove(&n) {
                let _ = r.send(Err(ERR_NO_ANSWER.into()));
            }
        }
        let (late, keep): (Vec<_>, Vec<_>) = self.asks.drain(..).partition(|a| a.1 <= now);
        self.asks = keep;
        for (_, _, r) in late {
            let _ = r.send(Err(ERR_NO_ANSWER.into()));
        }
    }
}

enum End {
    Stop,
    Dropped,
    Bye(String),
}

/// T-D4: canlı iş parçacığı boyunca bellekte tutulan kimlik (Keystore her yeniden denemede açılmaz). Bırakılınca
/// Noise özel anahtarı sıfırlanır.
struct Ident {
    kp: Keypair,
    desk: Vec<u8>,
}
impl Drop for Ident {
    fn drop(&mut self) {
        self.kp.private.zeroize();
    }
}

/// Kimliği ilk başarılı açılışta saklar; sonra hep aynısını verir.
fn ident<'a>(core: &Core, slot: &'a mut Option<Ident>) -> Result<&'a Ident, String> {
    if slot.is_none() {
        let (_, kp, desk) = core.identity()?;
        *slot = Some(Ident { kp, desk });
    }
    slot.as_ref().ok_or_else(|| "Anahtar açılamadı.".to_string())
}

/// T-O2: canlı bağlantı. `client::Link` ile aynı iletileri konuşur; farkı, ham soketin bir kopyasının bekçi iş
/// parçacığına verilebilmesi (bağlıyken yoklama gerekmez).
struct Conn {
    ch: Channel<TcpStream>,
    raw: TcpStream,
    n: u64,
}

fn dial(addr: &str, id: &Ident) -> Result<Conn, String> {
    let sa: SocketAddr = addr.trim().to_socket_addrs().map_err(|_| "Adres geçersiz.".to_string())?.next().ok_or("Adres geçersiz.")?;
    let mut s = TcpStream::connect_timeout(&sa, CONNECT_WAIT).map_err(|_| "Bilgisayara ulaşılamadı.".to_string())?;
    let _ = s.set_nodelay(true);
    s.set_read_timeout(Some(IO_WAIT)).map_err(|_| "Bağlantı kurulamadı.".to_string())?;
    s.set_write_timeout(Some(IO_WAIT)).map_err(|_| "Bağlantı kurulamadı.".to_string())?;
    s.write_all(&[MODE_NORMAL]).map_err(|_| "Bilgisayara ulaşılamadı.".to_string())?;
    let raw = s.try_clone().map_err(|_| "Bağlantı kurulamadı.".to_string())?;
    let ch = zodyvel_phone_proto::session::connect(s, &id.kp, b"", Trust::PinnedKey(&id.desk))
        .map_err(|_| "Bu bilgisayarla eşli değilsin ya da bağlantı reddedildi.".to_string())?;
    Ok(Conn { ch, raw, n: 0 })
}

impl Conn {
    fn send(&mut self, v: Value) -> Result<(), String> {
        self.ch.send(v.to_string().as_bytes()).map_err(|_| ERR_DROPPED.to_string())
    }
    /// Bekçi veri olduğunu söyledi: tek ileti okunur (yarım çerçeve için en çok `IO_WAIT`). Sonra soketin bekleme
    /// süresi bekçininkine döner.
    fn recv_now(&mut self) -> Result<Value, String> {
        let _ = self.raw.set_read_timeout(Some(IO_WAIT));
        let r = self.ch.recv().map_err(|_| ERR_DROPPED.to_string()).and_then(|b| {
            if b.len() > MAX_INCOMING * 8 {
                return Err("İleti çok büyük.".to_string());
            }
            serde_json::from_slice(&b).map_err(|_| "İleti okunamadı.".to_string())
        });
        let _ = self.raw.set_read_timeout(Some(DEAD_AFTER));
        r
    }
    fn ping(&mut self) -> Result<(), String> {
        self.send(json!({"t": "ping"}))
    }
    /// "agenda" | "folders"
    fn request(&mut self, what: &str) -> Result<(), String> {
        self.send(json!({"t": what}))
    }
    /// Eylemi imzalayıp yollar; dönen sayı `ack` ile eşleşir. Kullanıcı doğrulaması `signer` içinde olur.
    fn act(&mut self, action: &Value, signer: &dyn Signer) -> Result<u64, String> {
        self.n += 1;
        let a = action.to_string();
        let sig = signer.sign(&signing_bytes(&self.ch.handshake_hash, self.n, &a))?;
        self.send(json!({"t": "act", "n": self.n, "a": a, "sig": hex(&sig)}))?;
        Ok(self.n)
    }
}

/// T-O2: bekçi. Soketi engelleyerek bekler (en çok `DEAD_AFTER`; bağlı oturumda ping cevabı ondan çok önce gelir),
/// veri ya da kapanış görünce canlı iş parçacığına haber verir ve okunana dek (`resume`) bekler. Oturum bitince
/// (`resume` bırakılır, soket kapatılır) kendiliğinden çıkar.
fn watch(s: TcpStream, tx: Sender<Cmd>, gen: u64, resume: Receiver<()>) {
    let mut one = [0u8; 1];
    loop {
        let open = match s.peek(&mut one) {
            Ok(n) => n > 0,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => {
                match resume.try_recv() {
                    Err(TryRecvError::Disconnected) => return,
                    _ => continue,
                }
            }
            Err(_) => false,
        };
        if tx.send(Cmd::Wire { gen, open }).is_err() || !open || resume.recv().is_err() {
            return;
        }
    }
}

/// A-D8: mDNS araması sürerken çok yöne yayın kilidi açık; panikte de bırakılır.
struct Multicast<'a>(&'a dyn Platform);
impl<'a> Multicast<'a> {
    fn hold(p: &'a dyn Platform) -> Self {
        p.multicast(true);
        Multicast(p)
    }
}
impl Drop for Multicast<'_> {
    fn drop(&mut self) {
        self.0.multicast(false);
    }
}

enum Found {
    Conn(Box<Conn>),
    Stop,
    Nothing,
}

/// T-O6: arama sırasında gelen komutlara bakar. Durdurma → `true`; diğerleri bağlanınca işlenmek üzere `early`'ye.
fn stop_asked(rx: &Receiver<Cmd>, early: &mut Vec<Cmd>) -> bool {
    loop {
        match rx.try_recv() {
            Ok(Cmd::Stop) | Err(TryRecvError::Disconnected) => return true,
            Ok(Cmd::Wire { .. }) => {}
            Ok(c) => early.push(c),
            Err(TryRecvError::Empty) => return false,
        }
    }
}

/// G3: kayıtlı adres tutmuyorsa bilgisayarı yerel ağda arar; her adayı kayıtlı masaüstü anahtarıyla dener.
/// Doğrulanan adres kaydedilir. Adaylar arasında durdurma isteğine bakılır (T-O6).
fn search(core: &Arc<Core>, id: &Ident, rx: &Receiver<Cmd>, early: &mut Vec<Cmd>) -> Found {
    let Some(saved) = core.saved_addr() else { return Found::Nothing };
    if stop_asked(rx, early) {
        return Found::Stop;
    }
    core.set_link("searching", "Bilgisayar aranıyor…");
    let found = match core.platform.discover() {
        Some(list) => list,
        None => {
            let _lock = Multicast::hold(core.platform.as_ref());
            crate::discover::browse(crate::discover::BROWSE_FOR, crate::discover::MAX_CANDIDATES)
        }
    };
    for addr in found.into_iter().filter(|a| *a != saved).take(crate::discover::MAX_CANDIDATES) {
        if stop_asked(rx, early) {
            return Found::Stop;
        }
        if let Ok(c) = dial(&addr, id) {
            let _ = core.set_addr(&addr);
            return Found::Conn(Box::new(c));
        }
    }
    Found::Nothing
}

/// T-Y2: bağlantısızlık sayaçları. Saf mantık (zaman dışarıdan verilir) ki sınanabilsin.
#[derive(Debug)]
pub struct Offline {
    /// Son başarılı bağlantı (ya da iş parçacığının başı).
    since: Instant,
    fails: u32,
    search_every: Duration,
    last_search: Option<Instant>,
    /// Art arda "eşli değilsin" sayısı.
    unpaired: u32,
    /// T-O1: sıradaki yeniden deneme beklemesi (her denemede ikiye katlanır).
    retry: Duration,
}

impl Offline {
    pub fn new(now: Instant) -> Offline {
        Offline { since: now, fails: 0, search_every: crate::discover::SEARCH_EVERY, last_search: None, unpaired: 0, retry: RETRY_MIN }
    }
    /// Bağlantı kuruldu (el sıkışma geçti): adres doğru, arama sayaçları başa döner ("eşli değilsin" sayacı ve
    /// yeniden deneme beklemesi hariç; T-O1: bekleme ancak `session_end`'de, bağlantı kalıcı olduysa sıfırlanır).
    pub fn connected(&mut self, now: Instant) {
        self.since = now;
        self.fails = 0;
        self.search_every = crate::discover::SEARCH_EVERY;
    }
    /// T-O1: bağlantı bitti. `up`: kurulduğu an; `got_state`: durum iletisi geldi mi. Ancak ikisi birlikte
    /// (`STABLE_AFTER` boyunca ayakta + durum) bekleme başa döner; hemen kopan sunucuda üstel artış sürer.
    pub fn session_end(&mut self, now: Instant, up: Instant, got_state: bool) {
        if got_state && now.duration_since(up) >= STABLE_AFTER {
            self.retry = RETRY_MIN;
        }
        if got_state {
            self.since = now;
        }
    }
    /// T-O1: sıradaki bekleme: en az `floor`, en çok `cap`; sonraki için ikiye katlanır.
    pub fn backoff(&mut self, floor: Duration, cap: Duration) -> Duration {
        let w = self.retry.max(floor).min(cap);
        self.retry = (w * 2).min(cap);
        w
    }
    /// Bağlantı hatası; `true` → şimdi aranmalı (art arda hata ve aralık doldu). Her aramadan sonra aralık ikiye katlanır.
    pub fn failed(&mut self, now: Instant) -> bool {
        self.fails += 1;
        let due = self.last_search.is_none_or(|t| now.duration_since(t) >= self.search_every);
        if self.fails >= crate::discover::FAILS_BEFORE_SEARCH && due {
            if self.last_search.is_some() {
                self.search_every = (self.search_every * 2).min(SEARCH_MAX);
            }
            self.last_search = Some(now);
            return true;
        }
        false
    }
    /// Geri çekilmenin üst sınırı: uzun bağlantısızlıkta daha seyrek.
    pub fn retry_cap(&self, now: Instant) -> Duration {
        if now.duration_since(self.since) >= SLOW_AFTER { RETRY_SLOW } else { RETRY_MAX }
    }
    /// Uzun süredir hiç bağlantı yok: deneme durmalı.
    pub fn give_up(&self, now: Instant) -> bool {
        now.duration_since(self.since) >= GIVE_UP_AFTER
    }
    /// Bilgisayar "eşli değilsin" dedi; `true` → sınıra ulaşıldı, telefon kaldırılmış sayılır.
    pub fn unpaired(&mut self) -> bool {
        self.unpaired += 1;
        self.unpaired >= UNPAIRED_LIMIT
    }
    /// Başka bir sonuç geldi: "eşli değilsin" dizisi bozuldu.
    pub fn paired_again(&mut self) {
        self.unpaired = 0;
    }
    #[cfg(test)]
    pub fn search_every(&self) -> Duration {
        self.search_every
    }
}

/// `true` → kullanıcı durdurdu (`live_stop`); `false` → kalıcı bitiş (hizmet de kapanmalı).
/// `wake`: bekçilerin haber yolladığı kanal (komut kanalının kendisi).
fn run(core: &Arc<Core>, rx: Receiver<Cmd>, wake: Sender<Cmd>) -> bool {
    let mut mem = Mem::default();
    let mut off = Offline::new(Instant::now());
    let mut id: Option<Ident> = None;
    let mut gen = 0u64;
    loop {
        if off.give_up(Instant::now()) {
            core.set_link("idle", "Uzun süredir bilgisayara ulaşılamadı; uygulamayı açınca yeniden denenir.");
            return false;
        }
        core.set_link("connecting", "");
        // Bağlanırken gelen komutlar (bildirimden soğuk açılış) bağlanınca işlenir.
        let mut early: Vec<Cmd> = Vec::new();
        let mut res = ident(core, &mut id).and_then(|i| dial(&core.saved_addr().ok_or("Henüz eşleşmedin.")?, i));
        if res.is_err() && off.failed(Instant::now()) && core.load().is_some() {
            if let Some(i) = id.as_ref() {
                match search(core, i, &rx, &mut early) {
                    Found::Conn(c) => res = Ok(*c),
                    Found::Stop => return true,
                    Found::Nothing => {}
                }
            }
        }
        let end = match res {
            Ok(conn) => {
                gen += 1;
                let up = Instant::now();
                off.connected(up);
                let (e, got_state) = session(core, conn, &rx, &wake, gen, &mut mem, early);
                mem.drop_waiters();
                off.session_end(Instant::now(), up, got_state);
                e
            }
            Err(e) => {
                for c in early {
                    refuse(core, c);
                }
                core.set_link("retry", &e);
                let w = off.backoff(RETRY_MIN, off.retry_cap(Instant::now()));
                if wait_cmds(core, &rx, w) {
                    return true;
                }
                continue;
            }
        };
        if !matches!(&end, End::Bye(w) if w == "unpaired") {
            off.paired_again();
        }
        let cap = off.retry_cap(Instant::now());
        match end {
            End::Stop => return true,
            End::Dropped => {
                core.set_link("retry", "Bağlantı koptu; yeniden deneniyor.");
                notify_text(core, &mut mem, "Bağlantı yok · yeniden deneniyor");
                if wait_cmds(core, &rx, off.backoff(RETRY_MIN, cap)) {
                    return true;
                }
            }
            End::Bye(why) => {
                // T-Y2: "eşli değilsin" (bilinmeyen anahtar, doğrulanmış kanaldan) art arda gelirse "kaldırıldı" sayılır.
                let why = if why == "unpaired" && off.unpaired() { "revoked".to_owned() } else { why };
                // T-O1: (en az, en çok) bekleme; aradaki bekleme her seferinde ikiye katlanır.
                let (link, msg, retry): (&str, &str, Option<(Duration, Duration)>) = match why.as_str() {
                    "repair" => ("repair", "30 gündür bağlanmadın; yeniden eşleşmen gerekiyor.", None),
                    "revoked" => ("revoked", "Bu telefonun eşleşmesi bilgisayardan kaldırıldı.", None),
                    "unpaired" => ("retry", "Bilgisayar bu telefonu tanımadı; yeniden deneniyor.", Some((RETRY_MIN, cap))),
                    "network" | "locked" => ("retry", "Bilgisayar şu an bağlantıya kapalı (ağ Özel değil ya da kilitli).", Some((RETRY_LOCKED, RETRY_SLOW))),
                    "replaced" => ("retry", "Başka bir bağlantı bu telefonun yerini aldı.", Some((Duration::from_secs(30), cap))),
                    "busy" => ("retry", "Bilgisayar meşgul.", Some((Duration::from_secs(15), cap))),
                    _ => ("retry", "Bilgisayar bağlantıyı kapattı.", Some((Duration::from_secs(15), cap))),
                };
                core.set_link(link, msg);
                notify_text(core, &mut mem, if retry.is_some() { "Bağlantı yok · yeniden deneniyor" } else { "Eşleşme geçersiz · uygulamayı aç" });
                match retry {
                    // Kalıcı sonuç: bağlantı denemesi biter; kullanıcı uygulamada görür.
                    None => {
                        // G4: bilgisayar bu eşleşmeyi artık tanımıyor (kaldırıldı / 30 gün): hatırlatıcılar ve
                        // açılışta bağlanma da kapanır (kaybolan telefonda plan başlıkları kalmasın).
                        core.forget_remote();
                        core.platform.notice(&Notice::Info(msg.into()));
                        return false;
                    }
                    Some((floor, cap)) => {
                        if wait_cmds(core, &rx, off.backoff(floor, cap)) {
                            return true;
                        }
                    }
                }
            }
        }
    }
}

/// Bağlı değilken gelen komut: eylem/istek hemen "bağlı değilsin" alır. `true` → durdurma.
fn refuse(core: &Core, c: Cmd) -> bool {
    match c {
        Cmd::Stop => return true,
        Cmd::Decide { reply: None, .. } => core.platform.notice(&Notice::Info("Bilgisayara bağlı değilsin; onay gönderilemedi.".into())),
        Cmd::Decide { reply: Some(r), .. } | Cmd::Act { reply: r, .. } => {
            let _ = r.send(Err(ERR_OFFLINE.into()));
        }
        Cmd::Ask { reply, .. } => {
            let _ = reply.send(Err(ERR_OFFLINE.into()));
        }
        // Bitmiş bağlantının bekçisinden geç kalan haber.
        Cmd::Wire { .. } => {}
    }
    false
}

/// Bağlı değilken bekler. `true` → durdurma geldi.
fn wait_cmds(core: &Core, rx: &Receiver<Cmd>, total: Duration) -> bool {
    let end = Instant::now() + total;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        match rx.recv_timeout(left) {
            Ok(c) => {
                if refuse(core, c) {
                    return true;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return true,
            Err(RecvTimeoutError::Timeout) => return false,
        }
    }
}

fn notify_text(core: &Core, mem: &mut Mem, t: &str) {
    if mem.last_text != t {
        mem.last_text = t.to_owned();
        core.platform.notice(&Notice::Service(t.to_owned()));
    }
}

/// Bağlı oturum. Dönen `bool`: durum iletisi geldi mi (T-O1).
fn session(core: &Arc<Core>, mut conn: Conn, rx: &Receiver<Cmd>, wake: &Sender<Cmd>, gen: u64, mem: &mut Mem, early: Vec<Cmd>) -> (End, bool) {
    let (resume, resumed) = channel::<()>();
    let started = conn.raw.try_clone().and_then(|s| {
        conn.raw.set_read_timeout(Some(DEAD_AFTER))?;
        let tx = wake.clone();
        std::thread::Builder::new().name("zv-watch".into()).spawn(move || watch(s, tx, gen, resumed)).map(|_| ())
    });
    if started.is_err() {
        return (End::Dropped, false);
    }
    let mut got_state = false;
    let end = session_loop(core, &mut conn, rx, gen, mem, early, &mut got_state, &resume);
    // Bekçi soket kapanınca (Linux'ta hemen, aksi hâlde en geç `DEAD_AFTER` sonra) çıkar; geç kalan haberi `gen` ayıklar.
    let _ = conn.raw.shutdown(Shutdown::Both);
    drop(resume);
    (end, got_state)
}

#[allow(clippy::too_many_arguments)]
fn session_loop(core: &Arc<Core>, conn: &mut Conn, rx: &Receiver<Cmd>, gen: u64, mem: &mut Mem, early: Vec<Cmd>, got_state: &mut bool, resume: &Sender<()>) -> End {
    let mut last_rx = Instant::now();
    let mut last_ping = Instant::now();
    // Bağlantı kurulmadan gelen komutlar (bildirimden soğuk açılış) ilk durum gelene dek bekler: yoksa "geçersiz" sayılırdı.
    let mut deferred = early;
    loop {
        if *got_state {
            for c in std::mem::take(&mut deferred) {
                if run_cmd(core, conn, mem, c).is_err() {
                    return End::Dropped;
                }
            }
        }
        mem.expire();
        let now = Instant::now();
        if now.duration_since(last_rx) >= DEAD_AFTER {
            return End::Dropped;
        }
        if now.duration_since(last_ping) >= PING_EVERY {
            if conn.ping().is_err() {
                return End::Dropped;
            }
            last_ping = now;
        }
        // T-O2: tek uyuma noktası; komut, bekçi haberi, ping zamanı, ölü bağlantı süresi ya da bekleyen cevabın süresi.
        let mut until = (last_ping + PING_EVERY).min(last_rx + DEAD_AFTER);
        if let Some(d) = mem.next_deadline() {
            until = until.min(d);
        }
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => return End::Stop,
            Ok(Cmd::Wire { gen: g, open }) if g == gen => {
                if !open {
                    return End::Dropped;
                }
                let m = conn.recv_now();
                let _ = resume.send(());
                let Ok(m) = m else { return End::Dropped };
                last_rx = Instant::now();
                if m.get("t").and_then(Value::as_str) == Some("state") && !*got_state {
                    *got_state = true;
                    // G3: hatırlatıcılar açıksa ajanda aboneliği (masaüstü ilkini yollar, sonra değişince iter).
                    if core.reminders_enabled() && conn.request("agenda").is_err() {
                        return End::Dropped;
                    }
                }
                if let Some(end) = on_message(core, mem, &m) {
                    return end;
                }
            }
            Ok(Cmd::Wire { .. }) => {}
            Ok(c) if *got_state => {
                if run_cmd(core, conn, mem, c).is_err() {
                    return End::Dropped;
                }
            }
            Ok(c) => deferred.push(c),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// `Err(())`: bağlantı koptu.
fn run_cmd(core: &Core, link: &mut Conn, mem: &mut Mem, c: Cmd) -> Result<(), ()> {
    match c {
        Cmd::Decide { id, allow, reply } => decide_now(core, link, mem, &id, allow, reply),
        Cmd::Act { action, prompt, reply } => send_signed(core, link, mem, &action, prompt, None, Some(reply)),
        Cmd::Ask { what, reply } => {
            if link.request(what).is_err() {
                let _ = reply.send(Err(ERR_DROPPED.into()));
                return Err(());
            }
            mem.asks.push((what, Instant::now() + ASK_WAIT, reply));
            Ok(())
        }
        Cmd::Stop | Cmd::Wire { .. } => Ok(()),
    }
}

/// İmzalar ve yollar. Kimlik doğrulama iptali/hatası bağlantıyı bozmaz; hiçbir şey gitmez.
fn send_signed(core: &Core, link: &mut Conn, mem: &mut Mem, action: &Value, prompt: Prompt, approval: Option<String>, reply: Option<Reply<()>>) -> Result<(), ()> {
    let signer = core.signer(prompt);
    let rec = Recording { inner: &signer, err: Default::default() };
    match link.act(action, &rec) {
        Ok(n) => {
            // Süre imzadan SONRA başlar: kullanıcı parmak izinde beklerken işlemez.
            mem.acks.insert(n, Pending { approval, reply, deadline: Instant::now() + ACK_WAIT });
            Ok(())
        }
        Err(e) => {
            if let Some(why) = rec.err.borrow().clone() {
                let canceled = why.contains("İptal");
                match reply {
                    Some(r) => {
                        let _ = r.send(Err(if canceled { ERR_CANCELED.into() } else { why }));
                    }
                    None if !canceled => core.platform.notice(&Notice::Info(why)),
                    None => {}
                }
                Ok(())
            } else {
                match reply {
                    Some(r) => {
                        let _ = r.send(Err(ERR_DROPPED.into()));
                    }
                    None => core.platform.notice(&Notice::Info(format!("Onay gönderilemedi: {e}"))),
                }
                Err(())
            }
        }
    }
}

fn decide_now(core: &Core, link: &mut Conn, mem: &mut Mem, id: &str, allow: bool, reply: Option<Reply<()>>) -> Result<(), ()> {
    let fail = |reply: Option<Reply<()>>, msg: &str| match reply {
        Some(r) => {
            let _ = r.send(Err(msg.into()));
        }
        None => core.platform.notice(&Notice::Info(msg.into())),
    };
    let Some(ap) = mem.current.iter().find(|a| a.id == id).cloned() else {
        fail(reply, "Bu onay artık geçerli değil.");
        core.platform.notice(&Notice::Gone { id: id.to_owned() });
        return Ok(());
    };
    if ap.kind == "question" {
        fail(reply, "Bu bir soru; cevap formundan yanıtla.");
        return Ok(());
    }
    if ap.kind != "permission" && reply.is_none() {
        fail(None, "Bu istek yalnız bilgisayardan ya da uygulamadan cevaplanır.");
        return Ok(());
    }
    let title = if allow { "Zodyvel · İzin ver" } else { "Zodyvel · Reddet" };
    let subtitle = format!("{} · {}", ap.session, clip(ap.action.as_deref().unwrap_or(""), 80));
    let action = json!({"k": "decide", "id": id, "d": if allow { "allow" } else { "deny" }});
    send_signed(core, link, mem, &action, Prompt { title: title.into(), subtitle }, Some(id.to_owned()), reply)
}

fn on_message(core: &Arc<Core>, mem: &mut Mem, m: &Value) -> Option<End> {
    match m.get("t").and_then(Value::as_str) {
        Some("state") => {
            on_state(core, mem, m);
            None
        }
        Some("device") => {
            {
                let mut s = core.live_shared.lock().unwrap_or_else(|e| e.into_inner());
                s.hw = m.get("hw").and_then(Value::as_bool);
                s.hw_level = m.get("level").and_then(Value::as_str).map(str::to_owned);
            }
            core.platform.changed();
            None
        }
        Some("ack") => {
            let n = m.get("n").and_then(Value::as_u64).unwrap_or(0);
            if let Some(p) = mem.acks.remove(&n) {
                let ok = m.get("ok").and_then(Value::as_bool) == Some(true);
                let e = clip(m.get("err").and_then(Value::as_str).unwrap_or(""), 120);
                if let Some(r) = p.reply {
                    let _ = r.send(if ok { Ok(()) } else { Err(if e.is_empty() { "Bilgisayar kabul etmedi.".into() } else { e.clone() }) });
                }
                match (p.approval, ok) {
                    (Some(id), true) => core.platform.notice(&Notice::Gone { id }),
                    (Some(_), false) => core.platform.notice(&Notice::Info(format!("Onay kabul edilmedi{}{}", if e.is_empty() { "" } else { ": " }, clip(&e, 80)))),
                    (None, _) => {}
                }
            }
            None
        }
        Some("folders") => {
            let list = m.get("list").cloned().unwrap_or(Value::Array(vec![]));
            let pairs: Vec<(String, String)> = list
                .as_array()
                .map(|a| a.iter().filter_map(|f| Some((f.get("id")?.as_str()?.to_owned(), f.get("name")?.as_str()?.to_owned()))).collect())
                .unwrap_or_default();
            core.live_shared.lock().unwrap_or_else(|e| e.into_inner()).folders = pairs;
            answer_asks(mem, "folders", &list);
            None
        }
        Some("agenda") => {
            let mut v = m.clone();
            if let Some(o) = v.as_object_mut() {
                o.remove("t");
            }
            core.take_agenda(&v);
            answer_asks(mem, "agenda", &v);
            None
        }
        Some("bye") => Some(End::Bye(m.get("why").and_then(Value::as_str).unwrap_or("").to_owned())),
        _ => None,
    }
}

fn answer_asks(mem: &mut Mem, what: &str, v: &Value) {
    let (mine, rest): (Vec<_>, Vec<_>) = mem.asks.drain(..).partition(|a| a.0 == what);
    mem.asks = rest;
    for (_, _, r) in mine {
        let _ = r.send(Ok(v.clone()));
    }
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn on_state(core: &Arc<Core>, mem: &mut Mem, m: &Value) {
    let sessions: Vec<SessionInfo> = m
        .get("sessions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| {
                    Some(SessionInfo {
                        id: s.get("id")?.as_str()?.to_owned(),
                        name: s.get("name")?.as_str()?.to_owned(),
                        status: s.get("status")?.as_str()?.to_owned(),
                        tool: str_of(s, "tool"),
                        folder: str_of(s, "folder"),
                        started: s.get("started").and_then(Value::as_i64),
                        step: s.get("step").and_then(Value::as_str).map(str::to_owned),
                        steps: s.get("steps").and_then(|x| serde_json::from_value::<Vec<StepInfo>>(x.clone()).ok()),
                        reply: s.get("reply").and_then(Value::as_str).map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let name_of = |id: &str| sessions.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_else(|| "Oturum".into());
    let approvals: Vec<ApprovalInfo> = m
        .get("approvals")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let id = x.get("id")?.as_str()?.to_owned();
                    let sid = str_of(x, "session");
                    Some(ApprovalInfo {
                        session: name_of(&sid),
                        session_id: sid,
                        kind: x.get("kind")?.as_str()?.to_owned(),
                        action: x.get("action").and_then(Value::as_str).map(str::to_owned),
                        questions: x.get("questions").and_then(|q| serde_json::from_value::<Vec<QuestionInfo>>(q.clone()).ok()),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Yeni onaylar → bildirim; kalkanlar → bildirimi sil.
    for a in &approvals {
        if mem.notified.insert(a.id.clone()) {
            core.platform.notice(&Notice::Approval { id: a.id.clone(), session: a.session.clone(), kind: a.kind.clone() });
        }
    }
    let gone: Vec<String> = mem.notified.iter().filter(|id| !approvals.iter().any(|a| &a.id == *id)).cloned().collect();
    for id in gone {
        mem.notified.remove(&id);
        core.platform.notice(&Notice::Gone { id });
    }
    // Oturum durumu geçişleri: yalnız ÖNCEDEN bilinen oturumun done/error'a dönmesi bildirilir (ilk durum temel alınır).
    for s in &sessions {
        if let Some(prev) = mem.status.get(&s.id) {
            if prev != &s.status {
                match s.status.as_str() {
                    "done" => core.platform.notice(&Notice::Finished { session: s.name.clone() }),
                    "error" => core.platform.notice(&Notice::Failed { session: s.name.clone() }),
                    _ => {}
                }
            }
        }
    }
    mem.status = sessions.iter().map(|s| (s.id.clone(), s.status.clone())).collect();

    let changed = {
        let mut s = core.live_shared.lock().unwrap_or_else(|e| e.into_inner());
        let changed = s.link != "connected" || !s.detail.is_empty() || s.approvals != approvals || s.sessions != sessions;
        s.link = "connected".into();
        s.detail = String::new();
        s.approvals = approvals.clone();
        s.sessions = sessions;
        s.last_connected = Some(now_ms());
        changed
    };
    // A-O1: arayüz yalnız gerçekten bir şey değişince yeniden çeker.
    if changed {
        core.platform.changed();
    }
    mem.current = approvals;
    let text = match mem.current.len() {
        0 => "Bağlı".to_owned(),
        n => format!("Bağlı · {n} onay bekliyor"),
    };
    notify_text(core, mem, &text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ty2_search_backs_off_and_resets_on_connect() {
        let t0 = Instant::now();
        let mut o = Offline::new(t0);
        assert!(!o.failed(t0), "tek hatada aranmaz");
        assert!(o.failed(t0), "art arda ikinci hatada aranır");
        assert!(!o.failed(t0 + Duration::from_secs(30)), "aralık dolmadan yeniden aranmaz");
        let mut t = t0;
        let mut gaps = Vec::new();
        for _ in 0..8 {
            t += o.search_every();
            assert!(o.failed(t));
            gaps.push(o.search_every().as_secs());
        }
        assert_eq!(gaps, vec![120, 240, 480, 960, 1800, 1800, 1800, 1800], "60 sn → 30 dk üstel");
        o.connected(t);
        assert_eq!(o.search_every(), crate::discover::SEARCH_EVERY);
    }

    #[test]
    fn ty2_long_offline_slows_then_gives_up() {
        let t0 = Instant::now();
        let mut o = Offline::new(t0);
        assert_eq!(o.retry_cap(t0), RETRY_MAX);
        assert_eq!(o.retry_cap(t0 + SLOW_AFTER), RETRY_SLOW);
        assert!(!o.give_up(t0 + GIVE_UP_AFTER - Duration::from_secs(1)));
        assert!(o.give_up(t0 + GIVE_UP_AFTER));
        o.connected(t0 + GIVE_UP_AFTER);
        assert!(!o.give_up(t0 + GIVE_UP_AFTER + Duration::from_secs(60)), "bağlanınca sayaç sıfırlanır");
    }

    #[test]
    fn to1_backoff_resets_only_after_stable_session() {
        let t0 = Instant::now();
        let mut o = Offline::new(t0);
        let cap = o.retry_cap(t0);
        // Bağlanıp hemen kopan sunucu: bekleme üstel artar, el sıkışma başarısı sıfırlamaz.
        let mut waits = Vec::new();
        for i in 0..6 {
            let up = t0 + Duration::from_secs(i * 100);
            o.connected(up);
            o.session_end(up + Duration::from_secs(1), up, true);
            waits.push(o.backoff(RETRY_MIN, cap).as_secs());
        }
        assert_eq!(waits, vec![5, 10, 20, 40, 60, 60], "kısa bağlantılar sıfırlamaz, üst sınır 60 sn");
        // Uzun süren ama durum gelmeyen bağlantı da sıfırlamaz.
        o.session_end(t0 + Duration::from_secs(2000), t0, false);
        assert_eq!(o.backoff(RETRY_MIN, cap).as_secs(), 60);
        // Durum geldi ve `STABLE_AFTER` boyunca ayakta kaldı: başa döner.
        o.session_end(t0 + STABLE_AFTER, t0, true);
        assert_eq!(o.backoff(RETRY_MIN, cap), RETRY_MIN);
    }

    #[test]
    fn to1_locked_bye_backoff_is_exponential_and_capped() {
        let mut o = Offline::new(Instant::now());
        let waits: Vec<u64> = (0..6).map(|_| o.backoff(RETRY_LOCKED, RETRY_SLOW).as_secs()).collect();
        assert_eq!(waits, vec![30, 60, 120, 240, 300, 300], "kilitli: 30 sn → 5 dk");
    }

    #[test]
    fn ty2_unpaired_needs_consecutive_rejections() {
        let mut o = Offline::new(Instant::now());
        assert!(!o.unpaired());
        assert!(!o.unpaired());
        o.paired_again();
        assert!(!o.unpaired() && !o.unpaired(), "dizi bozulunca baştan sayılır");
        assert!(o.unpaired(), "{UNPAIRED_LIMIT}. art arda redde kaldırılmış sayılır");
    }
}
