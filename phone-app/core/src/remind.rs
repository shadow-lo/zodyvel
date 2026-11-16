//! G3 · Ajanda hatırlatıcıları. Masaüstünün yolladığı 14 günlük plandan telefonun kuracağı alarm listesini çıkarır.
//!
//! Kurallar:
//! - Yalnız saatli ve `notify` olan planlar; gün boyu olaylar alarm kurmaz.
//! - Liste her eşitlemede BAŞTAN kurulur (Android tarafı eskileri siler), böylece değişen/silinen planlar kendiliğinden düzelir.
//! - Saat hesabı (yerel tarih + saat − ön süre → an) telefonda yapılır (Android `java.time`, saat dilimi/yaz saati doğru);
//!   burada yalnız biçim denetlenir. Geçmişte kalanları da Android atlar.
//! - Başlık ≤ 80 karakter, denetim karakterleri atılır. Liste diskte yalnız Keystore ile şifreli durur (Android tarafı).
//! - Kapalıyken (`remind.off`) hiçbir alarm kurulmaz, kayıtlı liste silinir.
use serde::Serialize;
use serde_json::Value;

/// Telefonun kuracağı en çok alarm (Android'in uygulama başına 500 sınırının altında).
pub const MAX_REMINDERS: usize = 200;
pub const TITLE_MAX: usize = 80;
/// Ön süre üst sınırı (dk): bir günden uzun ön süre anlamsız, bozuk veriye karşı.
pub const LEAD_MAX: u32 = 1440;

#[derive(Serialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reminder {
    /// Kararlı 31 bit anahtar (plan kimliği + gün); Android `PendingIntent` istek kodu ve bildirim kimliği.
    pub key: i32,
    /// Yerel tarih "2026-11-14".
    pub date: String,
    /// Yerel saat "14:30".
    pub time: String,
    /// Ön süre, dakika (0 = tam saatinde).
    pub lead: u32,
    pub title: String,
}

fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s.get(r).filter(|x| x.bytes().all(|c| c.is_ascii_digit())).and_then(|x| x.parse::<u32>().ok());
    matches!((num(0..4), num(5..7), num(8..10)), (Some(y), Some(m), Some(d)) if (2000..=2200).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d))
}

fn valid_time(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 5 || b[2] != b':' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s.get(r).filter(|x| x.bytes().all(|c| c.is_ascii_digit())).and_then(|x| x.parse::<u32>().ok());
    matches!((num(0..2), num(3..5)), (Some(h), Some(m)) if h < 24 && m < 60)
}

fn clean_title(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = t.chars().take(TITLE_MAX).collect();
    if t.chars().count() > TITLE_MAX {
        out.pop();
        out.push('…');
    }
    out
}

/// FNV-1a, 31 bit (Android istek kodu pozitif `int`).
fn key_of(id: &str, date: &str) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in id.bytes().chain(std::iter::once(b'|')).chain(date.bytes()) {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    (h & 0x7fff_ffff) as i32
}

/// Ajanda iletisinden (`{"from","days","items":[...]}`) alarm listesi. Sıra: tarih, saat. Aynı (plan, gün) bir kez.
pub fn plan(agenda: &Value) -> Vec<Reminder> {
    let Some(items) = agenda.get("items").and_then(Value::as_array) else {
        return vec![];
    };
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Reminder> = items
        .iter()
        .filter_map(|it| {
            if it.get("notify").and_then(Value::as_bool) != Some(true) {
                return None;
            }
            let date = it.get("date")?.as_str()?;
            let time = it.get("time")?.as_str()?;
            if !valid_date(date) || !valid_time(time) {
                return None;
            }
            let id = match it.get("id")? {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => return None,
            };
            if !seen.insert((id.clone(), date.to_owned())) {
                return None;
            }
            let title = clean_title(it.get("title").and_then(Value::as_str).unwrap_or(""));
            let lead = it.get("lead").and_then(Value::as_u64).unwrap_or(0).min(LEAD_MAX as u64) as u32;
            Some(Reminder { key: key_of(&id, date), date: date.to_owned(), time: time.to_owned(), lead, title: if title.is_empty() { "Plan".into() } else { title } })
        })
        .collect();
    out.sort_by(|a, b| (&a.date, &a.time, a.key).cmp(&(&b.date, &b.time, b.key)));
    out.truncate(MAX_REMINDERS);
    // Anahtar çakışması (çok düşük olasılık): bir sonraki boş değere kaydır.
    let mut used = std::collections::HashSet::new();
    for r in &mut out {
        while !used.insert(r.key) {
            r.key = (r.key + 1) & 0x7fff_ffff;
        }
    }
    out
}

/// Listenin özeti: aynı liste yeniden gelirse Android'e tekrar gönderilmez.
pub fn signature(list: &[Reminder]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    list.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filters_and_orders() {
        let a = json!({"from":"2026-11-14","days":14,"items":[
            {"id":3,"date":"2026-11-15","time":"11:00","title":"Kahvaltı","minutes":60,"lead":10,"notify":true},
            {"id":1,"date":"2026-11-14","time":"14:30","title":"Proje toplantısı","minutes":60,"lead":15,"notify":true},
            {"id":2,"date":"2026-11-15","time":null,"title":"Doğum günü","minutes":0,"lead":0,"notify":true},
            {"id":4,"date":"2026-11-16","time":"16:00","title":"Rapor","minutes":0,"lead":0,"notify":false},
            {"id":5,"date":"bozuk","time":"16:00","title":"x","minutes":0,"lead":0,"notify":true},
            {"id":6,"date":"2026-11-16","time":"25:00","title":"x","minutes":0,"lead":0,"notify":true},
            {"id":1,"date":"2026-11-14","time":"14:30","title":"tekrar","minutes":60,"lead":15,"notify":true}
        ]});
        let p = plan(&a);
        assert_eq!(p.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(), ["Proje toplantısı", "Kahvaltı"]);
        assert_eq!((p[0].date.as_str(), p[0].time.as_str(), p[0].lead), ("2026-11-14", "14:30", 15));
        assert!(p.iter().all(|r| r.key >= 0));
    }

    #[test]
    fn keys_are_stable_and_unique_per_day() {
        let a = json!({"items":[
            {"id":7,"date":"2026-11-14","time":"09:00","title":"Okul","lead":0,"notify":true},
            {"id":7,"date":"2026-11-15","time":"09:00","title":"Okul","lead":0,"notify":true}
        ]});
        let p1 = plan(&a);
        let p2 = plan(&a);
        assert_eq!(p1, p2);
        assert_ne!(p1[0].key, p1[1].key);
        assert_eq!(signature(&p1), signature(&p2));
    }

    #[test]
    fn title_is_cleaned_and_clipped_lead_capped() {
        let long = "a".repeat(200);
        let a = json!({"items":[
            {"id":1,"date":"2026-11-14","time":"09:00","title":"Satır\nbir\tiki","lead":99999,"notify":true},
            {"id":2,"date":"2026-11-14","time":"10:00","title":long,"lead":5,"notify":true},
            {"id":3,"date":"2026-11-14","time":"11:00","title":"   ","lead":5,"notify":true}
        ]});
        let p = plan(&a);
        assert_eq!(p[0].title, "Satır bir iki");
        assert_eq!(p[0].lead, LEAD_MAX);
        assert_eq!(p[1].title.chars().count(), TITLE_MAX);
        assert!(p[1].title.ends_with('…'));
        assert_eq!(p[2].title, "Plan");
    }

    #[test]
    fn caps_count_and_handles_garbage() {
        let items: Vec<Value> = (0..300).map(|i| json!({"id":i,"date":"2026-11-20","time":"08:00","title":"x","lead":0,"notify":true})).collect();
        assert_eq!(plan(&json!({ "items": items })).len(), MAX_REMINDERS);
        assert!(plan(&json!({})).is_empty());
        assert!(plan(&json!({"items":"x"})).is_empty());
    }
}
