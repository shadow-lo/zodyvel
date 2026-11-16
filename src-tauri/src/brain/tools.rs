//! Zodyvel'in kendi yerel araç sunucusu (MCP, JSON-RPC) çekirdeği (Faz 13a). Saf ve süreç içi: bu modül
//! hiçbir ağ/boru dinleyicisi açmaz, hiçbir şey çalıştırmaz ve bellek tutmaz; Claude Code'a bağlama `server.rs`'te (Faz 13b). Araç listesi `actions::spec::ACTIONS`'tan **o anki erişim seviyesine göre** üretilir (Internal
//! eylemler hiçbir seviyede çıkmaz: ayar/seviye değiştirme ve kendi kartını onaylama listede yoktur).
//! `tools/call` yalnız `rules::hazirla` kapısından geçer; geçerse çağıran `actions::arac_calistir` ile
//! normal onay hattına (doğrula → planla → Read/Safe çalış, Confirm kart, 2 dk iptal) sokar.

use serde_json::{Map, Value, json};

use crate::actions::rules::{self, Red, Tur};
use crate::actions::spec::{Action, Erisim, Kind};

/// Bir JSON-RPC isteğinin işlenmesi.
#[derive(Debug)]
pub enum Yanit {
    /// Yanıt hazır (`initialize`, `tools/list`, `ping`, hata).
    Hemen(Value),
    /// Kapıdan geçmiş araç çağrısı; çağıran çalıştırır ve sonucu `cagri_yaniti` ile paketler.
    Cagri { id: Value, action: &'static Action, args: Map<String, Value> },
}

fn sema(kind: Kind) -> Value {
    match kind {
        Kind::Bool => json!({ "type": "boolean" }),
        Kind::Int { min, max } => json!({ "type": "integer", "minimum": min, "maximum": max }),
        Kind::Text { max } => json!({ "type": "string", "minLength": 1, "maxLength": max }),
        Kind::Choice { options } => json!({ "type": "string", "enum": options }),
        Kind::Date => json!({ "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$", "description": "YYYY-AA-GG" }),
        Kind::Time => json!({ "type": "string", "pattern": "^\\d{2}:\\d{2}$", "description": "SS:DD" }),
    }
}

/// Bir eylemin araç tanımı: ad, açıklama ve Arg/Kind'tan üretilen `inputSchema`.
pub fn arac_tanimi(action: &Action) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in action.args {
        let mut s = sema(arg.kind);
        s["title"] = json!(arg.label);
        properties.insert(arg.name.to_owned(), s);
        if !arg.optional {
            required.push(arg.name);
        }
    }
    json!({
        "name": rules::arac_adi(action),
        "description": action.title,
        "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
    })
}

/// Seviyenin gördüğü tüm araçlar.
pub fn arac_listesi(seviye: Erisim) -> Vec<Value> {
    rules::gorunen(seviye).map(arac_tanimi).collect()
}

fn yanit_ok(id: &Value, sonuc: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": sonuc })
}

fn yanit_hata(id: &Value, kod: i64, ileti: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": kod, "message": ileti } })
}

/// Çalışmış (ya da kart bekleyen) çağrının JSON-RPC yanıtı. `Ok(metin)` Zody'ye gider; `Err` araç hatasıdır.
pub fn cagri_yaniti(id: &Value, sonuc: Result<String, String>) -> Value {
    match sonuc {
        Ok(metin) => yanit_ok(id, json!({ "content": [{ "type": "text", "text": metin }], "isError": false })),
        Err(e) => yanit_ok(id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
    }
}

/// Bir JSON-RPC isteğini işler; bildirimlerde (`id` yok) `None`. `tur`: bu mesajın sayaç ve dış içerik durumu.
pub fn isle(istek: &Value, seviye: Erisim, tur: &mut Tur) -> Option<Yanit> {
    let id = istek.get("id")?.clone();
    Some(match istek.get("method").and_then(Value::as_str) {
        Some("initialize") => Yanit::Hemen(yanit_ok(
            &id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "zodyvel", "version": env!("CARGO_PKG_VERSION") },
            }),
        )),
        Some("tools/list") => Yanit::Hemen(yanit_ok(&id, json!({ "tools": arac_listesi(seviye) }))),
        Some("tools/call") => {
            let ad = istek["params"]["name"].as_str().unwrap_or("");
            let args = istek["params"].get("arguments").cloned().unwrap_or(Value::Null);
            match rules::hazirla(seviye, tur, ad, &args) {
                Ok((action, args)) => Yanit::Cagri { id, action, args },
                Err(Red::Izinsiz) => Yanit::Hemen(yanit_hata(&id, -32602, &Red::Izinsiz.ileti())),
                // Sınır, döngü ve argüman hataları Zody'nin okuyup kullanıcıya söyleyebileceği araç hatasıdır.
                Err(red) => Yanit::Hemen(cagri_yaniti(&id, Err(red.ileti()))),
            }
        }
        Some("ping") => Yanit::Hemen(yanit_ok(&id, json!({}))),
        _ => Yanit::Hemen(yanit_hata(&id, -32601, "Bilinmeyen yöntem.")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::spec;

    fn hemen(y: Option<Yanit>) -> Value {
        match y.expect("yanıt") {
            Yanit::Hemen(v) => v,
            Yanit::Cagri { .. } => panic!("hemen yanıt bekleniyordu"),
        }
    }

    fn liste(seviye: Erisim) -> Vec<String> {
        let r = hemen(isle(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}), seviye, &mut Tur::default()));
        r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
    }

    #[test]
    fn kapalida_sifir_arac() {
        assert!(liste(Erisim::Kapali).is_empty());
        let r = hemen(isle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"music_read"}}), Erisim::Kapali, &mut Tur::default()));
        assert!(r.get("error").is_some() && r["error"]["message"] == "Bilinmeyen ya da izin verilmeyen araç.");
    }

    #[test]
    fn dusukte_yalniz_okuma_ve_guvenli() {
        let adlar = liste(Erisim::Dusuk);
        assert!(adlar.contains(&"music_control".to_owned()) && adlar.contains(&"music_read".to_owned()));
        assert!(!adlar.contains(&"controls_radio".to_owned()), "kritik eylem Düşük'te görünmemeli");
        assert!(!adlar.contains(&"agents_task".to_owned()));
    }

    #[test]
    fn ortada_kritik_eylem_de_var_ama_internal_asla() {
        let adlar = liste(Erisim::Orta);
        assert!(adlar.contains(&"controls_radio".to_owned()) && adlar.contains(&"agents_task".to_owned()));
        for yasak in ["access_set", "access_get", "update_settings", "agents_decide", "audit_clear", "brain_ask", "actions_decide"] {
            assert!(!adlar.iter().any(|a| a == yasak), "{yasak}");
        }
        assert_eq!(adlar.len(), rules::gorunen(Erisim::Orta).count());
    }

    #[test]
    fn izinsiz_cagri_reddedilir_ve_internal_hic_cagrilmaz() {
        for (seviye, ad) in [
            (Erisim::Dusuk, "controls_radio"),
            (Erisim::Orta, "access_set"),
            (Erisim::Orta, "agents_decide"),
            (Erisim::Orta, "update_settings"),
            (Erisim::Orta, "yok_boyle_arac"),
        ] {
            let r = hemen(isle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":ad,"arguments":{}}}), seviye, &mut Tur::default()));
            assert_eq!(r["error"]["message"], "Bilinmeyen ya da izin verilmeyen araç.", "{ad}");
            assert!(r.get("result").is_none(), "{ad}");
        }
    }

    #[test]
    fn izinli_cagri_kapidan_gecer_ve_eylem_hatti_icin_hazirlanir() {
        let mut tur = Tur::yeni("sesi aç", false);
        let y = isle(&json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"controls_volume","arguments":{"level":30}}}), Erisim::Dusuk, &mut tur).unwrap();
        let Yanit::Cagri { action, args, .. } = y else { panic!("çağrı bekleniyordu") };
        assert_eq!(action.name, "controls.volume");
        assert_eq!(args["level"], 30);
        assert_eq!(tur.sayac(), 1);
    }

    #[test]
    fn sinir_ve_dongu_arac_hatasi_olarak_doner() {
        let mut tur = Tur::default();
        let cagri = |tur: &mut Tur, dk: i64| {
            isle(&json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"today_timer_set","arguments":{"minutes":dk}}}), Erisim::Dusuk, tur)
        };
        assert!(matches!(cagri(&mut tur, 5), Some(Yanit::Cagri { .. })));
        let d = hemen(cagri(&mut tur, 5));
        assert_eq!(d["result"]["isError"], true);
        assert!(d["result"]["content"][0]["text"].as_str().unwrap().contains("yinelendi"));
        for dk in 6..=9 {
            assert!(matches!(cagri(&mut tur, dk), Some(Yanit::Cagri { .. })));
        }
        let s = hemen(cagri(&mut tur, 10));
        assert!(s["result"]["content"][0]["text"].as_str().unwrap().contains("en çok 5"));
    }

    #[test]
    fn sema_arg_ve_kind_den_uretilir() {
        let t = arac_tanimi(spec::find("today.event_add").unwrap());
        assert_eq!(t["name"], "today_event_add");
        assert_eq!(t["inputSchema"]["required"], json!(["title", "date"]));
        assert_eq!(t["inputSchema"]["properties"]["time"]["type"], "string");
        let m = arac_tanimi(spec::find("music.control").unwrap());
        assert_eq!(m["inputSchema"]["properties"]["action"]["enum"], json!(["toggle", "next", "prev"]));
        let v = arac_tanimi(spec::find("controls.volume").unwrap());
        assert_eq!(v["inputSchema"]["properties"]["level"]["maximum"], 100);
        assert_eq!(v["inputSchema"]["required"], json!([]));
    }

    #[test]
    fn baslatma_ping_bildirim() {
        let r = hemen(isle(&json!({"jsonrpc":"2.0","id":"a","method":"initialize"}), Erisim::Kapali, &mut Tur::default()));
        assert_eq!(r["result"]["serverInfo"]["name"], "zodyvel");
        assert_eq!(r["id"], "a");
        assert!(isle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}), Erisim::Kapali, &mut Tur::default()).is_none());
        assert!(hemen(isle(&json!({"jsonrpc":"2.0","id":3,"method":"x"}), Erisim::Kapali, &mut Tur::default())).get("error").is_some());
        assert!(hemen(isle(&json!({"jsonrpc":"2.0","id":9,"method":"ping"}), Erisim::Kapali, &mut Tur::default())).get("result").is_some());
    }
}
