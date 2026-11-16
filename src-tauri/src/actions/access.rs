//! Erişim seviyesi durumu (Faz 13). Kalıcı seviye `settings.brain.access`'te durur ve yalnız arayüzün çağırdığı
//! `access_set` ile değişir (Internal: Zody, araç sunucusu ve ses çağıramaz; `update_settings` de onu değiştiremez).
//! Acil durdurmadan sonra oturum boyunca seviye Kapalı sayılır; kalıcı ayar dokunulmadan kalır ve kullanıcı
//! seviyeyi yeniden seçince ya da uygulama yeniden açılınca geri gelir.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::spec::Erisim;
use crate::settings;

/// Acil durdurma bu oturumda basıldı mı (diske yazılmaz).
static DURDU: AtomicBool = AtomicBool::new(false);

/// Ayardaki seviye ile oturum geçersiz kılmasından geçerli seviye (saf).
pub fn efektif(ayar: Erisim, durdu: bool) -> Erisim {
    if durdu { Erisim::Kapali } else { ayar }
}

pub fn kayitli(app: &AppHandle) -> Erisim {
    Erisim::parse(&settings::current(app).brain.access).unwrap_or_default()
}

/// Zody'nin o an sahip olduğu seviye: gerçek ayar + acil durdurma.
pub fn erisim_seviyesi(app: &AppHandle) -> Erisim {
    efektif(kayitli(app), DURDU.load(Ordering::SeqCst))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Clone)]
pub struct Durum {
    /// Ayarlardaki seviye ("closed" | "low" | "mid").
    pub level: &'static str,
    /// Şu an geçerli seviye (acil durdurma sonrası "closed").
    pub effective: &'static str,
    pub stopped: bool,
}

fn durum(app: &AppHandle) -> Durum {
    let stopped = DURDU.load(Ordering::SeqCst);
    Durum { level: kayitli(app).as_str(), effective: efektif(kayitli(app), stopped).as_str(), stopped }
}

fn duyur(app: &AppHandle) {
    let _ = app.emit("access://changed", durum(app));
}

#[tauri::command]
pub fn access_get(app: AppHandle) -> Durum {
    durum(&app)
}

/// Seviyeyi değiştirir. Yalnız arayüz çağırır; kaydı `spec`'te Internal. Düşürmek bekleyen kartları siler,
/// yeniden seçmek acil durdurma geçersiz kılmasını kaldırır.
#[tauri::command]
pub fn access_set(app: AppHandle, level: String) -> Result<Durum, String> {
    let yeni = Erisim::parse(&level).ok_or("Bilinmeyen erişim seviyesi.")?;
    let eski = efektif(kayitli(&app), DURDU.load(Ordering::SeqCst));
    settings::modify(&app, |s| s.brain.access = yeni.as_str().to_owned())?;
    DURDU.store(false, Ordering::SeqCst);
    if yeni < eski {
        super::clear_pending(&app);
    }
    super::kaydet_not(&app, yeni, "erişim seviyesi", &format!("Seviye: {}", yeni.as_str()), "kullanıcı", "ok");
    duyur(&app);
    Ok(durum(&app))
}

/// Acil durdurma: bekleyen kartları siler, çalışan beyin isteğini iptal eder, seviyeyi bu oturum için Kapalı yapar.
pub fn emergency_stop(app: &AppHandle) {
    DURDU.store(true, Ordering::SeqCst);
    super::clear_pending(app);
    crate::brain::cancel(app);
    super::kaydet_not(app, Erisim::Kapali, "acil durdurma", "Tüm eylemler iptal edildi", "kullanıcı", "iptal");
    duyur(app);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durdurma_kalici_ayari_degistirmeden_kapatir() {
        for ayar in [Erisim::Kapali, Erisim::Dusuk, Erisim::Orta] {
            assert_eq!(efektif(ayar, false), ayar);
            assert_eq!(efektif(ayar, true), Erisim::Kapali);
        }
    }

}
