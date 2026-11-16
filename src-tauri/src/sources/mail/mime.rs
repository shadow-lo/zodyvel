//! İleti metni çözme (saf, Linux'ta sınanır): MIME encoded-word başlıkları (`=?UTF-8?B?…?=`), karakter
//! kümeleri (UTF-8, Latin-1, Windows-1252/1254, ISO-8859-9), quoted-printable/base64 ve önizleme metni
//! (çok parçalı iletide `text/plain`, yoksa etiketleri soyulmuş HTML). Resim ya da bağlantı yüklenmez;
//! sonuç yalnız bellekte kalır.

/// Önizlemenin en çok karakter sayısı.
pub const PREVIEW_CHARS: usize = 1200;

/// Başlıktaki encoded-word'leri çözer. Yan yana iki encoded-word arasındaki boşluk düşer (RFC 2047).
pub fn decode_header(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::new();
    let mut rest: &str = &text;
    let mut pending_space = String::new();
    let mut last_was_word = false;
    while !rest.is_empty() {
        if let Some(start) = rest.find("=?") {
            let (before, tail) = rest.split_at(start);
            if let Some((decoded, used)) = encoded_word(tail) {
                if !(last_was_word && before.trim().is_empty()) {
                    out.push_str(&pending_space);
                    out.push_str(before);
                }
                pending_space.clear();
                out.push_str(&decoded);
                rest = &tail[used..];
                last_was_word = true;
                continue;
            }
            out.push_str(&pending_space);
            pending_space.clear();
            out.push_str(before);
            out.push_str("=?");
            rest = &tail[2..];
            last_was_word = false;
        } else {
            out.push_str(&pending_space);
            out.push_str(rest);
            break;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `=?charset?enc?text?=` → (metin, kullanılan bayt).
fn encoded_word(s: &str) -> Option<(String, usize)> {
    let body = s.strip_prefix("=?")?;
    let q1 = body.find('?')?;
    let charset = &body[..q1];
    let after = &body[q1 + 1..];
    let q2 = after.find('?')?;
    let enc = &after[..q2];
    let data_start = &after[q2 + 1..];
    let end = data_start.find("?=")?;
    let data = &data_start[..end];
    if data.contains(' ') {
        return None;
    }
    let bytes = match enc.to_ascii_uppercase().as_str() {
        "B" => base64(data.as_bytes()),
        "Q" => quoted_printable(data.replace('_', " ").as_bytes()),
        _ => return None,
    };
    // RFC 2231 dil eki: "UTF-8*tr".
    let charset = charset.split('*').next().unwrap_or(charset);
    let used = 2 + q1 + 1 + q2 + 1 + end + 2;
    Some((to_text(&bytes, charset), used))
}

/// Bayt dizisini karakter kümesine göre metne çevirir; bilinmeyen küme UTF-8 sayılır.
pub fn to_text(bytes: &[u8], charset: &str) -> String {
    let cs = charset.trim().trim_matches('"').to_ascii_lowercase();
    match cs.as_str() {
        "iso-8859-1" | "latin1" | "latin-1" => single_byte(bytes, false, false),
        "windows-1252" | "cp1252" => single_byte(bytes, true, false),
        "iso-8859-9" | "latin5" => single_byte(bytes, false, true),
        "windows-1254" | "cp1254" => single_byte(bytes, true, true),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Windows-1252'nin 0x80–0x9F aralığı.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘', '’', '“',
    '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
];

fn single_byte(bytes: &[u8], windows: bool, turkish: bool) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F if windows => CP1252_HIGH[(b - 0x80) as usize],
            0xD0 if turkish => 'Ğ',
            0xDD if turkish => 'İ',
            0xDE if turkish => 'Ş',
            0xF0 if turkish => 'ğ',
            0xFD if turkish => 'ı',
            0xFE if turkish => 'ş',
            _ => b as char,
        })
        .collect()
}

pub fn base64(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in data {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

pub fn quoted_printable(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        if c == b'=' {
            // Yumuşak satır sonu.
            if data.get(i + 1) == Some(&b'\r') && data.get(i + 2) == Some(&b'\n') {
                i += 3;
                continue;
            }
            if data.get(i + 1) == Some(&b'\n') {
                i += 2;
                continue;
            }
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (data.get(i + 1).and_then(|&b| hex(b)), data.get(i + 2).and_then(|&b| hex(b))) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Basit başlık bloğu: ad (küçük harf) → değer (katlanmış satırlar birleşik).
fn headers(block: &[u8]) -> Vec<(String, String)> {
    let text = String::from_utf8_lossy(block);
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((name, value)) = line.split_once(':') {
            out.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    out
}

fn header<'a>(list: &'a [(String, String)], name: &str) -> &'a str {
    list.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str()).unwrap_or("")
}

/// `text/plain; charset="utf-8"; boundary=x` → ("text/plain", parametre bulucu).
fn content_type(value: &str) -> (String, Vec<(String, String)>) {
    let mut parts = value.split(';');
    let kind = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().trim_matches('"').to_owned()))
        .collect();
    (if kind.is_empty() { "text/plain".into() } else { kind }, params)
}

fn param<'a>(params: &'a [(String, String)], name: &str) -> &'a str {
    params.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str()).unwrap_or("")
}

fn split_head_body(part: &[u8]) -> (&[u8], &[u8]) {
    for (i, w) in part.windows(4).enumerate() {
        if w == b"\r\n\r\n" {
            return (&part[..i], &part[i + 4..]);
        }
    }
    for (i, w) in part.windows(2).enumerate() {
        if w == b"\n\n" {
            return (&part[..i], &part[i + 2..]);
        }
    }
    (part, &[])
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// Çok parçalı gövdeyi parçalara böler (kesik son parça da döner).
fn multipart_parts<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let marker = format!("--{boundary}");
    let marker = marker.as_bytes();
    let mut parts = Vec::new();
    let Some(mut pos) = find(body, marker, 0) else { return parts };
    loop {
        let start = pos + marker.len();
        if body[start..].starts_with(b"--") {
            break;
        }
        let next = find(body, marker, start);
        let end = next.unwrap_or(body.len());
        let mut part = &body[start..end];
        // Sınır satırının sonu ve bir sonraki sınırdan önceki CRLF.
        while let [b'\r' | b'\n' | b' ' | b'\t', rest @ ..] = part {
            part = rest;
        }
        let part = part.strip_suffix(b"\r\n").or_else(|| part.strip_suffix(b"\n")).unwrap_or(part);
        parts.push(part);
        match next {
            Some(n) => pos = n,
            None => break,
        }
    }
    parts
}

/// Önizleme adayı: (tür, metin).
fn pick_text(head: &[(String, String)], body: &[u8], depth: u8) -> Option<(bool, String)> {
    let (kind, params) = content_type(header(head, "content-type"));
    let disposition = header(head, "content-disposition").to_ascii_lowercase();
    if disposition.starts_with("attachment") {
        return None;
    }
    if kind.starts_with("multipart/") {
        if depth > 4 {
            return None;
        }
        let boundary = param(&params, "boundary");
        if boundary.is_empty() {
            return None;
        }
        let mut html = None;
        for part in multipart_parts(body, boundary) {
            let (h, b) = split_head_body(part);
            match pick_text(&headers(h), b, depth + 1) {
                Some((true, text)) if !text.trim().is_empty() => return Some((true, text)),
                Some((false, text)) if html.is_none() => html = Some(text),
                _ => {}
            }
        }
        return html.map(|t| (false, t));
    }
    let plain = kind == "text/plain";
    if !plain && kind != "text/html" {
        return None;
    }
    let encoding = header(head, "content-transfer-encoding").to_ascii_lowercase();
    let raw = match encoding.as_str() {
        "base64" => base64(body),
        "quoted-printable" => quoted_printable(body),
        _ => body.to_vec(),
    };
    let text = to_text(&raw, param(&params, "charset"));
    Some((plain, if plain { text } else { strip_html(&text) }))
}

/// İletinin düz metin önizlemesi. `head`: üst düzey `Content-Type`/`Content-Transfer-Encoding` başlıkları,
/// `body`: `BODY[TEXT]` (kesik olabilir).
pub fn preview(head: &[u8], body: &[u8]) -> String {
    let text = pick_text(&headers(head), body, 0).map(|(_, t)| t).unwrap_or_default();
    tidy(&text, PREVIEW_CHARS)
}

/// Satırları kırpar, alıntı satırlarını (`>`) ve art arda boş satırları atar, `limit` karakterde keser.
pub fn tidy(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut blank = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('>') {
            continue;
        }
        if line.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if blank {
            out.push_str("\n\n");
        } else if !out.is_empty() {
            out.push('\n');
        }
        blank = false;
        out.push_str(&line.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if out.chars().count() > limit {
        out = out.chars().take(limit).collect::<String>().trim_end().to_owned();
        out.push('…');
    }
    out
}

/// HTML'den düz metin: script/style/head atılır, blok etiketleri satır sonu olur, varlıklar çözülür.
pub fn strip_html(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len() / 2);
    let mut i = 0;
    let bytes = html.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let Some(close) = html[i..].find('>') else { break };
            let tag = &lower[i + 1..i + close];
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let opening = !tag.starts_with('/');
            i += close + 1;
            if opening && matches!(name.as_str(), "script" | "style" | "head" | "title") {
                let end = format!("</{name}");
                i = lower[i..].find(&end).map(|p| i + p).unwrap_or(bytes.len());
                continue;
            }
            if tag.starts_with("!--") {
                continue;
            }
            if matches!(name.as_str(), "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "table" | "blockquote") {
                // Art arda blok etiketleri tek satır sonu (`<br>` hariç: o bilerek boş satır açar).
                if name == "br" || !out.trim_end_matches([' ', '\t']).ends_with('\n') {
                    out.push('\n');
                }
            } else if matches!(name.as_str(), "td" | "th") {
                out.push(' ');
            }
            continue;
        }
        let next = html[i..].find('<').map(|p| i + p).unwrap_or(bytes.len());
        out.push_str(&entities(&html[i..next]));
        i = next;
    }
    out
}

fn entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        // Bayt olarak aranır: 12. bayt çok baytlı bir harfin ortasına düşebilir ("& Şubat"), dizgi dilimi çöktürür.
        let Some(semi) = tail.bytes().take(12).position(|b| b == b';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..semi];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ if name.starts_with("#x") || name.starts_with("#X") => u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32),
            _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &tail[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_scan_never_splits_multibyte_letters() {
        // "&" ardından 12. bayt "ş"/"İ" ortasına düşen Türkçe metin (MHRS bildirimleri) çökmemeli.
        assert_eq!(entities("Randevu & Şubat İptal"), "Randevu & Şubat İptal");
        assert_eq!(entities("a &ğğğğğğğğ; b"), "a &ğğğğğğğğ; b");
        assert_eq!(entities("x &amp; y &#351;"), "x & y ş");
        assert_eq!(strip_html("<p>Randevu &İptal ş</p>"), "\nRandevu &İptal ş\n");
    }

    #[test]
    fn headers_decode_encoded_words() {
        assert_eq!(decode_header(b"=?UTF-8?B?w4dhbMSxxZ9tYSBwbGFuxLE=?="), "Çalışma planı");
        assert_eq!(decode_header(b"=?utf-8?Q?G=C3=BCnayd=C4=B1n_d=C3=BCnya?="), "Günaydın dünya");
        assert_eq!(decode_header(b"=?UTF-8?B?w4dh?= =?UTF-8?B?bMSxxZ9tYQ==?= plan"), "Çalışma plan");
        assert_eq!(decode_header(b"Re: =?ISO-8859-9?Q?=DEubat_raporu?="), "Re: Şubat raporu");
        assert_eq!(decode_header(b"=?windows-1254?Q?=FDyi_ak=FEamlar?="), "ıyi akşamlar");
        assert_eq!(decode_header(b"Fiyat =?bozuk"), "Fiyat =?bozuk");
        assert_eq!(decode_header("Düz UTF-8 konu".as_bytes()), "Düz UTF-8 konu");
    }

    #[test]
    fn decodes_transfer_encodings() {
        assert_eq!(quoted_printable(b"Merhaba=2C d=C3=BCnya=\r\n!"), "Merhaba, dünya!".as_bytes());
        assert_eq!(base64(b"TWVy\r\naGFiYQ=="), b"Merhaba");
    }

    #[test]
    fn preview_prefers_plain_part() {
        let head = b"Content-Type: multipart/alternative;\r\n boundary=\"b1\"\r\n";
        let body = b"--b1\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nSelam,\r\nyar=C4=B1n g=C3=B6r=C3=BC=C5=9Fel=\r\nim mi?\r\n\r\n> eski al=C4=B1nt=C4=B1\r\n--b1\r\nContent-Type: text/html\r\n\r\n<p>HTML</p>\r\n--b1--\r\n";
        assert_eq!(preview(head, body), "Selam,\nyarın görüşelim mi?");
    }

    #[test]
    fn preview_falls_back_to_html_and_nested_parts() {
        let head = b"Content-Type: multipart/mixed; boundary=out";
        let body = b"--out\r\nContent-Type: multipart/alternative; boundary=in\r\n\r\n--in\r\nContent-Type: text/html; charset=\"UTF-8\"\r\nContent-Transfer-Encoding: base64\r\n\r\nPGh0bWw+PHN0eWxlPnB7Y29sb3I6cmVkfTwvc3R5bGU+PHA+RmF0dXJhICZhbXA7IMO2ZGVtZTwvcD48aW1nIHNyYz0iaHR0cDovL3giPjxwPlRvcGxhbTogMTAmbmJzcDvigqw8L3A+PC9odG1sPg==\r\n--in--\r\n--out\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment\r\n\r\nJVBERi0=\r\n--out--";
        assert_eq!(preview(head, body), "Fatura & ödeme\nToplam: 10 €");
    }

    #[test]
    fn preview_handles_single_part_and_truncation() {
        let head = b"Content-Type: text/plain; charset=iso-8859-9\r\nContent-Transfer-Encoding: 8bit";
        assert_eq!(preview(head, b"G\xfcnayd\xfdn \xdeadow"), "Günaydın Şadow");
        assert_eq!(preview(b"", b"  sadece metin  "), "sadece metin");
        let long = "ç".repeat(2000);
        let p = preview(b"Content-Type: text/plain; charset=utf-8", long.as_bytes());
        assert_eq!(p.chars().count(), PREVIEW_CHARS + 1);
        assert!(p.ends_with('…'));
        // Kesik çok parçalı gövde (32 KB sınırı) yine de metin verir.
        let cut = b"--b\r\nContent-Type: text/plain\r\n\r\nYar\xc4\xb1m kal";
        assert_eq!(preview(b"Content-Type: multipart/mixed; boundary=b", cut), "Yarım kal");
    }

    #[test]
    fn html_entities_and_comments() {
        assert_eq!(tidy(&strip_html("<!-- x --><div>A&lt;B &#287;&#x131;</div><br>&bogus; son"), 100), "A<B ğı\n\n&bogus; son");
    }
}
