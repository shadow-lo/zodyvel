//! Claude plan limiti (Ayarlar → Özellikler → Plan limiti): Claude Code durum satırına köprü kurulumu
//! ve köprünün yazdığı `~/.zodyvel/limits.json`'ın okunması.
//!
//! - Kurulum `connections_prepare` ile aynı yoldan: önce önizleme ve yedek, uygulama `connections_apply`.
//! - Kullanıcının durum satırı silinmez: komutu kayda onaltılık eklenir (`--chain`), köprü onu aynen
//!   çalıştırır; kaldırınca geri yazılır. `padding`, `refreshInterval` gibi alanlar olduğu gibi kalır.
//! - Özellik kapalıyken dosya okunmaz.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value, json};
use tauri::{AppHandle, State};
use zodyvel_hook::Provider;
use zodyvel_hook::limits::{self, CHAIN_FLAG, LIMITS_FILE, STATUSLINE_MARKER};

use super::setup::{self, Plan, PreviewView, Setup};
use super::{hookbin, now_ms};

const KEY: &str = "statusLine";

/// Ayar dosyasına yazılan komut. Claude Code durum satırını Git Bash ya da PowerShell ile çalıştırır:
/// ikisinde de çalışsın diye ters bölü yerine düz bölü, boşluklu yolda kısa (8.3) yol.
fn hook_command(hook_path: &str) -> String {
    let path = if hook_path.contains(' ') {
        setup::short_path(hook_path).filter(|p| !p.contains(' ')).unwrap_or_else(|| format!("\"{hook_path}\""))
    } else {
        hook_path.to_owned()
    };
    format!("{} {STATUSLINE_MARKER}", path.replace('\\', "/"))
}

fn command_of(root: &Map<String, Value>) -> &str {
    root.get(KEY).and_then(|v| v.get("command")).and_then(Value::as_str).unwrap_or("")
}

fn is_ours(command: &str) -> bool {
    command.split_whitespace().any(|p| p == STATUSLINE_MARKER)
}

/// Kullanıcının kendi durum satırı komutu: bizimkiyse içine saklanan, değilse dosyadaki.
fn original(root: &Map<String, Value>) -> Option<String> {
    let command = command_of(root);
    if is_ours(command) { limits::chained(command) } else { Some(command.to_owned()).filter(|c| !c.trim().is_empty()) }
}

/// Yeni dosya içeriği ve değişiklik özeti. Diskte hiçbir şey değiştirmez.
pub fn plan(mut root: Map<String, Value>, install: bool, hook_path: &str) -> Result<Plan, String> {
    if root.get(KEY).is_some_and(|v| !v.is_object()) {
        return Err("`statusLine` alanı beklenen biçimde değil; otomatik düzenlenmedi.".into());
    }
    let ours = is_ours(command_of(&root));
    let original = original(&root);
    let mut changes = Vec::new();
    if install {
        let mut command = hook_command(hook_path);
        if let Some(original) = &original {
            command = format!("{command} {CHAIN_FLAG} {}", limits::hex_encode(original));
        }
        let entry = root.entry(KEY).or_insert_with(|| json!({}));
        let object = entry.as_object_mut().expect("nesne");
        object.insert("type".into(), json!("command"));
        object.insert("command".into(), json!(command));
        if ours {
            changes.push("Zodyvel durum satırı kaydı yenilenecek.".to_owned());
        }
        match &original {
            Some(original) => changes.push(format!(
                "Mevcut durum satırın korunur: “{original}” Zodyvel'den sonra aynı bilgiyle çalışır, çıktısı aynen görünür."
            )),
            None => changes.push(
                "Şu an durum satırın yok: Claude Code'un altında kısa bir limit yazısı çıkar (ör. “Claude %23 · hafta %41”). \
                 Durum satırı varken Claude Code alttaki bazı kısayol ipuçlarını göstermez."
                    .to_owned(),
            ),
        }
        changes.push(
            "Claude Code her yanıttan sonra 5 saatlik ve haftalık limit yüzdesini verir; köprü bunu %USERPROFILE%\\.zodyvel\\limits.json'a yazar. \
             İnternete bir şey gönderilmez."
                .to_owned(),
        );
        changes.push("Limitler yalnız Claude Pro/Max aboneliğinde ve oturumdaki ilk yanıttan sonra gelir.".to_owned());
    } else if !ours {
        changes.push("Kaldırılacak Zodyvel durum satırı yok.".to_owned());
    } else if let Some(original) = original {
        root[KEY]["command"] = json!(original);
        changes.push(format!("Önceki durum satırın geri gelir: “{original}”."));
    } else {
        root.remove(KEY);
        changes.push("Zodyvel durum satırı kaldırılacak (öncesinde durum satırın yoktu).".to_owned());
    }
    changes.push("Diğer ayarların ve hook'ların değişmeden kalır.".to_owned());
    let mut text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(Plan { text, changes })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInspection {
    pub path: String,
    pub installed: bool,
    /// Kullanıcının kendi durum satırı komutu (kuruluysa içinde saklanan).
    pub original: Option<String>,
    pub error: Option<String>,
    pub hook_ready: bool,
}

#[tauri::command]
pub fn statusline_inspect() -> StatusInspection {
    let path = setup::target_path(Provider::Claude);
    let read = setup::read(&path).and_then(|b| setup::decode(&b)).and_then(|t| setup::parse(t.as_deref()));
    let (installed, original, error) = match read {
        Ok(root) => (is_ours(command_of(&root)), original(&root), None),
        Err(e) => (false, None, Some(e)),
    };
    StatusInspection { path: path.display().to_string(), installed, original, error, hook_ready: hookbin::ready() }
}

/// Önizleme; uygulama `connections_apply` ile.
#[tauri::command]
pub fn statusline_prepare(setup: State<'_, Setup>, action: String) -> Result<PreviewView, String> {
    let install = match action.as_str() {
        "install" => true,
        "remove" => false,
        _ => return Err("Geçersiz işlem.".into()),
    };
    if install && !hookbin::ready() {
        return Err("zodyvel-hook.exe bulunamadı; uygulamayı yeniden başlatmayı dene.".into());
    }
    let path = setup::target_path(Provider::Claude);
    let bytes = setup::read(&path)?;
    let root = setup::parse(setup::decode(&bytes)?.as_deref())?;
    let mut plan = plan(root, install, &hookbin::installed_path().display().to_string())?;
    Ok(setup::remember(&setup, Provider::Claude, path, &bytes, &mut plan))
}

// ---- Okuma ----

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// Kullanılan yüzde (0–100).
    pub pct: f64,
    /// Sıfırlanma anı (ms).
    pub resets_at: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanLimits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// Değerlerin son değiştiği an (ms).
    pub at: u64,
}

fn limits_path() -> PathBuf {
    let bin = hookbin::installed_path();
    bin.parent().and_then(|p| p.parent()).map(|p| p.join(LIMITS_FILE)).unwrap_or_else(|| PathBuf::from(LIMITS_FILE))
}

/// Dosya içeriğinden süresi dolmamış pencereler; hiçbiri kalmadıysa `None`.
pub fn fresh(text: &str, now: u64) -> Option<PlanLimits> {
    let root: Value = serde_json::from_str(text).ok()?;
    let window = |key: &str| {
        let w = root.get(key)?;
        let pct = w.get("pct")?.as_f64()?.clamp(0.0, 100.0);
        let resets_at = w.get("resetsAt")?.as_u64()?.saturating_mul(1000);
        (resets_at > now).then_some(Window { pct, resets_at })
    };
    let limits = PlanLimits { five_hour: window("fiveHour"), seven_day: window("sevenDay"), at: root.get("at").and_then(Value::as_u64).unwrap_or(0) };
    (limits.five_hour.is_some() || limits.seven_day.is_some()).then_some(limits)
}

/// Özellik kapalıysa ya da bilgi yoksa `None`. Küçük bir dosya; yalnız hap ekrandayken istenir.
#[tauri::command]
pub fn plan_limits(app: AppHandle) -> Option<PlanLimits> {
    if !crate::settings::current(&app).features.plan_limit {
        return None;
    }
    fresh(&std::fs::read_to_string(limits_path()).ok()?, now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOOK: &str = r"C:\Users\a\.zodyvel\bin\zodyvel-hook.exe";

    fn root(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn install_chains_existing_status_line_and_remove_restores_it() {
        let user = r#"bash -c "jq -r '.model.display_name'""#;
        let before = root(json!({ "model": "opus", "statusLine": { "type": "command", "command": user, "padding": 2 } }));
        let installed = setup::parse(Some(&plan(before, true, HOOK).unwrap().text)).unwrap();
        let command = installed[KEY]["command"].as_str().unwrap();
        assert!(command.starts_with("C:/Users/a/.zodyvel/bin/zodyvel-hook.exe --zodyvel-statusline-v1 --chain "));
        assert_eq!(limits::chained(command).as_deref(), Some(user));
        assert_eq!((installed[KEY]["padding"].as_u64(), installed["model"].as_str()), (Some(2), Some("opus")));

        // Yeniden kurulum zinciri bozmaz (iç içe zincir olmaz).
        let again = setup::parse(Some(&plan(installed.clone(), true, HOOK).unwrap().text)).unwrap();
        assert_eq!(again[KEY]["command"], installed[KEY]["command"]);

        let removed = setup::parse(Some(&plan(again, false, HOOK).unwrap().text)).unwrap();
        assert_eq!(removed[KEY], json!({ "type": "command", "command": user, "padding": 2 }));
    }

    #[test]
    fn without_status_line_install_adds_and_remove_deletes() {
        let installed = setup::parse(Some(&plan(root(json!({ "model": "opus" })), true, HOOK).unwrap().text)).unwrap();
        let command = installed[KEY]["command"].as_str().unwrap();
        assert!(is_ours(command) && limits::chained(command).is_none());
        let removed = setup::parse(Some(&plan(installed, false, HOOK).unwrap().text)).unwrap();
        assert_eq!(removed, root(json!({ "model": "opus" })));

        let untouched = plan(root(json!({ "statusLine": { "command": "mine.sh" } })), false, HOOK).unwrap();
        assert!(untouched.changes[0].contains("yok"));
        assert!(plan(root(json!({ "statusLine": "bad" })), true, HOOK).is_err());
    }

    #[test]
    fn fresh_drops_expired_windows() {
        let text = r#"{"fiveHour":{"pct":73.4,"resetsAt":2000},"sevenDay":{"pct":41,"resetsAt":1000},"v":1,"at":5}"#;
        let limits = fresh(text, 1_500_000).unwrap();
        assert_eq!(limits.five_hour, Some(Window { pct: 73.4, resets_at: 2_000_000 }));
        assert_eq!((limits.seven_day, limits.at), (None, 5));
        assert_eq!(fresh(text, 2_000_000), None);
        assert_eq!(fresh("bozuk", 0), None);
    }
}
