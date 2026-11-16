//! Araç olaylarını adada gösterilecek kısa Türkçe metinlere çevirir.

use serde_json::Value;

const SHORT: usize = 72;

pub fn short(text: &str, limit: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.chars().count() > limit { line.chars().take(limit).collect::<String>() + "…" } else { line.to_owned() }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

fn field<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| input.get(*k).and_then(Value::as_str))
}

/// Çalışan bir aracın şu an ne yaptığını anlatan adım metni.
pub fn step(tool: &str, input: &Value) -> String {
    match tool {
        "Bash" | "PowerShell" | "shell" | "exec_command" => match field(input, &["command"]) {
            Some(cmd) => format!("Komut · {}", short(cmd, SHORT)),
            None => "Komut çalıştırıyor".into(),
        },
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => match field(input, &["file_path", "notebook_path"]) {
            Some(path) => format!("Düzenliyor · {}", file_name(path)),
            None => "Dosya düzenliyor".into(),
        },
        "apply_patch" => "Yama uyguluyor".into(),
        "Read" => match field(input, &["file_path"]) {
            Some(path) => format!("Okuyor · {}", file_name(path)),
            None => "Dosya okuyor".into(),
        },
        "Grep" | "Glob" => "Kodda arıyor".into(),
        "WebFetch" | "WebSearch" | "web_search" => "Web'de arıyor".into(),
        "Task" | "Agent" => "Alt ajan çalıştırıyor".into(),
        "TodoWrite" => "Planı güncelliyor".into(),
        "ExitPlanMode" => "Planını sunuyor".into(),
        other if other.starts_with("mcp__") => format!("{} kullanıyor", other.trim_start_matches("mcp__").replace("__", " · ")),
        other => format!("{other} kullanıyor"),
    }
}

/// Adım geçmişindeki tür ve ayrıntı ekranında içeriği üretmek için gereken ham veri.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Prompt,
    Read,
    Edit,
    Write,
    Bash,
    Search,
    Web,
    Agent,
    Plan,
    Question,
    Approval,
    Tool,
    Done,
    Error,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Raw {
    #[default]
    None,
    Read { path: String, offset: Option<u64>, limit: Option<u64> },
    Edit { path: String, old: String, new: String },
    Write { path: String, content: String },
    Bash { command: String, output: Option<String> },
    Patch { text: String },
}

fn number(input: &Value, key: &str) -> Option<u64> {
    input.get(key).and_then(Value::as_u64)
}

/// Bir araç çağrısını adım türüne, kısa hedefe (dosya adı, komut…) ve ham veriye çevirir.
pub fn classify(tool: &str, input: &Value) -> (Kind, String, Raw) {
    let text = |keys: &[&str]| field(input, keys).unwrap_or("").to_owned();
    match tool {
        "Bash" | "PowerShell" | "shell" | "exec_command" => {
            let command = text(&["command", "cmd"]);
            (Kind::Bash, short(&command, 40), Raw::Bash { command, output: None })
        }
        "Edit" | "MultiEdit" => {
            let path = text(&["file_path"]);
            let (old, new) = match input.get("edits").and_then(Value::as_array).and_then(|e| e.first()) {
                Some(first) => (text_of(first, "old_string"), text_of(first, "new_string")),
                None => (text(&["old_string"]), text(&["new_string"])),
            };
            (Kind::Edit, file_name(&path).to_owned(), Raw::Edit { path, old, new })
        }
        "Write" | "NotebookEdit" => {
            let path = text(&["file_path", "notebook_path"]);
            let content = text(&["content", "new_source"]);
            (Kind::Write, file_name(&path).to_owned(), Raw::Write { path, content })
        }
        "apply_patch" => {
            let patch = text(&["input", "patch", "command"]);
            (Kind::Edit, "yama".into(), Raw::Patch { text: patch })
        }
        "Read" => {
            let path = text(&["file_path"]);
            let raw = Raw::Read { path: path.clone(), offset: number(input, "offset"), limit: number(input, "limit") };
            (Kind::Read, file_name(&path).to_owned(), raw)
        }
        "Grep" | "Glob" => (Kind::Search, short(&text(&["pattern"]), 40), Raw::None),
        "WebFetch" | "WebSearch" | "web_search" => (Kind::Web, short(&text(&["url", "query"]), 40), Raw::None),
        "Task" | "Agent" => (Kind::Agent, short(&text(&["description"]), 40), Raw::None),
        "TodoWrite" | "update_plan" | "ExitPlanMode" => (Kind::Plan, String::new(), Raw::None),
        zodyvel_hook::ASK_TOOL => (Kind::Question, String::new(), Raw::None),
        other => (Kind::Tool, other.trim_start_matches("mcp__").replace("__", " · "), Raw::None),
    }
}

fn text_of(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}

/// Onay kartının başlığı ve ayrıntısı.
pub fn approval(tool: &str, input: &Value) -> (String, String) {
    let detail = |keys: &[&str]| field(input, keys).map(|s| short(s, 160));
    match tool {
        "Bash" | "PowerShell" | "shell" | "exec_command" => {
            ("Komut çalıştırmak istiyor".into(), detail(&["command"]).unwrap_or_default())
        }
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            ("Dosya değiştirmek istiyor".into(), detail(&["file_path", "notebook_path"]).unwrap_or_default())
        }
        "apply_patch" => {
            let files: Vec<&str> = field(input, &["command", "input", "patch"])
                .unwrap_or("")
                .lines()
                .filter_map(|l| ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|p| l.strip_prefix(p)))
                .map(|path| file_name(path.trim()))
                .collect();
            ("Yama uygulamak istiyor".into(), short(&files.join(", "), 160))
        }
        "WebFetch" => ("Web sayfası açmak istiyor".into(), detail(&["url"]).unwrap_or_default()),
        "WebSearch" | "web_search" => ("Web'de aramak istiyor".into(), detail(&["query"]).unwrap_or_default()),
        other => (format!("{other} kullanmak istiyor"), short(&compact(input), 160)),
    }
}

fn compact(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Planın kısa adı: ilk başlık, yoksa ilk dolu satır (Markdown işaretleri atılır).
pub fn plan_title(plan: &str) -> String {
    let lines = || plan.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("```"));
    let heading = lines().find(|l| l.starts_with('#')).map(|l| l.trim_start_matches('#').trim());
    let line = heading.or_else(|| lines().next()).unwrap_or("");
    let clean: String = line.trim_start_matches(['-', '*', '>', ' ']).replace("**", "").replace('`', "");
    if clean.is_empty() { "Plan".into() } else { short(&clean, 80) }
}

/// "Her zaman izin ver" önerisinin kullanıcıya gösterilen hâli: ne izinli olacak ve nereye yazılacak.
/// Ör. "Bash(npm test:*) · bu projede, yalnız sende".
pub fn always_label(entry: &Value) -> Option<String> {
    let s = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or("");
    let what = match s("type") {
        "addRules" => {
            let rules: Vec<String> = entry
                .get("rules")?
                .as_array()?
                .iter()
                .filter_map(|rule| {
                    let tool = rule.get("toolName")?.as_str()?;
                    Some(match rule.get("ruleContent").and_then(Value::as_str).filter(|c| !c.is_empty()) {
                        Some(content) => format!("{tool}({content})"),
                        None => format!("tüm {tool} istekleri"),
                    })
                })
                .collect();
            if rules.is_empty() {
                return None;
            }
            short(&rules.join(", "), 120)
        }
        "addDirectories" => {
            let dirs: Vec<&str> = entry.get("directories")?.as_array()?.iter().filter_map(Value::as_str).collect();
            format!("{} klasörüne erişim", short(&dirs.join(", "), 100))
        }
        "setMode" if s("mode") == "acceptEdits" => "dosya düzenlemelerini sormadan kabul et".into(),
        _ => return None,
    };
    let place = match s("destination") {
        "session" => "bu oturum boyunca",
        "localSettings" => "bu projede, yalnız sende",
        "projectSettings" => "bu projede, proje ayarına yazılır",
        "userSettings" => "tüm projelerde",
        _ => "",
    };
    Some(if place.is_empty() { what } else { format!("{what} · {place}") })
}

pub const TEMP_NAME: &str = "Geçici klasör";

/// Proje adı: çalışma klasörünün son parçası. Araçların açtığı geçici klasörler
/// ("scratch-2026-10-14-…", Temp altı) anlamsız bir ad yerine "Geçici klasör" olur.
pub fn project(cwd: &str) -> String {
    let path = cwd.trim_end_matches(['\\', '/']);
    let name = file_name(path);
    if name.is_empty() {
        return "Proje".into();
    }
    let lower = path.to_lowercase().replace('\\', "/");
    let temp = name.to_lowercase().starts_with("scratch-") || lower.contains("/appdata/local/temp/") || lower.starts_with("/tmp/");
    if temp { TEMP_NAME.into() } else { name.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn describes_common_tools() {
        assert_eq!(step("Bash", &json!({ "command": "pnpm test\n--watch" })), "Komut · pnpm test");
        assert_eq!(step("Edit", &json!({ "file_path": "C:\\p\\src\\App.svelte" })), "Düzenliyor · App.svelte");
        assert_eq!(step("mcp__github__create_issue", &json!({})), "github · create_issue kullanıyor");
        let (action, detail) = approval("Bash", &json!({ "command": "rm -rf node_modules" }));
        assert_eq!(action, "Komut çalıştırmak istiyor");
        assert_eq!(detail, "rm -rf node_modules");
    }

    #[test]
    fn classifies_tools_for_step_history() {
        let (kind, target, raw) = classify("Read", &json!({ "file_path": "C:\\p\\src\\a.ts", "offset": 10, "limit": 5 }));
        assert_eq!((kind, target.as_str()), (Kind::Read, "a.ts"));
        assert_eq!(raw, Raw::Read { path: "C:\\p\\src\\a.ts".into(), offset: Some(10), limit: Some(5) });
        let (kind, _, raw) = classify("MultiEdit", &json!({ "file_path": "b.rs", "edits": [{ "old_string": "a", "new_string": "b" }] }));
        assert_eq!(kind, Kind::Edit);
        assert_eq!(raw, Raw::Edit { path: "b.rs".into(), old: "a".into(), new: "b".into() });
        assert_eq!(classify("Bash", &json!({ "command": "pnpm test" })).1, "pnpm test");
        assert_eq!(classify(zodyvel_hook::ASK_TOOL, &json!({})).0, Kind::Question);
        assert_eq!(classify("mcp__github__create_issue", &json!({})).1, "github · create_issue");
    }

    #[test]
    fn project_from_cwd() {
        assert_eq!(project("C:\\Users\\a\\work\\zodyvel\\"), "zodyvel");
        assert_eq!(project("/home/a/api"), "api");
        assert_eq!(project(""), "Proje");
        assert_eq!(project("C:\\Users\\a\\scratch-2026-10-14-ab12"), "Geçici klasör");
        assert_eq!(project("C:\\Users\\a\\AppData\\Local\\Temp\\x1"), "Geçici klasör");
    }

    #[test]
    fn short_keeps_first_line_and_limits_length() {
        assert_eq!(short("\n  ilk satır \nikinci", 50), "ilk satır");
        assert_eq!(short("abcdef", 3), "abc…");
    }

    #[test]
    fn labels_always_allow_suggestions() {
        let rule = json!({ "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }], "behavior": "allow", "destination": "localSettings" });
        assert_eq!(always_label(&rule).unwrap(), "Bash(npm test:*) · bu projede, yalnız sende");
        let whole = json!({ "type": "addRules", "rules": [{ "toolName": "WebFetch" }], "destination": "session" });
        assert_eq!(always_label(&whole).unwrap(), "tüm WebFetch istekleri · bu oturum boyunca");
        let mode = json!({ "type": "setMode", "mode": "acceptEdits", "destination": "session" });
        assert_eq!(always_label(&mode).unwrap(), "dosya düzenlemelerini sormadan kabul et · bu oturum boyunca");
        assert!(always_label(&json!({ "type": "setMode", "mode": "bypassPermissions" })).is_none());
    }

    #[test]
    fn patch_approval_lists_files() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n-x\n+y\n*** Add File: docs/b.md\n+z\n*** End Patch";
        assert_eq!(approval("apply_patch", &json!({ "command": patch })).1, "a.rs, b.md");
    }

    #[test]
    fn plan_title_prefers_the_first_heading() {
        assert_eq!(plan_title("Giriş metni\n\n## **Kullanım** özeti\n- a"), "Kullanım özeti");
        assert_eq!(plan_title("\n- `usage.rs` ekle\n- test"), "usage.rs ekle");
        assert_eq!(plan_title("```\n```"), "Plan");
        assert_eq!(plan_title(""), "Plan");
    }
}
