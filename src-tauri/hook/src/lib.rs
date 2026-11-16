//! Zodyvel hook protokolü. Hem `zodyvel-hook.exe` hem Zodyvel uygulaması bu modülü kullanır.
//!
//! Akış: Claude Code / Codex bir olayda `zodyvel-hook.exe`'yi çalıştırır ve olay JSON'unu stdin'den
//! verir. Köprü yalnız gereken alanları ayıklar, tek satır JSON olarak Zodyvel'in named pipe'ına yazar.
//! Yalnız "adadan onay" modunda yanıt bekler: `PermissionRequest` (onay) ve seçenekli soru aracının
//! `PreToolUse` olayı (cevaplar). Karar gelirse aracın beklediği JSON'u stdout'a yazar. Zodyvel
//! kapalıysa, karar gelmezse veya bir şey ters giderse hiçbir şey yazmaz: araç kendi normal akışına
//! döner. Hiçbir istek kendiliğinden onaylanmaz, hiçbir soru kendiliğinden cevaplanmaz.
//!
//! Plan incelemesi: Claude Code plan modunda planı `ExitPlanMode` ile sunar; araç bunun için
//! `PermissionRequest` gönderir ve planın metni `tool_input.plan`'dadır. Plan okunmak için uzun
//! olduğundan tam metin ayrıca (sınırlı) gider ve karar daha uzun beklenir. Araç kendi onay penceresini
//! köprüyle aynı anda gösterir; köprünün beklemesi kullanıcıyı araçta bekletmez.
//!
//! Ajana not (yalnız Claude Code, adadan onay modu): kullanıcı adadan bir not sıraya koyabilir.
//! "Sonra" notu araç turu bitirirken (`Stop`) `decision: block` + gerekçe olarak gider; Claude durmaz,
//! notu yeni istek gibi okuyup devam eder. "Şimdi" notu bir sonraki araç çağrısında (`PreToolUse`) o
//! çağrıyı reddeder ve notu gerekçe olarak iletir. Bunun için köprü bu iki olayda kısa bir süre yanıt
//! bekler; sırada not yoksa Zodyvel hemen bağlantıyı kapatır ve hiçbir şey yazılmaz.
//!
//! Görünürlük: Claude Code hook'tan gelen metni sohbete kullanıcı mesajı olarak koyamaz. Gerekçe ve
//! `asyncRewake` metni Claude'a gider ama sohbette ya hiç görünmez (boştaki oturum) ya da arayüze göre
//! en fazla "Stop hook feedback" gibi bir geri bildirim satırı olur; arka plan hook'unun `systemMessage`'ı
//! da kullanıcıya gösterilmez. Bu yüzden her not ve görev, Claude'dan yanıtına notu alıntılayarak başlamasını ister:
//! kullanıcı ne yazdığını sohbette Claude'un yanıtının başında görür.

use serde_json::{Map, Value, json};

pub mod diff;
pub mod limits;

/// Ayar dosyalarında bizim kayıtlarımızı tanıyan işaret.
pub const MARKER: &str = "--zodyvel-hook-v1";
pub const PROTOCOL_VERSION: u64 = 1;
/// Köprünün kendi zaman aşımı; adadaki bekleme süresinden biraz uzun.
pub const WAIT_LIMIT_SECS: u64 = 100;
/// Adada kararın beklendiği süre.
pub const DECISION_TIMEOUT_SECS: u64 = 90;
/// Planın okunup karar verilmesi için adada beklenen süre.
pub const PLAN_TIMEOUT_SECS: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Yalnız izle: hiçbir olayda yanıt beklenmez.
    Observe,
    /// Adadan onay: `PermissionRequest` için karar beklenir.
    Interactive,
}

impl Mode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "observe" => Some(Self::Observe),
            "interactive" => Some(Self::Interactive),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Interactive => "interactive",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Allow,
    Deny,
    /// Seçenekli sorunun cevapları: soru metni → seçilen etiket(ler) (çoklu seçimde virgülle).
    Answer(Map<String, Value>),
    /// Onayla ve aracın önerdiği izin kuralını kaydet ("Her zaman izin ver"). Sayı, olaydaki
    /// `permission_suggestions` dizisinde [`always_choice`]'ın seçtiği önerinin sırasıdır.
    AllowAlways(usize),
    /// Reddet ve araca kullanıcının notunu ilet (plan: "şunu değiştir"). Araç notla devam eder.
    Revise(String),
    /// Kullanıcının sıraya koyduğu not: `Stop`'ta tur bitmez ve not okunur, `PreToolUse`'ta çağrı
    /// reddedilip not gerekçe olarak iletilir.
    Note(String),
}

/// Claude Code'un seçenekli soru aracı. Cevabı yalnız `PreToolUse` + `updatedInput` ile verilebilir.
pub const ASK_TOOL: &str = "AskUserQuestion";
/// Codex'in seçenekli soru aracı. Hook ile cevaplanamaz; ada yalnız bilgi kartı gösterir.
pub const CODEX_ASK_TOOL: &str = "request_user_input";
/// Claude Code'un plan sunma aracı (plan modundan çıkış). Onayı `PermissionRequest`'tedir.
pub const PLAN_TOOL: &str = "ExitPlanMode";
/// Adaya giden plan metninin üst sınırı (karakter).
pub const MAX_PLAN: usize = 60_000;

/// Adada kararın beklendiği süre: plan okunmak için daha uzun beklenir.
pub fn decision_timeout(tool: &str) -> u64 {
    if tool == PLAN_TOOL { PLAN_TIMEOUT_SECS } else { DECISION_TIMEOUT_SECS }
}

/// Not sorulan olaylarda köprünün en çok beklediği süre: Zodyvel yanıtı hemen verir, bu yalnız güvenlik.
pub const NOTE_WAIT_SECS: u64 = 3;

/// Tur sonunda arka planda adadan görev bekleyen köprü (Claude Code `asyncRewake` hook'u). Görev
/// gelince köprü 2 ile çıkar, Claude uyanır ve görevi yeni istek gibi alır.
pub const AWAIT_FLAG: &str = "--await-task";
/// Görev bekleme süresi; boştaki oturum bu kadar sonra adadan görev almaz (yeni turda yeniden başlar).
pub const AWAIT_TIMEOUT_SECS: u64 = 3 * 60 * 60;

/// Uyanan Claude'un okuyacağı metin (köprünün stderr'i).
pub fn task_message(task: &str) -> String {
    format!("Kullanıcı Zodyvel adasından yeni bir istek gönderdi; bunu kullanıcının yeni mesajı olarak ele al. {}", echo(task))
}

/// Notu sohbette görünür kılan istek + alıntı: Claude yanıtına alıntıyı aynen yazarak başlar.
fn echo(note: &str) -> String {
    let note = note_text(note);
    let mut lines = note.lines().map(str::trim_end);
    let mut quote = format!("> **Zodyvel adasından:** {}", lines.next().unwrap_or(""));
    for line in lines {
        quote.push_str("\n> ");
        quote.push_str(line);
    }
    format!(
        "Bu metin kullanıcının sohbetinde mesaj olarak görünmüyor; kullanıcı ne yazdığını görsün diye bir sonraki yanıtına aşağıdaki alıntıyı aynen yazarak başla, sonra isteği yerine getir:\n\n{quote}"
    )
}
/// Adaya giden notun üst sınırı (karakter).
pub const MAX_NOTE: usize = 2000;

/// Köprünün kendi zaman aşımı (yanıt beklenen olaylarda): adadaki bekleme süresinden biraz uzun.
pub fn wait_limit(event: &str, tool: &str) -> u64 {
    if event == "PermissionRequest" && tool == PLAN_TOOL { PLAN_TIMEOUT_SECS + 10 } else { WAIT_LIMIT_SECS }
}

/// Sıradaki not sorulan olaylar: yalnız Claude Code, adadan onay modu; tur sonu ve araç çağrısı
/// (soru aracı hariç, onun cevabı ayrı). Codex'in bu yanıtları desteklediği doğrulanmadı.
pub fn wants_note(provider: Provider, mode: Mode, event: &str, tool: &str) -> bool {
    provider == Provider::Claude
        && mode == Mode::Interactive
        && match event {
            "Stop" => true,
            "PreToolUse" => tool != ASK_TOOL,
            _ => false,
        }
}

/// Köprünün, olayı gönderen aracın penceresini (terminal, düzenleyici…) bulup ekleyeceği olaylar.
/// Ada "Terminale git" ile bu pencereyi öne getirir. Her araç çağrısında aranmaz (gecikme olmasın).
pub fn wants_window(event: &str, tool: &str) -> bool {
    match event {
        "SessionStart" | "UserPromptSubmit" | "PermissionRequest" | "Notification" => true,
        "PreToolUse" => tool == ASK_TOOL || tool == CODEX_ASK_TOOL,
        _ => false,
    }
}

/// "Her zaman izin ver" için kullanılabilecek ilk öneri: yalnız izin kuralı ekleme (allow),
/// klasör ekleme ya da düzenlemeleri kabul kipi. Daha geniş kipler (ör. tüm izinleri atlamak)
/// adadan hiçbir zaman önerilmez.
pub fn always_choice(payload: &Value) -> Option<usize> {
    payload.get("permission_suggestions")?.as_array()?.iter().position(|entry| {
        let s = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or("");
        match s("type") {
            "addRules" => s("behavior") == "allow" && entry.get("rules").and_then(Value::as_array).is_some_and(|r| !r.is_empty()),
            "addDirectories" => entry.get("directories").and_then(Value::as_array).is_some_and(|d| !d.is_empty()),
            "setMode" => s("mode") == "acceptEdits",
            _ => false,
        }
    })
}

/// Kullanıcıya özel pipe adı. Ayrıca pipe yalnız bu Windows kullanıcısına açık oluşturulur.
pub fn pipe_name(user: &str) -> String {
    let clean: String = user.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    format!(r"\\.\pipe\zodyvel-hooks-v1-{clean}")
}

pub fn current_pipe_name() -> String {
    pipe_name(&std::env::var("USERNAME").unwrap_or_default())
}

/// Köprünün Zodyvel'den yanıt bekleyeceği olaylar. Soru aracının onay isteği beklenmez: soru
/// `PreToolUse`'da adadan cevaplanır; cevaplanmadıysa araç kendi penceresinde sorar.
pub fn wants_reply(mode: Mode, event: &str, tool: &str) -> bool {
    mode == Mode::Interactive
        && match event {
            "PermissionRequest" => tool != ASK_TOOL,
            "PreToolUse" => tool == ASK_TOOL,
            _ => false,
        }
}

const KEEP: &[&str] = &[
    "hook_event_name",
    "session_id",
    "cwd",
    "agent_id",
    "agent_type",
    "turn_id",
    "tool_name",
    "tool_use_id",
    "tool_input",
    "notification_type",
    "message",
    "title",
    "reason",
    "source",
    "permission_mode",
    "last_assistant_message",
    "permission_suggestions",
];
const MAX_TEXT: usize = 1200;
/// Komut çıktısından adanın ayrıntı ekranına giden en fazla son kısım.
const MAX_OUTPUT: usize = 1500;
const OUTPUT_KEYS: &[&str] = &["stdout", "stderr", "output"];

/// Yalnız adanın kullandığı alanları tutar. İstem metni ve transkript yolu gönderilmez; araç
/// çıktısından yalnız komut çıktısının sonu (sınırlı) gider. Hepsi bu bilgisayarda kalır.
pub fn slim(payload: &Value) -> Value {
    let mut out = Map::new();
    if let Some(obj) = payload.as_object() {
        for key in KEEP {
            if let Some(value) = obj.get(*key) {
                out.insert((*key).to_owned(), clip(value, 0));
            }
        }
        if let Some(output) = obj.get("tool_response").and_then(output_tail) {
            out.insert("tool_output".to_owned(), Value::String(output));
        }
    }
    Value::Object(out)
}

/// Araç çıktısındaki metin alanlarının (stdout, stderr, output) sonu.
fn output_tail(response: &Value) -> Option<String> {
    let text = match response {
        Value::String(s) => s.clone(),
        Value::Object(map) => {
            let parts: Vec<&str> = OUTPUT_KEYS.iter().filter_map(|k| map.get(*k).and_then(Value::as_str)).filter(|s| !s.is_empty()).collect();
            parts.join("\n")
        }
        _ => return None,
    };
    let count = text.chars().count();
    if count == 0 {
        return None;
    }
    Some(if count > MAX_OUTPUT { format!("…{}", text.chars().skip(count - MAX_OUTPUT).collect::<String>()) } else { text })
}

fn clip(value: &Value, depth: usize) -> Value {
    match value {
        Value::String(s) if s.chars().count() > MAX_TEXT => Value::String(s.chars().take(MAX_TEXT).collect::<String>() + "…"),
        Value::Array(items) if depth < 4 => Value::Array(items.iter().take(20).map(|v| clip(v, depth + 1)).collect()),
        Value::Object(map) if depth < 4 => {
            Value::Object(map.iter().take(30).map(|(k, v)| (k.clone(), clip(v, depth + 1))).collect())
        }
        Value::Array(_) | Value::Object(_) => Value::Null,
        other => other.clone(),
    }
}

/// Kırpılmadan önce, tam olaydan hesaplanan ekler: onay isteğinde ve biten düzenlemede dosya bazında
/// "+N −M", plan sunulduysa planın tam metni (sınırlı). Biten "Write"ta dosya zaten yeni hâlinde
/// olduğundan içeriğin satırları eklenmiş sayılır.
pub fn extras(payload: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    let event = payload.get("hook_event_name").and_then(Value::as_str);
    if event == Some("PostToolUse") {
        let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
        let stats = match (tool, payload.get("tool_input")) {
            ("Write", Some(input)) => input.get("file_path").and_then(Value::as_str).map(|path| {
                let content = input.get("content").and_then(Value::as_str).unwrap_or("");
                json!([{ "path": path, "added": content.lines().count(), "removed": 0 }])
            }),
            ("Edit" | "MultiEdit" | "apply_patch", Some(input)) => diff::edit_stats(tool, input),
            _ => None,
        };
        if let Some(stats) = stats {
            out.insert("diff_stat".to_owned(), stats);
        }
    }
    if event == Some("PermissionRequest") {
        let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
        if let Some(stats) = payload.get("tool_input").and_then(|input| diff::edit_stats(tool, input)) {
            out.insert("diff_stat".to_owned(), stats);
        }
        if tool == PLAN_TOOL
            && let Some(plan) = payload.get("tool_input").and_then(plan_text)
        {
            out.insert("plan".to_owned(), Value::String(plan));
        }
    }
    out
}

/// Planın metni: araç onu `plan` alanına diskteki plan dosyasından koyar; yoksa dosyanın kendisi okunur.
fn plan_text(input: &Value) -> Option<String> {
    let text = match input.get("plan").and_then(Value::as_str).filter(|p| !p.trim().is_empty()) {
        Some(plan) => plan.to_owned(),
        None => {
            let path = input.get("planFilePath").and_then(Value::as_str).filter(|p| p.ends_with(".md"))?;
            let bytes = std::fs::read(path).ok()?;
            String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_PLAN * 4)]).into_owned()
        }
    };
    let count = text.chars().count();
    Some(if count > MAX_PLAN { text.chars().take(MAX_PLAN).collect::<String>() + "\n\n…" } else { text })
}

/// `extra`: köprünün eklediği alanlar (`diff_stat`, `window`); kırpılmış olaya eklenir.
pub fn envelope(provider: Provider, mode: Mode, payload: &Value, extra: Map<String, Value>) -> String {
    let mut slimmed = slim(payload);
    if let Some(obj) = slimmed.as_object_mut() {
        obj.extend(extra);
    }
    let message = json!({
        "v": PROTOCOL_VERSION,
        "provider": provider.as_str(),
        "mode": mode.as_str(),
        "payload": slimmed,
    });
    let mut line = message.to_string();
    line.push('\n');
    line
}

pub fn reply(decision: Option<&Decision>) -> String {
    let value = match decision {
        Some(Decision::Allow) => json!({ "decision": "allow" }),
        Some(Decision::Deny) => json!({ "decision": "deny" }),
        Some(Decision::Answer(answers)) => json!({ "decision": "answer", "answers": answers }),
        Some(Decision::AllowAlways(index)) => json!({ "decision": "allow_always", "suggestion": index }),
        Some(Decision::Revise(message)) => json!({ "decision": "deny", "message": message }),
        Some(Decision::Note(message)) => json!({ "decision": "note", "message": message }),
        None => json!({ "decision": null }),
    };
    let mut line = value.to_string();
    line.push('\n');
    line
}

pub fn parse_reply(line: &str) -> Option<Decision> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    match value.get("decision")?.as_str()? {
        "allow" => Some(Decision::Allow),
        "deny" => Some(match value.get("message").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty()) {
            Some(message) => Decision::Revise(message.to_owned()),
            None => Decision::Deny,
        }),
        "answer" => value.get("answers")?.as_object().cloned().map(Decision::Answer),
        "allow_always" => value.get("suggestion")?.as_u64().map(|i| Decision::AllowAlways(i as usize)),
        "note" => value.get("message")?.as_str().map(str::trim).filter(|m| !m.is_empty()).map(|m| Decision::Note(m.to_owned())),
        _ => None,
    }
}

/// Aracın beklediği karar JSON'u. `payload` köprünün aldığı özgün (kırpılmamış) olaydır.
/// - Onay (`PermissionRequest`): Claude Code ve Codex'te aynı biçim.
/// - Soru (`PreToolUse`, soru aracı): özgün `tool_input` + `answers`. Yalnız sorulan soruların
///   cevapları ve metin değerleri geçer; boş kalan bir cevapta hiçbir şey yazılmaz.
pub fn decision_output(event: &str, decision: &Decision, payload: &Value) -> Option<String> {
    let plan = payload.get("tool_name").and_then(Value::as_str) == Some(PLAN_TOOL);
    match (event, decision) {
        // Plan aracı kullanıcı etkileşimi ister: araç onayı ancak girdinin kendisiyle (değiştirilmeden) kabul eder.
        ("PermissionRequest", Decision::Allow) if plan => {
            Some(permission(json!({ "behavior": "allow", "updatedInput": payload.get("tool_input")?.as_object()? })))
        }
        ("PermissionRequest", Decision::Allow) => Some(permission(json!({ "behavior": "allow" }))),
        // Kural, aracın kendi önerisinden (özgün, kırpılmamış hâliyle) alınır; ada yalnız sırasını
        // söyler. Sıra köprünün seçtiğiyle uyuşmazsa hiçbir şey yazılmaz: araç kendisi sorar.
        ("PermissionRequest", Decision::AllowAlways(index)) => {
            if always_choice(payload) != Some(*index) {
                return None;
            }
            let entry = payload.get("permission_suggestions")?.get(*index)?.clone();
            let mut decision = json!({ "behavior": "allow", "updatedPermissions": [entry] });
            if plan {
                decision["updatedInput"] = Value::Object(payload.get("tool_input")?.as_object()?.clone());
            }
            Some(permission(decision))
        }
        ("PermissionRequest", Decision::Deny) if plan => Some(permission(json!({
            "behavior": "deny",
            "message": "Kullanıcı planı Zodyvel adasından onaylamadı. Plan modunda kal; neyin değişmesi gerektiğini kullanıcıya sor."
        }))),
        ("PermissionRequest", Decision::Deny) => {
            Some(permission(json!({ "behavior": "deny", "message": "Kullanıcı bu işlemi Zodyvel adasından reddetti." })))
        }
        ("PermissionRequest", Decision::Revise(note)) => {
            let what = if plan { "planı onaylamadı ve şunu istedi" } else { "bu işlemi reddetti ve şunu yazdı" };
            Some(permission(json!({ "behavior": "deny", "message": format!("Kullanıcı Zodyvel adasından {what}: {note}") })))
        }
        // Tur bitmez: Claude gerekçeyi okur ve devam eder.
        ("Stop", Decision::Note(note)) => Some(
            json!({ "decision": "block", "reason": format!("Kullanıcı Zodyvel adasından bir sonraki iş için not yazdı. {}", echo(note)) })
                .to_string(),
        ),
        // Bu çağrı yapılmaz; Claude notu gerekçe olarak görür ve ona göre devam eder.
        ("PreToolUse", Decision::Note(note)) if payload.get("tool_name").and_then(Value::as_str) != Some(ASK_TOOL) => Some(
            json!({ "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": format!("Kullanıcı Zodyvel adasından araya girdi; bu araç çağrısı yapılmadı. {}", echo(note))
            } })
            .to_string(),
        ),
        ("PreToolUse", Decision::Answer(answers)) => {
            let mut input = payload.get("tool_input")?.as_object()?.clone();
            let asked: Vec<&str> = input.get("questions")?.as_array()?.iter().filter_map(|q| q.get("question")?.as_str()).collect();
            let clean: Map<String, Value> = asked
                .iter()
                .filter_map(|q| answers.get(*q).and_then(Value::as_str).filter(|a| !a.trim().is_empty()).map(|a| ((*q).to_owned(), json!(a))))
                .collect();
            if asked.is_empty() || clean.len() != asked.len() {
                return None;
            }
            input.insert("answers".to_owned(), Value::Object(clean));
            Some(
                json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": input } })
                    .to_string(),
            )
        }
        _ => None,
    }
}

fn note_text(note: &str) -> String {
    note.trim().chars().take(MAX_NOTE).collect()
}

fn permission(decision: Value) -> String {
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_name_is_safe() {
        assert_eq!(pipe_name("Ömer A."), r"\\.\pipe\zodyvel-hooks-v1-_mer_A_");
    }

    #[test]
    fn waits_only_for_approvals_and_questions_in_interactive_mode() {
        assert!(wants_reply(Mode::Interactive, "PermissionRequest", "Bash"));
        assert!(!wants_reply(Mode::Observe, "PermissionRequest", "Bash"));
        assert!(!wants_reply(Mode::Interactive, "PreToolUse", "Bash"));
        assert!(wants_reply(Mode::Interactive, "PreToolUse", ASK_TOOL));
        assert!(!wants_reply(Mode::Observe, "PreToolUse", ASK_TOOL));
        // Soru adadan cevaplanmadıysa onay isteği araçta kalır.
        assert!(!wants_reply(Mode::Interactive, "PermissionRequest", ASK_TOOL));
    }

    #[test]
    fn slim_drops_prompt_and_transcript_keeps_output_tail() {
        let payload = json!({
            "hook_event_name": "PostToolUse",
            "session_id": "s1",
            "transcript_path": "C:/secret.jsonl",
            "prompt": "gizli istem",
            "tool_response": { "stdout": format!("baş{}son", "y".repeat(3000)), "interrupted": false },
            "tool_input": { "command": "x".repeat(5000) }
        });
        let out = slim(&payload);
        assert!(out.get("transcript_path").is_none());
        assert!(out.get("prompt").is_none());
        assert!(out.get("tool_response").is_none());
        let command = out["tool_input"]["command"].as_str().unwrap();
        assert_eq!(command.chars().count(), MAX_TEXT + 1);
        let output = out["tool_output"].as_str().unwrap();
        assert!(output.starts_with('…') && output.ends_with("son"));
        assert_eq!(output.chars().count(), MAX_OUTPUT + 1);
        assert!(slim(&json!({ "tool_response": { "stdout": "" } })).get("tool_output").is_none());
    }

    #[test]
    fn envelope_is_a_single_line() {
        let mut extra = Map::new();
        extra.insert("window".to_owned(), json!(42));
        let line = envelope(Provider::Codex, Mode::Interactive, &json!({ "hook_event_name": "Stop", "message": "a\nb" }), extra);
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["provider"], "codex");
        assert_eq!(value["payload"]["message"], "a\nb");
        assert_eq!(value["payload"]["window"], 42);
    }

    #[test]
    fn reply_round_trip() {
        assert_eq!(parse_reply(&reply(Some(&Decision::Allow))), Some(Decision::Allow));
        assert_eq!(parse_reply(&reply(Some(&Decision::Deny))), Some(Decision::Deny));
        let answers = json!({ "Hangisi?": "A" }).as_object().cloned().unwrap();
        assert_eq!(parse_reply(&reply(Some(&Decision::Answer(answers.clone())))), Some(Decision::Answer(answers)));
        assert_eq!(parse_reply(&reply(Some(&Decision::AllowAlways(2)))), Some(Decision::AllowAlways(2)));
        assert_eq!(parse_reply(&reply(None)), None);
        assert_eq!(parse_reply("bozuk"), None);
    }

    fn output(event: &str, decision: &Decision, payload: &Value) -> Value {
        serde_json::from_str(&decision_output(event, decision, payload).unwrap()).unwrap()
    }

    #[test]
    fn decision_output_matches_provider_schema() {
        let allow = output("PermissionRequest", &Decision::Allow, &json!({}));
        assert_eq!(allow["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let deny = output("PermissionRequest", &Decision::Deny, &json!({}));
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(deny["hookSpecificOutput"]["decision"]["message"].is_string());
        assert!(decision_output("PreToolUse", &Decision::Allow, &json!({})).is_none());
    }

    #[test]
    fn answers_echo_original_questions() {
        let questions = json!([
            { "question": "Çentik?", "header": "Çentik", "options": [{ "label": "İnce" }, { "label": "Küçük" }], "multiSelect": false },
            { "question": "Ayrıntı?", "header": "Ayrıntı", "options": [{ "label": "Adımlar" }], "multiSelect": true }
        ]);
        let payload = json!({ "tool_name": ASK_TOOL, "tool_input": { "questions": questions } });
        let answers = |v: Value| Decision::Answer(v.as_object().cloned().unwrap());

        let out = output("PreToolUse", &answers(json!({ "Çentik?": "İnce", "Ayrıntı?": "Adımlar", "uydurma": "x" })), &payload);
        let hso = &out["hookSpecificOutput"];
        assert_eq!(hso["hookEventName"], "PreToolUse");
        assert_eq!(hso["permissionDecision"], "allow");
        assert_eq!(hso["updatedInput"]["questions"], questions);
        assert_eq!(hso["updatedInput"]["answers"], json!({ "Çentik?": "İnce", "Ayrıntı?": "Adımlar" }));

        // Eksik ya da boş cevapta hiçbir şey yazılmaz: araç kendi penceresinde sorar.
        assert!(decision_output("PreToolUse", &answers(json!({ "Çentik?": "İnce" })), &payload).is_none());
        assert!(decision_output("PreToolUse", &answers(json!({ "Çentik?": "İnce", "Ayrıntı?": " " })), &payload).is_none());
        assert!(decision_output("PreToolUse", &answers(json!({ "Çentik?": "İnce" })), &json!({})).is_none());
    }

    fn suggestions() -> Value {
        json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "permission_suggestions": [
                { "type": "setMode", "mode": "bypassPermissions", "destination": "session" },
                { "type": "addRules", "rules": [{ "toolName": "Bash", "ruleContent": "npm test:*" }], "behavior": "allow", "destination": "localSettings" }
            ]
        })
    }

    #[test]
    fn always_allow_echoes_only_a_safe_suggestion() {
        let payload = suggestions();
        assert_eq!(always_choice(&payload), Some(1), "izinleri atlama kipi hiçbir zaman seçilmez");
        let out = output("PermissionRequest", &Decision::AllowAlways(1), &payload);
        let decision = &out["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        assert_eq!(decision["updatedPermissions"], json!([payload["permission_suggestions"][1]]));
        // Yanlış sıra ya da önerisiz olay: hiçbir şey yazılmaz.
        assert!(decision_output("PermissionRequest", &Decision::AllowAlways(0), &payload).is_none());
        assert!(decision_output("PermissionRequest", &Decision::AllowAlways(0), &json!({})).is_none());
        let only_bypass = json!({ "permission_suggestions": [{ "type": "setMode", "mode": "bypassPermissions" }] });
        assert_eq!(always_choice(&only_bypass), None);
        assert_eq!(always_choice(&json!({ "permission_suggestions": [{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }] })), Some(0));
    }

    #[test]
    fn notes_are_asked_only_from_interactive_claude() {
        assert!(wants_note(Provider::Claude, Mode::Interactive, "Stop", ""));
        assert!(wants_note(Provider::Claude, Mode::Interactive, "PreToolUse", "Bash"));
        assert!(!wants_note(Provider::Claude, Mode::Interactive, "PreToolUse", ASK_TOOL));
        assert!(!wants_note(Provider::Claude, Mode::Observe, "Stop", ""));
        assert!(!wants_note(Provider::Codex, Mode::Interactive, "Stop", ""));
        assert!(!wants_note(Provider::Claude, Mode::Interactive, "PostToolUse", "Bash"));
    }

    #[test]
    fn note_round_trip_and_outputs() {
        let note = Decision::Note("README'ye bir satır ekle".into());
        assert_eq!(parse_reply(&reply(Some(&note))), Some(note.clone()));
        assert_eq!(parse_reply(r#"{"decision":"note","message":"  "}"#), None);
        let stop = output("Stop", &note, &json!({ "hook_event_name": "Stop" }));
        assert_eq!(stop["decision"], "block");
        assert!(stop["reason"].as_str().unwrap().ends_with("README'ye bir satır ekle"));
        let pre = output("PreToolUse", &note, &json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }));
        assert_eq!(pre["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(pre["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap().contains("README"));
        // Not sohbette görünsün diye Claude'dan alıntıyla başlaması istenir.
        assert!(stop["reason"].as_str().unwrap().contains("\n\n> **Zodyvel adasından:** README'ye bir satır ekle"));
        assert!(pre["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap().contains("> **Zodyvel adasından:** README"));
        // Soru aracı notla reddedilmez; onay olaylarında not anlamsız.
        assert_eq!(decision_output("PreToolUse", &note, &json!({ "tool_name": ASK_TOOL })), None);
        assert_eq!(decision_output("PermissionRequest", &note, &json!({ "tool_name": "Bash" })), None);
    }

    #[test]
    fn idle_task_is_echoed_as_quote() {
        let message = task_message("  Testleri çalıştır\nsonra   \n\nözetle  ");
        assert!(message.starts_with("Kullanıcı Zodyvel adasından yeni bir istek gönderdi"));
        assert!(message.ends_with("\n\n> **Zodyvel adasından:** Testleri çalıştır\n> sonra\n> \n> özetle"));
        assert!(task_message("").ends_with("> **Zodyvel adasından:** "));
    }

    #[test]
    fn finished_edits_carry_line_counts() {
        let post = json!({ "hook_event_name": "PostToolUse", "tool_name": "Write", "tool_input": { "file_path": "b.md", "content": "a\nb\nc" } });
        assert_eq!(extras(&post)["diff_stat"], json!([{ "path": "b.md", "added": 3, "removed": 0 }]));
        let edit = json!({ "hook_event_name": "PostToolUse", "tool_name": "Edit", "tool_input": { "file_path": "a.ts", "old_string": "x", "new_string": "y\nz" } });
        assert_eq!(extras(&edit)["diff_stat"], json!([{ "path": "a.ts", "added": 2, "removed": 1 }]));
        assert!(extras(&json!({ "hook_event_name": "PostToolUse", "tool_name": "Read", "tool_input": {} })).is_empty());
    }

    #[test]
    fn extras_count_lines_only_for_permission_requests() {
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "a.ts", "old_string": "x", "new_string": "y\nz" } });
        let mut request = edit.clone();
        request["hook_event_name"] = json!("PermissionRequest");
        assert_eq!(extras(&request)["diff_stat"], json!([{ "path": "a.ts", "added": 2, "removed": 1 }]));
        let mut pre = edit;
        pre["hook_event_name"] = json!("PreToolUse");
        assert!(extras(&pre).is_empty());
    }

    #[test]
    fn window_is_looked_up_only_where_a_card_can_appear() {
        assert!(wants_window("PermissionRequest", "Bash"));
        assert!(wants_window("PreToolUse", CODEX_ASK_TOOL));
        assert!(!wants_window("PreToolUse", "Bash"));
        assert!(!wants_window("PostToolUse", "Edit"));
    }

    fn plan_request() -> Value {
        json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": PLAN_TOOL,
            "tool_input": { "plan": "# Plan\n\n1. Oku\n2. Yaz", "planFilePath": "C:/Users/u/.claude/plans/x.md" },
            "permission_suggestions": [{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }]
        })
    }

    #[test]
    fn plan_waits_longer_and_travels_whole() {
        assert_eq!(decision_timeout(PLAN_TOOL), PLAN_TIMEOUT_SECS);
        assert_eq!(decision_timeout("Bash"), DECISION_TIMEOUT_SECS);
        assert!(wait_limit("PermissionRequest", PLAN_TOOL) > PLAN_TIMEOUT_SECS);
        assert_eq!(wait_limit("PermissionRequest", "Bash"), WAIT_LIMIT_SECS);
        assert_eq!(wait_limit("PreToolUse", ASK_TOOL), WAIT_LIMIT_SECS);
        let mut long = plan_request();
        long["tool_input"]["plan"] = json!("ş".repeat(MAX_PLAN + 50));
        let plan = extras(&long)["plan"].as_str().unwrap().to_owned();
        assert!(plan.chars().count() <= MAX_PLAN + 3 && plan.ends_with('…'));
        // Kırpılmış olayda plan 1200 karakterde kesilir; tam metin ekte gider.
        let line = envelope(Provider::Claude, Mode::Interactive, &long, extras(&long));
        let value: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["payload"]["plan"].as_str().unwrap().chars().count(), plan.chars().count());
        assert!(extras(&json!({ "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "plan": "x" } })).get("plan").is_none());
    }

    #[test]
    fn plan_approval_echoes_the_input_and_rejection_keeps_planning() {
        let payload = plan_request();
        let allow = output("PermissionRequest", &Decision::Allow, &payload);
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(allow["hookSpecificOutput"]["decision"]["updatedInput"], payload["tool_input"], "araç girdisiz onayı yok sayar");
        let always = output("PermissionRequest", &Decision::AllowAlways(0), &payload);
        assert_eq!(always["hookSpecificOutput"]["decision"]["updatedPermissions"], payload["permission_suggestions"]);
        assert_eq!(always["hookSpecificOutput"]["decision"]["updatedInput"], payload["tool_input"]);
        let deny = output("PermissionRequest", &Decision::Deny, &payload);
        assert!(deny["hookSpecificOutput"]["decision"]["message"].as_str().unwrap().contains("Plan modunda kal"));
        let revise = output("PermissionRequest", &Decision::Revise("Testleri de ekle".into()), &payload);
        assert_eq!(revise["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert!(revise["hookSpecificOutput"]["decision"]["message"].as_str().unwrap().ends_with(": Testleri de ekle"));
        // Başka araçlarda onay girdiye dokunmaz.
        assert!(output("PermissionRequest", &Decision::Allow, &json!({ "tool_name": "Bash" }))["hookSpecificOutput"]["decision"].get("updatedInput").is_none());
    }

    #[test]
    fn revise_round_trip_and_blank_note_is_plain_deny() {
        let note = Decision::Revise("Önce testleri yaz".into());
        assert_eq!(parse_reply(&reply(Some(&note))), Some(note));
        assert_eq!(parse_reply(r#"{"decision":"deny","message":"  "}"#), Some(Decision::Deny));
    }
}
