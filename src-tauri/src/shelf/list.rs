//! Rafın listesi: yalnız dosyaların yolları tutulur (dosya kopyalanmaz, yerinde kalır). Saf, testli.

use serde::{Deserialize, Serialize};

/// Raftaki en çok öğe; dolunca en eskiler düşer.
pub const MAX_ITEMS: usize = 24;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: u64,
    pub path: String,
    pub name: String,
    /// Klasör mü (simge için).
    pub dir: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Shelf {
    /// En yeni başta.
    pub items: Vec<Item>,
    pub next_id: u64,
}

/// Yolun son parçası ("C:\\a\\rapor.pdf" → "rapor.pdf"); sürücü kökünde yolun kendisi.
pub fn file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    match trimmed.rsplit(['\\', '/']).next() {
        Some(name) if !name.is_empty() && !name.ends_with(':') => name.to_owned(),
        _ => path.to_owned(),
    }
}

impl Shelf {
    /// Bırakılan yolları ekler: aynı yol bir kez durur ve öne gelir; sınır aşılırsa en eskiler düşer.
    /// `(yol, klasör mü)`; eklenen (yeni) öğe sayısını döndürür.
    pub fn add(&mut self, dropped: impl IntoIterator<Item = (String, bool)>) -> usize {
        let mut added = 0;
        for (path, dir) in dropped {
            if path.trim().is_empty() {
                continue;
            }
            let existing = self.items.iter().position(|i| i.path.eq_ignore_ascii_case(&path));
            let item = match existing {
                Some(at) => self.items.remove(at),
                None => {
                    added += 1;
                    self.next_id += 1;
                    Item { id: self.next_id, name: file_name(&path), path, dir }
                }
            };
            self.items.insert(0, item);
        }
        self.items.truncate(MAX_ITEMS);
        added
    }

    /// Bir öğeyi ya da (`None`) hepsini kaldırır. Dosyalara dokunulmaz.
    pub fn remove(&mut self, id: Option<u64>) {
        match id {
            Some(id) => self.items.retain(|i| i.id != id),
            None => self.items.clear(),
        }
    }

    /// Yerinden silinmiş ya da taşınmış dosyaları raftan düşürür; bir şey değiştiyse true.
    pub fn retain_existing(&mut self, exists: impl Fn(&str) -> bool) -> bool {
        let before = self.items.len();
        self.items.retain(|i| exists(&i.path));
        self.items.len() != before
    }

    /// Verilen öğelerin yolları (raftaki sırayla).
    pub fn paths(&self, ids: &[u64]) -> Vec<String> {
        self.items.iter().filter(|i| ids.contains(&i.id)).map(|i| i.path.clone()).collect()
    }
}

/// Adaya sürüklenen şeyin özeti (Tauri olayından bağımsız, testli).
pub enum Drag {
    /// Ada üstüne dosya sürüklendi; kaç dosya.
    Enter(usize),
    Drop(Vec<(String, bool)>),
    Leave,
}

/// Arayüze giden sürükleme bildirimi: "enter" rafı açar, "drop" ekler, "leave" kapatır.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub kind: &'static str,
    pub count: usize,
}

/// Raf kapalıyken, raftan dışarı sürükleme sürerken (kendi penceremizin üstünden geçer) ya da
/// sürüklenen şey dosya değilse (metin, bağlantı) hiçbir şey olmaz; rafa da dokunulmaz. `to_session`:
/// imleç bırakırken bir AI oturumunun adının üstündeydi; dosyalar rafa eklenmez, oturuma gider ("send").
pub fn react(shelf: &mut Shelf, drag: Drag, on: bool, dragging_out: bool, to_session: bool) -> Option<Signal> {
    if !on || dragging_out {
        return None;
    }
    match drag {
        Drag::Enter(0) => None,
        Drag::Enter(count) => Some(Signal { kind: "enter", count }),
        Drag::Drop(paths) if paths.is_empty() => Some(Signal { kind: "leave", count: 0 }),
        Drag::Drop(paths) if to_session => Some(Signal { kind: "send", count: paths.len() }),
        Drag::Drop(paths) => Some(Signal { kind: "drop", count: shelf.add(paths) }),
        Drag::Leave => Some(Signal { kind: "leave", count: 0 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop(paths: &[&str]) -> Vec<(String, bool)> {
        paths.iter().map(|p| (p.to_string(), false)).collect()
    }

    #[test]
    fn names_come_from_the_last_part() {
        assert_eq!(file_name(r"C:\Users\a\Belgeler\rapor.pdf"), "rapor.pdf");
        assert_eq!(file_name(r"C:\Users\a\Proje\"), "Proje");
        assert_eq!(file_name("D:/x/y.png"), "y.png");
        assert_eq!(file_name(r"E:\"), r"E:\");
        assert_eq!(file_name("tek"), "tek");
    }

    #[test]
    fn add_keeps_one_copy_newest_first_and_caps() {
        let mut s = Shelf::default();
        assert_eq!(s.add(drop(&[r"C:\a.txt", r"C:\b.txt"])), 2);
        assert_eq!(s.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["b.txt", "a.txt"]);
        // Aynı dosya yeniden bırakılınca kopyası olmaz, öne gelir, kimliği değişmez.
        let id = s.items[1].id;
        assert_eq!(s.add(drop(&[r"c:\A.TXT", "  "])), 0);
        assert_eq!((s.items.len(), s.items[0].id), (2, id));
        let many: Vec<String> = (0..30).map(|n| format!(r"C:\d\{n}.txt")).collect();
        s.add(many.iter().map(|p| (p.clone(), false)));
        assert_eq!(s.items.len(), MAX_ITEMS);
        assert_eq!(s.items[0].name, "29.txt");
    }

    #[test]
    fn remove_clear_and_missing_files() {
        let mut s = Shelf::default();
        s.add(drop(&[r"C:\a", r"C:\b", r"C:\c"]));
        let b = s.items[1].id;
        assert_eq!(s.paths(&[b]), [r"C:\b"]);
        s.remove(Some(b));
        assert_eq!(s.items.len(), 2);
        assert!(s.retain_existing(|p| p != r"C:\a"));
        assert!(!s.retain_existing(|_| true));
        assert_eq!(s.items.len(), 1);
        s.remove(None);
        assert!(s.items.is_empty());
    }

    #[test]
    fn old_or_empty_file_loads() {
        let s: Shelf = serde_json::from_str("{}").unwrap();
        assert!(s.items.is_empty());
        let json = serde_json::to_string(&Item { id: 1, path: "C:\\a".into(), name: "a".into(), dir: true }).unwrap();
        assert_eq!(json, r#"{"id":1,"path":"C:\\a","name":"a","dir":true}"#);
    }

    #[test]
    fn drag_does_nothing_when_off_or_while_dragging_out() {
        let mut s = Shelf::default();
        assert_eq!(react(&mut s, Drag::Drop(drop(&[r"C:\a"])), false, false, false), None);
        assert_eq!(react(&mut s, Drag::Enter(2), false, false, false), None);
        assert_eq!(react(&mut s, Drag::Drop(drop(&[r"C:\a"])), true, true, false), None);
        assert!(s.items.is_empty(), "kapalıyken raf değişmez");
        // Metin ya da bağlantı sürüklenince (dosya yok) ada açılmaz.
        assert_eq!(react(&mut s, Drag::Enter(0), true, false, false), None);
    }

    #[test]
    fn drag_opens_adds_and_closes() {
        let mut s = Shelf::default();
        assert_eq!(react(&mut s, Drag::Enter(2), true, false, false), Some(Signal { kind: "enter", count: 2 }));
        let dropped = react(&mut s, Drag::Drop(drop(&[r"C:\a", r"C:\b"])), true, false, false);
        assert_eq!(dropped, Some(Signal { kind: "drop", count: 2 }));
        assert_eq!(react(&mut s, Drag::Drop(drop(&[r"C:\a"])), true, false, false), Some(Signal { kind: "drop", count: 0 }));
        assert_eq!(react(&mut s, Drag::Leave, true, false, false), Some(Signal { kind: "leave", count: 0 }));
        assert_eq!(s.items.len(), 2);
    }

    #[test]
    fn drop_on_a_session_does_not_touch_the_shelf() {
        let mut s = Shelf::default();
        let sent = react(&mut s, Drag::Drop(drop(&[r"C:\a", r"C:\b"])), true, false, true);
        assert_eq!(sent, Some(Signal { kind: "send", count: 2 }));
        assert!(s.items.is_empty());
        assert_eq!(react(&mut s, Drag::Drop(drop(&[r"C:\a"])), false, false, true), None, "raf kapalıyken hiçbir şey olmaz");
    }
}
