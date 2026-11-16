//! Telefon simülatörü (T2): gerçek köprü sunucusunu (`server.rs`) bilgisayarda dener.
//! SADECE GELİŞTİRME: simülatörün anahtarları çalışma klasöründe düz JSON'dadır
//! (gerçekte masaüstünde Kimlik Bilgisi Yöneticisi, telefonda Keystore).
//!
//!   Pencere 1: phone-sim desktop pair        → qr.txt yazar, kodu gösterir, Enter = "Bu telefonu eşle"
//!   Pencere 2: phone-sim phone pair qr.txt   → eşleşir (QR metni dosyadan okunur; PowerShell tırnak sorunu yok)
//!   Sonra:     phone-sim desktop             → normal mod (yalnız eşli telefon), sahte onay listesi
//!              phone-sim phone run           → durumu yazar, ilk onayı İZİN VER ile imzalı yollar
//!              phone-sim phone run deny      → reddet
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::sync::{Arc, Mutex};
use zodyvel_phone_proto::devices::*;
use zodyvel_phone_proto::net::{listening_allowed, query_profiles};
use zodyvel_phone_proto::pairing::*;
use zodyvel_phone_proto::proto::*;
use zodyvel_phone_proto::server::*;
use zodyvel_phone_proto::session::*;

struct FileVault;
impl Vault for FileVault {
    fn load(&self) -> Option<String> { std::fs::read_to_string("desktop-vault.json").ok() }
    fn store(&self, b: &str) -> Result<(), String> { std::fs::write("desktop-vault.json", b).map_err(|_| "yazılamadı".into()) }
    fn wipe(&self) -> Result<(), String> { std::fs::remove_file("desktop-vault.json").map_err(|e| e.to_string()) }
}

struct SimHost { approvals: Mutex<Vec<ApprovalView>> }
impl Host for SimHost {
    fn state(&self) -> StateView {
        StateView {
            sessions: vec![SessionView { id: "claude:sim".into(), tool: "claude".into(), name: "Deneme oturumu".into(), status: "waiting_approval".into(), step: "Dosya yaz".into(), reply: None, ..Default::default() }],
            approvals: self.approvals.lock().unwrap().clone(),
        }
    }
    fn act(&self, d: &str, a: Action) -> Result<(), String> {
        println!("[{d}] eylem: {a:?}");
        if let Action::Decide { id, .. } = &a {
            let mut ap = self.approvals.lock().unwrap();
            if !ap.iter().any(|x| &x.id == id) { return Err("Bu istek artık yok.".into()); }
            ap.retain(|x| &x.id != id);
        }
        Ok(())
    }
    fn folders(&self) -> Vec<FolderItem> { vec![FolderItem { id: folder_id(0, "sim"), name: "Deneme klasörü".into() }] }
    fn audit(&self, d: &str, w: &str, r: &str) { println!("denetim: {d} telefondan {w} ({r})"); }
    fn event(&self, e: Event) {
        if let Event::PairCode { code, name } = e { let _ = code; println!("Telefon (doğrulanmadı): {name} — telefonun ekranındaki 6 haneli kodu yaz ve Enter."); }
    }
    fn network_ok(&self) -> bool {
        std::env::var("ZODYVEL_SIM_FORCE").is_ok() || query_profiles().map(|p| listening_allowed(&p)).unwrap_or(true)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct PhoneState { private: String, public: String, sign: String, desk_pub: String, addr: String }

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["desktop"] => desktop(false),
        ["desktop", "pair"] => desktop(true),
        ["phone", "pair", qrfile] => phone_pair(qrfile),
        ["phone", "run"] => phone_run(true),
        ["phone", "run", "deny"] => phone_run(false),
        _ => eprintln!("kullanım: phone-sim desktop [pair] | phone pair qr.txt | phone run [deny]"),
    }
}

fn desktop(pair: bool) {
    let host = Arc::new(SimHost { approvals: Mutex::new(vec![ApprovalView { id: "ap1".into(), session: "claude:sim".into(), kind: "permission".into(), action: "Dosya yaz: notlar.txt".into(), questions: vec![] }]) });
    let bridge = match Bridge::start(host, Arc::new(FileVault), IpAddr::V4(Ipv4Addr::LOCALHOST), 0) {
        Ok(b) => b,
        Err(e) => { eprintln!("{e} (denemek için ZODYVEL_SIM_FORCE=1)"); return; }
    };
    std::fs::write("desktop-addr.txt", bridge.addr().to_string()).ok();
    println!("Dinliyor: {}", bridge.addr());
    if pair {
        let qr = bridge.open_pairing().expect("pencere");
        std::fs::write("qr.txt", qr.to_json()).expect("qr.txt");
        println!("QR yükü qr.txt dosyasına yazıldı (2 dk, tek kullanım). Telefon: phone-sim phone pair qr.txt");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
        if let Err(e) = bridge.confirm_pairing_code(line.trim()) { eprintln!("{e}"); }
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = std::fs::remove_file("qr.txt"); // jeton diskte kalmasın
        println!("Eşli cihazlar: {:?}", bridge.devices());
    } else {
        println!("Normal mod. Çıkmak için Enter.");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
    }
    bridge.stop();
}

fn tcp(addr: &str, mode: u8) -> TcpStream {
    use std::io::Write;
    let mut s = TcpStream::connect(addr.trim()).expect("masaüstüne bağlanılamadı");
    s.write_all(&[mode]).unwrap();
    s
}

fn phone_pair(qrfile: &str) {
    let q = QrPayload::from_json(std::fs::read_to_string(qrfile).expect("qr dosyası").trim()).unwrap_or_else(|e| { eprintln!("{e}"); std::process::exit(1) });
    let k = generate_keypair();
    let sk = SigningKey::random(&mut rand::rngs::OsRng);
    let apub = hex(sk.verifying_key().to_encoded_point(false).as_bytes());
    let mut ch = match connect(tcp(&q.addr, MODE_PAIR), &k, &q.token_bytes(), Trust::PinnedKey(&hex_decode(&q.fp).unwrap())) {
        Ok(c) => c,
        Err(e) => { eprintln!("eşleşme başarısız: {e:?}"); return; }
    };
    println!("Doğrulama kodu: {}  ← masaüstündekiyle AYNI mı?", ch.confirm);
    ch.send(json!({"t":"hello","name":"Sim Telefon","apub":apub}).to_string().as_bytes()).unwrap();
    ch.send(json!({"t":"pc","ok":true}).to_string().as_bytes()).unwrap(); // sim: telefonda "aynı" dendi
    let m: Value = serde_json::from_slice(&ch.recv().unwrap()).unwrap();
    println!("masaüstü: {m}");
    if m["t"] == "paired" {
        let st = PhoneState { private: hex(&k.private), public: hex(&k.public), sign: hex(&sk.to_bytes()), desk_pub: q.fp, addr: q.addr };
        std::fs::write("phone-state.json", serde_json::to_string_pretty(&st).unwrap()).unwrap();
    }
}

fn phone_run(allow: bool) {
    let st: PhoneState = serde_json::from_str(&std::fs::read_to_string("phone-state.json").expect("önce: phone pair")).unwrap();
    let addr = std::fs::read_to_string("desktop-addr.txt").unwrap_or(st.addr.clone());
    let kp = Keypair { private: hex_decode(&st.private).unwrap(), public: hex_decode(&st.public).unwrap() };
    let sk = SigningKey::from_slice(&hex_decode(&st.sign).unwrap()).unwrap();
    let mut ch = match connect(tcp(&addr, MODE_NORMAL), &kp, b"", Trust::PinnedKey(&hex_decode(&st.desk_pub).unwrap())) {
        Ok(c) => c,
        Err(e) => { eprintln!("bağlanamadı: {e:?}"); return; }
    };
    let mut s: Value = serde_json::from_slice(&ch.recv().unwrap()).unwrap();
    if s["t"] == "device" {
        println!("cihaz: {s}  (hw=false → korumasız, kısıtlı)");
        s = serde_json::from_slice(&ch.recv().unwrap()).unwrap();
    }
    println!("durum: {s}");
    let Some(id) = s["approvals"][0]["id"].as_str() else { println!("bekleyen onay yok"); return };
    let a = json!({"k":"decide","id":id,"d": if allow {"allow"} else {"deny"}}).to_string();
    let sig: Signature = sk.sign(&signing_bytes(&ch.handshake_hash, 1, &a));
    ch.send(json!({"t":"act","n":1,"a":a,"sig":hex(sig.to_der().as_bytes())}).to_string().as_bytes()).unwrap();
    println!("cevap: {}", String::from_utf8_lossy(&ch.recv().unwrap()));
}
