//! Çekirdek sınamaları: gerçek köprüye karşı eşleştirme, sarılı anahtar, kilit penceresi, iptal.
use p256::ecdsa::{signature::Signer as _, Signature, SigningKey};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zodyvel_phone_core::*;
use zodyvel_phone_proto::devices::Vault;
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
struct H { code: Mutex<Option<String>>, acts: Mutex<Vec<Action>>, agenda: Mutex<Vec<AgendaItem>> }
impl Host for H {
    fn state(&self) -> StateView {
        StateView { sessions: vec![SessionView { id: "c:1".into(), tool: "claude".into(), name: "x".into(), status: "running".into(), step: "".into(), reply: None, folder: "Zodyvel".into(), started: Some(1_794_000_000), steps: vec![StepView::new("read", "Okudu", "a.ts")] }], approvals: vec![ApprovalView { id: "ap1".into(), session: "c:1".into(), kind: "permission".into(), action: "a".into(), questions: vec![] }] }
    }
    fn act(&self, _: &str, a: Action) -> Result<(), String> {
        if matches!(&a, Action::Note { text, .. } if text == "reddet") { return Err("Bu oturum telefona açık değil.".into()); }
        self.acts.lock().unwrap().push(a);
        Ok(())
    }
    fn folders(&self) -> Vec<FolderItem> { vec![FolderItem { id: folder_id(1, "C:\\p"), name: "Proje".into() }] }
    fn agenda(&self) -> AgendaView { AgendaView::new("2026-11-14".into(), self.agenda.lock().unwrap().clone()) }
    fn audit(&self, _: &str, _: &str, _: &str) {}
    fn event(&self, e: Event) { if let Event::PairCode { code, .. } = e { *self.code.lock().unwrap() = Some(code); } }
    fn network_ok(&self) -> bool { true }
}

#[derive(Clone)]
struct Fake { secure: bool, key: Arc<Mutex<Option<SigningKey>>>, auth_ok: Arc<Mutex<bool>>, deleted: Arc<Mutex<u32>>, notices: Arc<Mutex<Vec<Notice>>>, svc: Arc<Mutex<Vec<bool>>>, challenge: Arc<Mutex<Vec<u8>>>, prompts: Arc<Mutex<Vec<String>>>, rems: Arc<Mutex<Vec<Vec<Reminder>>>>, found: Arc<Mutex<Option<Vec<String>>>>, searched: Arc<Mutex<u32>>, rem_delay: Arc<Mutex<u64>>, changes: Arc<Mutex<u32>> }
impl Fake {
    fn new(secure: bool) -> Self { Fake { secure, key: Default::default(), auth_ok: Arc::new(Mutex::new(true)), deleted: Default::default(), notices: Default::default(), svc: Default::default(), challenge: Default::default(), prompts: Default::default(), rems: Default::default(), found: Default::default(), searched: Default::default(), rem_delay: Default::default(), changes: Default::default() } }
}
impl Platform for Fake {
    fn device_secure(&self) -> bool { self.secure }
    fn device_name(&self) -> String { "Deneme Telefonu".into() }
    fn create_signing_key(&self, c: &[u8]) -> Result<KeyInfo, String> {
        *self.challenge.lock().unwrap() = c.to_vec();
        let sk = SigningKey::random(&mut rand::rngs::OsRng);
        let apub = zodyvel_phone_proto::pairing::hex(sk.verifying_key().to_encoded_point(false).as_bytes());
        *self.key.lock().unwrap() = Some(sk);
        Ok(KeyInfo { apub, attestation: vec![] })
    }
    fn sign(&self, m: &[u8], p: &Prompt) -> Result<Vec<u8>, String> {
        self.prompts.lock().unwrap().push(p.title.clone());
        if !*self.auth_ok.lock().unwrap() { return Err("İptal edildi.".into()); }
        let s: Signature = self.key.lock().unwrap().as_ref().ok_or("anahtar yok")?.sign(m);
        Ok(s.to_der().as_bytes().to_vec())
    }
    fn authenticate(&self, _p: &Prompt) -> Result<(), String> { if *self.auth_ok.lock().unwrap() { Ok(()) } else { Err("İptal edildi.".into()) } }
    fn wrap(&self, d: &[u8]) -> Result<Vec<u8>, String> { Ok(d.iter().map(|b| b ^ 0x5a).collect()) }
    fn unwrap(&self, d: &[u8]) -> Result<Vec<u8>, String> { Ok(d.iter().map(|b| b ^ 0x5a).collect()) }
    fn service(&self, on: bool) { self.svc.lock().unwrap().push(on); }
    fn notice(&self, n: &Notice) { self.notices.lock().unwrap().push(n.clone()); }
    fn delete_keys(&self) -> Result<(), String> { *self.deleted.lock().unwrap() += 1; *self.key.lock().unwrap() = None; Ok(()) }
    fn reminders(&self, l: &[Reminder]) { let d = *self.rem_delay.lock().unwrap(); if d > 0 && !l.is_empty() { std::thread::sleep(Duration::from_millis(d)); } self.rems.lock().unwrap().push(l.to_vec()); }
    fn changed(&self) { *self.changes.lock().unwrap() += 1; }
    fn discover(&self) -> Option<Vec<String>> { *self.searched.lock().unwrap() += 1; Some(self.found.lock().unwrap().clone().unwrap_or_default()) }
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("zv-core-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn bridge() -> (Bridge, Arc<H>) {
    let h = Arc::new(H::default());
    (Bridge::start(h.clone(), Arc::new(MemVault::default()), IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap(), h)
}
fn qr_for(b: &Bridge) -> String {
    let mut q = b.open_pairing().unwrap();
    q.addr = b.addr().to_string();
    q.to_json()
}

#[test]
fn full_flow_pair_probe_unlock_unpair() {
    let (b, h) = bridge();
    let dir = tmpdir("flow");
    let fake = Fake::new(true);
    let core = Core::new(Box::new(fake.clone()), dir.clone());
    assert_eq!(core.state().phase, "none");

    let begin = core.pair_begin(&qr_for(&b)).unwrap();
    assert_eq!(begin.code.len(), 6);
    assert_eq!(begin.fingerprint.matches(" · ").count(), 2);
    let t0 = Instant::now();
    while h.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    assert_eq!(h.code.lock().unwrap().clone().unwrap(), begin.code);
    b.confirm_pairing_code(&begin.code).unwrap();
    core.pair_finish(true).unwrap();
    assert_eq!(core.state().phase, "paired");

    // Diskte düz Noise özel anahtarı yok.
    let raw = std::fs::read_to_string(dir.join("pair.json")).unwrap();
    assert!(raw.contains("noise_priv_wrapped") && !raw.contains("\"private\""));

    assert_eq!(core.probe().unwrap(), Probe { sessions: 1, approvals: 1 });

    // İmza bağlantıda geçerli (Link.act çekirdeğin signer'ıyla).
    assert!(!core.is_unlocked());
    core.unlock().unwrap();
    assert!(core.is_unlocked());
    core.lock();
    assert!(!core.is_unlocked());
    *fake.auth_ok.lock().unwrap() = false;
    assert!(core.unlock().is_err() && !core.is_unlocked(), "doğrulama iptalinde pencere açılmaz");

    core.unpair().unwrap();
    assert_eq!(core.state().phase, "none");
    assert!(core.probe().is_err());
    assert!(*fake.deleted.lock().unwrap() >= 1);
}

#[test]
fn refuses_without_screen_lock_and_cleans_up_on_cancel() {
    let (b, _h) = bridge();
    let core = Core::new(Box::new(Fake::new(false)), tmpdir("nolock"));
    let e = core.pair_begin(&qr_for(&b)).unwrap_err();
    assert!(e.contains("ekran kilidi"));
    assert_eq!(b.devices().len(), 0);

    b.cancel_pairing();
    let fake = Fake::new(true);
    let core = Core::new(Box::new(fake.clone()), tmpdir("cancel"));
    core.pair_begin(&qr_for(&b)).unwrap();
    assert!(core.pair_finish(false).is_err());
    assert_eq!(core.state().phase, "none");
    assert!(*fake.deleted.lock().unwrap() >= 1, "yarım kalan eşleşmede Keystore anahtarı silinir");
}

#[test]
fn signer_goes_through_platform() {
    let fake = Fake::new(true);
    let core = Core::new(Box::new(fake.clone()), tmpdir("signer"));
    fake.create_signing_key(&[1]).unwrap();
    let s = core.signer(Prompt { title: "t".into(), subtitle: "s".into() });
    use zodyvel_phone_proto::client::Signer;
    assert!(s.sign(b"x").is_ok());
    *fake.auth_ok.lock().unwrap() = false;
    assert!(s.sign(b"x").is_err(), "kimlik doğrulama yoksa imza yok");
}

fn pair(core: &Core, b: &Bridge, h: &H) {
    let begin = core.pair_begin(&qr_for(b)).unwrap();
    let t0 = Instant::now();
    while h.code.lock().unwrap().is_none() { assert!(t0.elapsed() < Duration::from_secs(5)); std::thread::sleep(Duration::from_millis(20)); }
    b.confirm_pairing_code(&begin.code).unwrap();
    core.pair_finish(true).unwrap();
}
fn wait_for(what: &str, f: impl Fn() -> bool) {
    let t0 = Instant::now();
    while !f() { assert!(t0.elapsed() < Duration::from_secs(8), "zaman aşımı: {what}"); std::thread::sleep(Duration::from_millis(30)); }
}

#[test]
fn live_link_notifies_and_signs_decisions() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("live")));
    assert!(core.live_start().is_err(), "eşleşmeden başlamaz");
    pair(&core, &b, &h);

    core.live_start().unwrap();
    core.live_start().unwrap(); // ikinci çağrı ikinci iş parçacığı açmaz
    // Hizmet canlı iş parçacığının içinden açılır (eşzamansız).
    wait_for("ön plan hizmeti", || !fake.svc.lock().unwrap().is_empty());
    assert_eq!(*fake.svc.lock().unwrap(), vec![true]);
    wait_for("onay bildirimi", || fake.notices.lock().unwrap().iter().any(|n| matches!(n, Notice::Approval { id, kind, .. } if id == "ap1" && kind == "permission")));
    let st = core.live_state();
    assert!(st.running && st.link == "connected" && st.approvals.len() == 1);
    assert!(st.approvals[0].action.is_none(), "ayrıntı penceresi kapalıyken eylem metni görünmez");
    // Bildirim içeriği: eylem metni ("a") hiçbir bildirimde yok.
    assert!(!format!("{:?}", fake.notices.lock().unwrap()).contains("\"a\""));

    // Doğrulama iptal edilince hiçbir şey gitmez.
    *fake.auth_ok.lock().unwrap() = false;
    core.decide("ap1", true).unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    assert!(h.acts.lock().unwrap().is_empty(), "iptalde onay gitmez");

    *fake.auth_ok.lock().unwrap() = true;
    core.decide("ap1", true).unwrap();
    wait_for("onay eylemi", || !h.acts.lock().unwrap().is_empty());
    assert_eq!(h.acts.lock().unwrap()[0], Action::Decide { id: "ap1".into(), d: "allow".into() });
    wait_for("ack ile bildirim silinmesi", || fake.notices.lock().unwrap().iter().any(|n| matches!(n, Notice::Gone { id } if id == "ap1")));

    // Bilinmeyen onay: gönderilmez.
    let before = h.acts.lock().unwrap().len();
    core.decide("yok", true).unwrap();
    std::thread::sleep(Duration::from_millis(2200));
    assert_eq!(h.acts.lock().unwrap().len(), before);

    core.live_stop();
    assert_eq!(*fake.svc.lock().unwrap(), vec![true, false]);
    assert_eq!(core.live_state().link, "off");
    assert!(core.decide("ap1", true).is_err());
}

#[test]
fn live_can_be_turned_off_and_stays_off() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("liveoff")));
    pair(&core, &b, &h);
    core.live_set_enabled(false).unwrap();
    core.live_start().unwrap();
    assert!(!core.live_state().running && fake.svc.lock().unwrap().iter().all(|x| !x), "kapalıyken hizmet açılmaz");
    core.live_set_enabled(true).unwrap();
    wait_for("bağlantı", || core.live_state().link == "connected");
    core.unpair().unwrap();
    assert!(!core.live_state().running, "eşleşme silinince canlı bağlantı da durur");
}

fn ans(pairs: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned()))).collect()
}

#[test]
fn t4c_sessions_actions_folders_agenda() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("t4c")));
    pair(&core, &b, &h);
    assert_eq!(fake.challenge.lock().unwrap().len(), 32, "O9: anahtar sınaması el sıkışma özeti");
    let id = b.devices()[0].id.clone();
    b.mark_protected_for_tests(&id);
    core.live_start().unwrap();
    wait_for("bağlantı", || core.live_state().link == "connected" && core.live_state().hw == Some(true));

    // Kilitliyken: ad/klasör/durum/süre var, adım/soru yok.
    let st = core.live_state();
    let s0 = &st.sessions[0];
    assert_eq!((s0.id.as_str(), s0.folder.as_str(), s0.started, s0.status.as_str()), ("c:1", "Zodyvel", Some(1_794_000_000), "running"));
    assert!(s0.step.is_none() && s0.steps.is_none() && s0.reply.is_none());
    assert_eq!(st.approvals[0].session_id, "c:1");
    assert!(st.approvals[0].questions.is_none() && st.unlocked_until.is_none() && st.last_connected.is_some());
    core.unlock().unwrap();
    let st = core.live_state();
    assert_eq!(st.sessions[0].steps.as_ref().unwrap()[0].label, "Okudu");
    assert!(st.unlocked_until.is_some() && st.approvals[0].questions.is_some());

    // İmzalı eylemler ack'la döner.
    core.note("c:1", "merhaba", false).unwrap();
    core.task("c:1", "testleri yaz").unwrap();
    core.stop_session("c:1").unwrap();
    core.decide_wait("ap1", false).unwrap();
    let folders = core.folders().unwrap();
    let fid = folders[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(folders[0]["name"], "Proje");
    core.new_task(&fid, "yeni iş").unwrap();
    core.answer("ap1", ans(&[("Hangisi?", "A")])).unwrap();
    {
        let acts = h.acts.lock().unwrap();
        assert!(acts.contains(&Action::Note { session: "c:1".into(), text: "merhaba".into(), now: false }));
        assert!(acts.contains(&Action::Task { session: "c:1".into(), text: "testleri yaz".into() }));
        assert!(acts.contains(&Action::Stop { session: "c:1".into() }));
        assert!(acts.contains(&Action::NewTask { folder: fid.clone(), prompt: "yeni iş".into() }));
        assert_eq!(acts.len(), 6);
    }
    assert!(fake.prompts.lock().unwrap().iter().any(|p| p == "Zodyvel · Yeni görev"), "her eylem parmak izi ister");

    // Masaüstünün reddi metniyle döner; geçersiz eylem hiç imzalanmaz.
    assert_eq!(core.note("c:1", "reddet", true).unwrap_err(), "Bu oturum telefona açık değil.");
    let n = fake.prompts.lock().unwrap().len();
    assert_eq!(core.answer("ap1", ans(&[])).unwrap_err(), "Geçersiz eylem.");
    assert_eq!(core.note("c:1", "   ", true).unwrap_err(), "Geçersiz eylem.");
    assert_eq!(fake.prompts.lock().unwrap().len(), n, "geçersiz eylemde parmak izi sorulmaz");

    // Kullanıcı vazgeçer: hiçbir şey gitmez.
    *fake.auth_ok.lock().unwrap() = false;
    let before = h.acts.lock().unwrap().len();
    assert_eq!(core.stop_session("c:1").unwrap_err(), ERR_CANCELED);
    assert_eq!(h.acts.lock().unwrap().len(), before);
    *fake.auth_ok.lock().unwrap() = true;

    // Ajanda: bağlıyken istenir, kopuşta son kopya.
    let ag = core.agenda().unwrap();
    assert_eq!((ag["from"].as_str(), ag["days"].as_i64(), ag.get("t")), (Some("2026-11-14"), Some(14), None));
    core.live_stop();
    assert_eq!(core.agenda().unwrap()["from"], "2026-11-14");
    assert_eq!(core.note("c:1", "x", true).unwrap_err(), ERR_OFFLINE);
    assert_eq!(core.folders().unwrap_err(), ERR_OFFLINE);
}

#[test]
fn t4c_unprotected_phone_is_restricted() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("t4c-unprot")));
    pair(&core, &b, &h); // sahte telefonun kanıt zinciri yok → korumasız
    core.live_start().unwrap();
    wait_for("bağlantı", || core.live_state().link == "connected" && core.live_state().hw == Some(false));
    if !RESTRICT_UNPROTECTED {
        return;
    }
    let n = fake.prompts.lock().unwrap().len();
    for e in [core.note("c:1", "x", false), core.task("c:1", "x"), core.new_task("abc", "x")] {
        assert_eq!(e.unwrap_err(), zodyvel_phone_proto::devices::UNPROTECTED_DENIED);
    }
    assert_eq!(fake.prompts.lock().unwrap().len(), n, "kapalı eylemde parmak izi bile sorulmaz");
    assert_eq!(core.folders().unwrap().as_array().map(Vec::len), Some(0), "korumasıza klasör listesi gitmez");
    core.stop_session("c:1").unwrap();
    core.decide_wait("ap1", true).unwrap();
    assert_eq!(h.acts.lock().unwrap().len(), 2);
    core.live_stop();
}

fn item(id: u64, date: &str, time: Option<&str>, title: &str, lead: u32, notify: bool) -> AgendaItem {
    AgendaItem { id, date: date.into(), time: time.map(Into::into), title: title.into(), minutes: 30, lead, notify }
}

#[test]
fn g3_reminders_follow_agenda_and_switch() {
    let (b, h) = bridge();
    *h.agenda.lock().unwrap() = vec![item(1, "2026-11-14", Some("14:30"), "Proje toplantısı", 15, true), item(2, "2026-11-15", None, "Doğum günü", 0, true), item(3, "2026-11-16", Some("16:00"), "Rapor", 0, false)];
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("remind")));
    pair(&core, &b, &h);
    let st = core.live_state();
    assert!(st.reminders && !st.autostart && st.reminder_count.is_none(), "varsayılan: hatırlatıcı açık, açılışta bağlanma kapalı");

    // Bağlanınca ajanda kendiliğinden istenir ve alarm listesi kurulur (yalnız saatli + notify).
    core.live_start().unwrap();
    wait_for("ilk hatırlatma listesi", || !fake.rems.lock().unwrap().is_empty());
    let first = fake.rems.lock().unwrap().last().unwrap().clone();
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].title.as_str(), first[0].time.as_str(), first[0].lead), ("Proje toplantısı", "14:30", 15));
    assert_eq!(core.live_state().reminder_count, Some(1));

    // Plan değişince (masaüstü iter) liste baştan kurulur; silinen plan gider.
    *h.agenda.lock().unwrap() = vec![item(4, "2026-11-17", Some("09:00"), "Okul", 10, true), item(5, "2026-11-18", Some("10:00"), "Diş", 30, true)];
    b.push_agenda();
    wait_for("güncel liste", || fake.rems.lock().unwrap().last().is_some_and(|l| l.len() == 2));
    let second = fake.rems.lock().unwrap().last().unwrap().clone();
    assert!(second.iter().all(|r| r.title != "Proje toplantısı"));
    let calls = fake.rems.lock().unwrap().len();
    b.push_agenda(); // aynı plan: masaüstü de telefon da tekrar göndermez
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(fake.rems.lock().unwrap().len(), calls);

    // Kapatınca hepsi silinir; açınca son ajanda hemen yeniden kurulur.
    core.reminders_set_enabled(false).unwrap();
    assert!(fake.rems.lock().unwrap().last().unwrap().is_empty());
    let st = core.live_state();
    assert!(!st.reminders && st.reminder_count.is_none());
    // T-O5: kapalıyken ajanda tutulmaz; açınca bilgisayardan taze istenir ve gelince kurulur.
    assert_eq!(core.agenda().unwrap()["items"].as_array().map(Vec::len), Some(2), "bağlıyken ajanda yine okunur");
    core.reminders_set_enabled(true).unwrap();
    wait_for("yeniden kurulan liste", || fake.rems.lock().unwrap().last().is_some_and(|l| l.len() == 2));

    // Açılışta bağlanma ayarı; eşleşme silinince ayarlar ve alarmlar gider.
    core.autostart_set(true).unwrap();
    assert!(core.live_state().autostart);
    core.unpair().unwrap();
    assert!(fake.rems.lock().unwrap().last().unwrap().is_empty());
    let st = core.live_state();
    assert!(!st.autostart && st.reminders && st.reminder_count.is_none());
    assert!(core.autostart_set(true).is_err(), "eşleşmeden açılış ayarı açılmaz");
}

#[test]
fn g3_finds_desktop_after_ip_change() {
    let (b, h) = bridge();
    let (other, _) = bridge(); // başka bir bilgisayar (farklı anahtar): sahte duyuru gibi
    let fake = Fake::new(true);
    let dir = tmpdir("mdns");
    let core = Arc::new(Core::new(Box::new(fake.clone()), dir.clone()));
    pair(&core, &b, &h);
    let real = b.addr().to_string();
    assert_eq!(core.saved_addr().as_deref(), Some(real.as_str()));

    // IP değişti: kayıtlı adreste kimse yok. Arama iki aday bulur; yalnız anahtarı tutan kabul edilir.
    let raw = std::fs::read_to_string(dir.join("pair.json")).unwrap().replace(&real, "127.0.0.1:1");
    std::fs::write(dir.join("pair.json"), raw).unwrap();
    *fake.found.lock().unwrap() = Some(vec![other.addr().to_string(), real.clone()]);
    core.live_start().unwrap();
    let t0 = Instant::now();
    while core.live_state().link != "connected" { assert!(t0.elapsed() < Duration::from_secs(20), "arama ile bağlanamadı"); std::thread::sleep(Duration::from_millis(50)); }
    assert_eq!(core.saved_addr().as_deref(), Some(real.as_str()), "doğrulanan adres kaydedilir, sahte aday değil");
    assert_eq!(*fake.searched.lock().unwrap(), 1, "tek geçici hatada değil, art arda hatada bir kez aranır");
    core.live_stop();
}

#[test]
fn g4_revoked_phone_drops_reminders_and_autostart() {
    let (b, h) = bridge();
    *h.agenda.lock().unwrap() = vec![item(1, "2026-11-20", Some("14:30"), "Gizli toplantı", 15, true)];
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("g4revoke")));
    pair(&core, &b, &h);
    core.autostart_set(true).unwrap();
    core.live_start().unwrap();
    wait_for("hatırlatma kuruldu", || fake.rems.lock().unwrap().last().is_some_and(|l| l.len() == 1));
    let id = b.devices()[0].id.clone();
    assert_eq!(b.revoke(&id), Ok(true));
    wait_for("kaldırıldı", || core.live_state().link == "revoked");
    wait_for("alarmlar silindi", || fake.rems.lock().unwrap().last().is_some_and(|l| l.is_empty()));
    let st = core.live_state();
    assert!(!st.autostart && st.reminder_count.is_none(), "açılışta bağlanma kapandı");
    assert!(st.reminders, "kullanıcı ayarı değişmez");
    assert!(core.agenda().is_err(), "bellekteki ajanda bırakıldı");
}

#[test]
fn ty1_permanent_end_stops_foreground_service() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("ty1")));
    pair(&core, &b, &h);
    core.live_start().unwrap();
    wait_for("bağlantı", || core.live_state().link == "connected");
    let id = b.devices()[0].id.clone();
    assert_eq!(b.revoke(&id), Ok(true));
    wait_for("hizmet kapandı", || !core.live_state().running);
    assert_eq!(*fake.svc.lock().unwrap(), vec![true, false], "kalıcı bitişte ön plan hizmeti durur");
}

#[test]
fn ty2_phone_removed_while_offline_learns_it() {
    let (b, h) = bridge();
    *h.agenda.lock().unwrap() = vec![item(1, "2026-11-20", Some("14:30"), "Gizli toplantı", 15, true)];
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("ty2")));
    pair(&core, &b, &h);
    core.autostart_set(true).unwrap();
    core.live_start().unwrap();
    wait_for("hatırlatma kuruldu", || fake.rems.lock().unwrap().last().is_some_and(|l| l.len() == 1));
    core.live_stop();
    // Telefon bağlı değilken bilgisayardan kaldırılır: `bye revoked` ona hiç gitmez.
    let id = b.devices()[0].id.clone();
    assert_eq!(b.revoke(&id), Ok(true));
    core.live_start().unwrap();
    let t0 = Instant::now();
    while core.live_state().link != "revoked" { assert!(t0.elapsed() < Duration::from_secs(30), "kaldırıldığını öğrenmedi: {}", core.live_state().link); std::thread::sleep(Duration::from_millis(50)); }
    wait_for("iş parçacığı bitti", || !core.live_state().running);
    wait_for("alarmlar silindi", || fake.rems.lock().unwrap().last().is_some_and(|l| l.is_empty()));
    assert!(!core.live_state().autostart, "açılışta bağlanma kapandı");
    assert_eq!(fake.svc.lock().unwrap().last(), Some(&false), "hizmet kapalı");
    assert!(b.devices().is_empty(), "bilinmeyen telefon kayda eklenmez");
}

#[test]
fn to4_turning_reminders_off_wins_over_concurrent_sync() {
    let (b, h) = bridge();
    *h.agenda.lock().unwrap() = vec![item(1, "2026-11-20", Some("14:30"), "A", 15, true)];
    let fake = Fake::new(true);
    let core = Arc::new(Core::new(Box::new(fake.clone()), tmpdir("to4")));
    pair(&core, &b, &h);
    core.live_start().unwrap();
    wait_for("ilk liste", || fake.rems.lock().unwrap().last().is_some_and(|l| l.len() == 1));
    assert!(*fake.changes.lock().unwrap() > 0, "A-O1: durum değişikliği arayüze bildirilir");
    // Alarm kurulumu yavaş: kullanıcı tam o sırada kapatır.
    *fake.rem_delay.lock().unwrap() = 300;
    for i in 0..3u64 {
        *h.agenda.lock().unwrap() = vec![item(10 + i, "2026-11-21", Some("09:00"), "B", 10, true)];
        b.push_agenda();
        std::thread::sleep(Duration::from_millis(100));
        core.reminders_set_enabled(false).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(fake.rems.lock().unwrap().last().unwrap().is_empty(), "kapattıktan sonra alarm kurulmaz ({i})");
        assert!(core.live_state().reminder_count.is_none());
        core.reminders_set_enabled(true).unwrap();
        wait_for("yeniden açıldı", || fake.rems.lock().unwrap().last().is_some_and(|l| !l.is_empty()));
    }
    // T-O5: kapalıyken ajanda bellekte tutulmaz (bağlantı yokken son kopya yok).
    core.reminders_set_enabled(false).unwrap();
    core.live_stop();
    assert!(core.agenda().is_err());
}

#[test]
fn to6_stop_is_seen_between_search_candidates() {
    let (b, h) = bridge();
    let fake = Fake::new(true);
    let dir = tmpdir("to6");
    let core = Arc::new(Core::new(Box::new(fake.clone()), dir.clone()));
    pair(&core, &b, &h);
    let real = b.addr().to_string();
    let raw = std::fs::read_to_string(dir.join("pair.json")).unwrap().replace(&real, "127.0.0.1:1");
    std::fs::write(dir.join("pair.json"), raw).unwrap();
    // Bağlantıyı kabul edip hiç cevap vermeyen adaylar: her biri el sıkışma süresi (5 sn) kadar oyalar.
    let silent: Vec<std::net::TcpListener> = (0..3).map(|_| std::net::TcpListener::bind("127.0.0.1:0").unwrap()).collect();
    *fake.found.lock().unwrap() = Some(silent.iter().map(|l| l.local_addr().unwrap().to_string()).collect());
    core.live_start().unwrap();
    let t0 = Instant::now();
    while core.live_state().link != "searching" { assert!(t0.elapsed() < Duration::from_secs(25), "arama başlamadı"); std::thread::sleep(Duration::from_millis(50)); }
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    core.live_stop();
    assert!(t.elapsed() < Duration::from_secs(8), "durdurma üç adayın bitmesini bekledi: {:?}", t.elapsed());
    assert!(!core.live_state().running);
}
