//! Claude kullanım özeti: Claude Code'un yerel konuşma kayıtlarından (`~/.claude/projects/**/*.jsonl`)
//! token, tahmini API maliyeti ve haftalık çalışma özeti. Saf kısım (dosya okuma std ile); Tauri yok.
//!
//! - Yalnız istenince okunur (panel açıkken). Her dosyanın okunan yeri tutulur; sonraki okumada yalnız
//!   eklenen satırlar işlenir. Pencerenin (14 gün) dışındaki dosyalar hiç açılmaz.
//! - Satırların çoğu (araç çıktıları, dosya içerikleri) ayrıştırılmaz: önce metin içinde aranır.
//! - Fiyatlar milyon token başına USD (API liste fiyatı); bilinmeyen modelin maliyeti gösterilmez.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Aynı oturumda iki mesaj arası bundan uzunsa ara verilmiş sayılır (aktif süreye eklenmez).
const IDLE_GAP_MS: u64 = 5 * 60_000;
/// Proje ve model listelerinde en çok bu kadar satır.
const TOP: usize = 5;

// ---- Fiyatlar ----

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
}

/// Model kimliğini aile anahtarına indirger: `claude-opus-4-5-20251101` → `opus-4-5`,
/// `claude-3-5-haiku-20241022` → `haiku-3-5`, `claude-sonnet-4-20250514` → `sonnet-4-0`.
pub fn model_key(model: &str) -> Option<String> {
    let lower = model.trim().to_ascii_lowercase();
    let name = lower.split(['[', '@', ':']).next()?.trim();
    let name = name.strip_prefix("anthropic.").unwrap_or(name);
    let name = name.strip_prefix("claude-")?;
    let parts: Vec<&str> = name.split('-').filter(|p| !p.is_empty()).collect();
    let family = parts.iter().position(|p| matches!(*p, "opus" | "sonnet" | "haiku" | "fable" | "mythos"))?;
    // Sürüm: aileden önceki (eski adlandırma, claude-3-5-haiku) ya da sonraki kısa sayılar; tarih (8 hane) ve
    // "v1" gibi ekler atlanır.
    let numeric = |p: &&str| p.len() <= 2 && p.chars().all(|c| c.is_ascii_digit());
    let mut version: Vec<&str> = parts[..family].iter().copied().filter(numeric).collect();
    if version.is_empty() {
        version = parts[family + 1..].iter().copied().take_while(numeric).collect();
    }
    if version.is_empty() {
        return None;
    }
    if version.len() == 1 {
        version.push("0");
    }
    Some(format!("{}-{}", parts[family], version.join("-")))
}

/// Milyon token başına fiyat. `prompt`: bu istekteki toplam girdi (önbellek dahil); kademeli fiyatlı modeller için.
pub fn price(key: &str, prompt: u64) -> Option<Price> {
    let p = |input: f64, output: f64, cache_read: f64| Some(Price { input, output, cache_read });
    match key {
        "fable-5-1" | "mythos-5-1" => p(10.0, 50.0, 0.25),
        "fable-5-0" | "mythos-5-0" => p(10.0, 50.0, 1.0),
        "opus-5-5" => p(4.0, 20.0, 0.2),
        "opus-5-0" | "opus-4-8" | "opus-4-7" | "opus-4-6" | "opus-4-5" => p(5.0, 25.0, 0.5),
        "opus-4-1" | "opus-4-0" | "opus-3-0" => p(15.0, 75.0, 1.5),
        "sonnet-5-5" | "sonnet-5-0" => p(2.0, 10.0, 0.2),
        "sonnet-4-6" | "sonnet-4-5" | "sonnet-4-0" | "sonnet-3-7" => p(3.0, 15.0, 0.3),
        "haiku-5-5" if prompt <= 100_000 => p(0.1, 0.5, 0.01),
        "haiku-5-5" => p(0.5, 2.5, 0.05),
        "haiku-4-5" => p(1.0, 5.0, 0.1),
        "haiku-3-5" => p(0.8, 4.0, 0.08),
        _ => None,
    }
}

/// `opus-4-5` → `Opus 4.5`, `sonnet-4-0` → `Sonnet 4`.
pub fn model_label(key: &str) -> String {
    let mut parts = key.split('-');
    let family = parts.next().unwrap_or_default();
    let mut version: Vec<&str> = parts.collect();
    if version.len() > 1 && version.last() == Some(&"0") {
        version.pop();
    }
    let mut name: String = family.chars().take(1).flat_map(char::to_uppercase).collect();
    name.push_str(&family[family.chars().next().map_or(0, char::len_utf8)..]);
    format!("{name} {}", version.join("."))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_5m: u64,
    pub cache_1h: u64,
    pub cache_read: u64,
    /// Hızlı kip (`usage.speed == "fast"`): fiyat iki katı.
    pub fast: bool,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_5m + self.cache_1h + self.cache_read
    }
}

/// Bir mesajın tahmini API maliyeti (USD). Önbelleğe yazma: 5 dk 1,25×, 1 sa 2× girdi fiyatı.
pub fn cost(key: &str, t: &Tokens) -> Option<f64> {
    let prompt = t.input + t.cache_5m + t.cache_1h + t.cache_read;
    let p = price(key, prompt)?;
    let usd = (t.input as f64 * p.input
        + t.cache_5m as f64 * p.input * 1.25
        + t.cache_1h as f64 * p.input * 2.0
        + t.cache_read as f64 * p.cache_read
        + t.output as f64 * p.output)
        / 1_000_000.0;
    Some(if t.fast { usd * 2.0 } else { usd })
}

// ---- Satırlar ----

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    /// `message.id` + `requestId`: aynı yanıt birden çok satıra (ve devam eden oturumlarda başka dosyalara) yazılır.
    pub key: String,
    pub at: u64,
    pub session: String,
    pub project: String,
    /// Aile anahtarı (`opus-4-5`); tanınmayan modelde ham ad.
    pub model: String,
    pub tokens: Tokens,
    pub cost: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub at: u64,
    pub session: String,
    pub file: String,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, PartialEq)]
pub enum Entry {
    Msg(Msg),
    Edit(Edit),
}

/// Bir kayıt satırı. Yalnız asistan yanıtları (token) ve dosya düzenleme sonuçları (satır sayısı) işlenir.
pub fn parse_line(line: &str) -> Option<Entry> {
    let usage = line.contains("\"usage\"") && line.contains("\"assistant\"");
    let edit = line.contains("\"structuredPatch\"");
    if !usage && !edit {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let at = parse_time(v.get("timestamp")?.as_str()?)?;
    let session = v.get("sessionId").and_then(Value::as_str).unwrap_or_default().to_owned();
    match v.get("type").and_then(Value::as_str)? {
        "assistant" => {
            let message = v.get("message")?;
            let model = message.get("model").and_then(Value::as_str)?;
            if model.starts_with('<') {
                return None; // <synthetic>: aracın kendi yazdığı mesaj, API çağrısı değil.
            }
            let u = message.get("usage")?;
            let n = |o: &Value, k: &str| o.get(k).and_then(Value::as_u64).unwrap_or(0);
            let creation = n(u, "cache_creation_input_tokens");
            let (cache_5m, cache_1h) = match u.get("cache_creation") {
                Some(c) if c.is_object() => {
                    let (m5, h1) = (n(c, "ephemeral_5m_input_tokens"), n(c, "ephemeral_1h_input_tokens"));
                    // Ayrıntı eksikse kalan 5 dakikalık sayılır.
                    (m5 + creation.saturating_sub(m5 + h1), h1)
                }
                _ => (creation, 0),
            };
            let tokens = Tokens {
                input: n(u, "input_tokens"),
                output: n(u, "output_tokens"),
                cache_5m,
                cache_1h,
                cache_read: n(u, "cache_read_input_tokens"),
                fast: u.get("speed").and_then(Value::as_str) == Some("fast"),
            };
            if tokens.total() == 0 {
                return None;
            }
            let key = model_key(model);
            let cost = key.as_deref().and_then(|k| cost(k, &tokens));
            let id = message.get("id").and_then(Value::as_str).unwrap_or_default();
            let request = v.get("requestId").and_then(Value::as_str).unwrap_or_default();
            Some(Entry::Msg(Msg {
                key: if id.is_empty() && request.is_empty() { format!("{session}:{at}") } else { format!("{id}:{request}") },
                at,
                session,
                project: project_name(v.get("cwd").and_then(Value::as_str).unwrap_or_default()),
                model: key.unwrap_or_else(|| model.to_owned()),
                tokens,
                cost,
            }))
        }
        "user" => {
            let result = v.get("toolUseResult")?;
            let file = result.get("filePath").and_then(Value::as_str)?.to_owned();
            let (mut added, mut removed) = (0u32, 0u32);
            for hunk in result.get("structuredPatch").and_then(Value::as_array).into_iter().flatten() {
                for line in hunk.get("lines").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    if line.starts_with('+') {
                        added += 1;
                    } else if line.starts_with('-') {
                        removed += 1;
                    }
                }
            }
            // Yeni dosya: fark boştur, içeriğin tamamı eklenmiştir.
            if added == 0 && removed == 0 && result.get("type").and_then(Value::as_str) == Some("create") {
                added = result.get("content").and_then(Value::as_str).map_or(0, |c| c.lines().count() as u32);
            }
            Some(Entry::Edit(Edit { at, session, file, added, removed }))
        }
        _ => None,
    }
}

fn project_name(cwd: &str) -> String {
    let name = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default();
    if name.is_empty() { "?".into() } else { name.to_owned() }
}

/// `2026-10-15T09:41:05.123Z` → Unix ms. Yalnız UTC (`Z` ya da `+00:00`); kayıtlar böyle yazılır.
pub fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.strip_suffix('Z').or_else(|| s.strip_suffix("+00:00"))?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (Some(Some(y)), Some(Some(mo)), Some(Some(day))) = (d.next(), d.next(), d.next()) else { return None };
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.split(':').map(|p| p.parse::<i64>().ok());
    let (Some(Some(h)), Some(Some(mi)), Some(Some(sec))) = (t.next(), t.next(), t.next()) else { return None };
    let ms: i64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().ok()?;
    let days = days_from_civil(y, mo, day);
    let total = ((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + ms;
    u64::try_from(total).ok()
}

/// 1970-01-01'den bu yana gün (H. Hinnant'ın `days_from_civil`'i).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// ---- Okuma ----

#[derive(Default)]
struct FileLog {
    offset: u64,
    msgs: Vec<Msg>,
    edits: Vec<Edit>,
}

/// Okunmuş kayıtlar; dosya başına kaldığı yer.
#[derive(Default)]
pub struct Ledger {
    files: HashMap<PathBuf, FileLog>,
}

impl Ledger {
    /// `root` altındaki `.jsonl` dosyalarının yeni satırlarını okur. `since`'ten önce değişmiş dosyalar
    /// açılmaz ve unutulur; daha eski satırlar atılır. Okunan dosya sayısını döner.
    pub fn refresh(&mut self, root: &Path, since: u64) -> usize {
        let mut seen = HashSet::new();
        let mut read = 0;
        for path in jsonl_files(root, 4) {
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64);
            if modified < since {
                continue;
            }
            seen.insert(path.clone());
            let log = self.files.entry(path.clone()).or_default();
            if meta.len() < log.offset {
                *log = FileLog::default(); // Dosya baştan yazılmış.
            }
            if meta.len() == log.offset {
                continue;
            }
            if feed(&path, log).is_ok() {
                read += 1;
            }
        }
        self.files.retain(|path, _| seen.contains(path));
        for log in self.files.values_mut() {
            log.msgs.retain(|m| m.at >= since);
            log.edits.retain(|e| e.at >= since);
        }
        read
    }

    /// Kayıtları doğrudan ekler (testler için).
    #[cfg(test)]
    pub fn push(&mut self, file: &str, entry: Entry) {
        let log = self.files.entry(PathBuf::from(file)).or_default();
        match entry {
            Entry::Msg(m) => log.msgs.push(m),
            Entry::Edit(e) => log.edits.push(e),
        }
    }

    /// Aynı yanıtın kopyaları tekilleştirilmiş mesajlar (en büyük çıktı sayısı kalır) ve düzenlemeler.
    fn entries(&self) -> (Vec<&Msg>, Vec<&Edit>) {
        let mut unique: HashMap<&str, &Msg> = HashMap::new();
        for m in self.files.values().flat_map(|l| &l.msgs) {
            unique.entry(&m.key).and_modify(|best| if m.tokens.output > best.tokens.output { *best = m }).or_insert(m);
        }
        let mut msgs: Vec<&Msg> = unique.into_values().collect();
        msgs.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
        (msgs, self.files.values().flat_map(|l| &l.edits).collect())
    }

    pub fn summary(&self, today: Span, week: Span, last_week: Span, days: &[Span]) -> Summary {
        let (msgs, edits) = self.entries();
        // Oturumun projesi başladığı klasördür (oturum içinde `cd` ile klasör değişse de).
        let mut projects: HashMap<&str, &str> = HashMap::new();
        for m in &msgs {
            projects.entry(&m.session).or_insert(&m.project);
        }
        let totals = |span: Span| totals(&msgs, &edits, &projects, span);
        Summary {
            today: totals(today),
            week: totals(week),
            last_week: totals(last_week),
            days: days
                .iter()
                .map(|&span| {
                    let t = totals(span);
                    Day { cost: t.cost, tokens: t.tokens, active_ms: t.active_ms, sessions: t.sessions }
                })
                .collect(),
            files_read: self.files.len() as u32,
        }
    }
}

fn feed(path: &Path, log: &mut FileLog) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(log.offset))?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader.read_until(b'\n', &mut buf)?;
        // Yarım satır (araç hâlâ yazıyor): bir dahaki okumaya kalır.
        if n == 0 || buf.last() != Some(&b'\n') {
            return Ok(());
        }
        log.offset += n as u64;
        let Ok(line) = std::str::from_utf8(&buf) else { continue };
        match parse_line(line) {
            Some(Entry::Msg(m)) => log.msgs.push(m),
            Some(Entry::Edit(e)) => log.edits.push(e),
            None => {}
        }
    }
}

fn jsonl_files(root: &Path, depth: u32) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return out };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() && depth > 0 {
            out.extend(jsonl_files(&path, depth - 1));
        } else if kind.is_file() && path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
    out
}

// ---- Özet ----

/// `[from, to)` Unix ms; yerel gün sınırlarını arayüz hesaplar (yaz saati dahil).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Span {
    pub from: u64,
    pub to: u64,
}

impl Span {
    fn has(&self, at: u64) -> bool {
        at >= self.from && at < self.to
    }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub name: String,
    pub cost: f64,
    pub tokens: u64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    /// Girdi (önbellek yazma ve okuma dahil) ve çıktı token'ları.
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub tokens: u64,
    /// Fiyatı bilinen mesajların toplamı (USD).
    pub cost: f64,
    /// Fiyatı bilinmeyen modellerin token'ları (maliyete katılmadı).
    pub unpriced: u64,
    pub messages: u32,
    pub sessions: u32,
    pub active_ms: u64,
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    pub projects: Vec<Part>,
    pub models: Vec<Part>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub cost: f64,
    pub tokens: u64,
    pub active_ms: u64,
    pub sessions: u32,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub today: Totals,
    pub week: Totals,
    pub last_week: Totals,
    pub days: Vec<Day>,
    pub files_read: u32,
}

fn totals(msgs: &[&Msg], edits: &[&Edit], session_project: &HashMap<&str, &str>, span: Span) -> Totals {
    let mut t = Totals::default();
    let mut projects: HashMap<&str, Part> = HashMap::new();
    let mut models: HashMap<&str, Part> = HashMap::new();
    let mut last: HashMap<&str, u64> = HashMap::new();
    for m in msgs.iter().filter(|m| span.has(m.at)) {
        let k = &m.tokens;
        t.input += k.input + k.cache_5m + k.cache_1h + k.cache_read;
        t.output += k.output;
        t.cache_read += k.cache_read;
        t.tokens += k.total();
        t.messages += 1;
        match m.cost {
            Some(c) => t.cost += c,
            None => t.unpriced += k.total(),
        }
        let project = session_project.get(m.session.as_str()).copied().unwrap_or(&m.project);
        for (map, name) in [(&mut projects, project), (&mut models, m.model.as_str())] {
            let part = map.entry(name).or_insert_with(|| Part { name: name.to_owned(), ..Part::default() });
            part.cost += m.cost.unwrap_or(0.0);
            part.tokens += k.total();
        }
        // Mesajlar zamana göre sıralı: oturumdaki bir önceki mesajdan bu yana geçen süre (ara değilse).
        if let Some(prev) = last.insert(&m.session, m.at) {
            let gap = m.at.saturating_sub(prev);
            if gap <= IDLE_GAP_MS {
                t.active_ms += gap;
            }
        }
    }
    t.sessions = last.len() as u32;
    let mut files = HashSet::new();
    for e in edits.iter().filter(|e| span.has(e.at)) {
        files.insert(e.file.as_str());
        t.added += e.added;
        t.removed += e.removed;
    }
    t.files = files.len() as u32;
    t.projects = top(projects);
    t.models = top(models).into_iter().map(|p| Part { name: label_of(&p.name), ..p }).collect();
    t
}

fn label_of(model: &str) -> String {
    if price(model, 0).is_some() { model_label(model) } else { model.to_owned() }
}

fn top(map: HashMap<&str, Part>) -> Vec<Part> {
    let mut parts: Vec<Part> = map.into_values().collect();
    parts.sort_by(|a, b| b.cost.total_cmp(&a.cost).then(b.tokens.cmp(&a.tokens)).then_with(|| a.name.cmp(&b.name)));
    parts.truncate(TOP);
    parts
}

// ---- Tek oturumun bağlamı ----

/// Standart bağlam penceresi (token); bunu aşan oturum 1M pencerelidir.
const CONTEXT: u64 = 200_000;
const CONTEXT_LARGE: u64 = 1_000_000;

/// AI ayrıntısında oturumun modeli ve bağlam doluluğu. Kaynak: oturumun kendi kaydının son ana ajan
/// yanıtı (alt ajan satırları atlanır). Pencere kayıtta yazmaz: 200 bin token'ı aşan oturum 1M sayılır,
/// bu yüzden yüzde bir tahmindir.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub model: Option<String>,
    pub tokens: u64,
    pub window: u64,
}

/// Kaydın son kısmından (satırlar sırayla) bağlam; uygun yanıt yoksa `None`.
pub fn session_context(tail: &str) -> Option<SessionContext> {
    tail.lines().rev().find_map(|line| {
        if !line.contains("\"usage\"") || !line.contains("\"assistant\"") {
            return None;
        }
        let v: Value = serde_json::from_str(line).ok()?;
        if v.get("type").and_then(Value::as_str) != Some("assistant") || v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        let message = v.get("message")?;
        let model = message.get("model").and_then(Value::as_str)?;
        if model.starts_with('<') {
            return None;
        }
        let u = message.get("usage")?;
        let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
        let tokens = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
        if tokens == 0 {
            return None;
        }
        let window = if tokens > CONTEXT || model.contains("[1m]") { CONTEXT_LARGE } else { CONTEXT };
        Some(SessionContext { model: model_key(model).map(|k| model_label(&k)), tokens, window })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(id: &str, at: &str, session: &str, cwd: &str, model: &str, usage: &str) -> String {
        format!(
            r#"{{"cwd":"{cwd}","sessionId":"{session}","message":{{"id":"{id}","model":"{model}","role":"assistant","content":[],"usage":{usage}}},"requestId":"req_{id}","type":"assistant","timestamp":"{at}"}}"#
        )
    }

    #[test]
    fn session_context_reads_the_last_main_reply() {
        let main = assistant("m1", "2026-10-15T09:00:00Z", "s1", "C:/w/api", "claude-opus-4-5-20251101", r#"{"input_tokens":10,"cache_read_input_tokens":120000,"cache_creation_input_tokens":5000,"output_tokens":300}"#);
        let side = assistant("m2", "2026-10-15T09:01:00Z", "s1", "C:/w/api", "claude-haiku-4-5", r#"{"input_tokens":5,"output_tokens":3}"#)
            .replacen('{', r#"{"isSidechain":true,"#, 1);
        let tail = format!("{main}\n{side}\n{{\"type\":\"user\"}}\n");
        let context = session_context(&tail).unwrap();
        assert_eq!(context, SessionContext { model: Some("Opus 4.5".into()), tokens: 125_010, window: 200_000 });
        let big = assistant("m3", "2026-10-15T09:02:00Z", "s1", "C:/w/api", "claude-sonnet-4-5", r#"{"input_tokens":1,"cache_read_input_tokens":350000}"#);
        assert_eq!(session_context(&big).unwrap().window, 1_000_000);
        assert_eq!(session_context("{\"type\":\"user\"}"), None);
    }

    #[test]
    fn model_ids_map_to_families() {
        assert_eq!(model_key("claude-opus-4-5-20251101").as_deref(), Some("opus-4-5"));
        assert_eq!(model_key("claude-sonnet-4-20250514").as_deref(), Some("sonnet-4-0"));
        assert_eq!(model_key("claude-3-5-haiku-20241022").as_deref(), Some("haiku-3-5"));
        assert_eq!(model_key("claude-3-7-sonnet-20250219").as_deref(), Some("sonnet-3-7"));
        assert_eq!(model_key("claude-opus-5-5").as_deref(), Some("opus-5-5"));
        assert_eq!(model_key("claude-opus-5[1m]").as_deref(), Some("opus-5-0"));
        assert_eq!(model_key("anthropic.claude-haiku-4-5-20251001-v1:0").as_deref(), Some("haiku-4-5"));
        assert_eq!(model_key("gpt-5"), None);
        assert_eq!(model_label("opus-4-5"), "Opus 4.5");
        assert_eq!(model_label("sonnet-4-0"), "Sonnet 4");
        assert_eq!(model_label("fable-5-1"), "Fable 5.1");
    }

    #[test]
    fn cost_uses_cache_multipliers_tiers_and_fast_mode() {
        let t = Tokens { input: 1_000_000, output: 1_000_000, cache_5m: 1_000_000, cache_1h: 1_000_000, cache_read: 1_000_000, fast: false };
        // Opus 4.5: 5 + 25 + 6.25 + 10 + 0.5
        assert!((cost("opus-4-5", &t).unwrap() - 46.75).abs() < 1e-9);
        assert!((cost("opus-4-5", &Tokens { fast: true, ..t }).unwrap() - 93.5).abs() < 1e-9);
        let small = Tokens { input: 50_000, output: 10_000, ..Tokens::default() };
        assert!((cost("haiku-5-5", &small).unwrap() - (0.005 + 0.005)).abs() < 1e-12);
        let big = Tokens { input: 200_000, ..Tokens::default() };
        assert!((cost("haiku-5-5", &big).unwrap() - 0.1).abs() < 1e-12);
        assert_eq!(cost("gpt-5", &t), None);
    }

    #[test]
    fn lines_are_parsed_and_noise_is_skipped() {
        let usage = r#"{"input_tokens":10,"output_tokens":200,"cache_creation_input_tokens":1000,"cache_read_input_tokens":5000,"cache_creation":{"ephemeral_5m_input_tokens":400,"ephemeral_1h_input_tokens":600}}"#;
        let line = assistant("msg_1", "2026-10-15T09:00:00.500Z", "s1", r"C:\\work\\Psrc", "claude-opus-4-5-20251101", usage);
        let Some(Entry::Msg(m)) = parse_line(&line) else { panic!("mesaj bekleniyordu") };
        assert_eq!((m.project.as_str(), m.model.as_str(), m.key.as_str()), ("Psrc", "opus-4-5", "msg_1:req_msg_1"));
        assert_eq!((m.tokens.cache_5m, m.tokens.cache_1h, m.tokens.cache_read), (400, 600, 5000));
        assert_eq!(m.at, parse_time("2026-10-15T09:00:00.5Z").unwrap());
        assert!(m.cost.unwrap() > 0.0);
        // Ayrıntısız önbellek yazması 5 dakikalık sayılır.
        let plain = assistant("m", "2026-10-15T09:00:00Z", "s", "/a/b", "claude-sonnet-4-5", r#"{"input_tokens":1,"output_tokens":1,"cache_creation_input_tokens":7}"#);
        let Some(Entry::Msg(p)) = parse_line(&plain) else { panic!() };
        assert_eq!((p.tokens.cache_5m, p.project.as_str()), (7, "b"));
        assert_eq!(parse_line(&assistant("x", "2026-10-15T09:00:00Z", "s", "/a", "<synthetic>", r#"{"input_tokens":0,"output_tokens":0}"#)), None);
        assert_eq!(parse_line(r#"{"type":"user","message":{"content":"usage nedir"}}"#), None);
        assert_eq!(parse_line("bozuk {\"usage\" assistant"), None);
    }

    #[test]
    fn edits_count_patch_lines_and_new_files() {
        let edit = r#"{"type":"user","sessionId":"s1","timestamp":"2026-10-15T09:01:00Z","toolUseResult":{"filePath":"C:\\p\\a.rs","structuredPatch":[{"lines":[" x","-y","+z","+w"]}]}}"#;
        assert_eq!(
            parse_line(edit),
            Some(Entry::Edit(Edit { at: parse_time("2026-10-15T09:01:00Z").unwrap(), session: "s1".into(), file: "C:\\p\\a.rs".into(), added: 2, removed: 1 }))
        );
        let create = r#"{"type":"user","sessionId":"s1","timestamp":"2026-10-15T09:02:00Z","toolUseResult":{"type":"create","filePath":"b.md","content":"a\nb\nc","structuredPatch":[]}}"#;
        let Some(Entry::Edit(e)) = parse_line(create) else { panic!() };
        assert_eq!((e.added, e.removed), (3, 0));
    }

    #[test]
    fn summary_dedupes_and_counts_active_time_sessions_and_files() {
        let mut ledger = Ledger::default();
        let at = |s: &str| parse_time(s).unwrap();
        let msg = |key: &str, t: &str, session: &str, project: &str, output: u64| {
            Entry::Msg(Msg {
                key: key.into(),
                at: at(t),
                session: session.into(),
                project: project.into(),
                model: "opus-4-5".into(),
                tokens: Tokens { input: 1000, output, ..Tokens::default() },
                cost: cost("opus-4-5", &Tokens { input: 1000, output, ..Tokens::default() }),
            })
        };
        ledger.push("a.jsonl", msg("k1", "2026-10-15T09:00:00Z", "s1", "Psrc", 100));
        // Aynı yanıtın sonraki satırı (daha çok çıktı) ve başka dosyadaki kopyası: bir kez sayılır.
        ledger.push("a.jsonl", msg("k1", "2026-10-15T09:00:01Z", "s1", "Psrc", 300));
        ledger.push("b.jsonl", msg("k1", "2026-10-15T09:00:01Z", "s1", "Psrc", 300));
        ledger.push("a.jsonl", msg("k2", "2026-10-15T09:03:00Z", "s1", "Psrc", 100));
        // 30 dk ara: aktif süreye eklenmez.
        // Oturum içinde başka klasöre geçilse de proje oturumun başladığı klasördür.
        ledger.push("a.jsonl", msg("k3", "2026-10-15T09:33:00Z", "s1", "src-tauri", 100));
        ledger.push("c.jsonl", msg("k4", "2026-10-15T10:00:00Z", "s2", "Site", 100));
        ledger.push("a.jsonl", Entry::Edit(Edit { at: at("2026-10-15T09:01:00Z"), session: "s1".into(), file: "x.rs".into(), added: 3, removed: 1 }));
        ledger.push("a.jsonl", Entry::Edit(Edit { at: at("2026-10-15T09:02:00Z"), session: "s1".into(), file: "x.rs".into(), added: 1, removed: 0 }));
        ledger.push("a.jsonl", Entry::Edit(Edit { at: at("2026-10-14T09:02:00Z"), session: "s1".into(), file: "old.rs".into(), added: 9, removed: 9 }));

        let day = Span { from: at("2026-10-15T00:00:00Z"), to: at("2026-10-16T00:00:00Z") };
        let empty = Span { from: 0, to: 1 };
        let s = ledger.summary(day, day, empty, &[day]);
        let t = &s.today;
        assert_eq!((t.messages, t.sessions, t.output), (4, 2, 600));
        // k1 son satırı (09:00:01) → k2 (09:03:00): 179 sn; k3 30 dk sonra: sayılmaz.
        assert_eq!(t.active_ms, 179_000);
        assert_eq!((t.files, t.added, t.removed), (1, 4, 1));
        assert_eq!(t.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Psrc", "Site"]);
        assert_eq!(t.models[0].name, "Opus 4.5");
        assert!((t.cost - (4.0 * 1000.0 * 5.0 + 600.0 * 25.0) / 1e6).abs() < 1e-12);
        assert_eq!(s.last_week, Totals::default());
        assert_eq!(s.days[0].sessions, 2);
    }

    #[test]
    fn refresh_reads_only_new_complete_lines_and_forgets_old_files() {
        let dir = std::env::temp_dir().join(format!("zodyvel-usage-{}", std::process::id()));
        let project = dir.join("projects").join("C--work-Psrc");
        std::fs::create_dir_all(&project).unwrap();
        let file = project.join("s1.jsonl");
        let usage = r#"{"input_tokens":10,"output_tokens":20}"#;
        let one = assistant("m1", "2026-10-15T09:00:00Z", "s1", "/w/Psrc", "claude-opus-4-5", usage);
        let two = assistant("m2", "2026-10-15T09:01:00Z", "s1", "/w/Psrc", "claude-opus-4-5", usage);
        std::fs::write(&file, format!("{one}\n{{\"type\":\"user\"}}\n{}", &two[..20])).unwrap();
        let mut ledger = Ledger::default();
        assert_eq!(ledger.refresh(&dir, 0), 1);
        let span = Span { from: 0, to: u64::MAX };
        assert_eq!(ledger.summary(span, span, span, &[]).today.messages, 1);
        // Yarım satır tamamlanınca yalnız o okunur.
        std::fs::write(&file, format!("{one}\n{{\"type\":\"user\"}}\n{two}\n")).unwrap();
        assert_eq!(ledger.refresh(&dir, 0), 1);
        assert_eq!(ledger.summary(span, span, span, &[]).today.messages, 2);
        assert_eq!(ledger.refresh(&dir, 0), 0, "değişmeyen dosya yeniden okunmaz");
        // Pencereden eski dosya unutulur.
        assert_eq!(ledger.refresh(&dir, u64::MAX), 0);
        assert_eq!(ledger.summary(span, span, span, &[]).today.messages, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
