//! Kaynakların (Mail, GitHub…) ortak, Tauri'ye ve Windows'a bağlı olmayan kısmı: satır ve durum biçimi,
//! hangi kaynağın hangi gizli bilgiyi kullandığı ve "çalışmalı mı" kararı. Linux'ta da sınanır.
//!
//! Güvenlik kuralları (bu dalganın tüm kaynakları için bağlayıcı):
//! - Gizli bilgi (Gmail uygulama şifresi, GitHub tokenı) yalnız Windows Kimlik Bilgisi Yöneticisi'nde durur
//!   (`secrets.rs`); ayar dosyasına, kayda ya da arayüze olaya asla yazılmaz. Kayıtta yalnız "var/yok".
//! - Bağlantı yalnız TLS ile ve doğrudan hizmete (imap.gmail.com, api.github.com); arada üçüncü sunucu yok.
//! - İleti gövdesi ve satırlar yalnız bellekte (en çok `MAX_ITEMS`), diske yazılmaz. Kayda içerik yazılmaz;
//!   yalnız sayılar ve hata kodları.
//! - "Bağlantıyı kaldır" kaynağı durdurur, bellekteki satırları siler ve gizli bilgiyi de siler.
//! - Duraklat (tepsi), oturum kilidi ya da özelliğin kapanması: `Source::stop`, ağ tamamen durur.

use serde::{Deserialize, Serialize};

/// Bellekte tutulan en çok satır (kaynak başına).
pub const MAX_ITEMS: usize = 20;
/// GitHub "Depolarım"da en çok bu kadar depo.
pub const MAX_REPOS: usize = 10;
/// En çok bu kadar Gmail hesabı (her biri ayrı IMAP bağlantısı; hepsi tek Mail kutucuğunda).
pub const MAIL_SLOTS: usize = 5;

/// Tek satır şablonu; Mail ve GitHub ortak.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    /// Kaynağın kendi kimliği (IMAP UID, bildirim thread id…).
    pub id: String,
    /// Gönderen / depo#numara.
    pub title: String,
    /// Konu / PR başlığı / iş adı.
    pub summary: String,
    pub time_ms: i64,
    pub unread: bool,
    /// "ci_failed" | "review" | "notif" | None
    pub tag: Option<String>,
    /// Tarayıcıda aç.
    pub url: Option<String>,
    /// "open" | "mark_read" | "rerun" | "fix_with_claude" | "preview"
    pub actions: Vec<String>,
    /// Mail: iletinin geldiği hesap (birden çok Gmail bağlıyken satırda gösterilir).
    #[serde(default)]
    pub account: Option<String>,
}

/// Mail'de bağlı tek bir Gmail hesabının durumu (Ayarlar'daki hesap listesi, hesap başına hata).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountLine {
    /// Kimlik Bilgisi Yöneticisi yuvası (0 tabanlı, `MAIL_KEYS` sırası).
    pub slot: u8,
    pub address: String,
    /// "connecting" | "ok" | "error"
    pub status: String,
    pub error: Option<String>,
    pub badge: u32,
    /// Mail sekmesi (Faz 5b): Gmail kategorileri (Birincil, Güncellemeler, Tanıtımlar) ve son iletileri.
    /// Gmail süzgeci yoksa boş (sekme tek "Gelen kutusu" kartı gösterir).
    #[serde(default)]
    pub cats: Vec<MailCat>,
    /// Son başarılı çekiş (duvar saati, ms); "1 dk önce yenilendi".
    #[serde(default)]
    pub synced_ms: i64,
}

/// Bir Gmail kategorisinin okunmamış sayısı ve en yeni en çok `CAT_ITEMS` okunmamış iletisi.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailCat {
    /// "primary" | "updates" | "promotions"
    pub key: String,
    pub unread: u32,
    pub items: Vec<SourceItem>,
}

/// Mail sekmesindeki kategoriler, sırasıyla (Gmail `category:` adları).
pub const MAIL_CATS: [&str; 3] = ["primary", "updates", "promotions"];
/// Bir kategoride en çok bu kadar ileti (kartta ilk üçü, kategori listesinde hepsi).
pub const CAT_ITEMS: usize = 15;

/// GitHub'da son push edilen depo ("Depolarım" sekmesi; satır sayılarına ve rozete girmez).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoLine {
    /// "sahip/depo"
    pub slug: String,
    pub url: String,
    /// Son push anı.
    pub time_ms: i64,
    pub private: bool,
    /// Ana dal; boş depoda yok.
    pub branch: Option<String>,
    /// Ana dalın CI'ı: "ok" | "failed" | "pending" | "none"
    pub ci: String,
}

/// Bir kaynağın arayüze giden tüm durumu (`sources://update` olayı).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceState {
    /// "mail" | "github"
    pub id: String,
    /// "off" | "needs_setup" | "connecting" | "ok" | "error"
    pub status: String,
    /// Türkçe, kullanıcıya gösterilir. İçinde gizli bilgi ya da ileti içeriği olmaz.
    pub error: Option<String>,
    /// "ad@gmail.com" / "Ad"
    pub account: Option<String>,
    /// Okunmamış / bekleyen sayısı.
    pub badge: u32,
    /// 0 sakin … 3 acil (öne çıkan kaynak sırası için).
    pub urgency: u8,
    /// En çok `MAX_ITEMS`, yalnız bellekte.
    pub items: Vec<SourceItem>,
    /// Yalnız GitHub: son push edilen depolar (en çok `MAX_REPOS`).
    #[serde(default)]
    pub repos: Vec<RepoLine>,
    /// Yalnız Mail: bağlı hesaplar (yuva sırasıyla, en çok `MAIL_SLOTS`).
    #[serde(default)]
    pub accounts: Vec<AccountLine>,
}

pub const STATUSES: [&str; 5] = ["off", "needs_setup", "connecting", "ok", "error"];
/// "open_repo": GitHub "Depolarım"da son push edilen depoyu tarayıcıda aç (satır kimliği "sahip/depo").
pub const ITEM_ACTIONS: [&str; 6] = ["open", "mark_read", "rerun", "fix_with_claude", "preview", "open_repo"];

impl SourceState {
    /// Çalışmayan kaynağın durumu: satır yok, sayı yok.
    pub fn idle(id: &str, status: &str) -> Self {
        Self { id: id.into(), status: status.into(), ..Self::default() }
    }

    /// Kaynağın yayınladığı durumu sınırlar içine çeker: bilinmeyen durum "error", en çok `MAX_ITEMS` satır
    /// (en yenisi önde), bilinmeyen eylem adları düşer, urgency 0–3.
    pub fn normalized(mut self) -> Self {
        if !STATUSES.contains(&self.status.as_str()) {
            self.status = "error".into();
        }
        self.urgency = self.urgency.min(3);
        self.items.sort_by_key(|i| std::cmp::Reverse(i.time_ms));
        self.items.truncate(MAX_ITEMS);
        for item in &mut self.items {
            item.actions.retain(|a| ITEM_ACTIONS.contains(&a.as_str()));
        }
        self.repos.truncate(MAX_REPOS);
        self.accounts.truncate(MAIL_SLOTS);
        for account in &mut self.accounts {
            if !STATUSES.contains(&account.status.as_str()) {
                account.status = "error".into();
            }
            if account.status != "ok" {
                account.badge = 0;
            }
        }
        if self.status != "ok" {
            self.badge = 0;
            self.urgency = 0;
        }
        self
    }
}

/// Kurulum alanı → Kimlik Bilgisi Yöneticisi anahtarı. Yalnız bu anahtarlar yazılır/okunur. Mail'in
/// satırları 1. hesabındır; diğer hesaplar `MAIL_KEYS`'teki kendi anahtarlarını kullanır.
pub const FIELDS: &[(&str, &str, &str)] = &[
    ("mail", "address", "mail-gmail-address"),
    ("mail", "appPassword", "mail-gmail-app-password"),
    ("github", "token", "github-token"),
];

/// Gmail hesap yuvası → (adres, uygulama şifresi) anahtarları. 1. yuva tek hesaplı sürümün anahtarları
/// (önceden bağlanan hesap olduğu gibi kalır).
pub const MAIL_KEYS: [(&str, &str); MAIL_SLOTS] = [
    ("mail-gmail-address", "mail-gmail-app-password"),
    ("mail-gmail-address-2", "mail-gmail-app-password-2"),
    ("mail-gmail-address-3", "mail-gmail-app-password-3"),
    ("mail-gmail-address-4", "mail-gmail-app-password-4"),
    ("mail-gmail-address-5", "mail-gmail-app-password-5"),
];

/// Kaynağın kullandığı tüm gizli bilgi anahtarları (Mail'de her yuvanınki).
pub fn secret_keys(id: &str) -> Vec<&'static str> {
    if id == "mail" {
        return MAIL_KEYS.iter().flat_map(|(address, password)| [*address, *password]).collect();
    }
    FIELDS.iter().filter(|(source, _, _)| *source == id).map(|(_, _, key)| *key).collect()
}

/// `secrets.rs`'in kabul ettiği anahtar mı (yalnız bu dosyadaki anahtarlar).
pub fn known_key(key: &str) -> bool {
    FIELDS.iter().any(|(_, _, k)| *k == key) || MAIL_KEYS.iter().any(|(a, p)| *a == key || *p == key)
}

/// Kaynak çalışabilir mi: Mail'de en az bir hesabın iki bilgisi de var, diğerlerinde tüm anahtarlar var.
pub fn ready(id: &str, exists: impl Fn(&str) -> bool) -> bool {
    if id == "mail" {
        return MAIL_KEYS.iter().any(|(address, password)| exists(address) && exists(password));
    }
    secret_keys(id).into_iter().all(exists)
}

/// Kurulum formundan gelen alanları anahtarlara çevirir; bilinmeyen alan, boş değer ya da eksik alan hata.
/// Değerler kırpılır (uygulama şifresindeki boşluklar da: Google 4'lü gruplar hâlinde gösterir). `slot`
/// yalnız Mail'de anlamlı (hangi hesap yuvasına yazılacak); diğer kaynaklarda 0 olmalı.
pub fn map_fields(id: &str, slot: usize, fields: &[(String, String)]) -> Result<Vec<(&'static str, String)>, String> {
    let wanted: Vec<_> = FIELDS.iter().filter(|(source, _, _)| *source == id).collect();
    if wanted.is_empty() {
        return Err("Bilinmeyen kaynak.".into());
    }
    if slot >= if id == "mail" { MAIL_SLOTS } else { 1 } {
        return Err("Geçersiz hesap.".into());
    }
    // Mail'de alan adı → o yuvanın anahtarı.
    let slot_key = |key: &'static str| -> &'static str {
        match key {
            "mail-gmail-address" => MAIL_KEYS[slot].0,
            "mail-gmail-app-password" => MAIL_KEYS[slot].1,
            other => other,
        }
    };
    let mut out = Vec::new();
    for (name, value) in fields {
        let Some((_, field, key)) = wanted.iter().find(|(_, field, _)| field == name) else {
            return Err("Bilinmeyen alan.".into());
        };
        let key = slot_key(key);
        let value = if *field == "appPassword" { value.split_whitespace().collect::<String>() } else { value.trim().to_owned() };
        if value.is_empty() || value.len() > 512 {
            return Err("Alan boş ya da çok uzun.".into());
        }
        out.push((key, value));
    }
    if wanted.iter().any(|(_, _, key)| !out.iter().any(|(k, _)| *k == slot_key(key))) {
        return Err("Eksik alan.".into());
    }
    Ok(out)
}

/// Kurulum formlarının tarayıcıda açabildiği adresler (yalnız bu önekler; başka adres açılmaz).
pub const LINKS: [&str; 3] = [
    "https://myaccount.google.com/apppasswords",
    "https://support.google.com/accounts/answer/185833",
    "https://github.com/settings/tokens",
];

/// Adres izinli öneklerden biri mi: önek birebir ya da ardından `/`, `?`, `#` gelir (ör. `tokens/new?scopes=…`).
pub fn link_allowed(url: &str) -> bool {
    LINKS.iter().any(|prefix| url.strip_prefix(prefix).is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?', '#'])))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

/// O an neden çalışmadığı (ya da çalışması gerektiği).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    Run,
    /// Özellik kapalı.
    Off,
    /// Duraklat ya da oturum kilidi.
    Paused,
    /// Gizli bilgi yok.
    NeedsSetup,
}

pub fn gate(hub_on: bool, source_on: bool, paused: bool, locked: bool, has_secrets: bool) -> Gate {
    if !hub_on || !source_on {
        Gate::Off
    } else if paused || locked {
        Gate::Paused
    } else if !has_secrets {
        Gate::NeedsSetup
    } else {
        Gate::Run
    }
}

/// Mail'de tek hesabın bellekteki durumu: özet satırı ve o hesabın satırları.
#[derive(Clone, Debug, Default)]
pub struct MailAccount {
    pub line: AccountLine,
    pub items: Vec<SourceItem>,
}

/// Gmail hesaplarını tek Mail durumuna katlar (adada tek kutucuk, tek Zody): bağlı hesapların satırları
/// birleşir (en yeni önde), okunmamışlar toplanır. Bir hesap bağlıysa durum "ok", bağlanamayan hesabın
/// hatası `accounts`'ta görünür. Hiçbiri bağlı değilse bağlanan varsa "connecting", yoksa "error".
pub fn merge_mail(accounts: &[MailAccount]) -> SourceState {
    let live = |a: &&MailAccount| a.line.status == "ok";
    let status = if accounts.iter().any(|a| a.line.status == "ok") {
        "ok"
    } else if accounts.iter().any(|a| a.line.status == "connecting") {
        "connecting"
    } else {
        "error"
    };
    let badge = accounts.iter().filter(live).map(|a| a.line.badge).sum();
    let mut items: Vec<SourceItem> = accounts.iter().filter(live).flat_map(|a| a.items.iter().cloned()).collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.time_ms));
    items.truncate(MAX_ITEMS);
    let error = (status == "error").then(|| {
        let failed = accounts.iter().find_map(|a| a.line.error.as_deref().map(|e| (a.line.address.as_str(), e)));
        match failed {
            None => "Gmail bilgileri bulunamadı. Yeniden bağlan.".to_owned(),
            Some((_, e)) if accounts.len() == 1 => e.to_owned(),
            Some((address, e)) => format!("{address}: {e}"),
        }
    });
    let account = match accounts {
        [] => None,
        [one] => Some(one.line.address.clone()),
        many => Some(format!("{} hesap", many.len())),
    };
    SourceState {
        id: "mail".into(),
        status: status.into(),
        error,
        account,
        badge,
        urgency: if badge > 0 { 2 } else { 0 },
        items,
        repos: Vec::new(),
        accounts: accounts.iter().map(|a| a.line.clone()).collect(),
    }
}

/// Yeni okunmamış satır geldi mi (Zody'nin tek seferlik tepkisi ve dikkat sesi). İlk yükleme sessiz: önceki
/// durum "ok" değilse ya da satırın hesabı önceki durumda henüz bağlı değilse (sonradan bağlanan ya da
/// kopup dönen hesabın mevcut okunmamışları) yeni sayılmaz.
pub fn has_fresh(previous: &SourceState, next: &SourceState) -> bool {
    if previous.status != "ok" || next.status != "ok" {
        return false;
    }
    let was_live = |account: &Option<String>| match account {
        Some(address) if !previous.accounts.is_empty() => previous.accounts.iter().any(|a| &a.address == address && a.status == "ok"),
        _ => true,
    };
    next.items.iter().any(|i| i.unread && was_live(&i.account) && !previous.items.iter().any(|old| old.id == i.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, time_ms: i64) -> SourceItem {
        SourceItem { id: id.into(), time_ms, actions: vec!["open".into(), "sil".into()], ..SourceItem::default() }
    }

    #[test]
    fn state_is_capped_sorted_and_cleaned() {
        let items = (0..30).map(|i| item(&i.to_string(), i)).collect();
        let s = SourceState { id: "mail".into(), status: "ok".into(), urgency: 9, badge: 4, items, ..SourceState::default() }.normalized();
        assert_eq!(s.items.len(), MAX_ITEMS);
        assert_eq!(s.items[0].id, "29");
        assert_eq!(s.items[0].actions, vec!["open"]);
        assert_eq!((s.urgency, s.badge), (3, 4));
        let odd = SourceState { status: "boom".into(), badge: 3, urgency: 2, ..SourceState::default() }.normalized();
        assert_eq!((odd.status.as_str(), odd.badge, odd.urgency), ("error", 0, 0));
    }

    #[test]
    fn state_serializes_camel_case() {
        let s = SourceState { id: "github".into(), status: "ok".into(), items: vec![item("1", 5)], ..SourceState::default() };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"timeMs\":5") && json.contains("\"items\""), "{json}");
    }

    #[test]
    fn fields_map_only_known_keys() {
        let f = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let mail = map_fields("mail", 0, &f(&[("address", " ad@gmail.com "), ("appPassword", "abcd efgh ijkl mnop")])).unwrap();
        assert_eq!(mail, vec![("mail-gmail-address", "ad@gmail.com".into()), ("mail-gmail-app-password", "abcdefghijklmnop".into())]);
        let second = map_fields("mail", 1, &f(&[("address", "iki@gmail.com"), ("appPassword", "abcdefghijklmnop")])).unwrap();
        assert_eq!(second, vec![("mail-gmail-address-2", "iki@gmail.com".into()), ("mail-gmail-app-password-2", "abcdefghijklmnop".into())]);
        assert!(map_fields("mail", MAIL_SLOTS, &f(&[("address", "a@b.c"), ("appPassword", "x")])).is_err(), "yuva sınırı");
        assert!(map_fields("github", 1, &f(&[("token", "x")])).is_err(), "GitHub'da tek yuva");
        assert!(map_fields("mail", 0, &f(&[("address", "a@b.c")])).is_err(), "eksik alan");
        assert!(map_fields("mail", 2, &f(&[("address", "a@b.c")])).is_err(), "eksik alan (2. yuva dışı)");
        assert!(map_fields("mail", 0, &f(&[("address", "a@b.c"), ("appPassword", "x"), ("token", "y")])).is_err(), "başka kaynağın alanı");
        assert!(map_fields("github", 0, &f(&[("token", "   ")])).is_err(), "boş");
        assert!(map_fields("telegram", 0, &f(&[("token", "x")])).is_err());
        assert_eq!(secret_keys("github"), vec!["github-token"]);
        assert_eq!(secret_keys("mail").len(), MAIL_SLOTS * 2);
        assert!(secret_keys("mail").iter().all(|k| known_key(k)));
        assert!(known_key("github-token") && !known_key("mail-gmail-address-6") && !known_key("başka"));
    }

    #[test]
    fn mail_ready_with_any_full_account() {
        let has = |keys: &'static [&'static str]| move |k: &str| keys.contains(&k);
        assert!(!ready("mail", has(&[])));
        assert!(ready("mail", has(&["mail-gmail-address", "mail-gmail-app-password"])));
        assert!(ready("mail", has(&["mail-gmail-address-3", "mail-gmail-app-password-3"])));
        assert!(!ready("mail", has(&["mail-gmail-address", "mail-gmail-app-password-2"])), "yarım hesaplar birleşmez");
        assert!(ready("github", has(&["github-token"])) && !ready("github", has(&["mail-gmail-address"])));
    }

    fn account(slot: u8, address: &str, status: &str, badge: u32, items: Vec<SourceItem>) -> MailAccount {
        let items = items.into_iter().map(|i| SourceItem { account: Some(address.into()), unread: true, ..i }).collect();
        MailAccount { line: AccountLine { slot, address: address.into(), status: status.into(), error: None, badge, ..AccountLine::default() }, items }
    }

    #[test]
    fn mail_accounts_merge_into_one_state() {
        let a = account(0, "a@gmail.com", "ok", 2, vec![item("0:1", 10), item("0:2", 30)]);
        let b = account(1, "b@gmail.com", "ok", 1, vec![item("1:7", 20)]);
        let s = merge_mail(&[a.clone(), b.clone()]);
        assert_eq!((s.status.as_str(), s.badge, s.urgency), ("ok", 3, 2));
        assert_eq!(s.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), vec!["0:2", "1:7", "0:1"]);
        assert_eq!(s.account.as_deref(), Some("2 hesap"));
        assert_eq!(s.accounts.len(), 2);

        // Biri hata verirse diğeri görünmeye devam eder; hata hesap satırında.
        let broken = MailAccount { line: AccountLine { status: "error".into(), error: Some("Giriş reddedildi.".into()), ..b.line.clone() }, items: Vec::new() };
        let s = merge_mail(&[a.clone(), broken.clone()]);
        assert_eq!((s.status.as_str(), s.badge, s.items.len(), s.error.clone()), ("ok", 2, 2, None));
        assert_eq!(s.accounts[1].error.as_deref(), Some("Giriş reddedildi."));

        // Hepsi hatalı: hangi hesap olduğu yazar; tek hesapta adres eklenmez.
        let down = MailAccount { line: AccountLine { status: "error".into(), error: Some("Ulaşılamıyor.".into()), ..a.line.clone() }, items: Vec::new() };
        assert_eq!(merge_mail(&[down.clone(), broken]).error.as_deref(), Some("a@gmail.com: Ulaşılamıyor."));
        assert_eq!(merge_mail(std::slice::from_ref(&down)).error.as_deref(), Some("Ulaşılamıyor."));
        assert_eq!(merge_mail(std::slice::from_ref(&down)).account.as_deref(), Some("a@gmail.com"));
        let waiting = MailAccount { line: AccountLine { status: "connecting".into(), ..b.line }, items: Vec::new() };
        assert_eq!(merge_mail(&[down, waiting]).status, "connecting");
    }

    #[test]
    fn fresh_ignores_first_load_of_new_account() {
        let a = account(0, "a@gmail.com", "ok", 1, vec![item("0:1", 10)]);
        let b_wait = account(1, "b@gmail.com", "connecting", 0, Vec::new());
        let before = merge_mail(&[a.clone(), b_wait]);
        // B ilk kez bağlandı: mevcut okunmamışları "yeni" değil.
        let b = account(1, "b@gmail.com", "ok", 1, vec![item("1:5", 20)]);
        let after = merge_mail(&[a.clone(), b.clone()]);
        assert!(!has_fresh(&before, &after));
        // Sonra B'ye gerçekten yeni mail geldi.
        let b_new = account(1, "b@gmail.com", "ok", 2, vec![item("1:5", 20), item("1:6", 30)]);
        assert!(has_fresh(&after, &merge_mail(&[a.clone(), b_new])));
        // Tek hesaplı eski davranış: önceki "ok" değilse sessiz.
        assert!(!has_fresh(&SourceState::idle("mail", "connecting"), &after));
        // GitHub (hesap listesi yok): eski kural.
        let gh = |ids: &[&str]| SourceState { status: "ok".into(), items: ids.iter().map(|i| SourceItem { unread: true, ..item(i, 1) }).collect(), ..SourceState::default() };
        assert!(has_fresh(&gh(&["n:1"]), &gh(&["n:1", "n:2"])));
        assert!(!has_fresh(&gh(&["n:1", "n:2"]), &gh(&["n:1"])));
    }

    #[test]
    fn only_known_links_open() {
        assert!(link_allowed("https://myaccount.google.com/apppasswords"));
        assert!(link_allowed("https://github.com/settings/tokens/new?scopes=repo,notifications&description=Zodyvel"));
        assert!(!link_allowed("https://github.com/settings/tokensX"));
        assert!(!link_allowed("https://github.com.evil.com/settings/tokens"));
        assert!(!link_allowed("http://myaccount.google.com/apppasswords"));
        assert!(!link_allowed("https://github.com/settings/tokens/new\" & calc"));
        assert!(!link_allowed("file:///C:/Windows"));
    }

    #[test]
    fn gate_order() {
        assert_eq!(gate(true, true, false, false, true), Gate::Run);
        assert_eq!(gate(false, true, false, false, true), Gate::Off);
        assert_eq!(gate(true, false, true, true, false), Gate::Off);
        assert_eq!(gate(true, true, true, false, true), Gate::Paused);
        assert_eq!(gate(true, true, false, true, true), Gate::Paused);
        assert_eq!(gate(true, true, false, false, false), Gate::NeedsSetup);
    }
}
