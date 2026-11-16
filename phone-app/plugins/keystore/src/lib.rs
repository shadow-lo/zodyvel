//! Android Keystore + BiometricPrompt köprüsü. Komutlar yalnız Rust'tan (`Keystore`) çağrılır;
//! webview bu eklentiye erişemez (izin dosyası boş), böylece sayfa kodu imza ya da anahtar isteyemez.
use serde::{Deserialize, Serialize};
use tauri::{plugin::{Builder, TauriPlugin}, Manager, Runtime};

#[cfg(mobile)]
mod mobile;

#[derive(Debug, Deserialize)]
pub struct DeviceInfo {
    pub secure: bool,
    pub name: String,
}
#[derive(Debug, Deserialize)]
pub struct KeyResult {
    pub apub: String,
    #[serde(default)]
    pub attestation: Vec<String>,
}
#[derive(Serialize)]
struct Challenge<'a> {
    challenge: &'a str,
}
#[derive(Serialize)]
struct SignArgs<'a> {
    msg: &'a str,
    title: &'a str,
    subtitle: &'a str,
}
#[derive(Serialize)]
struct PromptArgs<'a> {
    title: &'a str,
    subtitle: &'a str,
}
#[derive(Serialize)]
struct DataArgs<'a> {
    data: &'a str,
}
#[derive(Deserialize)]
struct SigOut {
    sig: String,
}
#[derive(Deserialize)]
struct DataOut {
    data: String,
}
#[derive(Deserialize)]
struct Empty {}

pub struct Keystore<R: Runtime>(#[cfg(mobile)] tauri::plugin::PluginHandle<R>, #[cfg(desktop)] std::marker::PhantomData<fn() -> R>);

#[cfg(mobile)]
impl<R: Runtime> Keystore<R> {
    fn call<A: Serialize, T: serde::de::DeserializeOwned>(&self, cmd: &str, a: A) -> Result<T, String> {
        self.0.run_mobile_plugin(cmd, a).map_err(|e| e.to_string())
    }
}
#[cfg(desktop)]
impl<R: Runtime> Keystore<R> {
    fn call<A: Serialize, T: serde::de::DeserializeOwned>(&self, _cmd: &str, _a: A) -> Result<T, String> {
        Err("Bu bir telefon uygulaması; Keystore yalnız Android'de var.".into())
    }
}

impl<R: Runtime> Keystore<R> {
    pub fn device_info(&self) -> Result<DeviceInfo, String> {
        self.call("device_info", Empty2 {})
    }
    pub fn create_signing_key(&self, challenge_hex: &str) -> Result<KeyResult, String> {
        self.call("create_signing_key", Challenge { challenge: challenge_hex })
    }
    /// Bloklar: BiometricPrompt kapanana dek bekler. Webview/ana iş parçacığında çağırma.
    pub fn sign(&self, msg_hex: &str, title: &str, subtitle: &str) -> Result<String, String> {
        self.call::<_, SigOut>("sign", SignArgs { msg: msg_hex, title, subtitle }).map(|o| o.sig)
    }
    pub fn authenticate(&self, title: &str, subtitle: &str) -> Result<(), String> {
        self.call::<_, Empty>("authenticate", PromptArgs { title, subtitle }).map(|_| ())
    }
    pub fn wrap(&self, data_hex: &str) -> Result<String, String> {
        self.call::<_, DataOut>("wrap", DataArgs { data: data_hex }).map(|o| o.data)
    }
    pub fn unwrap(&self, data_hex: &str) -> Result<String, String> {
        self.call::<_, DataOut>("unwrap", DataArgs { data: data_hex }).map(|o| o.data)
    }
    pub fn delete_keys(&self) -> Result<(), String> {
        self.call::<_, Empty>("delete_keys", Empty2 {}).map(|_| ())
    }

    /// Kotlin → Rust olay kanalı (bildirimden gelen İzin ver/Reddet). Olay: `{"k":"decide","id":..,"allow":..}`.
    /// Kanal aç/kapa yapılamaz; uygulama başlarken bir kez kurulur. İşleyici hızlı dönmeli.
    pub fn set_event_handler<F: Fn(Event) + Send + Sync + 'static>(&self, f: F) -> Result<(), String> {
        let ch = tauri::ipc::Channel::<serde_json::Value>::new(move |body| {
            let v: serde_json::Value = match body {
                tauri::ipc::InvokeResponseBody::Json(s) => serde_json::from_str(&s).unwrap_or_default(),
                tauri::ipc::InvokeResponseBody::Raw(_) => return Ok(()),
            };
            if v.get("k").and_then(|k| k.as_str()) == Some("decide") {
                if let (Some(id), Some(allow)) = (v.get("id").and_then(|x| x.as_str()), v.get("allow").and_then(|x| x.as_bool())) {
                    f(Event::Decide { id: id.to_owned(), allow });
                }
            }
            Ok(())
        });
        self.call::<_, Empty>("set_events", EventsArgs { channel: ch }).map(|_| ())
    }
    pub fn service_start(&self) -> Result<(), String> {
        self.call::<_, Empty>("service_start", Empty2 {}).map(|_| ())
    }
    pub fn service_stop(&self) -> Result<(), String> {
        self.call::<_, Empty>("service_stop", Empty2 {}).map(|_| ())
    }
    /// Bildirim: kind = service | approval | gone | finished | failed | info. İçerik taşımaz (oturum adı, kısa metin).
    pub fn post_notice(&self, kind: &str, id: &str, session: &str, text: &str, perm: bool) -> Result<(), String> {
        self.call::<_, Empty>("post_notice", NoticeArgs { kind, id, session, text, perm }).map(|_| ())
    }
    /// G3: Ajanda hatırlatıcılarını baştan kurar. `json`: `[{key,date,time,lead,title}]`; boş dizi = hepsini sil.
    /// Kotlin listeyi Keystore ile şifreli saklar ve AlarmManager'a kurar.
    pub fn reminders_sync(&self, json: &str) -> Result<(), String> {
        self.call::<_, Empty>("reminders_sync", JsonArgs { json }).map(|_| ())
    }
    /// G3: mDNS araması sürerken Wi-Fi çok yöne yayın kilidi.
    pub fn multicast(&self, on: bool) -> Result<(), String> {
        self.call::<_, Empty>("multicast", OnArgs { on }).map(|_| ())
    }
    /// G3: çekirdeğin veri klasörü; açılış alıcısı (ekransız başlatma) bunu kullanır.
    pub fn set_core_dir(&self, dir: &str) -> Result<(), String> {
        self.call::<_, Empty>("set_core_dir", DirArgs { dir }).map(|_| ())
    }
    /// `true`: pil optimizasyonundan zaten muaf. `false`: sistem izin penceresi açıldı.
    pub fn battery_exempt(&self) -> Result<bool, String> {
        self.call::<_, Exempt>("battery_exempt", Empty2 {}).map(|e| e.exempt)
    }
}

/// Android bildirimlerinden gelen olaylar.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Decide { id: String, allow: bool },
}
#[derive(Serialize)]
struct EventsArgs {
    channel: tauri::ipc::Channel<serde_json::Value>,
}
#[derive(Serialize)]
struct NoticeArgs<'a> {
    kind: &'a str,
    id: &'a str,
    session: &'a str,
    text: &'a str,
    perm: bool,
}
#[derive(Deserialize)]
struct Exempt {
    exempt: bool,
}
#[derive(Serialize)]
struct Empty2 {}
#[derive(Serialize)]
struct JsonArgs<'a> {
    json: &'a str,
}
#[derive(Serialize)]
struct OnArgs {
    on: bool,
}
#[derive(Serialize)]
struct DirArgs<'a> {
    dir: &'a str,
}

pub trait KeystoreExt<R: Runtime> {
    fn keystore(&self) -> &Keystore<R>;
}
impl<R: Runtime, T: Manager<R>> KeystoreExt<R> for T {
    fn keystore(&self) -> &Keystore<R> {
        self.state::<Keystore<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("zkeystore")
        .setup(|app, api| {
            #[cfg(mobile)]
            let ks = mobile::init(app, api)?;
            #[cfg(desktop)]
            let ks = {
                let _ = api;
                Keystore::<R>(std::marker::PhantomData)
            };
            app.manage(ks);
            Ok(())
        })
        .build()
}
