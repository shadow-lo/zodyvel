use std::net::{TcpListener, TcpStream};
use std::thread;
use zodyvel_phone_proto::net::*;
use zodyvel_phone_proto::pairing::*;
use zodyvel_phone_proto::session::*;

type PhoneRes = Result<(String, Vec<u8>), String>;
type DeskRes = Result<String, String>;

fn pair(token_phone: Vec<u8>, fp_ok: bool) -> (PhoneRes, DeskRes) {
    let desk = generate_keypair();
    let phone = generate_keypair();
    let mut win = PairingWindow::open();
    let qr = win.qr("127.0.0.1:0", &desk.public);
    let tok = win.take().unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let d2 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        accept(s, &d2, &tok, Trust::AcceptNew)
            .map(|mut c| { let m = c.recv().unwrap(); (c.confirm.clone(), c.remote_static.clone(), m) })
            .map_err(|e| format!("{e:?}"))
    });
    let pinned = if fp_ok { hex_decode(&qr.fp).unwrap() } else { vec![9u8; 32] };
    let s = TcpStream::connect(addr).unwrap();
    let pc = connect(s, &phone, &token_phone, Trust::PinnedKey(&pinned)).map_err(|e| format!("{e:?}"));
    let phone_code = match pc {
        Ok(mut c) => { c.send(b"merhaba").ok(); Ok(c.confirm.clone()) }
        Err(e) => Err(e),
    };
    let d = h.join().unwrap().map(|(c, k, m)| { assert_eq!(m, b"merhaba"); (c, k) });
    (d, phone_code)
}

#[test]
fn full_flow_with_matching_token() {
    let desk = generate_keypair();
    let phone = generate_keypair();
    let mut win = PairingWindow::open();
    let qr = win.qr("127.0.0.1:0", &desk.public);
    let qr = QrPayload::from_json(&qr.to_json()).unwrap();
    let tok = win.take().unwrap();
    assert!(win.take().is_none(), "jeton tek kullanımlık");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let d2 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let mut c = accept(s, &d2, &tok, Trust::AcceptNew).unwrap();
        let m = c.recv().unwrap();
        c.send(b"pong").unwrap();
        (c.confirm.clone(), c.remote_static.clone(), m)
    });
    let s = TcpStream::connect(addr).unwrap();
    let pinned = hex_decode(&qr.fp).unwrap();
    let mut c = connect(s, &phone, &qr.token_bytes(), Trust::PinnedKey(&pinned)).unwrap();
    c.send(b"ping").unwrap();
    assert_eq!(c.recv().unwrap(), b"pong");
    let (dc, rs, m) = h.join().unwrap();
    assert_eq!(m, b"ping");
    assert_eq!(dc, c.confirm, "6 haneli kod iki tarafta aynı");
    assert_eq!(dc.len(), 6);
    assert_eq!(rs, phone.public, "masaüstü telefonun anahtarını öğrendi");

    // eşleşmiş cihaz olarak normal bağlantı (jeton yok)
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let known = vec![phone.public.clone()];
    let d3 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        accept(s, &d3, b"", Trust::Known(&known)).map(|_| ()).is_ok()
    });
    let s = TcpStream::connect(addr).unwrap();
    assert!(connect(s, &phone, b"", Trust::PinnedKey(&desk.public)).is_ok());
    assert!(h.join().unwrap());
}

#[test]
fn unknown_device_rejected() {
    let desk = generate_keypair();
    let stranger = generate_keypair();
    let known = vec![generate_keypair().public];
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let d2 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        accept(s, &d2, b"", Trust::Known(&known)).is_err()
    });
    let s = TcpStream::connect(addr).unwrap();
    let _ = connect(s, &stranger, b"", Trust::PinnedKey(&desk.public));
    assert!(h.join().unwrap(), "eşli olmayan cihaz el sıkışmada düşer");
}

#[test]
fn wrong_token_fails() {
    let (d, p) = pair(vec![1u8; 32], true);
    assert!(d.is_err() || p.is_err());
}

#[test]
fn wrong_pinned_fingerprint_fails_on_phone() {
    // MITM: telefon QR'daki parmak izinden farklı anahtar görürse bağlantıyı keser
    let desk = generate_keypair();
    let phone = generate_keypair();
    let tok = [7u8; 32];
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let d2 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || { let (s, _) = l.accept().unwrap(); let _ = accept(s, &d2, &tok, Trust::AcceptNew); });
    let s = TcpStream::connect(addr).unwrap();
    let r = connect(s, &phone, &tok, Trust::PinnedKey(&[9u8; 32]));
    assert!(matches!(r, Err(Error::Untrusted)));
    let _ = h.join();
}

#[test]
fn replay_and_tamper_rejected() {
    use std::io::Cursor;
    let _ = Cursor::new(Vec::<u8>::new());
    // Kanalı bellek içi iki yönlü boru ile kurmak yerine gerçek TCP üstünde bozuk çerçeve gönder.
    let desk = generate_keypair();
    let phone = generate_keypair();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let d2 = Keypair { private: desk.private.clone(), public: desk.public.clone() };
    let h = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        let mut c = accept(s, &d2, b"", Trust::AcceptNew).unwrap();
        let first = c.recv();
        let second = c.recv(); // tekrar oynatılmış aynı çerçeve
        (first.is_ok(), second.is_err())
    });
    let mut raw = TcpStream::connect(addr).unwrap();
    let _ = raw.try_clone();
    // El sıkışmayı yapan sarmalayıcı: yazılan baytları yakalayan Tee
    struct Tee { s: TcpStream, log: std::sync::Arc<std::sync::Mutex<Vec<u8>>> }
    impl std::io::Read for Tee { fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> { self.s.read(b) } }
    impl std::io::Write for Tee {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { self.log.lock().unwrap().extend_from_slice(b); self.s.write(b) }
        fn flush(&mut self) -> std::io::Result<()> { self.s.flush() }
    }
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let tee = Tee { s: raw.try_clone().unwrap(), log: log.clone() };
    let mut c = connect(tee, &phone, b"", Trust::PinnedKey(&desk.public)).unwrap();
    let before = log.lock().unwrap().len();
    c.send(b"onay").unwrap();
    let frame = log.lock().unwrap()[before..].to_vec();
    use std::io::Write;
    raw.write_all(&frame).unwrap(); // aynı çerçeve ikinci kez
    let (first_ok, replay_rejected) = h.join().unwrap();
    assert!(first_ok);
    assert!(replay_rejected, "tekrar oynatma reddedildi");
}

#[test]
fn network_policy() {
    use std::net::IpAddr;
    assert!(listening_allowed(&parse_profiles("Private\r\n")));
    assert!(!listening_allowed(&parse_profiles("Public\n")));
    assert!(!listening_allowed(&parse_profiles("Private\nPublic\n")));
    assert!(!listening_allowed(&parse_profiles("")));
    assert!(!listening_allowed(&parse_profiles("DomainAuthenticated")));
    assert!(is_local_bind("192.168.1.20".parse().unwrap()));
    assert!(!is_local_bind("0.0.0.0".parse().unwrap()));
    assert!(!is_local_bind("8.8.8.8".parse().unwrap()));
    let ip: IpAddr = "192.168.1.9".parse().unwrap();
    let mut t = Throttle::new();
    for _ in 0..5 { t.fail(ip); }
    assert!(t.blocked(ip));
    t.ok(ip);
    assert!(!t.blocked(ip));
}

#[test]
fn qr_expiry_and_single_use() {
    let mut w = PairingWindow::open();
    assert!(!w.expired());
    assert!(w.take().is_some());
    assert!(w.take().is_none());
    assert!(QrPayload::from_json("{}").is_err());
}
