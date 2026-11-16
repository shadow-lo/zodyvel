//! Satır farkı ve onay kartındaki "+N −M" sayıları.
//!
//! Köprü sayıları onay isteği gelince, aracın verdiği tam (kırpılmamış) metinden hesaplar; ada
//! aynı fonksiyonla fark ekranını üretir. Ortak baş ve son satırlar atlanır, kalan bölüm en uzun
//! ortak alt dizi (LCS) ile karşılaştırılır. Kalan bölüm çok büyükse tümü silinmiş + eklenmiş sayılır
//! (sayı biraz büyük çıkar ama hiçbir zaman eksik çıkmaz).

use std::fs;

use serde_json::{Value, json};

/// LCS tablosunun en fazla hücre sayısı (≈ 8 MB).
const MAX_CELLS: usize = 2_000_000;
/// "Write" isteğinde mevcut dosyayla karşılaştırma yapılacak en büyük dosya.
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Eski ve yeni metindeki satır sırası (0'dan).
    Same(usize, usize),
    Del(usize),
    Add(usize),
}

/// İki satır dizisi arasındaki fark, sırasıyla.
pub fn lines(old: &[&str], new: &[&str]) -> Vec<Op> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);

    let mut ops: Vec<Op> = (0..head).map(|i| Op::Same(i, i)).collect();
    if a.len().saturating_mul(b.len()) > MAX_CELLS {
        ops.extend((0..a.len()).map(|i| Op::Del(head + i)));
        ops.extend((0..b.len()).map(|j| Op::Add(head + j)));
    } else {
        // table[i][j]: a[i..] ile b[j..] arasındaki LCS uzunluğu.
        let width = b.len() + 1;
        let mut table = vec![0u32; (a.len() + 1) * width];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                table[i * width + j] =
                    if a[i] == b[j] { table[(i + 1) * width + j + 1] + 1 } else { table[(i + 1) * width + j].max(table[i * width + j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                ops.push(Op::Same(head + i, head + j));
                i += 1;
                j += 1;
            } else if i < a.len() && (j == b.len() || table[(i + 1) * width + j] >= table[i * width + j + 1]) {
                // Silinen satırlar eklenenlerden önce gelir.
                ops.push(Op::Del(head + i));
                i += 1;
            } else {
                ops.push(Op::Add(head + j));
                j += 1;
            }
        }
    }
    ops.extend((0..tail).map(|k| Op::Same(old.len() - tail + k, new.len() - tail + k)));
    ops
}

/// Eklenen ve silinen satır sayısı.
pub fn count(old: &str, new: &str) -> (usize, usize) {
    let (old, new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    lines(&old, &new).iter().fold((0, 0), |(add, del), op| match op {
        Op::Add(_) => (add + 1, del),
        Op::Del(_) => (add, del + 1),
        Op::Same(..) => (add, del),
    })
}

fn text<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|k| value.get(*k).and_then(Value::as_str)).unwrap_or("")
}

/// Mevcut metin dosyası; yoksa boş (yeni dosya). Büyük ya da ikili dosyada sayı verilmez.
fn current_file(path: &str) -> Option<String> {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_FILE => None,
        Ok(_) => {
            let bytes = fs::read(path).ok()?;
            (!bytes.contains(&0)).then(|| String::from_utf8_lossy(&bytes).into_owned())
        }
        Err(_) => Some(String::new()),
    }
}

/// Codex'in `apply_patch` metnindeki dosyalar ve satır sayıları.
fn patch_stats(patch: &str) -> Vec<(String, usize, usize)> {
    let mut files: Vec<(String, usize, usize)> = Vec::new();
    for line in patch.lines() {
        let header = ["*** Update File: ", "*** Add File: ", "*** Delete File: "].iter().find_map(|p| line.strip_prefix(p));
        if let Some(path) = header {
            files.push((path.trim().to_owned(), 0, 0));
        } else if line.starts_with("***") || line.starts_with("@@") {
            continue;
        } else if let Some(file) = files.last_mut() {
            match line.chars().next() {
                Some('+') => file.1 += 1,
                Some('-') => file.2 += 1,
                _ => {}
            }
        }
    }
    files
}

/// Dosya değiştiren bir araç isteğinin dosya bazında "+N −M" sayıları:
/// `[{ "path", "added", "removed" }]`. Dosya değiştirmeyen araçlarda `None`.
pub fn edit_stats(tool: &str, input: &Value) -> Option<Value> {
    let files: Vec<(String, usize, usize)> = match tool {
        "Edit" => {
            let (added, removed) = count(text(input, &["old_string"]), text(input, &["new_string"]));
            vec![(text(input, &["file_path"]).to_owned(), added, removed)]
        }
        "MultiEdit" => {
            let (added, removed) = input.get("edits").and_then(Value::as_array).into_iter().flatten().fold((0, 0), |(a, r), edit| {
                let (add, del) = count(text(edit, &["old_string"]), text(edit, &["new_string"]));
                (a + add, r + del)
            });
            vec![(text(input, &["file_path"]).to_owned(), added, removed)]
        }
        "Write" => {
            let path = text(input, &["file_path"]);
            let old = current_file(path)?;
            let (added, removed) = count(&old, text(input, &["content"]));
            vec![(path.to_owned(), added, removed)]
        }
        "apply_patch" => patch_stats(text(input, &["command", "input", "patch"])),
        _ => return None,
    };
    let files: Vec<Value> = files
        .into_iter()
        .filter(|(path, ..)| !path.is_empty())
        .take(MAX_FILES)
        .map(|(path, added, removed)| json!({ "path": path, "added": added, "removed": removed }))
        .collect();
    (!files.is_empty()).then_some(Value::Array(files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_only_changed_lines() {
        assert_eq!(count("a\nb\nc", "a\nB\nc"), (1, 1));
        assert_eq!(count("a\nb", "a\nb\nc\nd"), (2, 0));
        assert_eq!(count("a\nb\nc", "c"), (0, 2));
        assert_eq!(count("", "x\ny"), (2, 0));
        assert_eq!(count("same", "same"), (0, 0));
        // Araya eklenen satır, kalan her şeyi değişmiş saymaz.
        assert_eq!(count("1\n2\n3\n4\n5", "1\n2\nX\n3\n4\n5"), (1, 0));
    }

    #[test]
    fn diff_keeps_order() {
        let ops = lines(&["a", "b", "c"], &["a", "x", "c"]);
        assert_eq!(ops, vec![Op::Same(0, 0), Op::Del(1), Op::Add(1), Op::Same(2, 2)]);
    }

    #[test]
    fn huge_change_falls_back_to_full_replace() {
        let old: Vec<String> = (0..3000).map(|i| format!("eski {i}")).collect();
        let new: Vec<String> = (0..3000).map(|i| format!("yeni {i}")).collect();
        assert_eq!(count(&old.join("\n"), &new.join("\n")), (3000, 3000));
    }

    #[test]
    fn stats_for_edit_tools() {
        let edit = edit_stats("Edit", &json!({ "file_path": "C:\\p\\a.ts", "old_string": "x\ny", "new_string": "x\nz\nw" })).unwrap();
        assert_eq!(edit, json!([{ "path": "C:\\p\\a.ts", "added": 2, "removed": 1 }]));
        let multi = json!({ "file_path": "b.rs", "edits": [{ "old_string": "a", "new_string": "b" }, { "old_string": "", "new_string": "c" }] });
        assert_eq!(edit_stats("MultiEdit", &multi).unwrap()[0]["added"], 2);
        assert!(edit_stats("Bash", &json!({ "command": "ls" })).is_none());
    }

    #[test]
    fn write_compares_with_current_file() {
        let path = std::env::temp_dir().join(format!("zodyvel-diff-{}", std::process::id()));
        fs::write(&path, "1\n2\n3\n").unwrap();
        let input = json!({ "file_path": path.display().to_string(), "content": "1\n2\n3\n4\n" });
        assert_eq!(edit_stats("Write", &input).unwrap()[0]["added"], 1);
        assert_eq!(edit_stats("Write", &input).unwrap()[0]["removed"], 0);
        let _ = fs::remove_file(&path);
        // Dosya yoksa her satır eklenmiş sayılır.
        assert_eq!(edit_stats("Write", &input).unwrap()[0]["added"], 4);
    }

    #[test]
    fn stats_for_codex_patch() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@ fn main\n-eski\n+yeni\n+ek\n*** Add File: b.md\n+# Başlık\n*** End Patch";
        let stats = edit_stats("apply_patch", &json!({ "command": patch })).unwrap();
        assert_eq!(stats, json!([{ "path": "src/a.rs", "added": 2, "removed": 1 }, { "path": "b.md", "added": 1, "removed": 0 }]));
    }
}
