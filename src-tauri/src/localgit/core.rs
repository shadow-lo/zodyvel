//! Yerel Git'in saf kısmı: `git status` çıktısını ayrıştırma, hassas dosya kuralı, commit mesajı denetimi,
//! hata metinlerini Türkçeye çevirme, klasörlerde depo bulma ve GitHub Desktop listesinden yol çıkarma.
//! Tauri'ye ve Windows'a bağlı değil; Linux'ta sınanır. Git çalıştırmak `exec.rs`'te.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Bir depoda listelenen en çok değişiklik (fazlası yalnız sayı olarak döner).
pub const MAX_FILES: usize = 300;
/// Commit mesajının üst sınırı (karakter).
pub const MAX_MESSAGE: usize = 5000;
/// GitHub 50 MB üstünü uyarır, 100 MB üstünü reddeder; 50 MB üstü işaretlenir.
pub const BIG_FILE: u64 = 50 * 1024 * 1024;

/// Değişen tek dosya.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Depo köküne göre, `/` ile.
    pub path: String,
    /// Yeniden adlandırmada eski yol.
    pub orig: Option<String>,
    /// "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflict"
    pub kind: &'static str,
    /// Şifre, anahtar ya da `.env` gibi görünüyor: varsayılan olarak commit dışı.
    pub sensitive: bool,
    /// 50 MB üstü (GitHub reddedebilir).
    pub big: bool,
}

/// `git status --porcelain=v2 --branch -z` sonucu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// Dal adı; ayrık HEAD'de `None`.
    pub branch: Option<String>,
    /// "origin/main" gibi; yoksa dal GitHub'a hiç gönderilmemiş.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Henüz hiç commit yok.
    pub initial: bool,
    pub files: Vec<FileChange>,
    /// `MAX_FILES` dışında kalan değişiklik sayısı.
    pub more: u32,
}

impl Status {
    pub fn conflicts(&self) -> bool {
        self.files.iter().any(|f| f.kind == "conflict")
    }
}

fn kind_of(xy: &[u8]) -> &'static str {
    let (x, y) = (xy.first().copied().unwrap_or(b'.'), xy.get(1).copied().unwrap_or(b'.'));
    if x == b'D' || y == b'D' {
        "deleted"
    } else if x == b'A' {
        "added"
    } else if x == b'R' || x == b'C' {
        "renamed"
    } else {
        "modified"
    }
}

/// Ayrıştırır; tanınmayan satır atlanır. `-z`: kayıtlar NUL ile biter, yeniden adlandırmada eski yol ayrı kayıt.
pub fn parse_status(out: &[u8]) -> Status {
    let mut st = Status::default();
    let mut records = out.split(|b| *b == 0).filter(|r| !r.is_empty());
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace('\\', "/");
    let push = |st: &mut Status, f: FileChange| {
        if st.files.len() < MAX_FILES {
            st.files.push(f);
        } else {
            st.more += 1;
        }
    };
    while let Some(rec) = records.next() {
        if let Some(head) = rec.strip_prefix(b"# ") {
            let line = String::from_utf8_lossy(head);
            let (key, value) = line.split_once(' ').unwrap_or((&line, ""));
            match key {
                "branch.oid" => st.initial = value == "(initial)",
                "branch.head" => st.branch = (value != "(detached)" && !value.is_empty()).then(|| value.to_owned()),
                "branch.upstream" => st.upstream = (!value.is_empty()).then(|| value.to_owned()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        // Alanlar boşlukla ayrılır; yol son alan (boşluk içerebilir), bu yüzden alan sayısıyla bölünür.
        let fields = |n: usize| rec.splitn(n, |b| *b == b' ').collect::<Vec<_>>();
        match rec.first() {
            Some(b'1') => {
                let f = fields(9);
                if let [_, xy, .., path] = f.as_slice() {
                    push(&mut st, FileChange { path: text(path), kind: kind_of(xy), ..FileChange::default() });
                }
            }
            Some(b'2') => {
                let f = fields(10);
                let orig = records.next().map(text);
                if let [_, xy, .., path] = f.as_slice() {
                    push(&mut st, FileChange { path: text(path), orig, kind: kind_of(xy), ..FileChange::default() });
                }
            }
            Some(b'u') => {
                let f = fields(11);
                if let Some(path) = f.last().filter(|_| f.len() == 11) {
                    push(&mut st, FileChange { path: text(path), kind: "conflict", ..FileChange::default() });
                }
            }
            Some(b'?') => {
                if let Some(path) = rec.get(2..) {
                    push(&mut st, FileChange { path: text(path), kind: "untracked", ..FileChange::default() });
                }
            }
            _ => {}
        }
    }
    for f in &mut st.files {
        f.sensitive = sensitive(&f.path);
    }
    st
}

/// Şifre, anahtar ya da gizli ayar dosyası gibi görünüyor mu (yalnız ada bakılır, içerik okunmaz).
/// Örnek/şablon dosyaları (`.env.example`) hassas sayılmaz. Fazla işaretlemek eksikten iyidir.
pub fn sensitive(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_lowercase();
    let template = [".example", ".sample", ".template", ".dist", ".defaults"].iter().any(|s| name.ends_with(s));
    if name == ".env" || (name.starts_with(".env.") && !template) || (name.ends_with(".env") && !template) {
        return true;
    }
    const EXACT: [&str; 12] = [
        "id_rsa", "id_dsa", "id_ecdsa", "id_ed25519", ".npmrc", ".pypirc", ".netrc", "_netrc", ".git-credentials", ".htpasswd", "credentials", ".dockercfg",
    ];
    if EXACT.contains(&name.as_str()) {
        return true;
    }
    const EXT: [&str; 11] = ["pem", "key", "p12", "pfx", "keystore", "jks", "ppk", "kdbx", "asc", "gpg", "snk"];
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if name.contains('.') && EXT.contains(&ext) {
        return true;
    }
    // "secrets.json", "client_secret.txt", "credentials.yml", "service-account.json"; kod dosyaları (secrets.rs) değil.
    const DATA: [&str; 10] = ["", "json", "txt", "yaml", "yml", "toml", "ini", "cfg", "conf", "xml"];
    let data = !name.contains('.') || DATA.contains(&ext);
    let stem = name.split('.').next().unwrap_or("");
    data && !template && (["secret", "credential", "password", "passwd", "token"].iter().any(|w| stem.contains(w)) || stem.starts_with("service-account"))
}

/// Commit mesajı: kırpılır, satır sonları `\n` olur, denetim karakterleri atılır; boş ya da çok uzun hata.
pub fn clean_message(message: &str) -> Result<String, String> {
    let text = message.replace("\r\n", "\n").replace('\r', "\n");
    let text: String = text.chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    let text = text.trim();
    if text.is_empty() {
        return Err("Commit mesajını yaz.".into());
    }
    if text.chars().count() > MAX_MESSAGE {
        return Err("Commit mesajı çok uzun.".into());
    }
    Ok(text.to_owned())
}

/// Commit'e girecek yollar: seçilenler o anki değişikliklerde olmalı (başka yol commitlenemez), çakışma
/// olmamalı, hassas dosya ancak ayrıca onaylandıysa. Yeniden adlandırmada eski yol da eklenir.
/// Dönüş: (eklenecek izlenmeyen dosyalar, commit yolları).
pub fn plan_commit(status: &Status, picked: &[String], allow_sensitive: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
    if status.conflicts() {
        return Err("Depoda çözülmemiş çakışma var; önce GitHub Desktop'ta çöz.".into());
    }
    if picked.is_empty() {
        return Err("Commitlenecek dosya seçilmedi.".into());
    }
    let mut add = Vec::new();
    let mut paths = BTreeSet::new();
    for p in picked {
        let Some(f) = status.files.iter().find(|f| &f.path == p) else {
            return Err("Seçilen dosyalardan biri artık değişmiş görünmüyor; Yenile'ye bas.".into());
        };
        if f.sensitive && !allow_sensitive.contains(p) {
            return Err(format!("Hassas dosya onaysız seçildi: {}", f.path));
        }
        if f.kind == "untracked" {
            add.push(f.path.clone());
        }
        paths.insert(f.path.clone());
        if let Some(o) = &f.orig {
            paths.insert(o.clone());
        }
    }
    Ok((add, paths.into_iter().collect()))
}

/// Geri alma (discard) planı. `trash`: çalışma ağacındaki hâli Geri Dönüşüm Kutusu'na gidecek dosyalar
/// (kalıcı silme yok, GitHub Desktop gibi). `restore`: sonra son commit'teki (HEAD) hâline döndürülecek yollar.
#[derive(Debug, Default, PartialEq)]
pub struct Discard {
    pub trash: Vec<String>,
    pub restore: Vec<String>,
}

/// Seçilen dosyaların değişikliklerini geri alma planı. Çakışmalı dosya ve listede olmayan yol reddedilir.
pub fn plan_discard(status: &Status, picked: &[String]) -> Result<Discard, String> {
    if status.conflicts() {
        return Err("Depoda çözülmemiş çakışma var; önce GitHub Desktop'ta çöz.".into());
    }
    if picked.is_empty() {
        return Err("Geri alınacak dosya seçilmedi.".into());
    }
    let mut plan = Discard::default();
    let mut restore = BTreeSet::new();
    for p in picked {
        let Some(f) = status.files.iter().find(|f| &f.path == p) else {
            return Err("Seçilen dosyalardan biri artık değişmiş görünmüyor; Yenile'ye bas.".into());
        };
        if f.kind != "deleted" {
            plan.trash.push(f.path.clone());
        }
        if f.kind != "untracked" {
            restore.insert(f.path.clone());
            if let Some(o) = &f.orig {
                restore.insert(o.clone());
            }
        }
    }
    plan.restore = restore.into_iter().collect();
    Ok(plan)
}

/// Pathspec dosyası (`--pathspec-file-nul`): yollar NUL ile ayrılır.
pub fn pathspec(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        out.extend_from_slice(p.as_bytes());
        out.push(0);
    }
    out
}

/// Git'in hata metni bir giriş (kimlik) hatası mı.
pub fn auth_failed(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    ["authentication failed", "could not read username", "could not read password", "terminal prompts disabled", "permission denied (publickey)", "invalid username or password", "403", "401"]
        .iter()
        .any(|x| s.contains(x))
}

/// Zodyvel'e bağlı GitHub tokenıyla git'in ağ komutları için ortam değişkenleri: yalnız `https://github.com/`
/// adreslerine giden isteklere başlık olarak eklenir. Komut satırına ve diske yazılmaz (yalnız git sürecinin
/// ortamında durur). Token yoksa boş.
pub fn auth_env(token: Option<&str>) -> Vec<(String, String)> {
    use base64::Engine;
    let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) else { return Vec::new() };
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{token}"));
    vec![
        ("GIT_CONFIG_COUNT".into(), "1".into()),
        ("GIT_CONFIG_KEY_0".into(), "http.https://github.com/.extraheader".into()),
        ("GIT_CONFIG_VALUE_0".into(), format!("AUTHORIZATION: basic {basic}")),
    ]
}

/// `https://kullanıcı:token@host` içindeki kimlik bilgisini siler (hata metinleri kullanıcıya ve kayda gider).
pub fn strip_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        let (head, tail) = rest.split_at(i + 3);
        out.push_str(head);
        let end = tail.find(|c: char| c.is_whitespace() || c == '/' || c == '\'' || c == '"').unwrap_or(tail.len());
        match tail[..end].rfind('@') {
            Some(at) => out.push_str(&tail[at + 1..end]),
            None => out.push_str(&tail[..end]),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

/// Git'in (İngilizce, `LC_ALL=C`) hata çıktısını kısa Türkçe açıklamaya çevirir.
pub fn explain(stderr: &str) -> String {
    let s = stderr.to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| s.contains(x));
    if has(&["please tell me who you are", "unable to auto-detect email", "empty ident"]) {
        "Git'te adın ve e-postan ayarlı değil. GitHub Desktop › File › Options › Git'ten bir kez ayarla.".into()
    } else if auth_failed(stderr) {
        "GitHub girişi bulunamadı. Ayarlar > Kaynaklar > GitHub'da GitHub'ı bağla (token \"repo\" izniyle) ya da Git for Windows'u kurup bir kez giriş yap.".into()
    } else if has(&["not possible to fast-forward", "diverging branches"]) {
        "Hem sende hem GitHub'da yeni commit var (dallar ayrışmış); birleştirmeyi GitHub Desktop'ta yap.".into()
    } else if has(&["would be overwritten by merge", "would be overwritten by checkout"]) {
        "Gelen değişiklikler senin commitlenmemiş dosyalarına dokunuyor; önce onları commitle ya da geri al.".into()
    } else if has(&["non-fast-forward", "fetch first", "[rejected]", "rejected"]) {
        "GitHub'da senin bilgisayarında olmayan commitler var. Önce Pull yap.".into()
    } else if has(&["could not resolve host", "unable to access", "connection timed out", "failed to connect"]) {
        "GitHub'a ulaşılamadı; internet bağlantını kontrol et.".into()
    } else if has(&["index.lock", "another git process"]) {
        "Başka bir Git işlemi sürüyor (GitHub Desktop olabilir). Biraz sonra yeniden dene.".into()
    } else if has(&["nothing to commit", "no changes added to commit", "nothing added to commit"]) {
        "Commitlenecek değişiklik kalmadı; Yenile'ye bas.".into()
    } else if has(&["hook"]) {
        "Deponun commit kancası (hook) işlemi durdurdu.".into()
    } else if has(&["gpg failed", "signing failed"]) {
        "Commit imzalanamadı (GPG).".into()
    } else if has(&["large files", "exceeds github's file size limit", "gh001"]) {
        "GitHub çok büyük dosyayı reddetti (100 MB sınırı).".into()
    } else {
        let line = strip_credentials(stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Git hata verdi."));
        let line = line.trim_start_matches("fatal: ").trim_start_matches("error: ");
        line.chars().take(200).collect()
    }
}

// ─── Depo bulma ────────────────────────────────────────────────────────────────────────────────────

/// Bir klasörün altında bakılan en çok derinlik (klasörün kendisi 0).
const SCAN_DEPTH: usize = 2;
/// Bulunan en çok depo.
pub const MAX_REPOS: usize = 40;
/// Taramada atlanan klasörler.
const SKIP: [&str; 6] = ["node_modules", "target", "dist", "build", "vendor", "packages"];

pub fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Klasörün içindeki ya da üstündeki (en çok 6 basamak) deponun kökü.
pub fn repo_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().take(7).find(|d| is_repo(d)).map(Path::to_path_buf)
}

/// Eklenen klasörler: kendisi depo ise o, değilse içindeki depolar (2 kat, gizli ve bağımlılık klasörleri
/// atlanır; depo içine inilmez). `extra`: "Yeni görev"in son klasörleri gibi kendi başına depolar.
/// Sıra korunur, aynı depo bir kez, en çok `MAX_REPOS`.
pub fn discover(folders: &[String], extra: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if out.len() >= MAX_REPOS {
            return;
        }
        if is_repo(dir) {
            out.push(dir.to_path_buf());
            return;
        }
        if depth >= SCAN_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                !name.starts_with('.') && !name.starts_with('$') && !SKIP.contains(&name.as_str())
            })
            .map(|e| e.path())
            .collect();
        dirs.sort();
        for d in dirs {
            walk(&d, depth + 1, out);
        }
    }
    let mut found = Vec::new();
    for folder in folders {
        let path = Path::new(folder.trim());
        if path.is_absolute() && path.is_dir() {
            walk(path, 0, &mut found);
        }
    }
    found.extend(extra.iter().filter_map(|d| repo_root(d)));
    let mut seen = BTreeSet::new();
    // Bileşenlerden yeniden kurmak `C:\a/b` ile `C:\a\b`yi aynı yazar.
    found.retain(|p| seen.insert(p.components().collect::<PathBuf>().to_string_lossy().to_lowercase()));
    found.truncate(MAX_REPOS);
    found
}

/// Eklenecek klasör yolu: boş değil, mutlak, makul uzunlukta, denetim karakteri yok.
pub fn clean_folder(path: &str) -> Option<String> {
    let p = path.trim().trim_end_matches(['\\', '/']);
    let ok = !p.is_empty() && p.chars().count() <= 400 && !p.chars().any(char::is_control) && Path::new(p).is_absolute();
    ok.then(|| p.to_owned())
}

// ─── GitHub Desktop ────────────────────────────────────────────────────────────────────────────────

/// GitHub Desktop depo listesini kendi veritabanında (Chromium IndexedDB, LevelDB) tutar. Biçim belgeli
/// değil; dosyalardaki `X:\...` yolları ham baytlardan çıkarılır (tek baytlık Latin-1 ve UTF-16LE). Sıkıştırılmış
/// kısımlardaki yollar kaçabilir; çağıran her adayın diskte depo olduğunu doğrular. Yalnız yol okunur.
pub fn desktop_paths(bytes: &[u8]) -> Vec<String> {
    fn path_char(c: char) -> bool {
        !c.is_control() && !matches!(c, '"' | '<' | '>' | '|' | '*' | '?') && c != '\u{fffd}'
    }
    fn take(chars: &[char], out: &mut BTreeSet<String>) {
        let mut i = 0;
        while i + 2 < chars.len() {
            if chars[i].is_ascii_alphabetic() && chars[i + 1] == ':' && chars[i + 2] == '\\' && (i == 0 || !chars[i - 1].is_ascii_alphanumeric()) {
                let end = (i + 3..chars.len()).find(|&j| !path_char(chars[j])).unwrap_or(chars.len());
                let p: String = chars[i..end].iter().collect();
                let p = p.trim_end_matches(['\\', ' ']).to_owned();
                if p.len() > 3 && p.chars().count() <= 400 {
                    out.insert(p);
                }
                i = end;
            } else {
                i += 1;
            }
        }
    }
    let mut out = BTreeSet::new();
    let latin: Vec<char> = bytes.iter().map(|b| *b as char).collect();
    take(&latin, &mut out);
    for offset in 0..2 {
        let wide: Vec<char> = bytes
            .get(offset..)
            .unwrap_or(&[])
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| char::from_u32(u16::from_le_bytes(*c) as u32).unwrap_or('\u{fffd}'))
            .collect();
        take(&wide, &mut out);
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(parts: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in parts {
            out.extend_from_slice(p.as_bytes());
            out.push(0);
        }
        out
    }

    #[test]
    fn status_branch_and_files() {
        let out = z(&[
            "# branch.oid 1234567890abcdef",
            "# branch.head main",
            "# branch.upstream origin/main",
            "# branch.ab +2 -1",
            "1 .M N... 100644 100644 100644 aaa bbb src/main.rs",
            "1 A. N... 000000 100644 100644 000 ccc yeni klasör/a b.txt",
            "1 D. N... 100644 000000 000000 ddd 000 eski.txt",
            "2 R. N... 100644 100644 100644 eee eee R100 yeni_ad.rs",
            "eski_ad.rs",
            "u UU N... 100644 100644 100644 100644 f1 f2 f3 cakisma.txt",
            "? .env",
            "? notlar/.env.example",
        ]);
        let st = parse_status(&out);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.ahead, st.behind, st.initial), (2, 1, false));
        let kinds: Vec<_> = st.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("src/main.rs", "modified"),
                ("yeni klasör/a b.txt", "added"),
                ("eski.txt", "deleted"),
                ("yeni_ad.rs", "renamed"),
                ("cakisma.txt", "conflict"),
                (".env", "untracked"),
                ("notlar/.env.example", "untracked"),
            ]
        );
        assert_eq!(st.files[3].orig.as_deref(), Some("eski_ad.rs"));
        assert!(st.files[5].sensitive && !st.files[6].sensitive);
        assert!(st.conflicts());
    }

    #[test]
    fn status_detached_initial_and_cap() {
        let mut parts = vec!["# branch.oid (initial)".to_string(), "# branch.head (detached)".into()];
        parts.extend((0..MAX_FILES + 5).map(|i| format!("? f{i}")));
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        let st = parse_status(&z(&refs));
        assert!(st.initial && st.branch.is_none() && st.upstream.is_none());
        assert_eq!((st.files.len(), st.more), (MAX_FILES, 5));
    }

    #[test]
    fn sensitive_names() {
        for p in [".env", "app/.env.local", "prod.env", "id_rsa", "certs/server.pem", "a.KEY", "x.pfx", "secrets.json", "client_secret_123.json", "config/credentials", "credentials.yml", ".npmrc", ".git-credentials", "service-account-prod.json", "token.txt", "passwords.txt"] {
            assert!(sensitive(p), "{p}");
        }
        for p in [".env.example", "src/.env.sample", "src-tauri/src/secrets.rs", "README.md", "id_rsa.pub", "keyboard.ts", "monkey.png", "tokenizer.py", "lib/token.ts", "keys.json5"] {
            assert!(!sensitive(p), "{p}");
        }
    }

    #[test]
    fn message_rules() {
        assert_eq!(clean_message("  Düzeltme\r\n\r\nAyrıntı\u{7}  ").unwrap(), "Düzeltme\n\nAyrıntı");
        assert!(clean_message(" \n ").is_err());
        assert!(clean_message(&"a".repeat(MAX_MESSAGE + 1)).is_err());
    }

    #[test]
    fn commit_plan_checks_paths_and_sensitive() {
        let st = parse_status(&z(&["# branch.head main", "1 .M N... 1 1 1 a b src/a.rs", "2 R. N... 1 1 1 a a R100 yeni.rs", "eski.rs", "? .env", "? yeni.txt"]));
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let (add, paths) = plan_commit(&st, &s(&["src/a.rs", "yeni.rs", "yeni.txt"]), &[]).unwrap();
        assert_eq!(add, s(&["yeni.txt"]));
        assert_eq!(paths, s(&["eski.rs", "src/a.rs", "yeni.rs", "yeni.txt"]));
        assert!(plan_commit(&st, &s(&[".env"]), &[]).is_err(), "hassas dosya onaysız");
        assert!(plan_commit(&st, &s(&[".env"]), &s(&[".env"])).is_ok());
        assert!(plan_commit(&st, &s(&["../dışarı.txt"]), &[]).is_err(), "listede olmayan yol");
        assert!(plan_commit(&st, &[], &[]).is_err());
        let conflict = parse_status(&z(&["u UU N... 1 1 1 1 a b c x.txt", "? y.txt"]));
        assert!(plan_commit(&conflict, &s(&["y.txt"]), &[]).is_err());
    }

    #[test]
    fn discard_plan_trashes_then_restores() {
        let st = parse_status(&z(&["# branch.head main", "1 .M N... 1 1 1 a b src/a.rs", "1 D. N... 1 0 0 a 0 sil.rs", "2 R. N... 1 1 1 a a R100 yeni.rs", "eski.rs", "? .env"]));
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let plan = plan_discard(&st, &s(&["src/a.rs", "sil.rs", "yeni.rs", ".env"])).unwrap();
        assert_eq!(plan.trash, s(&["src/a.rs", "yeni.rs", ".env"]));
        assert_eq!(plan.restore, s(&["eski.rs", "sil.rs", "src/a.rs", "yeni.rs"]));
        assert!(plan_discard(&st, &s(&["yok.txt"])).is_err());
        assert!(plan_discard(&st, &[]).is_err());
        let conflict = parse_status(&z(&["u UU N... 1 1 1 1 a b c x.txt", "? y.txt"]));
        assert!(plan_discard(&conflict, &s(&["y.txt"])).is_err());
    }

    #[test]
    fn errors_are_turkish_and_clean() {
        assert!(explain("*** Please tell me who you are.").contains("adın"));
        assert!(explain("fatal: could not read Username for 'https://github.com': terminal prompts disabled").contains("giriş"));
        assert!(explain(" ! [rejected]        main -> main (fetch first)").contains("Pull"));
        assert!(explain("fatal: Not possible to fast-forward, aborting.").contains("ayrışmış"));
        assert!(explain("error: Your local changes to the following files would be overwritten by merge:").contains("geri al"));
        let odd = explain("fatal: repository 'https://ali:ghp_SECRET@github.com/a/b.git/' not found");
        assert!(!odd.contains("ghp_SECRET") && odd.contains("github.com/a/b.git"), "{odd}");
        assert_eq!(strip_credentials("x https://u:p@h.com/y z http://h2/w"), "x https://h.com/y z http://h2/w");
    }

    #[test]
    fn discover_scans_two_levels_and_dedupes() {
        let root = std::env::temp_dir().join(format!("zodyvel-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in ["work/api/.git", "work/group/site/.git", "work/group/deep/x/.git", "work/node_modules/lib/.git", "work/.hidden/r/.git", "work/api/sub/inner/.git", "solo/.git"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        std::fs::create_dir_all(root.join("solo/src")).unwrap();
        let folders = vec![root.join("work").to_string_lossy().into_owned(), "göreli/yol".into(), String::new()];
        let found = discover(&folders, &[root.join("solo/src"), root.join("work/api")]);
        let names: Vec<String> = found.iter().map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(names, vec!["work/api", "work/group/site", "solo"]);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(clean_folder(" C:\\work\\ ").as_deref().map(|p| p.ends_with("work")), if cfg!(windows) { Some(true) } else { None });
        assert!(clean_folder("/home/x/").is_some() || cfg!(windows));
        assert!(clean_folder("rel/x").is_none());
    }

    #[test]
    fn desktop_paths_latin1_and_utf16() {
        let mut bytes = b"\x00\x05path\"C:\\Users\\kullanici\\Masa\xfcst\xfc\\work\\Api\"\x10".to_vec();
        for unit in "D:\\Kod\\Şirket".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&[0, 0]);
        let found = desktop_paths(&bytes);
        assert!(found.contains(&"C:\\Users\\kullanici\\Masaüstü\\work\\Api".to_string()), "{found:?}");
        assert!(found.contains(&"D:\\Kod\\Şirket".to_string()), "{found:?}");
    }

    #[test]
    fn token_only_as_github_header() {
        assert!(auth_env(None).is_empty() && auth_env(Some("  ")).is_empty());
        let env = auth_env(Some("ghp_abc"));
        assert_eq!(env[1].1, "http.https://github.com/.extraheader");
        // base64("x-access-token:ghp_abc")
        assert_eq!(env[2].1, "AUTHORIZATION: basic eC1hY2Nlc3MtdG9rZW46Z2hwX2FiYw==");
        assert!(auth_failed("fatal: Authentication failed for 'https://github.com/a/b.git/'"));
        assert!(!auth_failed("fatal: Not possible to fast-forward, aborting."));
    }
}
