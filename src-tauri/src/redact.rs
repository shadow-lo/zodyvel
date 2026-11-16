//! "Sorun bildir" çıktısından kişisel veriyi ayıklar. Saf metin işlemi: dosya ya da pencere bilmez, test edilir.
//!
//! - Dosya yolları: `C:\Users\ali\Desktop\rapor.docx` → `C:\Users\…\Desktop\…`; yolun sonundaki dosya ya da
//!   klasör adı gider. Yol boşluk içerebildiği için yol, tırnak, `<>|*?`, satır sonu, `, ` ya da `; ` ile bitene
//!   kadar sürer (fazla ayıklamak eksik ayıklamaktan iyidir).
//! - Kullanıcı ve bilgisayar adı (`extra`): metnin her yerinde `…` olur.
//! - E-posta adresleri: `…@…`.
//! - Çok uzun satırlar kısaltılır (olay içeriği sızdırmasın diye).

/// Bir satırdan en çok bu kadar karakter kalır.
const LINE_MAX: usize = 300;

pub fn redact(text: &str, extra: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(&redact_line(line, extra));
        out.push('\n');
    }
    out
}

fn redact_line(line: &str, extra: &[&str]) -> String {
    let line = paths(line);
    let line = emails(&line);
    let mut line = names(&line, extra);
    if line.chars().count() > LINE_MAX {
        line = line.chars().take(LINE_MAX).collect::<String>() + "…";
    }
    line
}

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `X:\` ya da `X:/` ile başlayan yer (öncesi harf/rakam değilse).
fn drive_at(chars: &[char], i: usize) -> bool {
    chars.get(i).is_some_and(|c| c.is_ascii_alphabetic())
        && chars.get(i + 1) == Some(&':')
        && chars.get(i + 2).is_some_and(|c| is_sep(*c))
        && (i == 0 || !chars[i - 1].is_alphanumeric())
}

fn unc_at(chars: &[char], i: usize) -> bool {
    chars.get(i) == Some(&'\\') && chars.get(i + 1) == Some(&'\\') && (i == 0 || !chars[i - 1].is_alphanumeric())
}

fn path_ends(chars: &[char], i: usize) -> bool {
    match chars[i] {
        '"' | '\'' | '<' | '>' | '|' | '*' | '?' | ')' | ']' | '}' => true,
        ',' | ';' => chars.get(i + 1).is_none_or(|n| n.is_whitespace()),
        _ => false,
    }
}

fn paths(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        if !(drive_at(&chars, i) || unc_at(&chars, i)) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut end = i;
        while end < chars.len() && !path_ends(&chars, end) {
            end += 1;
        }
        // Yolun sonundaki boşluklar yola ait değil.
        while end > i && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let path: String = chars[i..end].iter().collect();
        out.push_str(&clean_path(&path));
        i = end;
    }
    out
}

fn clean_path(path: &str) -> String {
    let sep = if path.contains('\\') { '\\' } else { '/' };
    let mut parts: Vec<String> = path.split(is_sep).map(str::to_owned).collect();
    // Profil klasörü: .../Users/<ad>/...
    if let Some(at) = parts.iter().position(|p| p.eq_ignore_ascii_case("users"))
        && at + 1 < parts.len()
        && !parts[at + 1].is_empty()
    {
        parts[at + 1] = "…".into();
    }
    // Son parça (dosya ya da klasör adı); kök ve boş parçalar korunur.
    if parts.len() > 2
        && let Some(last) = parts.last_mut()
        && !last.is_empty()
    {
        *last = "…".into();
    }
    parts.join(&sep.to_string())
}

fn emails(line: &str) -> String {
    line.split_inclusive(char::is_whitespace)
        .map(|word| {
            let core = word.trim_end();
            let rest = &word[core.len()..];
            match core.find('@') {
                Some(at) if at > 0 && core[at + 1..].contains('.') => format!("…@…{rest}"),
                _ => word.to_owned(),
            }
        })
        .collect()
}

fn names(line: &str, extra: &[&str]) -> String {
    let mut line = line.to_owned();
    for name in extra.iter().filter(|n| n.chars().count() >= 3) {
        line = replace_ci(&line, name, "…");
    }
    line
}

/// Büyük/küçük harf gözetmeden değiştirir (yalnız tam sözcük sınırında, kısa adlar başka sözcükleri bozmasın).
fn replace_ci(haystack: &str, needle: &str, with: &str) -> String {
    let lower = lower_same_len(haystack);
    let needle = lower_same_len(needle);
    let mut out = String::with_capacity(haystack.len());
    let mut from = 0;
    while let Some(found) = lower[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let before = haystack[..start].chars().next_back();
        let after = haystack[end..].chars().next();
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
        out.push_str(&haystack[from..start]);
        out.push_str(if word(before) || word(after) { &haystack[start..end] } else { with });
        from = end;
    }
    out.push_str(&haystack[from..]);
    out
}

/// Harf harf küçültür; bayt uzunluğunu değiştiren harf (ör. İ) olduğu gibi kalır, böylece konumlar
/// özgün metinle aynı olur ve satırın geri kalanı yine ayıklanır.
fn lower_same_len(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(l), None) if l.len_utf8() == c.len_utf8() => l,
                _ => c,
            }
        })
        .collect()
}

/// Telefona giden kısa metin (onay özeti, soru): yollar, e-postalar ve gizli görünen değerler maskelenir.
/// Tek satırlık çıktı verir (boşluklar tek boşluğa iner).
pub fn redact_snippet(text: &str) -> String {
    let line = emails(&paths(text));
    let mut out: Vec<String> = Vec::new();
    let mut mask_next = false;
    for w in line.split_whitespace() {
        let lw = w.to_ascii_lowercase();
        if mask_next {
            if lw == "bearer" || lw == "basic" {
                out.push(w.to_owned());
                continue;
            }
            mask_next = false;
            out.push("…".into());
            continue;
        }
        // "Bearer X", "Authorization: X", "-H Authorization: Bearer X" → sonraki sözcük gizlenir.
        if lw == "bearer" || lw == "basic" || lw.trim_end_matches(':') == "authorization" || lw == "-u" || lw == "--password" || lw == "--token" {
            out.push(w.to_owned());
            mask_next = true;
            continue;
        }
        // ANAHTAR=değer / ANAHTAR:değer, anahtar adı gizli çağrışımlıysa değer gider.
        if let Some(i) = w.find(['=', ':'])
            && i > 0
            && ["token", "key", "secret", "pass", "pwd", "auth", "cred"].iter().any(|k| lw[..i].contains(k))
            && w.len() > i + 1
            && !w[i + 1..].starts_with("//")
        {
            out.push(format!("{}…", &w[..=i]));
            continue;
        }
        out.push(if looks_secret(w) { "…".into() } else { w.to_owned() });
    }
    out.join(" ")
}

/// Uzun, rastgele görünen belirteç: ≥ 24 karakter, harf ve rakam karışık, boşluksuz.
fn looks_secret(w: &str) -> bool {
    let t = w.trim_matches(|c: char| !c.is_alphanumeric());
    t.chars().count() >= 24
        && t.chars().all(|c| c.is_ascii_alphanumeric() || "_-+/=.".contains(c))
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_profile_and_file_names_go() {
        let out = redact(r"yol C:\Users\ali\Desktop\rapor final.docx hata", &[]);
        assert_eq!(out, "yol C:\\Users\\…\\Desktop\\…\n");
    }

    #[test]
    fn forward_slashes_and_quotes() {
        let out = redact(r#"açılamadı "D:/Users/Ayşe/Belgeler/gizli.pdf" kod 2"#, &[]);
        assert_eq!(out, "açılamadı \"D:/Users/…/Belgeler/…\" kod 2\n");
    }

    #[test]
    fn plain_text_and_short_roots_are_kept() {
        assert_eq!(redact("shelf: bırakıldı, 3 yeni, rafta 5", &[]), "shelf: bırakıldı, 3 yeni, rafta 5\n");
        assert_eq!(redact(r"kök C:\ tamam", &[]), "kök C:\\ tamam\n");
        assert_eq!(redact("saat 12:30 sonra", &[]), "saat 12:30 sonra\n");
    }

    #[test]
    fn comma_ends_a_path() {
        let out = redact(r"C:\Users\ali\a.txt, 2 dosya", &[]);
        assert_eq!(out, "C:\\Users\\…\\…, 2 dosya\n");
    }

    #[test]
    fn unc_paths() {
        assert_eq!(redact(r"\\sunucu\paylasim\dosya.txt", &[]), "\\\\sunucu\\paylasim\\…\n");
    }

    #[test]
    fn emails_and_names() {
        assert_eq!(redact("giriş ornek@example.com tamam", &[]), "giriş …@… tamam\n");
        let out = redact("Kullanıcı DEMIR makine Masaustu1; demirli ayrı", &["demir", "masaustu1", "ab"]);
        assert_eq!(out, "Kullanıcı … makine …; demirli ayrı\n");
    }

    #[test]
    fn names_go_even_next_to_dotted_capital_i() {
        assert_eq!(redact("İzin istendi: demir", &["demir"]), "İzin istendi: …\n");
    }

    #[test]
    fn long_lines_are_cut() {
        let long = "a".repeat(500);
        let out = redact(&long, &[]);
        assert_eq!(out.chars().count(), LINE_MAX + 2);
        assert!(out.ends_with("…\n"));
    }

    #[test]
    fn many_lines_keep_order() {
        let out = redact("bir\niki C:\\Users\\x\\y.txt\nüç", &[]);
        assert_eq!(out, "bir\niki C:\\Users\\…\\…\nüç\n");
    }
}

#[cfg(test)]
mod snippet_tests {
    use super::redact_snippet;

    #[test]
    fn secrets_paths_and_emails_are_masked_for_phone() {
        let s = redact_snippet(r#"curl -H Authorization: Bearer abcDEF123 https://x.io -o C:\Users\ali\Desktop\a.txt mail ali@x.com"#);
        assert!(!s.contains("abcDEF123") && !s.contains("ali@x.com") && !s.contains("ali\\"), "{s}");
        assert!(s.contains("curl") && s.contains("https://x.io"));
        assert_eq!(redact_snippet("export API_TOKEN=sk-123456 ls"), "export API_TOKEN=… ls");
        assert_eq!(redact_snippet("npm run build"), "npm run build");
        let long = redact_snippet("run ghp_1234567890abcdefghijklmnopqrstuvwxyz now");
        assert_eq!(long, "run … now");
    }
}
