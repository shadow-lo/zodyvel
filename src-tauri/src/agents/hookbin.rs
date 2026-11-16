//! `zodyvel-hook.exe`'yi sabit bir yere kopyalar: `%USERPROFILE%\.zodyvel\bin\`.
//! Ayar dosyalarındaki yol, uygulama güncellense veya başka klasöre taşınsa da değişmez.
//!
//! Neden AppData değil: MSIX paketli bir uygulamanın (ör. Claude masaüstü) içinden başlatılan
//! süreçlerin AppData yazmaları paketin özel klasörüne yönlendirilir. O yol paket dışındaki
//! araçlar (ör. Codex) için yoktur. Kullanıcı klasörünün kökü yönlendirilmez; `.codex` ve
//! `.claude` da oradadır.

use std::fs;
use std::path::PathBuf;

const FILE: &str = "zodyvel-hook.exe";

pub fn installed_path() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join(".zodyvel").join("bin").join(FILE)
}

fn bundled_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join(FILE);
    path.is_file().then_some(path)
}

pub fn ready() -> bool {
    installed_path().is_file()
}

/// Paketteki köprü farklıysa sabit konuma kopyalar. Çalışan bir köprü dosyayı kilitliyorsa
/// eski dosya yeniden adlandırılır (Windows çalışan exe'nin adının değişmesine izin verir).
pub fn ensure() {
    let Some(source) = bundled_path() else { return };
    let target = installed_path();
    let Ok(new) = fs::read(&source) else { return };
    if fs::read(&target).is_ok_and(|old| old == new) {
        return;
    }
    if let Some(dir) = target.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = target.with_extension("exe.new");
    if fs::write(&tmp, &new).is_err() {
        return;
    }
    if fs::rename(&tmp, &target).is_err() {
        let old = target.with_extension("exe.old");
        let _ = fs::remove_file(&old);
        if fs::rename(&target, &old).is_ok() {
            let _ = fs::rename(&tmp, &target);
        }
    }
}
