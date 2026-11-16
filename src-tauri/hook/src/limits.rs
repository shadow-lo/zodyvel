//! Claude plan limiti: Claude Code durum satırı (`statusLine`) köprüsü.
//!
//! Claude Code durum satırı komutuna her yanıttan sonra oturum JSON'unu stdin'den verir; Pro/Max
//! hesaplarda içinde `rate_limits.five_hour` ve `rate_limits.seven_day` (`used_percentage` 0–100,
//! `resets_at` Unix saniye) bulunur. Köprü bu iki pencereyi `~/.zodyvel/limits.json`'a yazar; Zodyvel
//! oradan okur. Kullanıcının önceki durum satırı komutu kayıtta onaltılık (`--chain <hex>`) saklanır:
//! köprü aynı girdiyle onu çalıştırır ve çıktısını aynen basar. Önceki komut yoksa kısa bir limit
//! satırı basar. Saf kısım; dosya ve süreç işleri `main.rs`'tedir.

use serde_json::{Map, Value, json};

/// Ayar dosyasındaki durum satırı kaydımızı tanıyan işaret.
pub const STATUSLINE_MARKER: &str = "--zodyvel-statusline-v1";
/// Önceki durum satırı komutu (UTF-8, onaltılık) bu bayraktan sonra gelir.
pub const CHAIN_FLAG: &str = "--chain";
/// `%USERPROFILE%\.zodyvel\` altında, köprünün yazıp Zodyvel'in okuduğu dosya.
pub const LIMITS_FILE: &str = "limits.json";
/// Önceki komut en çok bu kadar bekletilir; sonra durdurulur (Claude Code da yeni güncellemede iptal eder).
pub const CHAIN_TIMEOUT_SECS: u64 = 8;

/// Durum satırı girdisinden yalnız geçerli pencereler: `{"fiveHour": {"pct", "resetsAt"}, "sevenDay": …}`.
/// Hesapta limit yoksa (API anahtarı) ya da ilk yanıttan önce `None`.
pub fn windows(payload: &Value) -> Option<Map<String, Value>> {
    let limits = payload.get("rate_limits")?.as_object()?;
    let mut out = Map::new();
    for (key, name) in [("five_hour", "fiveHour"), ("seven_day", "sevenDay")] {
        let Some(window) = limits.get(key) else { continue };
        let pct = window.get("used_percentage").and_then(Value::as_f64).filter(|p| p.is_finite());
        let resets = window.get("resets_at").and_then(Value::as_f64).filter(|r| r.is_finite() && *r > 0.0);
        if let (Some(pct), Some(resets)) = (pct, resets) {
            out.insert(name.to_owned(), json!({ "pct": pct.clamp(0.0, 100.0), "resetsAt": resets as u64 }));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Dosyanın içeriği. `at`: pencerelerin son değiştiği an (ms).
pub fn record(windows: &Map<String, Value>, at_ms: u64) -> String {
    let mut root = windows.clone();
    root.insert("v".to_owned(), json!(1));
    root.insert("at".to_owned(), json!(at_ms));
    let mut text = Value::Object(root).to_string();
    text.push('\n');
    text
}

/// Dosyadaki pencereler yenileriyle aynı mı (aynıysa dosya yeniden yazılmaz).
pub fn same_windows(existing: &str, windows: &Map<String, Value>) -> bool {
    let Ok(Value::Object(old)) = serde_json::from_str::<Value>(existing) else { return false };
    ["fiveHour", "sevenDay"].iter().all(|k| old.get(*k) == windows.get(*k))
}

/// Önceki komut yokken basılan satır: "Claude %23 · hafta %41".
pub fn line(windows: &Map<String, Value>) -> String {
    let pct = |k: &str| windows.get(k).and_then(|w| w.get("pct")).and_then(Value::as_f64).map(|p| p.round() as u64);
    match (pct("fiveHour"), pct("sevenDay")) {
        (Some(five), Some(week)) => format!("Claude %{five} · hafta %{week}"),
        (Some(five), None) => format!("Claude %{five}"),
        (None, Some(week)) => format!("Claude hafta %{week}"),
        (None, None) => String::new(),
    }
}

pub fn hex_encode(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(hex: &str) -> Option<String> {
    if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

/// Kayıttaki komut metninden önceki komut (`--chain <hex>`).
pub fn chained(command: &str) -> Option<String> {
    let mut parts = command.split_whitespace();
    parts.by_ref().find(|p| *p == CHAIN_FLAG)?;
    hex_decode(parts.next()?).filter(|c| !c.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_windows_and_skips_missing_or_bad_ones() {
        let payload = json!({ "rate_limits": {
            "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
            "seven_day": { "used_percentage": 141.2, "resets_at": 1738857600 },
            "spend_limit": { "used_percentage": 5, "resets_at": 1 }
        }});
        let w = windows(&payload).unwrap();
        assert_eq!(w["fiveHour"], json!({ "pct": 23.5, "resetsAt": 1738425600u64 }));
        assert_eq!(w["sevenDay"]["pct"], 100.0);
        assert_eq!(w.len(), 2);
        assert_eq!(line(&w), "Claude %24 · hafta %100");

        let only_week = json!({ "rate_limits": { "five_hour": { "used_percentage": 3 }, "seven_day": { "used_percentage": 41, "resets_at": 9 } } });
        assert_eq!(line(&windows(&only_week).unwrap()), "Claude hafta %41");
        assert!(windows(&json!({ "model": {} })).is_none());
        assert!(windows(&json!({ "rate_limits": {} })).is_none());
    }

    #[test]
    fn record_round_trips_and_detects_change() {
        let w = windows(&json!({ "rate_limits": { "five_hour": { "used_percentage": 10, "resets_at": 5 } } })).unwrap();
        let text = record(&w, 42);
        assert!(same_windows(&text, &w));
        let other = windows(&json!({ "rate_limits": { "five_hour": { "used_percentage": 11, "resets_at": 5 } } })).unwrap();
        assert!(!same_windows(&text, &other));
        assert!(!same_windows("bozuk", &w));
    }

    #[test]
    fn chain_survives_quotes_spaces_and_turkish() {
        let original = r#"bash -c "echo 'Şu an' | jq -r '.model.display_name'""#;
        let command = format!("C:/Users/a/.zodyvel/bin/zodyvel-hook.exe {STATUSLINE_MARKER} {CHAIN_FLAG} {}", hex_encode(original));
        assert_eq!(chained(&command).as_deref(), Some(original));
        assert_eq!(chained(&format!("x {STATUSLINE_MARKER}")), None);
        assert_eq!(hex_decode("zz"), None);
        assert_eq!(hex_decode("abc"), None);
    }
}
