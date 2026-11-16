//! T4-a: telefon istemcisi (client.rs) gerçek köprüye karşı: eşleştirme, durum, imzalı eylem.
use p256::ecdsa::{signature::Signer as _, Signature, SigningKey};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zodyvel_phone_proto::client::*;
use zodyvel_phone_proto::devices::*;
use zodyvel_phone_proto::pairing::*;
use zodyvel_phone_proto::proto::*;
use zodyvel_phone_proto::server::*;
use zodyvel_phone_proto::session::generate_keypair;

#[derive(Default)]
struct MemVault(Mutex<Option<String>>);
impl Vault for MemVault {
    fn load(&self) -> Option<String> { self.0.lock().unwrap().clone() }
    fn store(&self, b: &str) -> Result<(), String> { *self.0.lock().unwrap() = Some(b.into()); Ok(()) }
    fn wipe(&self) -> Result<(), String> { *self.0.lock().unwrap() = None; Ok(()) }
}
#[derive(Default)]
struct H { acts: Mutex<Vec<Action>>, code: Mutex<Option<String>> }
impl Host for H {
    fn state(&self) -> StateView {
        StateView { sessions: vec![], approvals: vec![ApprovalView { id: "ap1".into(), session: "claude:1".into(), kind: "permission".into(), action: "Dosya yaz".into(), questions: vec![] }] }
    }
    fn act(&self, _d: &str, a: Action) -> Result<(), String> { self.acts.lock().unwrap().push(a); Ok(()) }
    fn folders(&self) -> Vec<FolderItem> { vec![] }
    fn audit(&self, _: &str, _: &str, _: &str) {}
    fn event(&self, e: Event) { if let Event::PairCode { code, .. } = e { *self.code.lock().unwrap() = Some(code); } }
    fn network_ok(&self) -> bool { true }
}
struct Sk(SigningKey);
impl Signer for Sk {
    fn sign(&self, m: &[u8]) -> Result<Vec<u8>, String> { let s: Signature = self.0.sign(m); Ok(s.to_der().as_bytes().to_vec()) }
}

#[test]
fn client_pairs_connects_and_acts() {
    let host = Arc::new(H::default());
    let b = Bridge::start(host.clone(), Arc::new(MemVault::default()), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap();
    let qr = b.open_pairing().unwrap();
    let kp = generate_keypair();
    let sk = SigningKey::random(&mut rand::rngs::OsRng);
    let apub = hex(sk.verifying_key().to_encoded_point(false).as_bytes());
    let qr = QrPayload::from_json(&qr.to_json()).unwrap();
    let addr = b.addr().to_string();
    let qr = QrPayload { addr, ..qr };
    // Uygulamanın yolu: anahtar el sıkışmadan sonra, sınama = el sıkışma özeti.
    let mut seen = Vec::new();
    let ps = start_pairing_with(&qr, &kp, "Deneme Telefonu", |hh| {
        seen = hh.to_vec();
        Ok((apub.clone(), vec!["00".into(), "00".into()]))
    })
    .unwrap();
    assert_eq!(seen.len(), 32, "sınama el sıkışma özeti (BLAKE2s, 32 bayt)");
    let t0 = Instant::now();
    while host.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    let code = host.code.lock().unwrap().clone().unwrap();
    assert_eq!(code, ps.confirm);
    b.confirm_pairing_code(&code).unwrap();
    let paired = ps.finish(true).unwrap();
    assert!(!paired.id.is_empty());
    assert_eq!(b.devices().len(), 1);

    // Geçersiz kanıt ("00"): eşleşme yine olur ama korumasız (O9: reddetme yok, işaretle).
    assert_eq!((paired.hw, paired.level.as_str()), (Some(false), ""));
    assert!(!b.devices()[0].hw && !b.devices()[0].hw_why.is_empty());

    let mut link = connect_normal(&paired.addr, &kp, &paired.desktop_pub).unwrap();
    let dev = link.recv(Duration::from_secs(3)).unwrap().unwrap();
    assert_eq!((dev["t"].as_str(), dev["hw"].as_bool()), (Some("device"), Some(false)));
    let st = link.recv(Duration::from_secs(3)).unwrap().unwrap();
    assert_eq!(st["t"], "state");
    assert!(link.recv(Duration::from_millis(50)).unwrap().is_none(), "sessizlik hata değil");
    let n = link.act(&json!({"k":"decide","id":"ap1","d":"allow"}), &Sk(sk)).unwrap();
    let ack = link.recv(Duration::from_secs(3)).unwrap().unwrap();
    assert_eq!((ack["t"].as_str(), ack["n"].as_u64(), ack["ok"].as_bool()), (Some("ack"), Some(n), Some(true)));
    assert_eq!(host.acts.lock().unwrap().len(), 1);
}

#[test]
fn client_refuses_wrong_desktop_and_declined_pairing() {
    let host = Arc::new(H::default());
    let b = Bridge::start(host.clone(), Arc::new(MemVault::default()), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap();
    let qr = b.open_pairing().unwrap();
    let kp = generate_keypair();
    // Yanlış parmak izi (araya girme): bağlantı kurulamaz.
    let bad = QrPayload { addr: b.addr().to_string(), fp: hex(&generate_keypair().public), ..qr.clone() };
    assert!(start_pairing(&bad, &kp, "X", &"04".repeat(65), &[]).is_err());
    // Kullanıcı "vazgeç" der.
    let good = QrPayload { addr: b.addr().to_string(), ..qr };
    let sk = SigningKey::random(&mut rand::rngs::OsRng);
    let apub = hex(sk.verifying_key().to_encoded_point(false).as_bytes());
    let ps = start_pairing(&good, &kp, "Y", &apub, &[]).unwrap();
    assert!(ps.finish(false).is_err());
    assert_eq!(b.devices().len(), 0);
}
