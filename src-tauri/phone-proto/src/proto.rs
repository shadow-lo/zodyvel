//! Telefon ↔ masaüstü ileti biçimi (Noise çerçevesi içinde JSON). Az veri ilkesi (Plan 3 · 2.5/1):
//! yalnız oturum adı, durum, kısa adım ve onay özeti gider; komut çıktısı, dosya içeriği, fark gitmez.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const MAX_ACTION_TEXT: usize = 2000;
pub const MAX_INCOMING: usize = 8192;

/// Karakter sayısına göre kırpar (UTF-8 sınırında güvenli).
pub fn clip(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct SessionView {
    pub id: String,
    pub tool: String,
    pub name: String,
    pub status: String,
    pub step: String,
    /// Yalnız masaüstünde "ayrıntı paylaşımı" açıksa dolu (varsayılan kapalı).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// T4-c: klasörün yalnız ADI (yol değil), ≤ 60; yoksa boş.
    pub folder: String,
    /// T4-c: son turun (yoksa oturumun) başlangıcı, unix saniye.
    pub started: Option<i64>,
    /// T4-c: son en çok `MAX_STEPS_VIEW` adım, eskiden yeniye; her biri `label`+`target` ≤ 80 karakter, maskeli.
    pub steps: Vec<StepView>,
}

/// Oturum ayrıntısında gösterilen adım sayısı (tasarım kararı, 14.11.2026).
pub const MAX_STEPS_VIEW: usize = 3;
/// Bir adım satırının (`label` + `target`) en çok karakteri.
pub const MAX_STEP_LINE: usize = 80;

/// Telefona giden tek adım. Metinler masaüstünde `redact_snippet`'ten geçirilmiş olmalı.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct StepView {
    /// read | edit | bash | search | other
    pub kind: String,
    /// Türkçe fiil ("Okudu", "Düzenledi"…), ≤ 30.
    pub label: String,
    /// Hedef (dosya adı, kısa komut…); `label` ile toplam ≤ 80 karakter (kırpma işareti dahil).
    pub target: String,
}

impl StepView {
    /// Uzunluk sınırlarını uygular (UTF-8 güvenli).
    pub fn new(kind: &str, label: &str, target: &str) -> Self {
        let cut = |s: &str, n: usize| -> String {
            if s.chars().count() <= n {
                return s.to_owned();
            }
            let mut o: String = s.chars().take(n.saturating_sub(1)).collect();
            o.push('…');
            o
        };
        let label = cut(label.trim(), 30);
        let room = MAX_STEP_LINE.saturating_sub(label.chars().count());
        StepView { kind: kind.to_owned(), target: cut(target.trim(), room), label }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct QuestionView {
    pub q: String,
    pub options: Vec<String>,
    pub multi: bool,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct ApprovalView {
    pub id: String,
    pub session: String,
    /// "permission" | "question" | "plan"
    pub kind: String,
    pub action: String,
    pub questions: Vec<QuestionView>,
}

/// Yeni görev klasörü: telefona yol değil, opak kimlik + ad gider (O6). Kimlik bağlantıdan bağımsız,
/// masaüstü sürecine özgü tuzla türetilir; eşleşmeyen kimlik reddedilir.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct FolderItem {
    pub id: String,
    pub name: String,
}

/// Klasör yolundan opak kimlik (FNV-1a 64 + süreç tuzu; gizli değil, yalnız sıra no yerine kararlı ad).
pub fn folder_id(salt: u64, path: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ salt;
    for b in path.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Ajanda penceresi (T2b): bugünden başlayıp kaç gün telefona gider.
pub const AGENDA_DAYS: i64 = 14;
/// Pencerede en çok kaç olay (gün boyu ve saatli birlikte).
pub const MAX_AGENDA_ITEMS: usize = 200;

/// Telefona giden tek bir ajanda olayı: yalnız o günün açılmış hali (başlık, tarih, saat, süre, ön süre).
/// Renk, tekrar kuralı, gün maskesi ve plan numarasından başka iç alan gitmez.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct AgendaItem {
    pub id: u64,
    /// Yerel tarih "2026-11-14".
    pub date: String,
    /// Yerel saat "14:00"; yoksa gün boyu.
    pub time: Option<String>,
    pub title: String,
    pub minutes: u32,
    pub lead: u32,
    /// Telefon bu olay için bildirim kursun mu (gün boyu olaylarda anlamsız).
    pub notify: bool,
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct AgendaView {
    /// Pencerenin ilk günü (masaüstünün yerel bugünü).
    pub from: String,
    pub days: i64,
    pub items: Vec<AgendaItem>,
}

impl AgendaView {
    pub fn new(from: String, mut items: Vec<AgendaItem>) -> Self {
        items.truncate(MAX_AGENDA_ITEMS);
        AgendaView { from, days: AGENDA_DAYS, items }
    }
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct StateView {
    pub sessions: Vec<SessionView>,
    pub approvals: Vec<ApprovalView>,
}

/// O9 · korumasız cihaz politikası (TEK yer). Kanıtı doğrulanamayan telefon reddedilmez, masaüstünde
/// "Korumasız" etiketlenir ve her etkileşimi telefonda parmak izi/PIN ile imzalar.
/// `false`: korumasız cihaz da tüm eylemleri yapar (yalnız etiket). `true`: korumasız cihazdan yalnız onay/ret,
/// soru cevabı, durdurma; not/görev/yeni görev ve klasör listesi kapalı.
/// tasarım kararı (14.11.2026): "Kısıtlı kalsın" → `true`.
pub const RESTRICT_UNPROTECTED: bool = true;

/// Telefondan gelebilecek eylemler. Yok olanlar bilerek yok: "her zaman izin ver", "revize et",
/// erişim seviyesi yükseltme, klasör yolu verme (klasör yalnız masaüstünün sunduğu listeden opak kimlik).
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "k", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Decide { id: String, d: String },
    Answer { id: String, answers: Map<String, Value> },
    Note { session: String, text: String, #[serde(default)] now: bool },
    Task { session: String, text: String },
    NewTask { folder: String, prompt: String },
    Stop { session: String },
}

impl Action {
    /// Denetim kaydına yazılan içeriksiz ad.
    pub fn label(&self) -> &'static str {
        match self {
            Action::Decide { d, .. } if d == "allow" => "onay verdi",
            Action::Decide { .. } => "reddetti",
            Action::Answer { .. } => "soruya cevap verdi",
            Action::Note { .. } => "ajana not yazdı",
            Action::Task { .. } => "oturuma görev verdi",
            Action::NewTask { .. } => "yeni görev başlattı",
            Action::Stop { .. } => "görevi durdurdu",
        }
    }

    /// O9: donanım kanıtı doğrulanamamış ("korumasız") cihazdan kabul edilenler. Not/Görev/Yeni görev ajana
    /// yeni talimat verir (bilgisayarda komut çalıştırabilir); bunlar yalnız korumalı cihazdan gelir.
    pub fn allowed_unprotected(&self) -> bool {
        matches!(self, Action::Decide { .. } | Action::Answer { .. } | Action::Stop { .. })
    }

    pub fn validate(&self) -> Result<(), String> {
        let id_ok = |s: &str| !s.is_empty() && s.len() <= 120;
        let text_ok = |s: &str| !s.trim().is_empty() && s.chars().count() <= MAX_ACTION_TEXT;
        let ok = match self {
            Action::Decide { id, d } => id_ok(id) && (d == "allow" || d == "deny"),
            Action::Answer { id, answers } => {
                id_ok(id)
                    && !answers.is_empty()
                    && answers.len() <= 10
                    && answers.iter().all(|(k, v)| k.chars().count() <= 400 && v.as_str().is_some_and(|s| s.chars().count() <= 500))
            }
            Action::Note { session, text, .. } | Action::Task { session, text } => id_ok(session) && text_ok(text),
            Action::NewTask { folder, prompt } => id_ok(folder) && folder.len() <= 32 && text_ok(prompt),
            Action::Stop { session } => id_ok(session),
        };
        if ok { Ok(()) } else { Err("Geçersiz eylem.".into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_view_fits_in_80_chars() {
        let s = StepView::new("edit", "Düzenledi", &"ğ".repeat(200));
        assert!(s.label.chars().count() + s.target.chars().count() <= MAX_STEP_LINE);
        assert!(s.target.ends_with('…'));
        let long = StepView::new("other", &"a".repeat(50), "x");
        assert_eq!(long.label.chars().count(), 30);
        assert_eq!(StepView::new("read", " Okudu ", " a.ts ").target, "a.ts");
    }
}
