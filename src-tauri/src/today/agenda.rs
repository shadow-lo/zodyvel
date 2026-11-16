//! Ajanda (Faz 5b): Bugün'ün etkinlikleri planlara dönüştü. Her plan bir başlangıç günü, saat, süre, renk
//! ve tekrar kuralı taşır (Hiç / Her gün / Hafta içi / Seçili günler); "Bugünü her güne uygula" o günün
//! tekrarsız planlarını her güne çevirir. Veri `today.json`'da, `CalEvent` olarak kalır: eski etkinlikler
//! süresiz, tekrarsız ve tam saatinde haber veren planlar olarak okunur (göç yok).
//!
//! Saf: zaman ve günler dışarıdan verilir (testli). Gün numarası 1970-01-01'den bu yana yerel gündür.

use serde::{Deserialize, Serialize};

use super::{CalEvent, clean_text, days_from_civil, meter_color, valid_date, valid_time};

/// Bir plan en çok bir gün sürer.
const MAX_PLAN_MINUTES: u32 = 24 * 60;
/// "Haber ver" en çok bu kadar dakika önce.
const MAX_LEAD: u32 = 120;
const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    #[default]
    None,
    Daily,
    Weekdays,
    /// `days` bit maskesi: bit 0 Pazartesi … bit 6 Pazar.
    Days,
}

pub fn yes() -> bool {
    true
}

/// Yeni plan ya da düzenleme formu (arayüzden). Eski `addEvent` biçimi de (yalnız tarih, saat, başlık) olur.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanForm {
    pub date: String,
    pub time: Option<String>,
    pub title: String,
    #[serde(default)]
    pub minutes: u32,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub repeat: Repeat,
    #[serde(default)]
    pub days: u8,
    #[serde(default = "yes")]
    pub notify: bool,
    #[serde(default)]
    pub lead: u32,
    #[serde(default = "yes")]
    pub phone: bool,
}

impl PlanForm {
    /// Doğrulanmış plan (kimliği çağıran verir).
    pub fn into_event(self, id: u64) -> Result<CalEvent, String> {
        let time = self.time.filter(|t| !t.is_empty());
        if !valid_date(&self.date) || time.as_deref().is_some_and(|t| !valid_time(t)) {
            return Err("Geçersiz tarih ya da saat.".into());
        }
        if self.minutes > MAX_PLAN_MINUTES {
            return Err("Plan en çok bir gün sürebilir.".into());
        }
        if self.lead > MAX_LEAD {
            return Err("En çok 2 saat önce haber verilebilir.".into());
        }
        let days = self.days & 0x7f;
        if self.repeat == Repeat::Days && days == 0 {
            return Err("Tekrar için en az bir gün seç.".into());
        }
        let title = clean_text(&self.title)?;
        Ok(CalEvent {
            id,
            date: self.date,
            // Saatsiz (gün boyu) planın süresi olmaz.
            minutes: if time.is_some() { self.minutes } else { 0 },
            time,
            title,
            color: meter_color(&self.color),
            repeat: self.repeat,
            days: if self.repeat == Repeat::Days { days } else { 0 },
            notify: self.notify,
            lead: self.lead,
            phone: self.phone,
        })
    }
}

/// "2026-11-10" → gün numarası.
pub fn day_of(date: &str) -> Option<i64> {
    let mut parts = date.split('-').map(|p| p.parse::<i64>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next()) else { return None };
    Some(days_from_civil(y, m, d))
}

/// Haftanın günü, 0 Pazartesi … 6 Pazar (1970-01-01 Perşembe).
pub fn weekday(day: i64) -> u8 {
    (day + 3).rem_euclid(7) as u8
}

/// Plan bu günde var mı (tekrar kuralıyla; başlangıç gününden önce yok).
pub fn occurs_on(event: &CalEvent, day: i64) -> bool {
    let Some(start) = day_of(&event.date) else { return false };
    if day < start {
        return false;
    }
    match event.repeat {
        Repeat::None => day == start,
        Repeat::Daily => true,
        Repeat::Weekdays => weekday(day) < 5,
        Repeat::Days => event.days & (1 << weekday(day)) != 0,
    }
}

fn minute_of(time: &str) -> Option<i64> {
    let (h, m) = time.split_once(':')?;
    Some(h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?)
}

/// Haber anları (`now` gününün bir öncesinden bir sonrasına): (plan, haber anı). Yerel ms.
fn alerts(events: &[CalEvent], now: i64) -> impl Iterator<Item = (&CalEvent, i64)> {
    let today = now.div_euclid(DAY_MS);
    events.iter().filter(|e| e.notify).flat_map(move |e| {
        let start = e.time.as_deref().and_then(minute_of);
        (today - 1..=today + 1).filter_map(move |day| {
            let start = start?;
            occurs_on(e, day).then(|| (e, (day * 24 * 60 + start - i64::from(e.lead)) * 60_000))
        })
    })
}

/// `(last, now]` aralığında haber anı gelen planlar; `late`'ten geç kalanlar atlanır.
pub fn due(events: &[CalEvent], last: i64, now: i64, late: i64) -> Vec<&CalEvent> {
    let mut out: Vec<&CalEvent> = Vec::new();
    for (e, at) in alerts(events, now) {
        if at > last && at <= now && now - at <= late && !out.iter().any(|o| o.id == e.id) {
            out.push(e);
        }
    }
    out
}

/// `now`'dan sonraki ilk haber anına kalan süre (ms).
pub fn next_in(events: &[CalEvent], now: i64) -> Option<i64> {
    alerts(events, now).map(|(_, at)| at).filter(|&at| at > now).min().map(|at| at - now)
}

/// "Döngü": `date` gününün tekrarsız, saatli planları her gün tekrarlanır (`on`), ya da o günden başlayan
/// her günkü planlar yeniden tek seferlik olur. Değiştiyse `true`.
pub fn loop_day(events: &mut [CalEvent], date: &str, on: bool) -> bool {
    let mut changed = false;
    for e in events.iter_mut().filter(|e| e.date == date && e.time.is_some()) {
        let next = match (on, e.repeat) {
            (true, Repeat::None) => Repeat::Daily,
            (false, Repeat::Daily) => Repeat::None,
            _ => continue,
        };
        e.repeat = next;
        changed = true;
    }
    changed
}

/// Gün numarası → "2026-11-14" (`days_from_civil`'in tersi).
pub fn date_of(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Telefona giden ajanda penceresi (T2b): `from_day`'den başlayıp `days` gün, her tekrarı o güne açılmış
/// halde. Yalnız "Telefona da hatırlat" açık planlar gider; tekrar kuralı, renk ve gün maskesi gitmez
/// (veri azaltma). Başlık telefona gitmeden önce maskelenir ve kırpılır. Sıra: gün, saat (gün boyu önce).
pub fn phone_window(events: &[CalEvent], from_day: i64, days: i64) -> Vec<zodyvel_phone_proto::proto::AgendaItem> {
    use zodyvel_phone_proto::proto::{AgendaItem, MAX_AGENDA_ITEMS, clip};
    let mut out: Vec<(i64, i64, AgendaItem)> = Vec::new();
    for day in from_day..from_day + days {
        for e in events.iter().filter(|e| e.phone && occurs_on(e, day)) {
            let minute = e.time.as_deref().and_then(minute_of);
            out.push((
                day,
                minute.unwrap_or(-1),
                AgendaItem {
                    id: e.id,
                    date: date_of(day),
                    time: e.time.clone(),
                    title: clip(&crate::redact::redact_snippet(&e.title), 80),
                    minutes: e.minutes,
                    lead: e.lead,
                    notify: e.notify && minute.is_some(),
                },
            ));
        }
    }
    out.sort_by_key(|(d, m, i)| (*d, *m, i.id));
    out.into_iter().map(|(_, _, i)| i).take(MAX_AGENDA_ITEMS).collect()
}

#[cfg(test)]
mod tests {
    use super::super::local_ms;
    use super::*;

    fn plan(json: &str) -> CalEvent {
        serde_json::from_str::<PlanForm>(json).unwrap().into_event(1).unwrap()
    }

    #[test]
    fn old_events_read_as_plain_plans() {
        let old: CalEvent = serde_json::from_str(r#"{"id":3,"date":"2026-10-14","time":"14:00","title":"Toplantı"}"#).unwrap();
        assert_eq!((old.minutes, old.repeat, old.notify, old.lead, old.color.as_str()), (0, Repeat::None, true, 0, ""));
    }

    #[test]
    fn forms_are_validated() {
        let p = plan(r##"{"date":"2026-11-09","time":"07:30","title":" Koşu ","minutes":45,"color":"#2ec4b6","repeat":"weekdays","notify":true,"lead":5}"##);
        assert_eq!((p.title.as_str(), p.minutes, p.repeat, p.lead, p.color.as_str()), ("Koşu", 45, Repeat::Weekdays, 5, "#2ec4b6"));
        // Gün boyu planın süresi yok; seçili günler maskesi yalnız "Günler"de kalır.
        let p = plan(r#"{"date":"2026-11-09","time":null,"title":"x","minutes":30,"days":3}"#);
        assert_eq!((p.minutes, p.days), (0, 0));
        for bad in [
            r#"{"date":"2026-11-09","time":"07:30","title":"x","minutes":1441}"#,
            r#"{"date":"2026-11-09","time":"07:30","title":"x","lead":121}"#,
            r#"{"date":"2026-11-09","time":"07:30","title":"x","repeat":"days","days":0}"#,
            r#"{"date":"2026-11-9","time":"07:30","title":"x"}"#,
        ] {
            assert!(serde_json::from_str::<PlanForm>(bad).unwrap().into_event(1).is_err(), "{bad}");
        }
    }

    #[test]
    fn repeat_rules_pick_days() {
        // 9 Kasım 2026 Pazartesi.
        let monday = day_of("2026-11-09").unwrap();
        assert_eq!(weekday(monday), 0);
        assert_eq!(weekday(day_of("1970-01-01").unwrap()), 3);
        let mut p = plan(r#"{"date":"2026-11-09","time":"08:00","title":"x"}"#);
        assert!(occurs_on(&p, monday) && !occurs_on(&p, monday + 1));
        p.repeat = Repeat::Daily;
        assert!(occurs_on(&p, monday + 30) && !occurs_on(&p, monday - 1));
        p.repeat = Repeat::Weekdays;
        assert!(occurs_on(&p, monday + 4) && !occurs_on(&p, monday + 5) && !occurs_on(&p, monday + 6) && occurs_on(&p, monday + 7));
        p.repeat = Repeat::Days;
        p.days = 0b100_0010; // Salı ve Pazar
        let hits: Vec<i64> = (0..7).filter(|d| occurs_on(&p, monday + d)).collect();
        assert_eq!(hits, [1, 6]);
    }

    #[test]
    fn repeating_plans_remind_every_day_with_lead() {
        let mut p = plan(r#"{"date":"2026-11-09","time":"22:00","title":"Ders","repeat":"daily","lead":5}"#);
        p.id = 7;
        let events = vec![p, plan(r#"{"date":"2026-11-09","time":"10:00","title":"Sessiz","notify":false,"repeat":"daily"}"#)];
        let at = |d, h, m| local_ms(2026, 11, d, h, m, 0, 0);
        let late = 10 * 60_000;
        // 5 dk önce, her gün; ertesi günün haberi de aynı kurala uyar. Sessiz plan haber vermez.
        assert!(due(&events, at(12, 21, 0), at(12, 21, 54), late).is_empty());
        assert_eq!(due(&events, at(12, 21, 54), at(12, 21, 55), late).iter().map(|e| e.id).collect::<Vec<_>>(), [7]);
        assert!(due(&events, at(12, 9, 0), at(12, 10, 0), late).is_empty());
        assert_eq!(next_in(&events, at(12, 22, 0)), Some(at(13, 21, 55) - at(12, 22, 0)));
        // Başlangıçtan önce yok.
        assert_eq!(next_in(&events, at(7, 12, 0)), None);
        // Gece yarısını geçen haber: 00:02'deki plan 5 dk önceden, önceki gün 23:57'de.
        let night = vec![plan(r#"{"date":"2026-11-09","time":"00:02","title":"x","repeat":"daily","lead":5}"#)];
        assert_eq!(due(&night, at(12, 23, 56), at(12, 23, 57), late).len(), 1);
    }

    #[test]
    fn loop_day_turns_the_day_into_a_daily_routine() {
        let mut events = vec![
            plan(r#"{"date":"2026-11-09","time":"08:00","title":"a"}"#),
            plan(r#"{"date":"2026-11-09","time":null,"title":"gün boyu"}"#),
            plan(r#"{"date":"2026-11-09","time":"09:00","title":"b","repeat":"weekdays"}"#),
            plan(r#"{"date":"2026-11-10","time":"08:00","title":"c"}"#),
        ];
        assert!(loop_day(&mut events, "2026-11-09", true));
        let reps: Vec<Repeat> = events.iter().map(|e| e.repeat).collect();
        assert_eq!(reps, [Repeat::Daily, Repeat::None, Repeat::Weekdays, Repeat::None]);
        assert!(!loop_day(&mut events, "2026-11-09", true));
        assert!(loop_day(&mut events, "2026-11-09", false));
        assert_eq!(events[0].repeat, Repeat::None);
    }

    #[test]
    fn date_of_inverts_day_of() {
        for d in ["1970-01-01", "2026-11-13", "2028-02-29", "2026-12-31", "2027-03-01"] {
            assert_eq!(date_of(day_of(d).unwrap()), d);
        }
    }

    #[test]
    fn phone_window_expands_repeats_and_minimizes() {
        let monday = day_of("2026-11-09").unwrap();
        let mut a = plan(r##"{"date":"2026-11-09","time":"08:00","title":"Koşu","minutes":30,"color":"#ff0000","repeat":"weekdays","lead":5}"##);
        a.id = 1;
        let mut b = plan(r#"{"date":"2026-11-10","time":null,"title":"Doğum günü"}"#);
        b.id = 2;
        let mut hidden = plan(r#"{"date":"2026-11-10","time":"09:00","title":"Gizli","phone":false}"#);
        hidden.id = 3;
        let mut quiet = plan(r#"{"date":"2026-11-10","time":"07:00","title":"Sessiz","notify":false}"#);
        quiet.id = 4;
        let items = phone_window(&[a, b, hidden, quiet], monday, 14);
        // Hafta içi koşu 10 gün, + gün boyu + sessiz; telefona kapalı plan yok.
        assert_eq!(items.iter().filter(|i| i.id == 1).count(), 10);
        assert!(items.iter().all(|i| i.id != 3));
        assert!(items.iter().all(|i| i.date.as_str() >= "2026-11-09" && i.date.as_str() <= "2026-11-22"));
        // 10 Kasım sırası: gün boyu (saatsiz) önce, sonra 07:00, sonra 08:00.
        let tue: Vec<(u64, Option<&str>, bool)> = items.iter().filter(|i| i.date == "2026-11-10").map(|i| (i.id, i.time.as_deref(), i.notify)).collect();
        assert_eq!(tue, [(2, None, false), (4, Some("07:00"), false), (1, Some("08:00"), true)]);
        // Tek seferlik plan yalnız kendi gününde.
        assert_eq!(items.iter().filter(|i| i.id == 2).count(), 1);
        // Pencere dışı: 13. gün (23 Kasım) yok.
        assert!(items.iter().all(|i| i.date != "2026-11-23"));
    }

    #[test]
    fn phone_window_masks_secrets_in_titles_and_clips() {
        let mut p = plan(r#"{"date":"2026-11-09","time":"08:00","title":"mail ali@example.com Authorization: Bearer abc123xyz"}"#);
        p.id = 1;
        let items = phone_window(&[p], day_of("2026-11-09").unwrap(), 14);
        assert!(!items[0].title.contains("ali@example.com") && !items[0].title.contains("abc123xyz"), "{}", items[0].title);
        assert!(items[0].title.chars().count() <= 81);
    }
}
