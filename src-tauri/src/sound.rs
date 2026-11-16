//! Bildirim sesleri: onay/soru, bitti, hata, zamanlayıcı ve açılış selamı.
//!
//! Üç kaynak: Zody seti (kendi sentezimiz, `scripts/ses/uret.mjs`; ikili dosyaya gömülü), kullanıcının
//! Windows ses düzeni ya da kullanıcının yüklediği WAV (ayar klasöründe `sounds/<olay>.wav`).
//! Her ses buradan geçer: ana anahtar, olayın kendi anahtarı ve Windows "Rahatsız etme" burada denetlenir.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::settings::{self, Settings};
use crate::{debuglog, island, win};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cue {
    Attention,
    Done,
    Error,
    Timer,
    Hello,
}

pub const CUES: [Cue; 5] = [Cue::Attention, Cue::Done, Cue::Error, Cue::Timer, Cue::Hello];

/// Yüklenen dosyanın üst sınırı: bildirim sesi kısa olmalı; PlaySound dosyayı bütün okur.
const MAX_UPLOAD: usize = 2 * 1024 * 1024;

impl Cue {
    pub fn name(self) -> &'static str {
        match self {
            Cue::Attention => "attention",
            Cue::Done => "done",
            Cue::Error => "error",
            Cue::Timer => "timer",
            Cue::Hello => "hello",
        }
    }

    pub fn parse(text: &str) -> Option<Cue> {
        CUES.into_iter().find(|c| c.name() == text)
    }

    fn zody(self) -> &'static [u8] {
        match self {
            Cue::Attention => include_bytes!("../sounds/attention.wav"),
            Cue::Done => include_bytes!("../sounds/done.wav"),
            Cue::Error => include_bytes!("../sounds/error.wav"),
            Cue::Timer => include_bytes!("../sounds/timer.wav"),
            Cue::Hello => include_bytes!("../sounds/hello.wav"),
        }
    }

    /// Windows ses düzenindeki karşılığı (Ses denetim masası → Sesler'de kullanıcı değiştirebilir).
    fn windows_alias(self) -> &'static str {
        match self {
            Cue::Attention => "Notification.Default",
            Cue::Done => "SystemAsterisk",
            Cue::Error => "SystemHand",
            Cue::Timer => "Notification.Reminder",
            Cue::Hello => "WindowsLogon",
        }
    }
}

fn custom_path(app: &AppHandle, cue: Cue) -> Option<PathBuf> {
    settings::data_dir(app).map(|dir| dir.join("sounds").join(format!("{}.wav", cue.name())))
}

/// Bir olayın sesi. Kapalıysa ya da Rahatsız etme açıksa hiçbir şey çalmaz.
pub fn play(app: &AppHandle, cue: Cue) {
    let current = settings::current(app);
    if !current.island.sound || !current.sounds.enabled(cue) {
        return;
    }
    // Açık mod "Zody sessiz" diyorsa (Modlar, Faz 5a).
    if crate::today::quiet(app) {
        debuglog::line(format!("ses: {} susturuldu (mod)", cue.name()));
        return;
    }
    if current.sounds.respect_quiet && win::do_not_disturb() {
        debuglog::line(format!("ses: {} susturuldu (Rahatsız etme)", cue.name()));
        return;
    }
    emit(app, &current, cue);
}

/// Seçili kaynaktan çalar; anahtarlara ve Rahatsız etme'ye bakmaz (ayarlardaki "Dinle" de bunu kullanır).
fn emit(app: &AppHandle, current: &Settings, cue: Cue) {
    if current.sounds.custom.contains_key(cue.name())
        && let Some(path) = custom_path(app, cue).filter(|p| p.is_file())
    {
        win::play_file(&path);
        return;
    }
    match current.sounds.pack.as_str() {
        "windows" => win::play_alias(cue.windows_alias()),
        _ => win::play_wav(cue.zody()),
    }
}

/// Kullanıcının yüklediği dosya ayardan kaldırılınca diskten de silinir.
pub fn remove_custom(app: &AppHandle, cue: Cue) {
    if let Some(path) = custom_path(app, cue) {
        let _ = fs::remove_file(path);
    }
}

/// PlaySound yalnız WAV çalar: RIFF/WAVE başlığı ve bir `fmt ` parçası aranır.
fn looks_like_wav(bytes: &[u8]) -> bool {
    bytes.len() > 44 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" && bytes.windows(4).take(256).any(|w| w == b"fmt ")
}

fn cue_arg(kind: &str) -> Result<Cue, String> {
    Cue::parse(kind).ok_or_else(|| "Geçersiz ses.".to_string())
}

/// Adadan: yeni onay/soru, bir ajanın bitmesi ya da hata vermesi.
#[tauri::command]
pub fn sound_play(app: AppHandle, kind: String) -> Result<(), String> {
    play(&app, cue_arg(&kind)?);
    Ok(())
}

#[tauri::command]
pub fn sound_preview(app: AppHandle, kind: String) -> Result<(), String> {
    let cue = cue_arg(&kind)?;
    emit(&app, &settings::current(&app), cue);
    Ok(())
}

/// Ayarlar sayfasındaki uyarı için: Rahatsız etme şu an açık mı.
#[tauri::command]
pub fn sound_quiet() -> bool {
    win::do_not_disturb()
}

/// Kullanıcının seçtiği WAV dosyası: doğrulanır, ayar klasörüne kopyalanır, ayar güncellenir.
#[tauri::command]
pub fn sound_upload(app: AppHandle, kind: String, name: String, bytes: Vec<u8>) -> Result<Settings, String> {
    let cue = cue_arg(&kind)?;
    if bytes.len() > MAX_UPLOAD {
        return Err("Dosya çok büyük (en çok 2 MB). Kısa bir ses seç.".into());
    }
    if !looks_like_wav(&bytes) {
        return Err("Yalnız WAV dosyası çalınabilir. MP3 ise önce WAV'a dönüştür.".into());
    }
    let path = custom_path(&app, cue).ok_or("Ayar klasörü bulunamadı.")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Ses kaydedilemedi: {e}"))?;
    }
    let tmp = path.with_extension("wav.tmp");
    fs::write(&tmp, &bytes).and_then(|_| fs::rename(&tmp, &path)).map_err(|e| format!("Ses kaydedilemedi: {e}"))?;
    let next = settings::modify(&app, |s| {
        s.sounds.custom.insert(cue.name().into(), name);
    })?;
    emit(&app, &next, cue);
    Ok(next)
}

/// Sustur kısayolu ve tepsi: ana anahtarı çevirir; adaya kısa bir bildirim için haber verir.
pub fn toggle(app: &AppHandle) {
    match settings::toggle_sound(app) {
        Ok(on) => {
            let _ = app.emit_to(island::LABEL, "island://sound", on);
            if on {
                emit(app, &settings::current(app), Cue::Attention);
            }
        }
        Err(error) => debuglog::line(format!("ses: değiştirilemedi {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_names_round_trip() {
        for cue in CUES {
            assert_eq!(Cue::parse(cue.name()), Some(cue));
            assert_eq!(serde_json::to_string(&cue).unwrap(), format!("\"{}\"", cue.name()));
        }
        assert_eq!(Cue::parse("../hello"), None);
    }

    #[test]
    fn embedded_set_is_valid_wav() {
        for cue in CUES {
            assert!(looks_like_wav(cue.zody()), "{}", cue.name());
        }
        assert!(!looks_like_wav(b"ID3\x04\0\0\0\0\0\0 mp3 gibi"));
        assert!(!looks_like_wav(b"RIFF"));
    }
}
