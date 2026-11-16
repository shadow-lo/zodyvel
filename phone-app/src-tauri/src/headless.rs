//! G3 · Ekransız başlatma (telefon açılışı). Tauri ekranı yokken Kotlin ön plan hizmeti `Headless.nativeStart(dir)`
//! çağırır; burada çekirdek kurulur ve canlı bağlantı başlar (eşleşme varsa, kullanıcı kapatmadıysa).
//!
//! Platform JNI ile Kotlin `app.zodyvel.zkeystore.Headless` nesnesinin durağan yöntemlerine gider. Ekransızken
//! parmak izi ve imza YOKTUR (`sign`/`authenticate` hata döner): onay, cevap vb. uygulama açılınca yapılır. Bildirimden
//! gelen "İzin ver" zaten uygulamayı açar. Günlüğe hiçbir şey yazılmaz.
//!
//! T-D1: aynı JNI platformu `fallback()` ile saklanır; Activity kapanıp süreç ön plan hizmetiyle yaşarsa (`lib.rs`)
//! çağrılar ölü Activity'nin eklentisine değil buraya gider.
use crate::{core_for, set_platform};
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::sync::{Arc, OnceLock};
use zodyvel_phone_core::{KeyInfo, Notice, Platform, Prompt, Reminder};
use zodyvel_phone_proto::pairing::{hex, hex_decode};

const NO_UI: &str = "Uygulamayı aç.";
/// T-O3: tek çağrının yerel referans çerçevesi (en çok 5 dize + sınıf + dönüş).
const FRAME: i32 = 8;

/// T-D1: Activity kapanınca geçilecek JNI platformu (ilk kurulan kalır).
static FALLBACK: OnceLock<Arc<dyn Platform>> = OnceLock::new();

pub(crate) fn fallback() -> Option<Arc<dyn Platform>> {
    FALLBACK.get().cloned()
}

struct Jni {
    vm: JavaVM,
    /// `Headless` sınıfı: yerel iş parçacığından `FindClass` uygulama sınıflarını bulamaz, bu yüzden baştan tutulur.
    cls: GlobalRef,
}

impl Jni {
    /// Durağan yöntemi çağırır; Java istisnası temizlenir ve `None` döner. T-O3: kalıcı bağlı iş parçacığında yerel
    /// referanslar (dizeler, dönen nesne) her çağrıdan sonra çerçeveyle birlikte silinir; tablo büyümez.
    fn call(&self, name: &str, sig: &str, args: &[&str], flag: Option<bool>) -> Option<Option<String>> {
        let mut env = self.vm.attach_current_thread_permanently().ok()?;
        let r = env.with_local_frame(FRAME, |env| -> jni::errors::Result<Option<Option<String>>> { Ok(self.call_in(env, name, sig, args, flag)) });
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }
        r.ok().flatten()
    }

    fn call_in(&self, env: &mut JNIEnv, name: &str, sig: &str, args: &[&str], flag: Option<bool>) -> Option<Option<String>> {
        let mut owned: Vec<JObject> = Vec::with_capacity(args.len());
        for a in args {
            owned.push(env.new_string(a).ok()?.into());
        }
        let mut jargs: Vec<JValue> = owned.iter().map(JValue::Object).collect();
        if let Some(b) = flag {
            jargs.push(JValue::Bool(b as u8));
        }
        let cls: &JClass = <&JClass>::from(self.cls.as_obj());
        let r = env.call_static_method(cls, name, sig, &jargs);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }
        let v = r.ok()?;
        // Dönüş: String (ya da null) veya boolean/void.
        if sig.ends_with("Ljava/lang/String;") {
            let o = v.l().ok()?;
            if o.is_null() {
                return Some(None);
            }
            let js = JString::from(o);
            let s: String = env.get_string(&js).ok()?.into();
            Some(Some(s))
        } else if sig.ends_with('Z') {
            Some(Some(if v.z().ok()? { "1".into() } else { String::new() }))
        } else {
            Some(None)
        }
    }
    fn hex_call(&self, name: &str, data: &[u8]) -> Result<Vec<u8>, String> {
        let out = self.call(name, "(Ljava/lang/String;)Ljava/lang/String;", &[&hex(data)], None).flatten().ok_or("Anahtar işlemi yapılamadı.")?;
        hex_decode(&out).ok_or_else(|| "Anahtar işlemi yapılamadı.".to_string())
    }
}

impl Platform for Jni {
    fn device_secure(&self) -> bool {
        self.call("deviceSecure", "()Z", &[], None).flatten().is_some_and(|s| !s.is_empty())
    }
    fn device_name(&self) -> String {
        self.call("deviceName", "()Ljava/lang/String;", &[], None).flatten().unwrap_or_else(|| "Telefon".into())
    }
    fn create_signing_key(&self, _challenge: &[u8]) -> Result<KeyInfo, String> {
        Err(NO_UI.into())
    }
    fn sign(&self, _msg: &[u8], _prompt: &Prompt) -> Result<Vec<u8>, String> {
        Err(NO_UI.into())
    }
    fn authenticate(&self, _prompt: &Prompt) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn wrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        self.hex_call("wrap", data)
    }
    fn unwrap(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        self.hex_call("unwrap", data)
    }
    fn delete_keys(&self) -> Result<(), String> {
        Err(NO_UI.into())
    }
    fn service(&self, on: bool) {
        let _ = self.call("service", "(Z)V", &[], Some(on));
    }
    fn notice(&self, n: &Notice) {
        let (kind, id, session, text, perm) = match n {
            Notice::Service(t) => ("service", "", "", t.as_str(), false),
            Notice::Approval { id, session, kind } => ("approval", id.as_str(), session.as_str(), "", kind == "permission"),
            Notice::Gone { id } => ("gone", id.as_str(), "", "", false),
            Notice::Finished { session } => ("finished", "", session.as_str(), "", false),
            Notice::Failed { session } => ("failed", "", session.as_str(), "", false),
            Notice::Info(t) => ("info", "", "", t.as_str(), false),
        };
        let _ = self.call("notice", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Z)V", &[kind, id, session, text], Some(perm));
    }
    fn reminders(&self, list: &[Reminder]) {
        if let Ok(j) = serde_json::to_string(list) {
            let _ = self.call("reminders", "(Ljava/lang/String;)V", &[&j], None);
        }
    }
    fn multicast(&self, on: bool) {
        let _ = self.call("multicast", "(Z)V", &[], Some(on));
    }
}

/// T-D1: uygulama ekranla açıldıysa (ekransız başlatma olmadan) yedek JNI platformu burada, Activity'nin iş
/// parçacığında hazırlanır. `Headless.app` boşsa uygulama bağlamı yazılır (Kotlin yöntemleri onu kullanır; Activity
/// değil uygulama bağlamı, sızıntı yok). Hata olursa yedek kurulmaz.
pub(crate) fn adopt(env: &mut JNIEnv, activity: &JObject) {
    if FALLBACK.get().is_some() {
        return;
    }
    let r = env.with_local_frame(FRAME, |env| -> jni::errors::Result<Jni> {
        let ctx = env.call_method(activity, "getApplicationContext", "()Landroid/content/Context;", &[])?.l()?;
        // Ana iş parçacığında `FindClass` sistem sınıf yükleyicisini kullanabilir; uygulamanınki açıkça alınır.
        let loader = env.call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
        let name = env.new_string("app.zodyvel.zkeystore.Headless")?;
        let cls = JClass::from(env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?);
        let field = env.get_static_field_id(&cls, "app", "Landroid/content/Context;")?;
        if env.get_static_field(&cls, "app", "Landroid/content/Context;")?.l()?.is_null() {
            env.set_static_field(&cls, field, JValue::Object(&ctx))?;
        }
        Ok(Jni { vm: env.get_java_vm()?, cls: env.new_global_ref(&cls)? })
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    if let Ok(j) = r {
        let _ = FALLBACK.set(Arc::new(j));
    }
}

/// Kotlin: `Headless.nativeStart(dir)`. Hızlı döner; bağlantı kendi iş parçacığında kurulur.
#[no_mangle]
pub extern "system" fn Java_app_zodyvel_zkeystore_Headless_nativeStart<'l>(mut env: JNIEnv<'l>, class: JClass<'l>, dir: JString<'l>) {
    let Ok(dir) = env.get_string(&dir).map(String::from) else {
        return;
    };
    let (Ok(vm), Ok(cls)) = (env.get_java_vm(), env.new_global_ref(&class)) else {
        return;
    };
    let p: Arc<dyn Platform> = FALLBACK.get_or_init(|| Arc::new(Jni { vm, cls })).clone();
    // Uygulama zaten açıksa Tauri platformu kalır.
    set_platform(p, false);
    let core = core_for(std::path::PathBuf::from(dir));
    std::thread::spawn(move || {
        let _ = core.live_start();
    });
}
