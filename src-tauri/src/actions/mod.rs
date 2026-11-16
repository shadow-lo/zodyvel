//! Eylem kaydı ve genel onay merkezi.
//!
//! Arayüzün çağırdığı her komut `spec::ACTIONS`'ta adı, argüman şeması ve risk düzeyiyle durur. Zody
//! (ileride kural ayrıştırıcı, ses ve model) bir şey yapmak istediğinde komutu doğrudan değil
//! `actions_request` ile ister: argümanlar denetlenir, `Read`/`Safe` eylem hemen çalışır, `Confirm`
//! eylem bekleyen istek olur ve `actions://pending` ile arayüze gider; kullanıcı `actions_decide` ile
//! onaylar ya da reddeder. `Internal` eylemler (ajan onayı dahil) Zody'ye hiç açılmaz.
//!
//! Arayüzün kendi düğmeleri eskisi gibi komutları doğrudan çağırır; tıklayan zaten kullanıcıdır.
//! Ajanların izin istekleri kendi onay kartında kalır (`agents::state`).

pub mod access;
pub mod audit;
pub mod ops;
pub mod rules;
pub mod spec;

use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::agents::{AgentHub, now_ms};
use crate::media::MediaHub;
use crate::shelf::ShelfHub;
use crate::today::{Op, TodayHub};
use crate::{agents, autostart, controls, media, power, settings, shelf, sound, sources, system, today};
use spec::{Action, Call, Pending, Queue, Risk};

#[derive(Default)]
pub struct Center {
    queue: Mutex<Queue>,
}

impl Center {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|p| p.into_inner())
    }
}

pub fn init(app: &AppHandle) {
    app.manage(Center::default());
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Outcome {
    /// Çalıştı; `result` komutun döndürdüğü.
    Done { result: Value },
    /// Onay bekliyor.
    Pending { request: Pending },
    /// Kullanıcı reddetti.
    Denied,
}

/// Süresi dolan (2 dk) ve kuyruk dolunca düşen kartları denetim kaydına "zaman aşımı" diye yazar.
fn suresi_dolanlari_kaydet(app: &AppHandle) {
    let dolan = app.state::<Center>().lock().take_expired(now_ms());
    let seviye = access::erisim_seviyesi(app);
    for p in dolan {
        kaydet(app, seviye, &p.istek, p.action, &p.summary, "zaman aşımı", now_ms(), "iptal");
    }
}

fn publish(app: &AppHandle) {
    suresi_dolanlari_kaydet(app);
    let list = app.state::<Center>().lock().list(now_ms());
    let _ = app.emit("actions://pending", &list);
}

/// Yeni kart için tek seferlik süre bekçisi: kimse cevap vermese de süre dolunca kart düşer, arayüze ve denetim
/// kaydına yansır. Kart yokken hiçbir zamanlayıcı çalışmaz; kart başına bir iş parçacığı, dolunca biter.
fn sure_bekcisi(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new().name("zody-kart-suresi".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(spec::PENDING_TTL_MS + 500));
        publish(&app);
    });
}

/// Bekleyen tüm kartları siler (seviye düştü ya da acil durdurma).
pub fn clear_pending(app: &AppHandle) {
    app.state::<Center>().lock().clear();
    publish(app);
}

fn audit_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    settings::data_dir(app).map(|d| d.join("denetim"))
}

/// Denetim satırı yazar; yazılamazsa eylemi engellemez (kayıt yardımcıdır, kapı değil).
#[allow(clippy::too_many_arguments)]
fn kaydet(app: &AppHandle, seviye: spec::Erisim, istek: &str, eylem: &str, ozet: &str, onay: &str, onay_ms: u64, sonuc: &str) {
    let Some(dir) = audit_dir(app) else { return };
    // Yazılan metin ve mail gövdesi parola içerebilir: kayda yalnız eylemin adı gider, içerik yazılmaz.
    let ozet = if matches!(eylem, "window.type" | "mail.draft") { "içerik kayda yazılmaz" } else { ozet };
    let (y, mo, d, h, mi, s, _) = crate::win::local_time();
    let kayit = audit::Kayit {
        t: now_ms(),
        zaman: format!("{h:02}:{mi:02}:{s:02}"),
        istek: istek.to_owned(),
        seviye: seviye.as_str().to_owned(),
        eylem: eylem.to_owned(),
        ozet: ozet.to_owned(),
        onay: onay.to_owned(),
        onay_ms,
        sonuc: sonuc.to_owned(),
    };
    let _ = audit::yaz(&dir, &format!("{y:04}-{mo:02}-{d:02}"), &kayit);
}

/// G4: telefondan gelen doğrulanmış (parmak izi + Keystore imzalı) eylemin kalıcı denetim satırı (Plan 3 · 2.5/8).
/// İçerik yazılmaz: yalnız cihaz (ad · kimlik), eylemin adı ve sonuç. Reddedilen eylem de yazılır.
pub fn kaydet_telefon(app: &AppHandle, cihaz: &str, eylem: &str, sonuc: &str) {
    let Some(dir) = audit_dir(app) else { return };
    let (y, mo, d, h, mi, s, _) = crate::win::local_time();
    let t = now_ms();
    let kayit = audit::Kayit {
        t,
        zaman: format!("{h:02}:{mi:02}:{s:02}"),
        istek: "telefondan".into(),
        seviye: "telefon".into(),
        eylem: "phone.action".into(),
        ozet: format!("{cihaz} telefondan {eylem}"),
        onay: "telefon (parmak izi)".into(),
        onay_ms: t,
        sonuc: sonuc.to_owned(),
    };
    let _ = audit::yaz(&dir, &format!("{y:04}-{mo:02}-{d:02}"), &kayit);
}

/// Seviye değişimi ve acil durdurma gibi tek satırlık kayıtlar.
pub fn kaydet_not(app: &AppHandle, seviye: spec::Erisim, eylem: &str, ozet: &str, onay: &str, sonuc: &str) {
    kaydet(app, seviye, "", eylem, ozet, onay, now_ms(), sonuc);
}

fn sonuc_metni(r: &Result<Value, String>) -> String {
    match r {
        Ok(_) => "ok".into(),
        Err(e) => format!("hata: {e}"),
    }
}

/// Kaydın tamamı (ayarlar sayfası, kural ayrıştırıcı ve testler için).
#[tauri::command]
pub fn actions_list() -> Vec<Action> {
    spec::ACTIONS.to_vec()
}

#[tauri::command]
pub fn actions_pending(app: AppHandle) -> Vec<Pending> {
    app.state::<Center>().lock().list(now_ms())
}

/// Son denetim kayıtları (en yeni önce). Yalnız arayüz.
#[tauri::command]
pub fn audit_recent(app: AppHandle, limit: Option<usize>) -> Vec<audit::Kayit> {
    audit_dir(&app).map(|d| audit::son(&d, limit.unwrap_or(50).min(200))).unwrap_or_default()
}

/// Denetim kaydını siler; silinen dosya sayısı döner. Yalnız arayüz.
#[tauri::command]
pub fn audit_clear(app: AppHandle) -> usize {
    audit_dir(&app).map(|d| audit::temizle(&d)).unwrap_or(0)
}

/// Ortak yol: onay gerekiyorsa sıraya, değilse çalıştır. Hem arayüz/sohbet isteği hem araç çağrısı buradan geçer.
async fn dispatch(app: &AppHandle, action: &'static Action, args: Map<String, Value>, source: &str, istek: &str, zorunlu_kart: bool) -> Result<Outcome, String> {
    let seviye = access::erisim_seviyesi(app);
    let call = spec::plan(action, args.clone())?;
    if zorunlu_kart || action.risk == Risk::Confirm {
        let request = app.state::<Center>().lock().push(action, args, source, istek, now_ms());
        publish(app);
        sure_bekcisi(app);
        kaydet(app, seviye, istek, action.name, &request.summary, "bekliyor", 0, "bekliyor");
        return Ok(Outcome::Pending { request });
    }
    let ozet = spec::describe(action, &args);
    let sonuc = run(app, call).await;
    kaydet(app, seviye, istek, action.name, &ozet, "gerekmedi", 0, &sonuc_metni(&sonuc));
    sonuc.map(|result| Outcome::Done { result })
}

/// Arayüzün/sohbetin eylem isteği (kullanıcının yazdığı komutlar). Onay gerekmiyorsa hemen çalışır, gerekiyorsa
/// sıraya girer. Zody'nin araç çağrıları buradan değil `arac_calistir`'dan geçer (seviye kapısı orada).
#[tauri::command]
pub async fn actions_request(app: AppHandle, action: String, args: Option<Value>, source: Option<String>) -> Result<Outcome, String> {
    let action = spec::find(&action).ok_or("Bilinmeyen eylem.")?;
    let args = spec::validate(action, &args.unwrap_or(Value::Null))?;
    let source = source.unwrap_or_else(|| "zody".into());
    dispatch(&app, action, args, &source, &source, false).await
}

/// Zody'nin araç çağrısı (araç sunucusundan). `rules::hazirla` kapısından geçmiş olmalıdır; burada seviye
/// bir kez daha denetlenir. Dış içerik bayrağı varsa Güvenli eylemler de kart ister.
pub async fn arac_calistir(app: &AppHandle, tur: &rules::Tur, action: &'static Action, args: Map<String, Value>) -> Result<Outcome, String> {
    if !rules::izinli(access::erisim_seviyesi(app), action) {
        return Err(rules::Red::Izinsiz.ileti());
    }
    dispatch(app, action, args, "zody", &tur.istek, rules::kart_gerekir(action, tur.dis_icerik)).await
}

/// Bekleyen isteğe kullanıcının kararı. Yalnız arayüzün onay kartı çağırır; Zody ve ses çağırmaz.
#[tauri::command]
pub async fn actions_decide(app: AppHandle, id: u64, allow: bool) -> Result<Outcome, String> {
    let pending = app.state::<Center>().lock().take(id, now_ms());
    publish(&app);
    let pending = pending.ok_or("Bu istek artık geçerli değil.")?;
    let seviye = access::erisim_seviyesi(&app);
    let kim_ne_zaman = now_ms();
    if !allow {
        kaydet(&app, seviye, &pending.istek, pending.action, &pending.summary, "reddedildi", kim_ne_zaman, "reddedildi");
        return Ok(Outcome::Denied);
    }
    let action = spec::find(pending.action).ok_or("Bilinmeyen eylem.")?;
    let sonuc = run(&app, spec::plan(action, pending.args)?).await;
    kaydet(&app, seviye, &pending.istek, pending.action, &pending.summary, "kullanıcı", kim_ne_zaman, &sonuc_metni(&sonuc));
    sonuc.map(|result| Outcome::Done { result })
}

fn text(args: &Map<String, Value>, name: &str) -> String {
    args.get(name).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn int(args: &Map<String, Value>, name: &str) -> Option<u64> {
    args.get(name).and_then(Value::as_u64)
}

fn flag(args: &Map<String, Value>, name: &str) -> Option<bool> {
    args.get(name).and_then(Value::as_bool)
}

fn json<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Denetlenmiş çağrıyı arkasındaki komutla çalıştırır; komutun kendi denetimleri (özellik kapalı vb.)
/// aynen geçerli. Argümanlar `spec::validate`'ten geçtiği için burada tür ve sınır sorunu yoktur.
async fn run(app: &AppHandle, call: Call) -> Result<Value, String> {
    let (command, a) = match call {
        Call::Today(op) => {
            let op: Op = serde_json::from_value(op).map_err(|e| e.to_string())?;
            return json(today::today_apply(app.state::<Arc<TodayHub>>(), op)?);
        }
        Call::Direct { command, args } => (command, args),
    };
    let agents = || app.state::<Arc<AgentHub>>();
    match command {
        "music_snapshot" => json(media::music_snapshot(app.state::<Arc<MediaHub>>())),
        "music_control" => json(media::music_control(app.state::<Arc<MediaHub>>(), text(&a, "action"))?),
        "music_seek" => json(media::music_seek(app.state::<Arc<MediaHub>>(), int(&a, "positionMs").unwrap_or(0))?),
        "controls_read" => json(controls::controls_read().await?),
        "controls_volume" => {
            let level = int(&a, "level").map(|n| n.min(100) as u8);
            json(controls::controls_volume(level, flag(&a, "muted")).await?)
        }
        "controls_radio" => json(controls::controls_radio(text(&a, "kind"), flag(&a, "on").unwrap_or(false)).await?),
        "controls_dark" => json(controls::controls_dark(flag(&a, "on").unwrap_or(false)).await?),
        "controls_outputs" => json(controls::controls_outputs().await?),
        "controls_output" => json(controls::controls_output(text(&a, "id")).await?),
        "controls_open" => json(controls::controls_open(text(&a, "page"))?),
        "open_task_manager" => json(system::open_task_manager()?),
        "apps_launch" => json(crate::apps::apps_launch(app.clone(), text(&a, "id"))?),
        "power_status" => json(power::power_status()),
        "sound_quiet" => json(sound::sound_quiet()),
        "today_snapshot" => json(today::today_snapshot(app.state::<Arc<TodayHub>>())),
        "shelf_snapshot" => json(shelf::shelf_snapshot(app.state::<Arc<ShelfHub>>())),
        "shelf_remove" => json(shelf::shelf_remove(app.state::<Arc<ShelfHub>>(), int(&a, "id"))),
        "shelf_reveal" => json(shelf::shelf_reveal(app.state::<Arc<ShelfHub>>(), int(&a, "id").unwrap_or(0))?),
        "agents_snapshot" => json(agents::agents_snapshot(agents())),
        "agents_focus" => json(agents::agents_focus(agents(), text(&a, "session"))?),
        "agents_open" => json(agents::agents_open(agents(), text(&a, "session"), text(&a, "target"))?),
        "agents_note" => json(agents::agents_note(app.clone(), agents(), text(&a, "session"), text(&a, "text"), flag(&a, "now").unwrap_or(false))?),
        "agents_note_remove" => json(agents::agents_note_remove(agents(), text(&a, "session"), int(&a, "id").unwrap_or(0))?),
        "agents_task" => json(agents::agents_task(app.clone(), agents(), text(&a, "session"), text(&a, "text"))?),
        "sources_snapshot" => json(sources::sources_snapshot()),
        "source_action" => json(sources::source_action(text(&a, "id"), text(&a, "itemId"), text(&a, "action")).await?),
        "source_preview" => json(sources::source_preview(text(&a, "id"), text(&a, "itemId")).await?),
        "source_refresh" => json(sources::source_refresh(app.clone(), text(&a, "id")).await?),
        "get_settings" => json(settings::get_settings(app.state())),
        "open_settings" => {
            settings::open_settings(app.clone()).await;
            Ok(Value::Null)
        }
        "open_data_folder" => json(settings::open_data_folder(app.clone())?),
        "autostart_get" => json(autostart::autostart_get()),
        "autostart_set" => json(autostart::autostart_set(app.clone(), flag(&a, "enabled").unwrap_or(false))?),
        // Faz 13b
        "web_open" => json(ops::web_open(text(&a, "url"))?),
        "web_search" => json(ops::web_search(text(&a, "query"), a.get("engine").and_then(Value::as_str).map(str::to_owned))?),
        "claude_chat" => json(ops::claude_chat(text(&a, "text"))?),
        "app_open" => json(ops::app_open(text(&a, "name"))?),
        "app_close" => json(ops::app_close(text(&a, "name"))?),
        "files_copy" => json(ops::files_copy(app.clone(), text(&a, "source"), text(&a, "targetDir"))?),
        "files_move" => json(ops::files_move(app.clone(), text(&a, "source"), text(&a, "targetDir"))?),
        "window_type" => json(ops::window_type(text(&a, "text"))?),
        "mail_draft" => json(ops::mail_draft(a.get("to").and_then(Value::as_str).map(str::to_owned), text(&a, "subject"), text(&a, "body"))?),
        _ => Err("Bu eylem Zody'ye kapalı.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Yalnız Windows'ta derlenir (Tauri): `today.*` eylemlerinin planı gerçek `today::Op`'a çevrilmeli.
    #[test]
    fn today_plans_parse_as_ops() {
        for action in spec::ACTIONS.iter().filter(|a| a.command == "today_apply") {
            let args: Map<String, Value> = action
                .args
                .iter()
                .map(|arg| {
                    let value = match arg.name {
                        "date" => Value::from("2026-10-21"),
                        "time" => Value::from("10:00"),
                        "id" | "minutes" => Value::from(5),
                        _ => Value::from("deneme"),
                    };
                    (arg.name.to_owned(), value)
                })
                .collect();
            let Call::Today(op) = spec::plan(action, args).unwrap() else { panic!("{}", action.name) };
            serde_json::from_value::<Op>(op).unwrap_or_else(|e| panic!("{}: {e}", action.name));
        }
    }
}
