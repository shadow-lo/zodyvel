//! Faz 13 kuralları (saf, Tauri'siz): seviye kapısı, mesaj başına eylem sınırı, döngü ve dış içerik.
//! Zody'nin araç çağrısı yalnız `hazirla` kapısından geçer; kapı seviyeyi, argümanları ve sınırları denetler.

use serde_json::{Map, Value};

use super::spec::{self, Action, Erisim, Risk};

/// Bir kullanıcı mesajı (tur) başına en çok bu kadar eylem.
pub const TUR_EN_COK: usize = 5;

/// Kapının reddetme nedenleri. Metinleri Zody'ye ve kullanıcıya olduğu gibi söylenir.
#[derive(Debug, PartialEq, Eq)]
pub enum Red {
    /// Bilinmeyen eylem ya da bu seviyede izin yok (ikisi aynı metni verir: hangi eylemlerin var olduğu sızmasın).
    Izinsiz,
    Arguman(String),
    /// Bu mesajda 5 eylem yapıldı.
    Sinir,
    /// Aynı eylem aynı argümanlarla yinelendi.
    Dongu,
}

impl Red {
    pub fn ileti(&self) -> String {
        match self {
            Red::Izinsiz => "Bilinmeyen ya da izin verilmeyen araç.".into(),
            Red::Arguman(e) => e.clone(),
            Red::Sinir => format!("Bu mesajda en çok {TUR_EN_COK} eylem yapılabilir; kullanıcıya sorulmalı."),
            Red::Dongu => "Aynı eylem aynı bilgilerle yinelendi; durdum, kullanıcıya sorulmalı.".into(),
        }
    }
}

/// Bir mesajın (turun) durumu: sayaç, görülen çağrılar, istek özeti ve dış içerik bayrağı.
#[derive(Debug, Default)]
#[allow(dead_code)] // araç sunucusu bağlantısı sonraki iş (Faz 13b); testlerde kullanılıyor
pub struct Tur {
    sayac: usize,
    gorulen: Vec<String>,
    /// Turda web sayfası, mail, ekran metni gibi dış içerik var: Güvenli eylemler de onay kartına düşer.
    pub dis_icerik: bool,
    /// Kullanıcının isteğinin kısa özeti (denetim kaydı için; sınırlanır ve ayıklanır).
    pub istek: String,
}

#[allow(dead_code)]
impl Tur {
    pub fn yeni(istek: &str, dis_icerik: bool) -> Self {
        Self { istek: super::audit::guvenli(istek), dis_icerik, ..Self::default() }
    }

    pub fn sayac(&self) -> usize {
        self.sayac
    }
}

/// Seviye bu eylemi görmeye yetiyor mu. `Internal` (min_level yok) hiçbir seviyede geçmez.
pub fn izinli(seviye: Erisim, action: &Action) -> bool {
    action.min_level.is_some_and(|en_az| seviye >= en_az) && action.risk != Risk::Internal
}

/// Araç adı: eylem adındaki noktalar alt çizgi olur ("music.control" → "music_control"); MCP ad kuralı.
pub fn arac_adi(action: &Action) -> String {
    action.name.replace('.', "_")
}

/// Seviyenin gördüğü eylemler (araç listesi bundan üretilir).
pub fn gorunen(seviye: Erisim) -> impl Iterator<Item = &'static Action> {
    spec::ACTIONS.iter().filter(move |a| izinli(seviye, a))
}

/// Onay kartı gerekir mi: Confirm her zaman; dış içerik varsa Safe de.
#[allow(dead_code)]
pub fn kart_gerekir(action: &Action, dis_icerik: bool) -> bool {
    action.risk == Risk::Confirm || (dis_icerik && action.risk == Risk::Safe)
}

/// Çağrının anahtarı: aynı eylem + aynı argümanlar. `Map` sıralı (preserve_order) olabilir; ada göre sıralanır.
fn anahtar(action: &Action, args: &Map<String, Value>) -> String {
    let mut parts: Vec<String> = args.iter().map(|(k, v)| format!("{k}={v}")).collect();
    parts.sort();
    format!("{}|{}", action.name, parts.join("&"))
}

/// Tek kapı: araç adı → eylem, seviye, argüman denetimi, döngü ve sınır. Başarılıysa sayaç artar.
pub fn hazirla(seviye: Erisim, tur: &mut Tur, arac: &str, args: &Value) -> Result<(&'static Action, Map<String, Value>), Red> {
    let action = gorunen(seviye).find(|a| arac_adi(a) == arac).ok_or(Red::Izinsiz)?;
    let temiz = spec::validate(action, args).map_err(Red::Arguman)?;
    let key = anahtar(action, &temiz);
    if tur.gorulen.contains(&key) {
        return Err(Red::Dongu);
    }
    if tur.sayac >= TUR_EN_COK {
        return Err(Red::Sinir);
    }
    tur.sayac += 1;
    tur.gorulen.push(key);
    Ok((action, temiz))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tur() -> Tur {
        Tur::yeni("deneme", false)
    }

    #[test]
    fn kapali_seviyede_sifir_arac() {
        assert_eq!(gorunen(Erisim::Kapali).count(), 0);
        let mut t = tur();
        assert_eq!(hazirla(Erisim::Kapali, &mut t, "music_read", &json!({})).unwrap_err(), Red::Izinsiz);
        assert_eq!(t.sayac(), 0);
    }

    #[test]
    fn dusuk_okur_ve_guvenli_yapar_ama_kritik_yapmaz() {
        let mut t = tur();
        assert!(hazirla(Erisim::Dusuk, &mut t, "music_read", &json!({})).is_ok());
        assert!(hazirla(Erisim::Dusuk, &mut t, "music_control", &json!({"action":"next"})).is_ok());
        assert_eq!(hazirla(Erisim::Dusuk, &mut t, "controls_radio", &json!({"kind":"wifi","on":false})).unwrap_err(), Red::Izinsiz);
        assert_eq!(hazirla(Erisim::Dusuk, &mut t, "agents_task", &json!({"session":"a","text":"b"})).unwrap_err(), Red::Izinsiz);
        assert!(gorunen(Erisim::Dusuk).all(|a| a.risk == Risk::Read || a.risk == Risk::Safe));
    }

    #[test]
    fn orta_kritik_eylemi_da_gorur_ve_kart_ister() {
        let mut t = tur();
        let (a, _) = hazirla(Erisim::Orta, &mut t, "controls_radio", &json!({"kind":"wifi","on":false})).unwrap();
        assert!(kart_gerekir(a, false));
        let (b, _) = hazirla(Erisim::Orta, &mut t, "music_read", &json!({})).unwrap();
        assert!(!kart_gerekir(b, false));
        assert!(gorunen(Erisim::Orta).count() > gorunen(Erisim::Dusuk).count());
    }

    #[test]
    fn her_seviyede_her_eylem_beklenen_gibi() {
        for action in spec::ACTIONS {
            for seviye in [Erisim::Kapali, Erisim::Dusuk, Erisim::Orta] {
                let beklenen = match action.risk {
                    Risk::Internal => false,
                    Risk::Read | Risk::Safe => seviye >= Erisim::Dusuk,
                    Risk::Confirm => seviye >= Erisim::Orta,
                };
                assert_eq!(izinli(seviye, action), beklenen, "{} @ {seviye:?}", action.name);
            }
        }
    }

    #[test]
    fn asla_yapmaz_listesi_hicbir_seviyede_arac_degil() {
        for ad in ["update_settings", "access_set", "access_get", "agents_decide", "audit_clear", "audit_recent", "agents_answer", "git_push", "source_connect"] {
            let action = spec::find(ad).expect(ad);
            assert!(!izinli(Erisim::Orta, action), "{ad}");
            let mut t = tur();
            assert_eq!(hazirla(Erisim::Orta, &mut t, ad, &json!({})).unwrap_err(), Red::Izinsiz, "{ad}");
        }
        assert!(gorunen(Erisim::Orta).all(|a| !a.name.contains("access") && !a.name.contains("settings.write") && a.name != "agents.decide"));
    }

    #[test]
    fn dis_icerik_guvenli_eylemi_karta_dusurur() {
        let guvenli = spec::find("music.control").unwrap();
        let okuma = spec::find("music.read").unwrap();
        assert!(!kart_gerekir(guvenli, false));
        assert!(kart_gerekir(guvenli, true));
        assert!(!kart_gerekir(okuma, true));
        assert!(Tur::yeni("x", true).dis_icerik);
    }

    #[test]
    fn mesaj_basina_bes_eylem() {
        let mut t = tur();
        for dakika in 1..=TUR_EN_COK as i64 {
            assert!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": dakika})).is_ok());
        }
        assert_eq!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": 99})).unwrap_err(), Red::Sinir);
        assert_eq!(t.sayac(), TUR_EN_COK);
    }

    #[test]
    fn ayni_eylem_ayni_argumanla_dongudur() {
        let mut t = tur();
        assert!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": 5})).is_ok());
        assert_eq!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": 5})).unwrap_err(), Red::Dongu);
        // Farklı argüman döngü değil.
        assert!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": 6})).is_ok());
        assert_eq!(t.sayac(), 2);
    }

    #[test]
    fn gecersiz_arguman_sayaci_artirmaz() {
        let mut t = tur();
        assert!(matches!(hazirla(Erisim::Dusuk, &mut t, "today_timer_set", &json!({"minutes": 0})), Err(Red::Arguman(_))));
        assert_eq!(t.sayac(), 0);
    }

    #[test]
    fn arac_adlari_benzersiz_ve_mcp_uyumlu() {
        let adlar: Vec<String> = gorunen(Erisim::Orta).map(arac_adi).collect();
        for (i, ad) in adlar.iter().enumerate() {
            assert!(ad.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'), "{ad}");
            assert!(!adlar[i + 1..].contains(ad), "{ad}");
        }
    }
}
