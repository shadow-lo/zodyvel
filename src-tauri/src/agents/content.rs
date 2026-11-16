//! Ayrıntı ekranında bir adımın içeriği: okunan satırlar, düzenleme farkı, komut ve çıktısı.
//!
//! Yalnız kullanıcı ayrıntıyı açınca hesaplanır (olaylarla birlikte taşınmaz). Dosyalar bu
//! bilgisayarda, yapay zekânın zaten eriştiği yoldan okunur; içerik hiçbir yere gönderilmez.
//! Büyük ve ikili dosyalar okunmaz, satır sayısı ve uzunluğu sınırlıdır.

use std::fs;

use serde::Serialize;
use zodyvel_hook::diff::{self, Op};

use super::describe::Raw;

const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_LINES: usize = 80;
const MAX_LINE_CHARS: usize = 240;
const CONTEXT: usize = 3;
const DEFAULT_READ: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    Add,
    Del,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    /// Dosyadaki satır numarası (bilinmiyorsa yok).
    pub n: Option<usize>,
    pub text: String,
    pub mark: Option<Mark>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct StepContent {
    /// "file" | "diff" | "output" | "none"
    pub kind: &'static str,
    pub path: Option<String>,
    pub lines: Vec<Line>,
    pub note: Option<String>,
}

fn clip(line: &str) -> String {
    let line = line.trim_end_matches('\r');
    if line.chars().count() > MAX_LINE_CHARS { line.chars().take(MAX_LINE_CHARS).collect::<String>() + "…" } else { line.to_owned() }
}

fn plain(n: Option<usize>, text: &str) -> Line {
    Line { n, text: clip(text), mark: None }
}

fn marked(n: Option<usize>, text: &str, mark: Mark) -> Line {
    Line { n, text: clip(text), mark: Some(mark) }
}

/// Metin dosyasını okur; yoksa, büyükse veya ikiliyse gerekçeyle hata döner.
fn read_text(path: &str) -> Result<String, String> {
    let meta = fs::metadata(path).map_err(|_| "Dosya artık yok ya da okunamadı.".to_owned())?;
    if meta.len() > MAX_FILE {
        return Err("Dosya çok büyük; içerik gösterilmiyor.".into());
    }
    let bytes = fs::read(path).map_err(|_| "Dosya okunamadı.".to_owned())?;
    if bytes.contains(&0) {
        return Err("İkili dosya; içerik gösterilmiyor.".into());
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `needle`'ın başladığı satır (1'den).
fn line_of(text: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    text.find(needle).map(|pos| text[..pos].matches('\n').count() + 1)
}

pub fn build(raw: &Raw) -> StepContent {
    match raw {
        Raw::None => StepContent { kind: "none", ..Default::default() },
        Raw::Read { path, offset, limit } => read(path, *offset, *limit),
        Raw::Edit { path, old, new } => edit(path, old, new),
        Raw::Write { path, content } => StepContent {
            kind: "diff",
            path: Some(path.clone()),
            lines: content.lines().take(MAX_LINES).enumerate().map(|(i, l)| marked(Some(i + 1), l, Mark::Add)).collect(),
            note: None,
        },
        Raw::Bash { command, output } => {
            let mut lines = vec![plain(None, &format!("$ {}", command.lines().next().unwrap_or("")))];
            match output {
                Some(out) => {
                    let all: Vec<&str> = out.lines().collect();
                    lines.extend(all[all.len().saturating_sub(MAX_LINES)..].iter().map(|l| plain(None, l)));
                }
                None => lines.push(plain(None, "(çıktı bekleniyor)")),
            }
            StepContent { kind: "output", path: None, lines, note: None }
        }
        Raw::Patch { text } => StepContent {
            kind: "diff",
            path: None,
            lines: text
                .lines()
                .take(MAX_LINES)
                .map(|l| match l.chars().next() {
                    Some('+') if !l.starts_with("+++") => marked(None, &l[1..], Mark::Add),
                    Some('-') if !l.starts_with("---") => marked(None, &l[1..], Mark::Del),
                    _ => plain(None, l),
                })
                .collect(),
            note: None,
        },
    }
}

fn read(path: &str, offset: Option<u64>, limit: Option<u64>) -> StepContent {
    let mut out = StepContent { kind: "file", path: Some(path.to_owned()), ..Default::default() };
    match read_text(path) {
        Ok(text) => {
            let start = offset.map(|o| o.max(1) as usize).unwrap_or(1);
            let count = limit.map(|l| l as usize).unwrap_or(DEFAULT_READ).min(MAX_LINES);
            out.lines = text.lines().enumerate().skip(start - 1).take(count).map(|(i, l)| plain(Some(i + 1), l)).collect();
        }
        Err(note) => out.note = Some(note),
    }
    out
}

/// Düzenleme farkı: dosyadaki yerini bulabilirse satır numaraları ve çevresiyle.
fn edit(path: &str, old: &str, new: &str) -> StepContent {
    let mut out = StepContent { kind: "diff", path: Some(path.to_owned()), ..Default::default() };
    let file = read_text(path).ok();
    // Düzenleme uygulandıysa yeni metin, henüz uygulanmadıysa eski metin dosyadadır.
    let located = file.as_deref().and_then(|t| line_of(t, new).map(|n| (n, new)).or_else(|| line_of(t, old).map(|n| (n, old))));
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    match (file.as_deref(), located) {
        (Some(text), Some((start, present))) => {
            let all: Vec<&str> = text.lines().collect();
            let before = start.saturating_sub(1 + CONTEXT);
            out.lines.extend((before..start - 1).map(|i| plain(Some(i + 1), all[i])));
            out.lines.extend(block(&old_lines, &new_lines, start));
            let after = start - 1 + present.lines().count().max(1);
            out.lines.extend((after..(after + CONTEXT).min(all.len())).map(|i| plain(Some(i + 1), all[i])));
        }
        _ => {
            out.lines.extend(block(&old_lines, &new_lines, 0).into_iter().map(|l| Line { n: None, ..l }));
            if file.is_none() {
                out.note = Some("Dosya okunamadı; yalnız değişen metin gösteriliyor.".into());
            }
        }
    }
    out.lines.truncate(MAX_LINES);
    out
}

/// Değişen metin bloğu satır satır: aynı kalan satırlar düz, yalnız değişenler işaretli.
/// `start`: bloğun dosyadaki ilk satırı (1'den).
fn block(old: &[&str], new: &[&str], start: usize) -> Vec<Line> {
    diff::lines(old, new)
        .into_iter()
        .map(|op| match op {
            Op::Same(_, j) => plain(Some(start + j), new[j]),
            Op::Del(i) => marked(Some(start + i), old[i], Mark::Del),
            Op::Add(j) => marked(Some(start + j), new[j], Mark::Add),
        })
        .collect()
}

/// Bekleyen bir onay isteğinin farkı: dosya henüz değişmedi. "Write" mevcut dosyayla
/// karşılaştırılır (yalnız değişen yerler ve çevresi); diğerleri adım içeriğiyle aynı.
pub fn build_request(raw: &Raw) -> StepContent {
    let Raw::Write { path, content } = raw else { return build(raw) };
    let mut out = StepContent { kind: "diff", path: Some(path.clone()), ..Default::default() };
    let old = match read_text(path) {
        Ok(text) => text,
        Err(_) if fs::metadata(path).is_err() => {
            out.note = Some("Yeni dosya.".into());
            String::new()
        }
        Err(note) => {
            out.note = Some(note);
            return out;
        }
    };
    let (old, new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), content.lines().collect());
    let ops = diff::lines(&old, &new);
    // Değişen satırın çevresindeki CONTEXT satır gösterilir; aradaki uzun aynı bölümler "⋯" olur.
    let near = |k: usize| ops[k.saturating_sub(CONTEXT)..(k + CONTEXT + 1).min(ops.len())].iter().any(|op| !matches!(op, Op::Same(..)));
    let mut skipped = false;
    for (k, op) in ops.iter().enumerate() {
        match *op {
            Op::Same(..) if !near(k) => {
                if !skipped {
                    out.lines.push(plain(None, "⋯"));
                    skipped = true;
                }
                continue;
            }
            Op::Same(_, j) => out.lines.push(plain(Some(j + 1), new[j])),
            Op::Del(i) => out.lines.push(marked(Some(i + 1), old[i], Mark::Del)),
            Op::Add(j) => out.lines.push(marked(Some(j + 1), new[j], Mark::Add)),
        }
        skipped = false;
    }
    if out.lines.len() > MAX_LINES {
        out.lines.truncate(MAX_LINES);
        out.note.get_or_insert_with(|| "Fark uzun; ilk satırlar gösteriliyor.".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, text: &str) -> String {
        let path = std::env::temp_dir().join(format!("zodyvel-content-{}-{name}", std::process::id()));
        fs::write(&path, text).unwrap();
        path.display().to_string()
    }

    #[test]
    fn read_shows_requested_lines_with_numbers() {
        let path = temp("read.txt", "a\nb\nc\nd\ne\n");
        let c = build(&Raw::Read { path: path.clone(), offset: Some(2), limit: Some(2) });
        assert_eq!(c.kind, "file");
        assert_eq!(c.lines, vec![plain(Some(2), "b"), plain(Some(3), "c")]);
        let missing = build(&Raw::Read { path: format!("{path}.yok"), offset: None, limit: None });
        assert!(missing.lines.is_empty() && missing.note.is_some());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_locates_change_and_adds_context() {
        let path = temp("edit.ts", "1\n2\n3\n4\nYENİ\n6\n7\n8\n9\n");
        let c = build(&Raw::Edit { path: path.clone(), old: "ESKİ".into(), new: "YENİ".into() });
        let marks: Vec<(Option<usize>, &str, Option<Mark>)> = c.lines.iter().map(|l| (l.n, l.text.as_str(), l.mark)).collect();
        assert_eq!(
            marks,
            vec![
                (Some(2), "2", None),
                (Some(3), "3", None),
                (Some(4), "4", None),
                (Some(5), "ESKİ", Some(Mark::Del)),
                (Some(5), "YENİ", Some(Mark::Add)),
                (Some(6), "6", None),
                (Some(7), "7", None),
                (Some(8), "8", None),
            ]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_without_file_shows_only_change() {
        let c = build(&Raw::Edit { path: "C:\\yok\\x.ts".into(), old: "a".into(), new: "b".into() });
        assert_eq!(c.lines, vec![marked(None, "a", Mark::Del), marked(None, "b", Mark::Add)]);
        assert!(c.note.is_some());
    }

    #[test]
    fn bash_shows_command_and_output_tail() {
        let c = build(&Raw::Bash { command: "pnpm test\n--x".into(), output: Some("ok\n17 passed".into()) });
        assert_eq!(c.lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["$ pnpm test", "ok", "17 passed"]);
    }

    #[test]
    fn binary_files_are_not_shown() {
        let path = std::env::temp_dir().join(format!("zodyvel-content-{}-bin", std::process::id()));
        fs::write(&path, [0u8, 1, 2]).unwrap();
        let c = build(&Raw::Read { path: path.display().to_string(), offset: None, limit: None });
        assert!(c.lines.is_empty() && c.note.unwrap().contains("İkili"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_marks_only_changed_lines() {
        let c = build(&Raw::Edit { path: "C:\\yok\\x.ts".into(), old: "a\nb\nc".into(), new: "a\nB\nc".into() });
        let marks: Vec<Option<Mark>> = c.lines.iter().map(|l| l.mark).collect();
        assert_eq!(marks, vec![None, Some(Mark::Del), Some(Mark::Add), None]);
    }

    #[test]
    fn pending_write_diffs_against_current_file() {
        let old: String = (1..=20).map(|i| format!("{i}\n")).collect();
        let path = temp("write.txt", &old);
        let new = old.replace("10\n", "on\n");
        let c = build_request(&Raw::Write { path: path.clone(), content: new });
        let changed: Vec<(Option<usize>, &str, Option<Mark>)> =
            c.lines.iter().filter(|l| l.mark.is_some()).map(|l| (l.n, l.text.as_str(), l.mark)).collect();
        assert_eq!(changed, vec![(Some(10), "10", Some(Mark::Del)), (Some(10), "on", Some(Mark::Add))]);
        // Uzak, değişmeyen satırlar gösterilmez.
        assert_eq!(c.lines.first().map(|l| l.text.as_str()), Some("⋯"));
        assert!(c.lines.iter().all(|l| l.text != "1" && l.text != "20"));
        let fresh = build_request(&Raw::Write { path: format!("{path}.yeni"), content: "x\ny".into() });
        assert_eq!(fresh.lines.len(), 2);
        assert_eq!(fresh.note.as_deref(), Some("Yeni dosya."));
        let _ = fs::remove_file(path);
    }
}
