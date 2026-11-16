//! T2 köprü sınamaları: döngü arayüzünde gerçek soketlerle eşleştirme, imzalı eylem, ret yolları.
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zodyvel_phone_proto::devices::*;
use zodyvel_phone_proto::session::*;
use zodyvel_phone_proto::pairing::*;
use zodyvel_phone_proto::proto::*;
use zodyvel_phone_proto::server::*;

#[derive(Default)]
struct MemVault(Mutex<Option<String>>);
impl Vault for MemVault {
    fn load(&self) -> Option<String> { self.0.lock().unwrap().clone() }
    fn store(&self, b: &str) -> Result<(), String> { *self.0.lock().unwrap() = Some(b.into()); Ok(()) }
    fn wipe(&self) -> Result<(), String> { *self.0.lock().unwrap() = None; Ok(()) }
}

#[derive(Default)]
struct TestHost {
    net: Mutex<bool>,
    acts: Mutex<Vec<Action>>,
    audits: Mutex<Vec<String>>,
    code: Mutex<Option<String>>,
    agenda: Mutex<Vec<AgendaItem>>,
}
impl Host for TestHost {
    fn state(&self) -> StateView {
        StateView {
            sessions: vec![SessionView { id: "claude:1".into(), tool: "claude".into(), name: "Deneme".into(), status: "waiting_approval".into(), step: "Dosya yaz".into(), reply: None, folder: "Zodyvel".into(), started: Some(1_794_000_000), steps: vec![StepView::new("read", "Okudu", "a.ts"), StepView::new("edit", "Düzenledi", "b.ts")] }],
            approvals: vec![ApprovalView { id: "ap1".into(), session: "claude:1".into(), kind: "permission".into(), action: "Dosya yaz".into(), questions: vec![] }],
        }
    }
    fn act(&self, _d: &str, a: Action) -> Result<(), String> { self.acts.lock().unwrap().push(a); Ok(()) }
    fn folders(&self) -> Vec<FolderItem> { vec![FolderItem { id: folder_id(7, "C:\\a"), name: "Proje A".into() }] }
    fn agenda(&self) -> AgendaView { AgendaView::new("2026-11-13".into(), self.agenda.lock().unwrap().clone()) }
    fn audit(&self, d: &str, w: &str, r: &str) { self.audits.lock().unwrap().push(format!("{d}:{w}:{r}")); }
    fn event(&self, e: Event) { if let Event::PairCode { code, .. } = e { *self.code.lock().unwrap() = Some(code); } }
    fn network_ok(&self) -> bool { *self.net.lock().unwrap() }
}

struct Phone { kp: Keypair, sk: SigningKey }
impl Phone {
    fn new() -> Self { Phone { kp: generate_keypair(), sk: SigningKey::random(&mut rand::rngs::OsRng) } }
    fn apub(&self) -> String { hex(self.sk.verifying_key().to_encoded_point(false).as_bytes()) }
}
fn conn(addr: std::net::SocketAddr, mode: u8) -> TcpStream {
    use std::io::Write;
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(&[mode]).unwrap();
    s
}
fn rj<S: std::io::Read + std::io::Write>(ch: &mut Channel<S>) -> Value { serde_json::from_slice(&ch.recv().unwrap()).unwrap() }
fn sj<S: std::io::Read + std::io::Write>(ch: &mut Channel<S>, v: Value) { ch.send(v.to_string().as_bytes()).unwrap(); }

fn setup() -> (Bridge, Arc<TestHost>, Arc<MemVault>) {
    let host = Arc::new(TestHost::default());
    *host.net.lock().unwrap() = true;
    let vault = Arc::new(MemVault::default());
    let b = Bridge::start(host.clone(), vault.clone(), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap();
    (b, host, vault)
}

/// Eşleştirmeyi baştan sona yapar; telefonu döndürür.
#[allow(dead_code)]
fn pair_phone(b: &Bridge, host: &TestHost) -> Phone {
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"Deneme Telefonu","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    assert_eq!(host.code.lock().unwrap().clone().unwrap(), ch.confirm, "iki tarafta aynı kod");
    { let c = host.code.lock().unwrap().clone().unwrap(); b.confirm_pairing_code(&c).unwrap(); }
    sj(&mut ch, json!({"t":"pc","ok":true}));
    assert_eq!(rj(&mut ch)["t"], "paired");
    ph
}

fn unreachable_pin(b: &Bridge, ph: &Phone) -> Channel<TcpStream> {
    // Masaüstü açık anahtarı QR'dan öğrenilir (gerçek telefon eşleşmede saklar).
    let qr_fp = DESK.with(|d| d.borrow().clone().unwrap());
    let mut ch = connect(conn(b.addr(), MODE_NORMAL), &ph.kp, b"", Trust::PinnedKey(&hex_decode(&qr_fp).unwrap())).unwrap();
    // O9: ilk ileti cihazın koruma durumu.
    let dev = rj(&mut ch);
    assert_eq!(dev["t"], "device");
    ch
}
thread_local!(static DESK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) });

fn act(ch: &mut Channel<TcpStream>, ph: &Phone, n: u64, a: Value) -> Value {
    let a = a.to_string();
    let sig: Signature = ph.sk.sign(&signing_bytes(&ch.handshake_hash, n, &a));
    sj(ch, json!({"t":"act","n":n,"a":a,"sig":hex(sig.to_der().as_bytes())}));
    loop { let m = rj(ch); if m["t"] == "ack" { return m; } }
}

fn paired() -> (Bridge, Arc<TestHost>, Phone) {
    let (b, host, _v) = setup();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    DESK.with(|d| *d.borrow_mut() = Some(qr.fp.clone()));
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"Deneme Telefonu","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    { let c = host.code.lock().unwrap().clone().unwrap(); b.confirm_pairing_code(&c).unwrap(); }
    sj(&mut ch, json!({"t":"pc","ok":true}));
    let p = rj(&mut ch);
    assert_eq!((p["t"].as_str(), p["hw"].as_bool()), (Some("paired"), Some(false)), "kanıtsız telefon korumasız işaretlenir");
    // Var olan sınamalar korumalı cihaz davranışını sınar; korumasız durum ayrı sınamada.
    let id = b.devices()[0].id.clone();
    b.mark_protected_for_tests(&id);
    (b, host, ph)
}

#[test]
fn pairing_then_signed_actions() {
    let (b, host, ph) = paired();
    assert_eq!(b.devices().len(), 1);
    let mut ch = unreachable_pin(&b, &ph);
    let st = rj(&mut ch);
    assert_eq!(st["t"], "state");
    assert_eq!(st["approvals"][0]["id"], "ap1");
    assert!(!st.to_string().contains("reply"), "az veri: ayrıntı varsayılan gitmez");
    // T4-c: klasör adı, başlangıç ve son adımlar gider.
    assert_eq!((st["sessions"][0]["folder"].as_str(), st["sessions"][0]["started"].as_i64()), (Some("Zodyvel"), Some(1_794_000_000)));
    assert_eq!(st["sessions"][0]["steps"][1]["label"], "Düzenledi");
    let ack = act(&mut ch, &ph, 1, json!({"k":"decide","id":"ap1","d":"allow"}));
    assert_eq!(ack["ok"], true);
    assert_eq!(host.acts.lock().unwrap().len(), 1);
    {
        let a = host.audits.lock().unwrap()[0].clone();
        // G4 (D3): kayıtta ad + kimlik ve sonuç.
        assert!(a.starts_with("Deneme Telefonu · ") && a.ends_with(":onay verdi:ok"), "{a}");
    }
    // Aynı sayaç tekrar: reddedilir.
    assert_eq!(act(&mut ch, &ph, 1, json!({"k":"decide","id":"ap1","d":"allow"}))["ok"], false);
    // "Her zaman izin ver" ve revize telefonda yok.
    assert_eq!(act(&mut ch, &ph, 2, json!({"k":"decide","id":"ap1","d":"always"}))["ok"], false);
    assert_eq!(act(&mut ch, &ph, 3, json!({"k":"decide","id":"ap1","d":"revise"}))["ok"], false);
    assert_eq!(host.acts.lock().unwrap().len(), 1);
    let fid = folder_id(7, "C:\\a");
    // Sıra no (sayı) artık geçersiz: klasör yalnız opak kimlikle verilir.
    assert_eq!(act(&mut ch, &ph, 4, json!({"k":"new_task","folder":0,"prompt":"x"}))["ok"], false);
    // Klasör yolu verilemez (bilinmeyen alan reddedilir).
    assert_eq!(act(&mut ch, &ph, 5, json!({"k":"new_task","folder":fid,"prompt":"x","cwd":"C:\\"}))["ok"], false);
    assert_eq!(act(&mut ch, &ph, 6, json!({"k":"new_task","folder":fid,"prompt":"merhaba"}))["ok"], true);
    sj(&mut ch, json!({"t":"folders"}));
    let l = rj(&mut ch);
    assert_eq!((l["list"][0]["name"].as_str(), l["list"][0]["id"].as_str()), (Some("Proje A"), Some(fid.as_str())));
    assert!(!l.to_string().contains("C:"), "yol telefona gitmez");
}

#[test]
fn bad_signature_is_rejected_and_connection_drops() {
    let (b, host, ph) = paired();
    let mut ch = unreachable_pin(&b, &ph);
    let _ = rj(&mut ch);
    let evil = Phone::new();
    for n in 1..=3 {
        let a = json!({"k":"decide","id":"ap1","d":"allow"}).to_string();
        let sig: Signature = evil.sk.sign(&signing_bytes(&ch.handshake_hash, n, &a));
        sj(&mut ch, json!({"t":"act","n":n,"a":a,"sig":hex(sig.to_der().as_bytes())}));
        let m = rj(&mut ch);
        assert_eq!(m["ok"], false);
    }
    assert!(ch.recv().is_err(), "3 kötü imzadan sonra bağlantı kesilir");
    assert!(host.acts.lock().unwrap().is_empty());
}

#[test]
fn signature_is_bound_to_session() {
    let (b, host, ph) = paired();
    let mut c1 = unreachable_pin(&b, &ph);
    let _ = rj(&mut c1);
    let a = json!({"k":"decide","id":"ap1","d":"allow"}).to_string();
    let old_sig: Signature = ph.sk.sign(&signing_bytes(&c1.handshake_hash, 1, &a));
    let mut c2 = unreachable_pin(&b, &ph);
    let _ = rj(&mut c2);
    sj(&mut c2, json!({"t":"act","n":1,"a":a,"sig":hex(old_sig.to_der().as_bytes())}));
    assert_eq!(rj(&mut c2)["ok"], false, "başka oturumun imzası geçmez");
    assert!(host.acts.lock().unwrap().is_empty());
}

#[test]
fn unpaired_device_and_wrong_token_fail() {
    let (b, host, ph) = paired();
    let stranger = Phone::new();
    let fp = DESK.with(|d| d.borrow().clone().unwrap());
    let r = connect(conn(b.addr(), MODE_NORMAL), &stranger.kp, b"", Trust::PinnedKey(&hex_decode(&fp).unwrap()));
    // T-Y2: el sıkışma tamamlanır ama yalnız `bye unpaired` gelir, sonra bağlantı kapanır.
    let mut c = r.expect("el sıkışma tamamlanır");
    let m = rj(&mut c);
    assert_eq!((m["t"].as_str(), m["why"].as_str()), (Some("bye"), Some("unpaired")), "eşli olmayan cihaz düşer");
    assert!(c.recv().is_err(), "red dışında hiçbir şey gelmez");
    assert!(b.devices().len() == 1, "bilinmeyen anahtar kayda eklenmez");
    // Yanlış jetonla eşleştirme.
    let _qr = b.open_pairing().unwrap();
    let r = connect(conn(b.addr(), MODE_PAIR), &stranger.kp, &[7u8; 32], Trust::PinnedKey(&hex_decode(&fp).unwrap()));
    assert!(r.is_err());
    assert!(host.acts.lock().unwrap().is_empty());
    let _ = ph;
}

#[test]
fn revoke_cuts_connection_and_trust() {
    let (b, _host, ph) = paired();
    let mut ch = unreachable_pin(&b, &ph);
    let _ = rj(&mut ch);
    let id = b.devices()[0].id.clone();
    assert!(b.revoke(&id).unwrap());
    let m = rj(&mut ch);
    assert_eq!((m["t"].as_str(), m["why"].as_str()), (Some("bye"), Some("revoked")));
    assert!(b.devices().is_empty());
    let r = connect(conn(b.addr(), MODE_NORMAL), &ph.kp, b"", Trust::PinnedKey(&hex_decode(&DESK.with(|d| d.borrow().clone().unwrap())).unwrap()));
    let mut c = r.expect("el sıkışma tamamlanır");
    let m = rj(&mut c);
    assert_eq!((m["t"].as_str(), m["why"].as_str()), (Some("bye"), Some("unpaired")), "kaldırılan telefon yeniden gelince eşli olmadığını öğrenir");
    assert!(c.recv().is_err());
}

#[test]
fn state_push_and_network_gate() {
    let (b, host, ph) = paired();
    let mut ch = unreachable_pin(&b, &ph);
    let _ = rj(&mut ch);
    // Ağ Ortak olunca bağlantı düşer.
    *host.net.lock().unwrap() = false;
    let m = rj(&mut ch);
    assert_eq!(m["why"], "network");
    b.push_state();
}

#[test]
fn pairing_declined_on_desktop_adds_nothing() {
    let (b, host, _v) = setup();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"X","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    b.decline_pairing();
    assert_eq!(rj(&mut ch)["why"], "declined");
    assert!(b.devices().is_empty());
    // Pencere tek kullanımlıktı: aynı QR ikinci kez çalışmaz.
    assert!(connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).is_err());
}

#[test]
fn refuses_public_network_and_non_local_bind() {
    let host = Arc::new(TestHost::default()); // net=false
    assert!(Bridge::start(host.clone(), Arc::new(MemVault::default()), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).is_err());
    *host.net.lock().unwrap() = true;
    assert!(Bridge::start(host.clone(), Arc::new(MemVault::default()), IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0).is_err());
    assert!(Bridge::start(host, Arc::new(MemVault::default()), "8.8.8.8".parse().unwrap(), 0).is_err());
}

#[test]
fn vault_blob_stays_under_credman_limit() {
    let mut d = VaultData { private: "a".repeat(64), public: "b".repeat(64), devices: vec![] };
    for i in 0..MAX_DEVICES {
        d.devices.push(Device { id: format!("{i:08}"), name: "n".repeat(24), pubkey: "c".repeat(64), apub: "d".repeat(130), paired_at: 1_794_000_000, last_seen: 1_794_000_000, ..Default::default() });
    }
    assert!(serde_json::to_string(&d).unwrap().len() < 2400);
    // G4: en kötü durum: korumasız nedeni (en uzun Türkçe metin), seviye, 3 seri özeti, liste beklemesi.
    for dev in d.devices.iter_mut() {
        dev.hw_level = "strongbox".into();
        dev.hw_why = "Telefonun önyüklemesi doğrulanmamış (kilidi açık ya da değiştirilmiş sistem).".into();
        dev.rv = vec!["0123abcd".into(); zodyvel_phone_proto::attest::MAX_RV];
        dev.rp = true;
    }
    let n = serde_json::to_string(&d).unwrap().len();
    assert!(n < 2560, "Kimlik Bilgisi Yöneticisi sınırı: {n}");
}

#[test]
fn old_device_must_repair() {
    let (b, _h, ph) = paired();
    let id = b.devices()[0].id.clone();
    b.age_device_for_tests(&id, REPAIR_AFTER_SECS + 10);
    let qr_fp = DESK.with(|d| d.borrow().clone().unwrap());
    let mut ch = connect(conn(b.addr(), MODE_NORMAL), &ph.kp, b"", Trust::PinnedKey(&hex_decode(&qr_fp).unwrap())).unwrap();
    let m = rj(&mut ch);
    assert_eq!(m["why"], "repair");
    assert!(b.devices().is_empty());
}

// ---- Güvenlik düzeltmeleri (Y1, O1–O4, O10, D3) ----

/// Hata verebilen kimlik deposu (Windows Kimlik Bilgisi Yöneticisi arızasını taklit eder).
#[derive(Default)]
struct FlakyVault {
    blob: Mutex<Option<String>>,
    fail_store: std::sync::atomic::AtomicBool,
    fail_wipe: std::sync::atomic::AtomicBool,
    fail_load: std::sync::atomic::AtomicBool,
    stores: std::sync::atomic::AtomicUsize,
}
impl Vault for FlakyVault {
    fn load(&self) -> Option<String> { self.blob.lock().unwrap().clone() }
    fn load_checked(&self) -> Result<Option<String>, String> {
        if self.fail_load.load(std::sync::atomic::Ordering::SeqCst) { Err("okunamadı".into()) } else { Ok(self.load()) }
    }
    fn store(&self, b: &str) -> Result<(), String> {
        if self.fail_store.load(std::sync::atomic::Ordering::SeqCst) { return Err("yazılamadı".into()); }
        self.stores.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.blob.lock().unwrap() = Some(b.into());
        Ok(())
    }
    fn wipe(&self) -> Result<(), String> {
        if self.fail_wipe.load(std::sync::atomic::Ordering::SeqCst) { return Err("silinemedi".into()); }
        *self.blob.lock().unwrap() = None;
        Ok(())
    }
}

fn paired_with(vault: Arc<dyn Vault>) -> (Bridge, Arc<TestHost>, Phone) {
    let host = Arc::new(TestHost::default());
    *host.net.lock().unwrap() = true;
    let b = Bridge::start(host.clone(), vault, IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    DESK.with(|d| *d.borrow_mut() = Some(qr.fp.clone()));
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"Deneme Telefonu","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    let c = host.code.lock().unwrap().clone().unwrap();
    b.confirm_pairing_code(&c).unwrap();
    sj(&mut ch, json!({"t":"pc","ok":true}));
    let p = rj(&mut ch);
    assert_eq!((p["t"].as_str(), p["hw"].as_bool()), (Some("paired"), Some(false)), "kanıtsız telefon korumasız işaretlenir");
    // Var olan sınamalar korumalı cihaz davranışını sınar; korumasız durum ayrı sınamada.
    let id = b.devices()[0].id.clone();
    b.mark_protected_for_tests(&id);
    (b, host, ph)
}

#[test]
fn y1_revoke_failure_is_reported_device_stays_untrusted_and_bridge_locks() {
    let v = Arc::new(FlakyVault::default());
    let (b, host, ph) = paired_with(v.clone());
    let mut ch = unreachable_pin(&b, &ph);
    let _ = rj(&mut ch);
    v.fail_store.store(true, std::sync::atomic::Ordering::SeqCst);
    let id = b.devices()[0].id.clone();
    let err = b.revoke(&id).unwrap_err();
    assert!(err.contains("silinemedi"), "{err}");
    assert!(b.devices().is_empty(), "bellekte güvensiz kalır");
    assert!(b.locked_down(), "güvenli taraf: dinleme kilitlenir");
    // Açık oturum kesilir, yeni bağlantı eylem yapamaz.
    let m = rj(&mut ch);
    assert_eq!(m["t"], "bye");
    let r = connect(conn(b.addr(), MODE_NORMAL), &ph.kp, b"", Trust::PinnedKey(&hex_decode(&DESK.with(|d| d.borrow().clone().unwrap())).unwrap()));
    assert!(r.is_err() || r.unwrap().recv().is_err());
    assert!(host.acts.lock().unwrap().is_empty());
    // Kayıt hâlâ eski cihazı tutuyor: kullanıcıya hata gösterilmeli (yukarıdaki Err), sessiz değil.
    assert!(v.load().unwrap().contains(&id));
}

#[test]
fn y1_revoke_unknown_id_is_false_not_error() {
    let (b, _h, _ph) = paired();
    assert_eq!(b.revoke("yok"), Ok(false));
}

#[test]
fn y1_revoke_all_reports_error() {
    let v = Arc::new(FlakyVault::default());
    let (b, _h, _ph) = paired_with(v.clone());
    v.fail_store.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(b.revoke_all().is_err());
}

#[test]
fn o10_unreadable_vault_does_not_reset_keys() {
    let v = Arc::new(FlakyVault::default());
    let (b, _h, _ph) = paired_with(v.clone());
    b.stop();
    let before = v.load().unwrap();
    let stores = v.stores.load(std::sync::atomic::Ordering::SeqCst);
    v.fail_load.store(true, std::sync::atomic::Ordering::SeqCst);
    let host = Arc::new(TestHost::default());
    *host.net.lock().unwrap() = true;
    assert!(Bridge::start(host.clone(), v.clone(), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).is_err(), "okunamıyorsa başlamaz");
    assert_eq!(v.stores.load(std::sync::atomic::Ordering::SeqCst), stores, "üstüne yazılmadı");
    assert_eq!(v.load().unwrap(), before);
    // Bozuk blob da sessizce silinmez.
    v.fail_load.store(false, std::sync::atomic::Ordering::SeqCst);
    *v.blob.lock().unwrap() = Some("{bozuk".into());
    assert!(Bridge::start(host, v.clone(), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).is_err());
    assert_eq!(v.load().unwrap(), "{bozuk");
}

#[test]
fn o1_failed_handshake_does_not_burn_the_qr() {
    let (b, host, _v) = setup();
    let qr = b.open_pairing().unwrap();
    // Jetonsuz saldırgan: yalnız mod baytı, sonra yanlış jetonla el sıkışma.
    { use std::io::Write; let mut s = TcpStream::connect(b.addr()).unwrap(); s.write_all(&[MODE_PAIR]).unwrap(); }
    let evil = Phone::new();
    assert!(connect(conn(b.addr(), MODE_PAIR), &evil.kp, &[9u8; 32], Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).is_err());
    assert!(b.pairing_open(), "pencere hâlâ açık");
    // Gerçek telefon QR ile eşleşebilir (tek kullanımlık hâlâ). Saldırganın yuvası (IP başına 1) bırakılana ve
    // yavaşlatma (G4) bitene dek kısa süre yeniden denenir.
    let ph = Phone::new();
    let t0 = Instant::now();
    let mut ch = loop {
        match connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())) {
            Ok(c) => break c,
            Err(e) => { assert!(t0.elapsed() < Duration::from_secs(5), "{e:?}"); std::thread::sleep(Duration::from_millis(100)); }
        }
    };
    sj(&mut ch, json!({"t":"hello","name":"Gerçek","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    b.decline_pairing();
    assert_eq!(rj(&mut ch)["why"], "declined");
}

#[test]
fn o2_wrong_typed_code_cancels_pairing_single_attempt() {
    let (b, host, _v) = setup();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"Saldırgan <b>","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    assert_eq!(b.pairing_name().unwrap(), "Saldırgan b", "ad süzülür");
    let real = host.code.lock().unwrap().clone().unwrap();
    let wrong = if real == "000000" { "000001" } else { "000000" };
    assert!(b.confirm_pairing_code(wrong).is_err());
    assert_eq!(rj(&mut ch)["why"], "declined");
    // Tek deneme: doğru kod bile artık işe yaramaz.
    assert!(b.confirm_pairing_code(&real).is_err());
    assert!(b.devices().is_empty());
}

#[test]
fn o3_idle_connections_cannot_hold_slots_forever() {
    let (b, host, _v) = setup();
    use std::io::{Read, Write};
    // Saldırgan bağlanır ve susar; aynı IP'den ikinci bağlantı hemen reddedilir.
    let mut idle = TcpStream::connect(b.addr()).unwrap();
    idle.write_all(&[MODE_NORMAL]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let mut second = TcpStream::connect(b.addr()).unwrap();
    second.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let _ = second.write_all(&[MODE_NORMAL]);
    let mut buf = [0u8; 1];
    assert!(matches!(second.read(&mut buf), Ok(0) | Err(_)), "ikinci bağlantı kesilir");
    // El sıkışma toplam süresi (4 sn) dolunca boşta bağlantı kesilir, gerçek telefon yine bağlanır.
    std::thread::sleep(Duration::from_millis(4300));
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    sj(&mut ch, json!({"t":"hello","name":"Gerçek","apub":ph.apub()}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    b.decline_pairing();
    let _ = idle;
}

#[test]
fn o4_action_after_revoke_is_not_executed() {
    let (b, host, ph) = paired();
    let mut ch = unreachable_pin(&b, &ph);
    let _ = rj(&mut ch);
    let id = b.devices()[0].id.clone();
    b.revoke(&id).unwrap();
    // Bye iletisi okunmadan imzalı eylem gönderilir.
    let a = json!({"k":"decide","id":"ap1","d":"allow"}).to_string();
    let sig: Signature = ph.sk.sign(&signing_bytes(&ch.handshake_hash, 1, &a));
    sj(&mut ch, json!({"t":"act","n":1,"a":a,"sig":hex(sig.to_der().as_bytes())}));
    let t0 = Instant::now();
    while let Ok(m) = ch.recv() {
        let m: Value = serde_json::from_slice(&m).unwrap();
        assert_ne!(m["ok"], true, "eylem kabul edilmemeli");
        assert!(t0.elapsed() < Duration::from_secs(5));
    }
    assert!(host.acts.lock().unwrap().is_empty());
}

#[test]
fn d3_name_filter_drops_bidi_and_markup() {
    assert_eq!(clean_name("A\u{202e}b<i>c"), "Abic");
    assert_eq!(clean_name("\u{2066}\u{2069}"), "Telefon");
}

#[test]
fn y1_wipe_verified_falls_back_to_overwrite_and_reports_total_failure() {
    let v = FlakyVault::default();
    let (b, _h, _ph) = paired_with(Arc::new(FlakyVault::default()));
    drop(b);
    *v.blob.lock().unwrap() = Some(serde_json::to_string(&VaultData { private: "a".repeat(64), public: "b".repeat(64), devices: vec![Device { id: "x".into(), name: "n".into(), pubkey: "c".into(), apub: "d".into(), paired_at: 1, last_seen: 1, ..Default::default() }] }).unwrap());
    v.fail_wipe.store(true, std::sync::atomic::Ordering::SeqCst);
    wipe_verified(&v).expect("silme yerine boş kayıt yazılır");
    let d: VaultData = serde_json::from_str(&v.load().unwrap()).unwrap();
    assert!(d.devices.is_empty() && d.private.is_empty());
    // Hem silme hem yazma bozuksa hata yüzeye çıkar (ve kayıt hâlâ dolu).
    *v.blob.lock().unwrap() = Some("{\"private\":\"a\",\"public\":\"b\",\"devices\":[]}".into());
    v.fail_store.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(wipe_verified(&v).is_err());
}

fn item(id: u64, date: &str, title: &str) -> AgendaItem {
    AgendaItem { id, date: date.into(), time: Some("09:00".into()), title: title.into(), minutes: 30, lead: 5, notify: true }
}

#[test]
fn t2b_agenda_arrives_on_connect_on_push_and_on_request() {
    let (b, host, ph) = paired();
    *host.agenda.lock().unwrap() = vec![item(1, "2026-11-13", "Koşu")];
    let mut ch = unreachable_pin(&b, &ph);
    assert_eq!(rj(&mut ch)["t"], "state");
    // Ajanda istenmeden gitmez; telefon isteyince gelir ve sonra değişiklikler itilir.
    b.push_agenda();
    sj(&mut ch, json!({"t":"ping"}));
    assert_eq!(rj(&mut ch)["t"], "pong", "istenmeyen ajanda gitmez");
    sj(&mut ch, json!({"t":"agenda"}));
    let a = rj(&mut ch);
    assert_eq!((a["t"].as_str(), a["from"].as_str(), a["days"].as_i64()), (Some("agenda"), Some("2026-11-13"), Some(14)));
    assert_eq!(a["items"][0]["title"], "Koşu");
    // Az veri: yalnız bu alanlar gider (renk, tekrar kuralı, gün maskesi yok).
    let keys: Vec<&str> = a["items"][0].as_object().unwrap().keys().map(String::as_str).collect();
    for k in ["id", "date", "time", "title", "minutes", "lead", "notify"] { assert!(keys.contains(&k), "{k}"); }
    assert_eq!(keys.len(), 7, "{keys:?}");
    // Plan eklenince itilir; değişmediyse aynı ileti tekrar gitmez.
    *host.agenda.lock().unwrap() = vec![item(1, "2026-11-13", "Koşu"), item(2, "2026-11-14", "Toplantı")];
    b.push_agenda();
    let a = rj(&mut ch);
    assert_eq!(a["items"].as_array().unwrap().len(), 2);
    b.push_agenda();
    sj(&mut ch, json!({"t":"ping"}));
    assert_eq!(rj(&mut ch)["t"], "pong", "değişmeyen ajanda tekrar gönderilmez");
    // Plan silinince listeden düşer.
    *host.agenda.lock().unwrap() = vec![item(2, "2026-11-14", "Toplantı")];
    b.push_agenda();
    let a = rj(&mut ch);
    assert_eq!((a["items"].as_array().unwrap().len(), a["items"][0]["id"].as_u64()), (1, Some(2)));
    // İstek üzerine yeniden gönderir.
    sj(&mut ch, json!({"t":"agenda"}));
    assert_eq!(rj(&mut ch)["t"], "agenda");
}

#[test]
fn t2b_agenda_is_capped() {
    let many: Vec<AgendaItem> = (0..500).map(|i| item(i, "2026-11-13", "x")).collect();
    assert_eq!(AgendaView::new("2026-11-13".into(), many).items.len(), MAX_AGENDA_ITEMS);
}

#[test]
fn o6_folder_ids_are_stable_salted_and_distinct() {
    assert_eq!(folder_id(1, "C:\\a"), folder_id(1, "C:\\a"));
    assert_ne!(folder_id(1, "C:\\a"), folder_id(2, "C:\\a"));
    assert_ne!(folder_id(1, "C:\\a"), folder_id(1, "C:\\b"));
    assert_eq!(folder_id(1, "C:\\a").len(), 16);
    assert!(Action::NewTask { folder: "".into(), prompt: "x".into() }.validate().is_err());
    assert!(Action::NewTask { folder: "a".repeat(33), prompt: "x".into() }.validate().is_err());
    assert!(Action::NewTask { folder: folder_id(1, "p"), prompt: "x".into() }.validate().is_ok());
}

#[test]
fn o9_unprotected_phone_is_marked_and_told() {
    let (b, host, _v) = setup();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    DESK.with(|d| *d.borrow_mut() = Some(qr.fp.clone()));
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    // Bozuk kanıt zinciri: eşleşme reddedilmez, cihaz korumasız kaydedilir.
    sj(&mut ch, json!({"t":"hello","name":"Eski Telefon","apub":ph.apub(),"att":["3000","00"]}));
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    { let c = host.code.lock().unwrap().clone().unwrap(); b.confirm_pairing_code(&c).unwrap(); }
    sj(&mut ch, json!({"t":"pc","ok":true}));
    let p = rj(&mut ch);
    assert_eq!((p["t"].as_str(), p["hw"].as_bool(), p["level"].as_str()), (Some("paired"), Some(false), Some("")));
    let d = &b.devices()[0];
    assert!(!d.hw && d.hw_level.is_empty() && !d.hw_why.is_empty());
    assert!(!d.hw_why.contains("3000"), "neden metni içerik taşımaz");

    let mut ch = connect(conn(b.addr(), MODE_NORMAL), &ph.kp, b"", Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    let dev = rj(&mut ch);
    assert_eq!((dev["t"].as_str(), dev["hw"].as_bool()), (Some("device"), Some(false)));
    assert_eq!(rj(&mut ch)["t"], "state", "durum korumasız telefona da gider");
    // Onay/ret, cevap, durdurma her iki politikada açık.
    assert_eq!(act(&mut ch, &ph, 1, json!({"k":"decide","id":"ap1","d":"allow"}))["ok"], true);
    assert_eq!(act(&mut ch, &ph, 2, json!({"k":"stop","session":"claude:1"}))["ok"], true);
    if !RESTRICT_UNPROTECTED {
        // Kısıt kapalı: korumasız telefon da yeni görev başlatır, klasörleri alır.
        assert_eq!(act(&mut ch, &ph, 3, json!({"k":"new_task","folder":folder_id(7, "C:\\a"),"prompt":"x"}))["ok"], true);
        return;
    }
    for (n, a) in [(3, json!({"k":"note","session":"claude:1","text":"x"})), (4, json!({"k":"task","session":"claude:1","text":"x"})), (5, json!({"k":"new_task","folder":folder_id(7, "C:\\a"),"prompt":"x"}))] {
        let ack = act(&mut ch, &ph, n, a);
        assert_eq!((ack["ok"].as_bool(), ack["err"].as_str()), (Some(false), Some(UNPROTECTED_DENIED)));
    }
    assert_eq!(host.acts.lock().unwrap().len(), 2, "kapalı eylemler masaüstüne ulaşmaz");
    sj(&mut ch, json!({"t":"folders"}));
    loop {
        let m = rj(&mut ch);
        if m["t"] == "folders" {
            assert_eq!(m["list"].as_array().map(Vec::len), Some(0), "korumasıza klasör adı gitmez");
            break;
        }
    }
}

#[test]
fn o9_oversized_hello_is_refused() {
    let (b, host, _v) = setup();
    let ph = Phone::new();
    let qr = b.open_pairing().unwrap();
    let mut ch = connect(conn(b.addr(), MODE_PAIR), &ph.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    let big: Vec<String> = (0..10).map(|_| "ab".repeat(2000)).collect(); // ~40 KB > 32 KB sınırı
    sj(&mut ch, json!({"t":"hello","name":"X","apub":ph.apub(),"att":big}));
    std::thread::sleep(Duration::from_millis(300));
    assert!(host.code.lock().unwrap().is_none(), "sınırı aşan hello eşleştirmeyi başlatmaz");
}

// ---- G4 ----

fn wait_for<F: Fn() -> bool>(f: F) {
    let t0 = Instant::now();
    while !f() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
}

#[test]
fn g4_revocation_list_downgrades_and_pending_upgrades() {
    use zodyvel_phone_proto::attest::{rv_hash, Revocations};
    let (b, _h, ph) = paired();
    let id = b.devices()[0].id.clone();
    // Eşleşmede liste yoktu: zincir geçerli (tee), karar beklemede.
    b.set_attest_for_tests(&id, false, "tee", vec![rv_hash("abc")], true);
    let mut ch = unreachable_pin(&b, &ph);
    assert_eq!(rj(&mut ch)["t"], "state");
    let clean = Revocations::from_json(r#"{"entries":{"999":{"status":"REVOKED"}}}"#).unwrap();
    assert_eq!(b.apply_revocations(&clean), Ok(1));
    assert!(b.devices()[0].hw, "listede değil: korumalı");
    let m = loop { let m = rj(&mut ch); if m["t"] == "device" { break m; } };
    assert_eq!((m["hw"].as_bool(), m["level"].as_str()), (Some(true), Some("tee")));
    // Aynı liste tekrar: değişiklik yok.
    assert_eq!(b.apply_revocations(&clean), Ok(0));
    // Sonradan iptal edildi: korumasız, açık oturuma bildirilir, kısıtlı eylem reddedilir.
    let listed = Revocations::from_json(r#"{"entries":{"ABC":{"status":"REVOKED","reason":"KEY_COMPROMISE"}}}"#).unwrap();
    assert_eq!(b.apply_revocations(&listed), Ok(1));
    let d = &b.devices()[0];
    assert!(!d.hw && d.hw_why.contains("iptal"));
    let m = loop { let m = rj(&mut ch); if m["t"] == "device" { break m; } };
    assert_eq!(m["hw"], false);
    let ack = act(&mut ch, &ph, 1, json!({"k":"new_task","folder":folder_id(7, "C:\\a"),"prompt":"x"}));
    assert_eq!(ack["ok"], false, "oturum sürerken düşürülen koruma hemen geçerli");
}

#[test]
fn g4_pairing_decision_without_list_is_pending() {
    use zodyvel_phone_proto::attest::{rv_hash, Attested, Level, Revocations};
    let a = Attested { level: Level::Tee, rv: vec![rv_hash("abc")] };
    let (hw, why, rp) = decide_revocation_for_tests(a.clone(), None);
    assert!(!hw && rp && why.contains("internet"));
    let (hw, _, rp) = decide_revocation_for_tests(a.clone(), Some(&Revocations::from_json(r#"{"entries":{}}"#).unwrap()));
    assert!(hw && !rp);
    let (hw, why, _) = decide_revocation_for_tests(a, Some(&Revocations::from_json(r#"{"entries":{"abc":{}}}"#).unwrap()));
    assert!(!hw && why.contains("iptal"));
}

#[test]
fn g4_disconnect_refreshes_last_seen() {
    let (b, _h, ph) = paired();
    let id = b.devices()[0].id.clone();
    b.age_device_for_tests(&id, 29 * 24 * 3600);
    let old = b.last_seen_for_tests(&id).unwrap();
    {
        let mut ch = unreachable_pin(&b, &ph);
        assert_eq!(rj(&mut ch)["t"], "state");
    }
    wait_for(|| b.last_seen_for_tests(&id).unwrap() > old + 3600);
    assert!(!b.devices()[0].expired);
    b.age_device_for_tests(&id, 31 * 24 * 3600);
    assert!(b.devices()[0].expired, "arayüz 30 günü aşanı gösterir");
}

#[test]
fn g4_message_flood_closes_the_session() {
    let (b, _h, ph) = paired();
    let mut ch = unreachable_pin(&b, &ph);
    assert_eq!(rj(&mut ch)["t"], "state");
    for _ in 0..21 { if ch.send(json!({"t":"ping"}).to_string().as_bytes()).is_err() { break; } }
    let mut got_bye = false;
    for _ in 0..40 {
        match ch.recv() {
            Ok(b) => { let m: Value = serde_json::from_slice(&b).unwrap(); if m["t"] == "bye" { assert_eq!(m["why"], "busy"); got_bye = true; break; } }
            Err(_) => break,
        }
    }
    assert!(got_bye);
}

#[test]
fn g4_denied_actions_are_audited_too() {
    let (b, host, ph) = paired();
    let id = b.devices()[0].id.clone();
    b.set_attest_for_tests(&id, false, "", vec![], false);
    let mut ch = unreachable_pin(&b, &ph);
    assert_eq!(rj(&mut ch)["t"], "state");
    let ack = act(&mut ch, &ph, 1, json!({"k":"new_task","folder":folder_id(7, "C:\\a"),"prompt":"x"}));
    assert_eq!(ack["ok"], false);
    let a = host.audits.lock().unwrap().last().cloned().unwrap();
    assert!(a.contains("reddedildi"), "{a}");
}

#[test]
fn g4_reason_codes_round_trip_and_old_text_still_reads() {
    let d = Device { id: "x".into(), hw_why: zodyvel_phone_proto::server::REVOKED_WHY.into(), ..Default::default() };
    let j = serde_json::to_string(&d).unwrap();
    assert!(j.contains("\"hw_why\":\"#16\""), "{j}");
    assert_eq!(serde_json::from_str::<Device>(&j).unwrap().hw_why, zodyvel_phone_proto::server::REVOKED_WHY);
    let old: Device = serde_json::from_str(r#"{"id":"x","name":"n","pubkey":"p","apub":"a","paired_at":1,"last_seen":1,"hw":false,"hw_level":"","hw_why":"Eski düz metin."}"#).unwrap();
    assert_eq!(old.hw_why, "Eski düz metin.");
    let long = Device { hw_why: "ç".repeat(200), ..Default::default() };
    let back: Device = serde_json::from_str(&serde_json::to_string(&long).unwrap()).unwrap();
    assert_eq!(back.hw_why.chars().count(), WHY_MAX);
}

// ---- G1–G4 inceleme (M-O1, M-O3) ----

#[test]
fn m_o1_stop_ends_pairing_and_session_threads() {
    let (b, _h, ph) = paired();
    let probe = b.thread_probe_for_tests();
    // Bağlı oturum (okuyan + yazan) ve el sıkışması bitmiş, `hello` bekleyen (20 sn) eşleştirme.
    let mut session = unreachable_pin(&b, &ph);
    assert_eq!(rj(&mut session)["t"], "state");
    let ph2 = Phone::new();
    let qr = b.open_pairing().unwrap();
    let _pairing = connect(conn(b.addr(), MODE_PAIR), &ph2.kp, &qr.token_bytes(), Trust::PinnedKey(&hex_decode(&qr.fp).unwrap())).unwrap();
    wait_for(|| probe() == 3);
    let t0 = Instant::now();
    b.stop();
    wait_for(|| probe() == 0);
    // Windows'ta `shutdown` bekleyen okumayı uyandırmaz: oturum en geç 2 sn'lik denetim aralığında, eşleştirme 1 sn'de biter.
    assert!(t0.elapsed() < Duration::from_secs(3), "iş parçacıkları beklemeden biter: {:?}", t0.elapsed());
}

#[test]
fn m_o3_pending_device_without_serials_does_not_stay_pending() {
    use zodyvel_phone_proto::attest::Revocations;
    let (b, _h, _ph) = paired();
    let id = b.devices()[0].id.clone();
    b.set_attest_for_tests(&id, false, "tee", vec![], true);
    let list = Revocations::from_json(r#"{"entries":{}}"#).unwrap();
    assert_eq!(b.apply_revocations(&list), Ok(1));
    let d = &b.devices()[0];
    assert!(!d.hw && d.hw_why.contains("uzunluğu"), "korumasız olarak sonuçlanır");
    assert_eq!(b.apply_revocations(&list), Ok(0), "artık beklemede değil");
}
