//! Küçük IMAP istemcisi (yalnız Gmail için gereken komutlar), saf Rust: Tauri'ye ve Windows'a bağlı değil,
//! Linux'ta sahte bir bağlantıyla sınanır. Bağlantı `Transport` arkasında (Windows'ta `tls.rs`).
//!
//! Komutlar: `LOGIN`, `CAPABILITY`, `SELECT INBOX`, `UID SEARCH UNSEEN` (Gmail'de "Birincil" süzgeciyle),
//! `UID FETCH` (başlık özeti, önizleme `BODY.PEEK` ile, okundu yapmaz), `UID STORE +FLAGS.SILENT (\Seen)`,
//! `IDLE`/`DONE`. Yanıtlar `{n}` literal'leriyle birlikte çerçevelenip ayrıştırılır.
//!
//! Güvenlik: şifre yalnız `LOGIN` satırında sokete gider; hata metinlerinde ve kayıtta geçmez.

use std::time::{Duration, SystemTime};

use super::mime;

/// Bağlantı katmanı. `recv` en çok `timeout` bekler; `interruptible` iken bekleyen bir komut/istek varsa
/// `Recv::Interrupt` döner (IDLE'dan çıkmak için), değilse istekleri bekletir.
pub trait Transport {
    fn send(&mut self, data: &[u8]) -> Result<(), Fail>;
    fn recv(&mut self, timeout: Duration, interruptible: bool) -> Result<Recv, Fail>;
}

#[derive(Debug, PartialEq)]
pub enum Recv {
    Data(Vec<u8>),
    Timeout,
    Interrupt,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fail {
    /// Bağlantı koptu / kurulamadı (yeniden denenir). İç ayrıntı yalnız kayıt için (içerik yok).
    Net(String),
    /// Giriş reddedildi; Türkçe, kullanıcıya gösterilir. Kendiliğinden yeniden denenmez.
    Auth(String),
    /// Sunucu komutu reddetti (NO/BAD).
    No(String),
    /// Sunucu zamanında yanıt vermedi.
    Timeout,
    /// `stop()` çağrıldı.
    Stopped,
}

/// Tek komutun yanıt bekleme süresi.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

// ─── Çerçeveleme ─────────────────────────────────────────────────────────────────────────────────

/// Gelen baytları tam yanıtlara böler: CRLF ile biten satır; satır `{n}` ile bitiyorsa ardından n baytlık
/// literal ve satırın devamı da yanıta dahildir.
#[derive(Default)]
pub struct Framer {
    buf: Vec<u8>,
}

/// Tek yanıtın en büyük boyutu (önizleme 32 KB ile sınırlı; fazlası bozuk sunucu sayılır).
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

impl Framer {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_response(&mut self) -> Result<Option<Vec<u8>>, Fail> {
        let mut pos = 0;
        loop {
            let Some(rel) = self.buf[pos..].windows(2).position(|w| w == b"\r\n") else {
                if self.buf.len() > MAX_RESPONSE {
                    return Err(Fail::Net("yanıt çok büyük".into()));
                }
                return Ok(None);
            };
            let line_end = pos + rel;
            match literal_len(&self.buf[pos..line_end]) {
                Some(n) => {
                    if n > MAX_RESPONSE {
                        return Err(Fail::Net("literal çok büyük".into()));
                    }
                    let after = line_end + 2 + n;
                    // Literal'ler tek tek değil toplamda da sınırlı (çok sayıda literal belleği şişirmesin).
                    if after > MAX_RESPONSE {
                        return Err(Fail::Net("yanıt çok büyük".into()));
                    }
                    if self.buf.len() < after {
                        return Ok(None);
                    }
                    pos = after;
                }
                None => {
                    let rest = self.buf.split_off(line_end + 2);
                    let mut out = std::mem::replace(&mut self.buf, rest);
                    out.truncate(line_end);
                    return Ok(Some(out));
                }
            }
        }
    }
}

/// Satır `{123}` ya da `{123+}` ile bitiyorsa 123.
fn literal_len(line: &[u8]) -> Option<usize> {
    let line = line.strip_suffix(b"}")?;
    let open = line.iter().rposition(|&b| b == b'{')?;
    let digits = &line[open + 1..];
    let digits = digits.strip_suffix(b"+").unwrap_or(digits);
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

// ─── Ayrıştırma ─────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Atom(String),
    /// Tırnaklı dize ya da literal (ham bayt).
    Str(Vec<u8>),
    Nil,
    List(Vec<Value>),
}

impl Value {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Str(b) => Some(b),
            Value::Atom(a) => Some(a.as_bytes()),
            _ => None,
        }
    }
    fn list(&self) -> &[Value] {
        match self {
            Value::List(l) => l,
            _ => &[],
        }
    }
    fn number(&self) -> Option<u64> {
        match self {
            Value::Atom(a) => a.parse().ok(),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    No,
    Bad,
    Bye,
    Preauth,
}

impl Status {
    fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_uppercase().as_str() {
            "OK" => Status::Ok,
            "NO" => Status::No,
            "BAD" => Status::Bad,
            "BYE" => Status::Bye,
            "PREAUTH" => Status::Preauth,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Untagged {
    Status(Status, String),
    Capability(Vec<String>),
    Exists(u32),
    Expunge(u32),
    Recent(u32),
    Search(Vec<u32>),
    Fetch(u32, Vec<(String, Value)>),
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    Continue,
    Tagged(String, Status, String),
    Untagged(Untagged),
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn skip_spaces(&mut self) {
        while self.s.get(self.i) == Some(&b' ') {
            self.i += 1;
        }
    }

    fn word(&mut self) -> &'a str {
        self.skip_spaces();
        let start = self.i;
        while self.i < self.s.len() && self.s[self.i] != b' ' {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).unwrap_or("")
    }

    fn rest(&mut self) -> String {
        self.skip_spaces();
        String::from_utf8_lossy(&self.s[self.i..]).into_owned()
    }

    fn value(&mut self) -> Option<Value> {
        self.skip_spaces();
        match *self.s.get(self.i)? {
            b'(' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_spaces();
                    match self.s.get(self.i) {
                        Some(b')') => {
                            self.i += 1;
                            return Some(Value::List(items));
                        }
                        None => return Some(Value::List(items)),
                        _ => items.push(self.value()?),
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = Vec::new();
                while let Some(&c) = self.s.get(self.i) {
                    self.i += 1;
                    match c {
                        b'\\' => {
                            if let Some(&n) = self.s.get(self.i) {
                                out.push(n);
                                self.i += 1;
                            }
                        }
                        b'"' => break,
                        _ => out.push(c),
                    }
                }
                Some(Value::Str(out))
            }
            b'{' | b'~' => {
                let from = self.i + usize::from(self.s[self.i] == b'~');
                let close = from + self.s[from..].iter().position(|&b| b == b'}')?;
                let n = literal_len(&self.s[from..=close])?;
                let start = close + 3; // "}\r\n"
                let end = start.checked_add(n)?;
                let bytes = self.s.get(start..end)?.to_vec();
                self.i = end;
                Some(Value::Str(bytes))
            }
            b')' => None,
            _ => {
                let start = self.i;
                while let Some(&c) = self.s.get(self.i) {
                    match c {
                        b' ' | b'(' | b')' => break,
                        b'[' => {
                            // `BODY[HEADER.FIELDS (A B)]<0>`: köşeli parantez içi tek atom.
                            let close = self.s[self.i..].iter().position(|&b| b == b']').map(|p| self.i + p)?;
                            self.i = close + 1;
                        }
                        _ => self.i += 1,
                    }
                }
                let atom = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
                Some(if atom.eq_ignore_ascii_case("NIL") { Value::Nil } else { Value::Atom(atom) })
            }
        }
    }
}

pub fn parse(line: &[u8]) -> Response {
    let mut lx = Lexer { s: line, i: 0 };
    let first = lx.word();
    if first == "+" {
        return Response::Continue;
    }
    if first != "*" {
        let tag = first.to_owned();
        let status = Status::parse(lx.word()).unwrap_or(Status::Bad);
        return Response::Tagged(tag, status, lx.rest());
    }
    let word = lx.word().to_owned();
    if let Some(status) = Status::parse(&word) {
        return Response::Untagged(Untagged::Status(status, lx.rest()));
    }
    if word.eq_ignore_ascii_case("CAPABILITY") {
        return Response::Untagged(Untagged::Capability(lx.rest().split_whitespace().map(|w| w.to_ascii_uppercase()).collect()));
    }
    if word.eq_ignore_ascii_case("SEARCH") {
        return Response::Untagged(Untagged::Search(lx.rest().split_whitespace().filter_map(|w| w.parse().ok()).collect()));
    }
    let Ok(n) = word.parse::<u32>() else { return Response::Untagged(Untagged::Other) };
    let kind = lx.word().to_ascii_uppercase();
    Response::Untagged(match kind.as_str() {
        "EXISTS" => Untagged::Exists(n),
        "EXPUNGE" => Untagged::Expunge(n),
        "RECENT" => Untagged::Recent(n),
        "FETCH" => {
            let list = lx.value().unwrap_or(Value::List(Vec::new()));
            let mut items = Vec::new();
            let mut it = list.list().iter();
            while let (Some(key), Some(value)) = (it.next(), it.next()) {
                if let Value::Atom(k) = key {
                    items.push((k.to_ascii_uppercase(), value.clone()));
                }
            }
            Untagged::Fetch(n, items)
        }
        _ => Untagged::Other,
    })
}

// ─── İleti özeti ────────────────────────────────────────────────────────────────────────────────

/// Satır için gereken kadarı (içerik yok; önizleme ayrı ve istenince).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub uid: u32,
    pub seen: bool,
    pub time_ms: i64,
    /// Gönderenin adı, yoksa adresi.
    pub from: String,
    pub subject: String,
    /// `<…>` olmadan.
    pub message_id: Option<String>,
    pub thread_id: Option<u64>,
}

fn item<'a>(items: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn item_prefix<'a>(items: &'a [(String, Value)], prefix: &str) -> Option<&'a Value> {
    items.iter().find(|(k, _)| k.starts_with(prefix)).map(|(_, v)| v)
}

pub fn message(items: &[(String, Value)]) -> Option<Message> {
    let uid = item(items, "UID")?.number()? as u32;
    let seen = item(items, "FLAGS").is_some_and(|f| f.list().iter().any(|v| matches!(v, Value::Atom(a) if a.eq_ignore_ascii_case("\\Seen"))));
    let time_ms = item(items, "INTERNALDATE").and_then(Value::bytes).and_then(|b| internal_date_ms(&String::from_utf8_lossy(b))).unwrap_or(0);
    let env = item(items, "ENVELOPE").map(Value::list).unwrap_or(&[]);
    let subject = env.get(1).and_then(Value::bytes).map(mime::decode_header).unwrap_or_default();
    let from = env.get(2).and_then(|v| v.list().first()).map(address).unwrap_or_default();
    let message_id = env
        .get(9)
        .and_then(Value::bytes)
        .map(|b| String::from_utf8_lossy(b).trim().trim_start_matches('<').trim_end_matches('>').to_owned())
        .filter(|s| !s.is_empty() && !s.contains(char::is_whitespace));
    let thread_id = item(items, "X-GM-THRID").and_then(Value::number);
    Some(Message { uid, seen, time_ms, from, subject, message_id, thread_id })
}

/// ENVELOPE adresi `(ad adl kutu alan)` → ad, yoksa kutu@alan.
fn address(v: &Value) -> String {
    let parts = v.list();
    let name = parts.first().and_then(Value::bytes).map(mime::decode_header).unwrap_or_default();
    let name = name.trim().trim_matches('"').trim().to_owned();
    if !name.is_empty() {
        return name;
    }
    let text = |i: usize| parts.get(i).and_then(Value::bytes).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
    match (text(2), text(3)) {
        (m, h) if !m.is_empty() && !h.is_empty() => format!("{m}@{h}"),
        (m, _) => m,
    }
}

/// `21-Oct-2026 21:54:32 +0300` → Unix ms.
pub fn internal_date_ms(s: &str) -> Option<i64> {
    let mut it = s.split_whitespace();
    let date = it.next()?;
    let time = it.next()?;
    let zone = it.next().unwrap_or("+0000");
    let mut d = date.split('-');
    let day: i64 = d.next()?.parse().ok()?;
    let name = d.next()?;
    let month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name))? as i64
        + 1;
    let year: i64 = d.next()?.parse().ok()?;
    let mut t = time.split(':').map(|x| x.parse::<i64>().ok());
    let (h, m, sec) = (t.next()??, t.next()??, t.next()??);
    let sign = if zone.starts_with('-') { -1 } else { 1 };
    let z = zone.trim_start_matches(['+', '-']);
    let zh: i64 = z.get(0..2)?.parse().ok()?;
    let zm: i64 = z.get(2..4)?.parse().ok()?;
    // Gün sayısı (Howard Hinnant, days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + h * 3600 + m * 60 + sec - sign * (zh * 3600 + zm * 60);
    Some(secs * 1000)
}

// ─── Hata metinleri ─────────────────────────────────────────────────────────────────────────────

/// `LOGIN` reddinin Türkçe karşılığı (sunucu metni kullanıcıya gösterilmez).
pub fn login_error(text: &str) -> String {
    let t = text.to_ascii_lowercase();
    if t.contains("application-specific password") {
        "Gmail normal şifreyi kabul etmiyor. İki adımlı doğrulamayı açıp bir uygulama şifresi oluştur.".into()
    } else if t.contains("web browser") || t.contains("webalert") {
        "Google bu girişi engelledi. Tarayıcıda Gmail'e bir kez girip tekrar dene.".into()
    } else if t.contains("not enabled for imap") {
        "Bu hesapta IMAP kapalı. Gmail ayarlarından IMAP'i açıp tekrar dene.".into()
    } else if t.contains("authenticationfailed") || t.contains("invalid credentials") {
        "Uygulama şifresi ya da adres yanlış. Yeni bir uygulama şifresi alıp tekrar dene.".into()
    } else {
        "Gmail girişi reddetti. Adresi ve uygulama şifresini kontrol et.".into()
    }
}

// ─── Bağlantı bilgisi ───────────────────────────────────────────────────────────────────────────

/// İletiyi Gmail'in web arayüzünde açan adres (doğru hesap `authuser` ile seçilir).
pub fn web_url(account: &str, message_id: Option<&str>) -> String {
    let base = format!("https://mail.google.com/mail/?authuser={}", url_encode(account));
    match message_id {
        Some(id) => format!("{base}#search/rfc822msgid%3A{}", url_encode(id)),
        None => format!("{base}#inbox"),
    }
}

/// Gmail'de bir kategorinin sekmesi (Birincil gelen kutusudur).
pub fn category_url(account: &str, category: &str) -> String {
    let base = format!("https://mail.google.com/mail/?authuser={}", url_encode(account));
    match category {
        "updates" | "promotions" | "social" | "forums" => format!("{base}#category/{category}"),
        _ => format!("{base}#inbox"),
    }
}

/// O hesapla Gmail'de yeni ileti penceresi (tam ekran yazma).
pub fn compose_url(account: &str) -> String {
    format!("https://mail.google.com/mail/?authuser={}&view=cm&fs=1", url_encode(account))
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// IMAP tırnaklı dizesi. CR/LF ya da ASCII dışı karakter varsa `None` (komut enjeksiyonu yok).
pub fn quote(s: &str) -> Option<String> {
    if !s.is_ascii() || s.contains(['\r', '\n', '\0']) {
        return None;
    }
    Some(format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
}

// ─── İstemci ────────────────────────────────────────────────────────────────────────────────────

/// IDLE'ın bitme nedeni.
#[derive(Debug, PartialEq)]
pub enum IdleEnd {
    /// Gelen kutusu değişti (`EXISTS`/`EXPUNGE`/`FETCH`): yeniden çekilmeli.
    Changed,
    /// Süre doldu (bağlantıyı canlı tutmak için yeniden IDLE).
    Expired,
    /// Bekleyen bir istek var (önizleme, okundu yap, yenile, durdur).
    Interrupt,
    /// Bilgisayar uykudan döndü: bağlantı büyük olasılıkla ölü, baştan kurulmalı.
    Slept,
}

pub struct Client<T: Transport> {
    pub io: T,
    framer: Framer,
    tag: u32,
    pub capabilities: Vec<String>,
}

impl<T: Transport> Client<T> {
    pub fn new(io: T) -> Self {
        Self { io, framer: Framer::default(), tag: 0, capabilities: Vec::new() }
    }

    /// Sıradaki tam yanıt; `timeout` içinde gelmezse `Ok(None)`, istek beklerse `Interrupt`.
    fn read(&mut self, timeout: Duration, interruptible: bool) -> Result<Result<Response, Recv>, Fail> {
        loop {
            if let Some(line) = self.framer.next_response()? {
                return Ok(Ok(parse(&line)));
            }
            match self.io.recv(timeout, interruptible)? {
                Recv::Data(d) => self.framer.push(&d),
                other => return Ok(Err(other)),
            }
        }
    }

    fn read_strict(&mut self, timeout: Duration) -> Result<Response, Fail> {
        match self.read(timeout, false)? {
            Ok(r) => Ok(r),
            Err(_) => Err(Fail::Timeout),
        }
    }

    fn next_tag(&mut self) -> String {
        self.tag += 1;
        format!("z{}", self.tag)
    }

    /// Sunucunun karşılama satırı.
    pub fn greeting(&mut self) -> Result<(), Fail> {
        match self.read_strict(COMMAND_TIMEOUT)? {
            Response::Untagged(Untagged::Status(Status::Ok | Status::Preauth, _)) => Ok(()),
            _ => Err(Fail::Net("karşılama yok".into())),
        }
    }

    /// Komutu gönderir, etiketli yanıta kadar gelen etiketsiz yanıtları döndürür. NO/BAD → `Fail::No`.
    pub fn command(&mut self, cmd: &str) -> Result<Vec<Untagged>, Fail> {
        let tag = self.next_tag();
        self.io.send(format!("{tag} {cmd}\r\n").as_bytes())?;
        self.finish(&tag)
    }

    fn finish(&mut self, tag: &str) -> Result<Vec<Untagged>, Fail> {
        let mut out = Vec::new();
        loop {
            match self.read_strict(COMMAND_TIMEOUT)? {
                Response::Tagged(t, status, text) if t == tag => {
                    return match status {
                        Status::Ok => Ok(out),
                        _ => Err(Fail::No(text)),
                    };
                }
                Response::Untagged(Untagged::Status(Status::Bye, _)) => return Err(Fail::Net("sunucu kapattı".into())),
                Response::Untagged(Untagged::Capability(caps)) => {
                    self.capabilities = caps.clone();
                    out.push(Untagged::Capability(caps));
                }
                Response::Untagged(u) => out.push(u),
                _ => {}
            }
        }
    }

    pub fn has(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }

    pub fn login(&mut self, user: &str, password: &str) -> Result<(), Fail> {
        let (Some(u), Some(p)) = (quote(user), quote(password)) else {
            return Err(Fail::Auth("Adres ya da uygulama şifresi geçersiz karakter içeriyor.".into()));
        };
        match self.command(&format!("LOGIN {u} {p}")) {
            Ok(_) => {}
            Err(Fail::No(text)) => return Err(Fail::Auth(login_error(&text))),
            Err(e) => return Err(e),
        }
        // Gmail giriş sonrası yetenekleri ayrıca söylemeyebilir.
        if !self.has("IDLE") {
            self.command("CAPABILITY")?;
        }
        Ok(())
    }

    pub fn select_inbox(&mut self) -> Result<(), Fail> {
        self.command("SELECT INBOX").map(|_| ())
    }

    /// Okunmamış iletilerin UID'leri (küçükten büyüğe). `primary`: Gmail "Birincil" sekmesi.
    pub fn unseen(&mut self, primary: bool) -> Result<Vec<u32>, Fail> {
        self.unseen_in(if primary { Some("primary") } else { None })
    }

    /// Okunmamışlar; `category` verilirse yalnız o Gmail kategorisi ("primary", "updates", "promotions";
    /// sabit listeden gelir, kullanıcı metni değildir).
    pub fn unseen_in(&mut self, category: Option<&str>) -> Result<Vec<u32>, Fail> {
        let cmd = match category {
            Some(c) if c.bytes().all(|b| b.is_ascii_lowercase()) => format!("UID SEARCH UNSEEN X-GM-RAW \"category:{c}\""),
            _ => "UID SEARCH UNSEEN".to_owned(),
        };
        let mut uids: Vec<u32> = self
            .command(&cmd)?
            .into_iter()
            .flat_map(|u| match u {
                Untagged::Search(v) => v,
                _ => Vec::new(),
            })
            .collect();
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    /// Satır özetleri (gövde yok).
    pub fn summaries(&mut self, uids: &[u32]) -> Result<Vec<Message>, Fail> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let set = uids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let gm = if self.has("X-GM-EXT-1") { " X-GM-THRID" } else { "" };
        let mut out: Vec<Message> = self
            .command(&format!("UID FETCH {set} (UID FLAGS INTERNALDATE ENVELOPE{gm})"))?
            .into_iter()
            .filter_map(|u| match u {
                Untagged::Fetch(_, items) => message(&items),
                _ => None,
            })
            .filter(|m| uids.contains(&m.uid))
            .collect();
        out.sort_by_key(|m| std::cmp::Reverse(m.uid));
        out.dedup_by_key(|m| m.uid);
        Ok(out)
    }

    /// Düz metin önizleme; `BODY.PEEK` iletiyi okundu yapmaz. Gövdenin en çok ilk 32 KB'ı istenir.
    pub fn preview(&mut self, uid: u32) -> Result<String, Fail> {
        let cmd = format!("UID FETCH {uid} (BODY.PEEK[HEADER.FIELDS (CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] BODY.PEEK[TEXT]<0.32768>)");
        for u in self.command(&cmd)? {
            if let Untagged::Fetch(_, items) = u {
                if item(&items, "UID").and_then(Value::number) != Some(uid as u64) {
                    continue;
                }
                let head = item_prefix(&items, "BODY[HEADER").and_then(Value::bytes).unwrap_or(&[]);
                let body = item_prefix(&items, "BODY[TEXT]").and_then(Value::bytes).unwrap_or(&[]);
                return Ok(mime::preview(head, body));
            }
        }
        Err(Fail::No("ileti yok".into()))
    }

    pub fn mark_seen(&mut self, uid: u32) -> Result<(), Fail> {
        self.command(&format!("UID STORE {uid} +FLAGS.SILENT (\\Seen)")).map(|_| ())
    }

    /// IDLE yokken: istek gelene ya da süre dolana kadar bekler; arada gelen etiketsiz yanıt değişiklik sayılır.
    pub fn wait(&mut self, timeout: Duration) -> Result<IdleEnd, Fail> {
        Ok(match self.read(timeout, true)? {
            Ok(Response::Untagged(Untagged::Status(Status::Bye, _))) => return Err(Fail::Net("sunucu kapattı".into())),
            Ok(_) => IdleEnd::Changed,
            Err(Recv::Interrupt) => IdleEnd::Interrupt,
            Err(_) => IdleEnd::Expired,
        })
    }

    /// IDLE'a girer ve bir değişiklik, istek ya da süre sonuna kadar bekler; uyku dönüşü dışında `DONE`
    /// ile çıkıp sunucunun onayını bekler. Bekleme `slice` dilimleriyle yapılır (ağ trafiği yok; yalnız
    /// duvar saatinin sıçramasına bakılır).
    pub fn idle(&mut self, max: Duration, slice: Duration) -> Result<IdleEnd, Fail> {
        let tag = self.next_tag();
        self.io.send(format!("{tag} IDLE\r\n").as_bytes())?;
        let mut changed = false;
        loop {
            match self.read_strict(COMMAND_TIMEOUT)? {
                Response::Continue => break,
                Response::Tagged(t, _, text) if t == tag => return Err(Fail::No(text)),
                Response::Untagged(Untagged::Status(Status::Bye, _)) => return Err(Fail::Net("sunucu kapattı".into())),
                Response::Untagged(u) => changed |= is_change(&u),
                _ => {}
            }
        }
        let mut waited = Duration::ZERO;
        let end = loop {
            if changed {
                break IdleEnd::Changed;
            }
            if waited >= max {
                break IdleEnd::Expired;
            }
            let wait = slice.min(max - waited);
            let before = SystemTime::now();
            let got = self.read(wait, true)?;
            let wall = SystemTime::now().duration_since(before).unwrap_or_default();
            match got {
                Ok(Response::Untagged(Untagged::Status(Status::Bye, _))) => return Err(Fail::Net("sunucu kapattı".into())),
                Ok(Response::Untagged(u)) => changed |= is_change(&u),
                Ok(_) => {}
                Err(Recv::Interrupt) => break IdleEnd::Interrupt,
                Err(_) => {
                    if wall > wait + Duration::from_secs(90) {
                        return Ok(IdleEnd::Slept);
                    }
                    waited += wait;
                }
            }
        };
        self.io.send(b"DONE\r\n")?;
        self.finish(&tag)?;
        Ok(end)
    }
}

fn is_change(u: &Untagged) -> bool {
    matches!(u, Untagged::Exists(_) | Untagged::Expunge(_) | Untagged::Fetch(..))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Sahte sunucu: her `send`'e karşılık hazır yanıtlar sıraya girer.
    struct Fake {
        sent: Vec<String>,
        /// (gönderilen satırın başlangıcı, verilecek yanıt).
        script: VecDeque<(&'static str, Vec<Recv>)>,
        pending: VecDeque<Recv>,
    }

    impl Fake {
        fn new(greeting: &str, script: Vec<(&'static str, Vec<Recv>)>) -> Self {
            Self { sent: Vec::new(), script: script.into(), pending: VecDeque::from([data(greeting)]) }
        }
    }

    fn data(s: &str) -> Recv {
        Recv::Data(s.as_bytes().to_vec())
    }

    impl Transport for Fake {
        fn send(&mut self, d: &[u8]) -> Result<(), Fail> {
            let line = String::from_utf8_lossy(d).into_owned();
            let (expect, replies) = self.script.pop_front().unwrap_or_else(|| panic!("beklenmeyen komut: {line}"));
            let body = line.split_once(' ').map(|(_, b)| b).unwrap_or(&line);
            assert!(body.starts_with(expect) || line.starts_with(expect), "beklenen {expect:?}, gelen {line:?}");
            self.sent.push(line);
            self.pending.extend(replies);
            Ok(())
        }
        fn recv(&mut self, _t: Duration, interruptible: bool) -> Result<Recv, Fail> {
            match self.pending.pop_front() {
                Some(Recv::Interrupt) if !interruptible => panic!("kesilemez beklemede istek"),
                Some(r) => Ok(r),
                None => Ok(Recv::Timeout),
            }
        }
    }

    #[test]
    fn framer_handles_literals_and_split_chunks() {
        let mut f = Framer::default();
        f.push(b"* 1 FETCH (UID 5 BODY[TEXT] {5}\r\nab");
        assert_eq!(f.next_response().unwrap(), None);
        f.push(b"\r\nd)\r\nz1 OK done\r\n* 2 EXI");
        assert_eq!(f.next_response().unwrap().unwrap(), b"* 1 FETCH (UID 5 BODY[TEXT] {5}\r\nab\r\nd)");
        assert_eq!(f.next_response().unwrap().unwrap(), b"z1 OK done");
        assert_eq!(f.next_response().unwrap(), None);
        f.push(b"STS\r\n");
        assert_eq!(parse(&f.next_response().unwrap().unwrap()), Response::Untagged(Untagged::Exists(2)));
    }

    #[test]
    fn parses_fetch_with_envelope() {
        let line = b"* 12 FETCH (UID 4821 FLAGS (\\Recent) INTERNALDATE \"21-Oct-2026 21:54:32 +0300\" ENVELOPE (\"Tue, 21 Oct 2026 21:54:30 +0300\" \"=?UTF-8?B?VGVrbGlmIGfDvG5jZWxsZW1lc2k=?=\" ((\"=?UTF-8?Q?Ay=C5=9Fe_Y=C4=B1lmaz?=\" NIL \"ayse\" \"ornek.com\")) ((NIL NIL \"ayse\" \"ornek.com\")) NIL ((NIL NIL \"ben\" \"gmail.com\")) NIL NIL NIL \"<CAB12@mail.gmail.com>\") X-GM-THRID 1780000000000000001)";
        let Response::Untagged(Untagged::Fetch(12, items)) = parse(line) else { panic!() };
        let m = message(&items).unwrap();
        assert_eq!(m.uid, 4821);
        assert!(!m.seen);
        assert_eq!(m.subject, "Teklif güncellemesi");
        assert_eq!(m.from, "Ayşe Yılmaz");
        assert_eq!(m.message_id.as_deref(), Some("CAB12@mail.gmail.com"));
        assert_eq!(m.thread_id, Some(1_780_000_000_000_000_001));
        assert_eq!(m.time_ms, internal_date_ms("21-Oct-2026 18:54:32 +0000").unwrap());
    }

    #[test]
    fn envelope_literal_subject_and_address_fallback() {
        let line = b"* 3 FETCH (UID 9 FLAGS (\\Seen \\Flagged) ENVELOPE (NIL {9}\r\nK\xc4\xb1sa \"x\" ((NIL NIL \"bot\" \"servis.io\")) NIL NIL NIL NIL NIL NIL NIL))";
        let Response::Untagged(Untagged::Fetch(_, items)) = parse(line) else { panic!() };
        let m = message(&items).unwrap();
        assert_eq!((m.uid, m.seen, m.subject.as_str(), m.from.as_str()), (9, true, "Kısa \"x\"", "bot@servis.io"));
        assert_eq!(m.message_id, None);
    }

    #[test]
    fn dates() {
        assert_eq!(internal_date_ms("01-Jan-1970 00:00:00 +0000"), Some(0));
        assert_eq!(internal_date_ms(" 1-Mar-2024 03:00:00 +0300"), Some(1_709_251_200_000));
        assert_eq!(internal_date_ms("29-Feb-2024 12:00:00 -0130"), Some(1_709_213_400_000));
        assert_eq!(internal_date_ms("bozuk"), None);
    }

    #[test]
    fn login_errors_are_turkish_and_hide_server_text() {
        assert!(login_error("[AUTHENTICATIONFAILED] Invalid credentials (Failure)").starts_with("Uygulama şifresi"));
        assert!(login_error("[ALERT] Application-specific password required: https://support.google.com/x").contains("İki adımlı"));
        assert!(login_error("[ALERT] Please log in via your web browser").contains("Tarayıcıda"));
        assert!(!login_error("tuhaf bir şey").contains("tuhaf"));
    }

    #[test]
    fn quoting_blocks_injection() {
        assert_eq!(quote("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert_eq!(quote("x\r\nz2 DELETE INBOX"), None);
        assert_eq!(quote("şifre"), None);
    }

    #[test]
    fn urls() {
        assert_eq!(web_url("ad.soyad@gmail.com", Some("CAB+1@mail.gmail.com")), "https://mail.google.com/mail/?authuser=ad.soyad%40gmail.com#search/rfc822msgid%3ACAB%2B1%40mail.gmail.com");
        assert_eq!(web_url("a@b.c", None), "https://mail.google.com/mail/?authuser=a%40b.c#inbox");
        assert_eq!(category_url("a@b.c", "updates"), "https://mail.google.com/mail/?authuser=a%40b.c#category/updates");
        assert_eq!(category_url("a@b.c", "primary"), "https://mail.google.com/mail/?authuser=a%40b.c#inbox");
        assert_eq!(compose_url("a@b.c"), "https://mail.google.com/mail/?authuser=a%40b.c&view=cm&fs=1");
    }

    #[test]
    fn session_flow() {
        let fake = Fake::new(
            "* OK Gimap ready\r\n",
            vec![
                ("LOGIN \"ben@gmail.com\" \"abcdabcdabcdabcd\"", vec![data("* CAPABILITY IMAP4rev1 IDLE X-GM-EXT-1\r\nz1 OK ben authenticated (Success)\r\n")]),
                ("SELECT INBOX", vec![data("* FLAGS (\\Seen)\r\n* 120 EXISTS\r\nz2 OK [READ-WRITE] INBOX selected.\r\n")]),
                ("UID SEARCH UNSEEN X-GM-RAW", vec![data("* SEARCH 77 75\r\nz3 OK SEARCH completed\r\n")]),
                ("UID FETCH 75,77 (UID FLAGS INTERNALDATE ENVELOPE X-GM-THRID)", vec![data("* 1 FETCH (UID 75 FLAGS () ENVELOPE (NIL \"A\" NIL NIL NIL NIL NIL NIL NIL NIL))\r\n"), data("* 2 FETCH (UID 77 FLAGS () ENVELOPE (NIL \"B\" NIL NIL NIL NIL NIL NIL NIL NIL))\r\nz4 OK\r\n")]),
                ("UID FETCH 77 (BODY.PEEK[HEADER.FIELDS", vec![data("* 2 FETCH (UID 77 BODY[HEADER.FIELDS (CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] {41}\r\nContent-Type: text/plain; charset=utf-8\r\n BODY[TEXT]<0> {6}\r\nSelam!)\r\nz5 OK\r\n")]),
                ("UID STORE 77 +FLAGS.SILENT (\\Seen)", vec![data("z6 OK\r\n")]),
                ("IDLE", vec![data("+ idling\r\n"), Recv::Timeout, data("* 121 EXISTS\r\n")]),
                ("DONE", vec![data("z7 OK IDLE terminated\r\n")]),
                ("IDLE", vec![data("+ idling\r\n"), Recv::Interrupt]),
                ("DONE", vec![data("z8 OK\r\n")]),
            ],
        );
        let mut c = Client::new(fake);
        c.greeting().unwrap();
        c.login("ben@gmail.com", "abcdabcdabcdabcd").unwrap();
        assert!(c.has("IDLE") && c.has("X-GM-EXT-1"));
        c.select_inbox().unwrap();
        assert_eq!(c.unseen(true).unwrap(), vec![75, 77]);
        let list = c.summaries(&[75, 77]).unwrap();
        assert_eq!(list.iter().map(|m| (m.uid, m.subject.as_str())).collect::<Vec<_>>(), vec![(77, "B"), (75, "A")]);
        assert_eq!(c.preview(77).unwrap(), "Selam!");
        c.mark_seen(77).unwrap();
        assert_eq!(c.idle(Duration::from_secs(600), Duration::from_secs(60)).unwrap(), IdleEnd::Changed);
        assert_eq!(c.idle(Duration::from_secs(600), Duration::from_secs(60)).unwrap(), IdleEnd::Interrupt);
        assert!(c.io.sent.iter().all(|l| !l.contains("abcd") || l.starts_with("z1 LOGIN")));
    }

    #[test]
    fn category_search_uses_fixed_names_only() {
        let fake = Fake::new(
            "* OK hi\r\n",
            vec![
                ("UID SEARCH UNSEEN X-GM-RAW \"category:updates\"", vec![data("* SEARCH 9 4\r\nz1 OK\r\n")]),
                ("UID SEARCH UNSEEN\r", vec![data("* SEARCH 2\r\nz2 OK\r\n")]),
            ],
        );
        let mut c = Client::new(fake);
        c.greeting().unwrap();
        assert_eq!(c.unseen_in(Some("updates")).unwrap(), vec![4, 9]);
        // Beklenmeyen ad (büyük harf, tırnak) süzgeçsiz aramaya düşer, komuta girmez.
        assert_eq!(c.unseen_in(Some("x\" OR")).unwrap(), vec![2]);
    }

    #[test]
    fn idle_expires_and_login_failure_maps() {
        let fake = Fake::new(
            "* OK hi\r\n",
            vec![
                ("LOGIN", vec![data("z1 NO [AUTHENTICATIONFAILED] Invalid credentials (Failure)\r\n")]),
                ("IDLE", vec![data("+ ok\r\n")]),
                ("DONE", vec![data("z2 OK\r\n")]),
            ],
        );
        let mut c = Client::new(fake);
        c.greeting().unwrap();
        let Err(Fail::Auth(msg)) = c.login("a@b.c", "x") else { panic!() };
        assert!(msg.starts_with("Uygulama şifresi"));
        // Sahte bağlantı hemen "zaman aşımı" döner: 3 dilim sonra süre biter.
        assert_eq!(c.idle(Duration::from_secs(180), Duration::from_secs(60)).unwrap(), IdleEnd::Expired);
    }

    #[test]
    fn bye_during_command_is_network_failure() {
        let fake = Fake::new("* OK hi\r\n", vec![("SELECT INBOX", vec![data("* BYE gidiyorum\r\n")])]);
        let mut c = Client::new(fake);
        c.greeting().unwrap();
        assert!(matches!(c.select_inbox(), Err(Fail::Net(_))));
    }
}
