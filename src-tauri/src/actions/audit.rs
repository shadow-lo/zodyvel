//! Denetim kaydı (Faz 13): `%APPDATA%\Zodyvel\denetim\YYYY-MM-DD.jsonl`, her satır bir eylem.
//! Zaman, istek özeti, seviye, eylem, onay (kim/ne zaman) ve sonuç. Şifre, anahtar, ekran görüntüsü ya da
//! uzun veri blokları asla yazılmaz: metinler `guvenli` ile ayıklanır. Dosya sistemi yalnız bu modülde.

use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Bu kadar günlük dosya tutulur; eskisi yeni gün dosyası açılırken silinir.
const GUN_SAYISI: usize = 30;
/// Tek metin alanının üst sınırı (karakter).
const EN_UZUN: usize = 160;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Kayit {
    /// Unix ms.
    pub t: u64,
    /// Yerel "SS:DD:SS".
    pub zaman: String,
    pub istek: String,
    /// "closed" | "low" | "mid".
    pub seviye: String,
    pub eylem: String,
    pub ozet: String,
    /// "gerekmedi" | "bekliyor" | "kullanıcı" | "reddedildi" | "süre doldu" | "iptal".
    pub onay: String,
    /// Onayın verildiği an (Unix ms), yoksa 0.
    pub onay_ms: u64,
    /// "ok" | "hata: …" | "reddedildi" | "bekliyor" | "iptal".
    pub sonuc: String,
}

const GIZLI_ANAHTARLAR: [&str; 9] = ["password", "passwd", "parola", "sifre", "şifre", "token", "secret", "apikey", "api_key"];

/// Denetim kaydına girecek metni temizler: yollar/e-postalar (redact), gizli anahtarın değeri, uzun
/// kesintisiz bloklar (base64/ekran görüntüsü), `data:` adresleri; sonra kırpılır.
pub fn guvenli(text: &str) -> String {
    let redacted = crate::redact::redact(text, &[]);
    let mut out: Vec<String> = Vec::new();
    let mut gizle_sonraki = false;
    for token in redacted.split_whitespace() {
        let low = token.to_lowercase();
        let anahtar = GIZLI_ANAHTARLAR.iter().any(|k| low.contains(k));
        if gizle_sonraki {
            gizle_sonraki = false;
            out.push("[gizli]".into());
        } else if anahtar && (low.ends_with(':') || low.ends_with('=')) {
            out.push(token.to_owned());
            gizle_sonraki = true;
        } else if anahtar && (low.contains('=') || low.contains(':')) {
            let adi = token.split(['=', ':']).next().unwrap_or("");
            out.push(format!("{adi}=[gizli]"));
        } else if low.starts_with("data:") || token.chars().count() > 60 {
            out.push("[veri]".into());
        } else {
            out.push(token.to_owned());
        }
    }
    let joined = out.join(" ");
    if joined.chars().count() > EN_UZUN { joined.chars().take(EN_UZUN).collect::<String>() + "…" } else { joined }
}

/// Kaydı bugünün dosyasına ekler. Alanlar yazmadan önce yeniden ayıklanır (çağıran unutsa bile).
pub fn yaz(dir: &Path, tarih: &str, kayit: &Kayit) -> std::io::Result<()> {
    if !tarih.bytes().all(|c| c.is_ascii_digit() || c == b'-') {
        return Err(std::io::Error::other("geçersiz tarih"));
    }
    fs::create_dir_all(dir)?;
    let yol = dir.join(format!("{tarih}.jsonl"));
    let yeni = !yol.exists();
    let mut k = kayit.clone();
    k.istek = guvenli(&k.istek);
    k.ozet = guvenli(&k.ozet);
    k.sonuc = guvenli(&k.sonuc);
    let satir = serde_json::to_string(&k).map_err(std::io::Error::other)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&yol)?;
    writeln!(f, "{satir}")?;
    if yeni {
        eskileri_sil(dir);
    }
    Ok(())
}

fn gunluk_dosyalar(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect())
        .unwrap_or_default();
    files.sort();
    files
}

fn eskileri_sil(dir: &Path) {
    let files = gunluk_dosyalar(dir);
    if files.len() > GUN_SAYISI {
        for old in &files[..files.len() - GUN_SAYISI] {
            let _ = fs::remove_file(old);
        }
    }
}

/// En yeni `n` kayıt, yeniden eskiye.
pub fn son(dir: &Path, n: usize) -> Vec<Kayit> {
    let mut out = Vec::new();
    for yol in gunluk_dosyalar(dir).into_iter().rev() {
        let Ok(text) = fs::read_to_string(&yol) else { continue };
        for line in text.lines().rev() {
            if let Ok(k) = serde_json::from_str::<Kayit>(line) {
                out.push(k);
                if out.len() >= n {
                    return out;
                }
            }
        }
    }
    out
}

/// Tüm günlük dosyalarını siler; kaç dosya gittiğini döndürür.
pub fn temizle(dir: &Path) -> usize {
    gunluk_dosyalar(dir).into_iter().filter(|p| fs::remove_file(p).is_ok()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gecici(ad: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("zodyvel-denetim-{ad}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn kayit(eylem: &str, t: u64) -> Kayit {
        Kayit { t, zaman: "10:00:00".into(), istek: "müziği aç".into(), seviye: "low".into(), eylem: eylem.into(), ozet: "Müzik".into(), onay: "gerekmedi".into(), sonuc: "ok".into(), ..Kayit::default() }
    }

    #[test]
    fn yazar_okur_ve_temizler() {
        let d = gecici("yaz");
        yaz(&d, "2026-11-13", &kayit("music.control", 1)).unwrap();
        yaz(&d, "2026-11-13", &kayit("music.read", 2)).unwrap();
        yaz(&d, "2026-11-14", &kayit("controls.dark", 3)).unwrap();
        let son3 = son(&d, 10);
        assert_eq!(son3.iter().map(|k| k.eylem.as_str()).collect::<Vec<_>>(), ["controls.dark", "music.read", "music.control"]);
        assert_eq!(son(&d, 2).len(), 2);
        let satir = fs::read_to_string(d.join("2026-11-13.jsonl")).unwrap();
        assert_eq!(satir.lines().count(), 2);
        assert_eq!(temizle(&d), 2);
        assert!(son(&d, 10).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn sifre_ve_ekran_goruntusu_yazilmaz() {
        let d = gecici("gizli");
        let mut k = kayit("x.y", 1);
        k.ozet = format!("Giriş · Şifre: hunter2 token=abc123 data:image/png;base64,AAAA {}", "A".repeat(500));
        k.istek = r"şifre: gizli123 yaz C:\Users\ali\Desktop\x.txt".into();
        yaz(&d, "2026-11-13", &k).unwrap();
        let ham = fs::read_to_string(d.join("2026-11-13.jsonl")).unwrap();
        for yasak in ["hunter2", "abc123", "base64", "AAAA", "gizli123", "ali"] {
            assert!(!ham.contains(yasak), "{yasak} sızdı: {ham}");
        }
        assert!(ham.contains("[gizli]") && ham.contains("[veri]"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn otuz_gunden_eskisi_silinir_ve_bozuk_tarih_reddedilir() {
        let d = gecici("eski");
        for gun in 1..=31 {
            yaz(&d, &format!("2026-10-{gun:02}"), &kayit("a.b", gun)).unwrap();
        }
        assert_eq!(gunluk_dosyalar(&d).len(), GUN_SAYISI);
        assert!(!d.join("2026-10-01.jsonl").exists());
        assert!(yaz(&d, "../x", &kayit("a.b", 1)).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn guvenli_kisaltir() {
        assert!(guvenli(&"merhaba ".repeat(100)).chars().count() <= EN_UZUN + 1);
        assert_eq!(guvenli("müzik aç"), "müzik aç");
    }
}
