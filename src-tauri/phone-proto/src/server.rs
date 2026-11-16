//! Köprü sunucusu (T2): eşleştirme, normal bağlantı, durum itme, imzalı eylem alma.
//! Masaüstü/Windows'tan bağımsızdır: dışarıyla `Host` (ajan durumu, ağ kuralı, denetim) ve
//! `Vault` (gizli depo) arayüzleriyle konuşur; böylece her yerde sınanır.
use crate::devices::*;
use crate::net::Throttle;
use crate::session::{accept, generate_keypair, Channel, Keypair, RecvHalf, SendHalf, Trust};
use crate::pairing::{hex, hex_decode, PairingWindow, QrPayload};
use crate::proto::*;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use crate::{MODE_NORMAL, MODE_PAIR};
/// Kimliği doğrulanmış oturum sınırı (doğrulanmamış bağlantılardan ayrı sayılır).
const MAX_CONNS: usize = 6;
/// G4: bağlı oturumda `last_seen` en geç bu aralıkla güncellenir (30 gün kuralı kesintisiz bağlantıda da doğru say).
const TOUCH_EVERY: Duration = Duration::from_secs(3600);
/// G4: doğrulanmış oturumda saniyede en çok ileti (ping/folders/agenda dahil); aşan bağlantı kapanır.
const MAX_MSGS_PER_SEC: usize = 20;
/// Henüz el sıkışmamış bağlantılar: toplam ve IP başına sınır, el sıkışma için TOPLAM süre.
const MAX_PENDING: usize = 3;
/// M-D2: son bağlandığı IP'den gelen kayıtlı cihaz için ek yuva (yabancı IP'ler bu yuvayı dolduramaz).
const RESERVED_PENDING: usize = 1;
/// Akılda tutulan en çok "bilinen cihaz IP'si" (yalnız bellekte).
const MAX_KNOWN_IPS: usize = 8;
const HANDSHAKE_TOTAL: Duration = Duration::from_secs(4);
/// İş parçacığı sınırı: oturum başına iki (okuyan + yazan), el sıkışma başına bir.
const MAX_THREADS: usize = 2 * MAX_CONNS + MAX_PENDING + RESERVED_PENDING;
/// M-D1: kabul döngüsü bloklamaz; bu aralıkla durdurma bayrağına ve süresi dolan el sıkışmalara bakar.
const ACCEPT_POLL: Duration = Duration::from_millis(500);
/// M-D4: bağlı oturumda okuma beklemesi. Giden iletiler ayrı yazan iş parçacığından gider; bu aralık yalnız
/// ağ / kilit / durdurma / son görülme denetimi için (saniyede 10 uyanma yerine 2 sn'de bir). Windows'ta
/// `shutdown` bekleyen okumayı uyandırmadığından durdurma en geç bu kadar sürer.
const SESSION_CHECK: Duration = Duration::from_secs(2);
/// El sıkışmadaki tek okumanın en uzun süresi (M-D2).
const HANDSHAKE_READ: Duration = Duration::from_secs(2);
/// Gelmeye başlamış bir çerçevenin okunması ve her yazma için süre.
const IO_TIMEOUT: Duration = Duration::from_secs(5);
/// M-O1: eşleştirmedeki uzun beklemelerde durdurma bayrağına bakma aralığı.
const STOP_POLL: Duration = Duration::from_secs(1);
const PAIR_CONFIRM_WAIT: Duration = Duration::from_secs(60);
/// El sıkışmadan sonra `hello` beklemesi: telefon anahtarını (StrongBox'ta yavaş olabilir) şimdi üretir,
/// çünkü kanıt sınaması bu oturumun el sıkışma özetidir.
const HELLO_WAIT: Duration = Duration::from_secs(20);
/// `hello` (kanıt zinciri dahil) için en büyük ileti; diğer iletiler `MAX_INCOMING`.
const MAX_HELLO: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Cihaz listesi / bağlantı / eşleştirme durumu değişti (arayüz durumu yeniden okur).
    Changed,
    /// Eşleştirme: iki ekranda aynı çıkması gereken kod.
    PairCode { code: String, name: String },
}

/// Köprünün dışarıdan ihtiyaç duyduğu her şey.
pub trait Host: Send + Sync + 'static {
    fn state(&self) -> StateView;
    /// Doğrulanmış eylemi uygular; hata metni içerik taşımaz.
    fn act(&self, device: &str, a: Action) -> Result<(), String>;
    /// Yeni görev için masaüstünün izinli klasörleri: opak kimlik + ad (yol verilmez).
    fn folders(&self) -> Vec<FolderItem>;
    /// Önümüzdeki 14 günün ajanda olayları (T2b). Varsayılan: yok.
    fn agenda(&self) -> AgendaView {
        AgendaView::default()
    }
    /// "telefondan" denetim kaydı; içerik yok. `device`: "ad · kimlik" (ad benzersiz değil), `result`: "ok" ya da
    /// "hata: …" (içeriksiz hata metni). Başarılı ve başarısız her doğrulanmış eylem yazılır.
    fn audit(&self, device: &str, what: &str, result: &str);
    fn event(&self, e: Event);
    /// Windows ağ profili Özel mi (önbellekli olabilir).
    fn network_ok(&self) -> bool;
    /// G4: Google Key Attestation iptal listesi (masaüstünün indirdiği son kopya). `None`: hiç alınamadı.
    fn revocations(&self) -> Option<Arc<crate::attest::Revocations>> {
        None
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub paired_at: u64,
    pub last_seen: u64,
    pub connected: bool,
    /// O9: donanım kanıtı doğrulandı mı; değilse cihaz "korumasız" ve kısıtlı.
    pub hw: bool,
    pub hw_level: String,
    pub hw_why: String,
    /// G4: 30 günden uzun süredir bağlanmadı; bağlanınca yeniden eşleştirme istenecek.
    pub expired: bool,
}

enum Out {
    State(String),
    Agenda(String),
    /// G4: koruma durumu değişti (iptal listesi): telefona yeni `device` iletisi.
    Device(bool, String),
    /// Telefon ajandayı istedi: hemen gönder, sonra değişiklikleri it.
    AgendaNow,
    /// Okuyan yarının cevabı (pong, ack, klasörler).
    Reply(String),
    Bye(&'static str),
}
struct Conn {
    id: u64,
    device: String,
    tx: Sender<Out>,
}
#[derive(Default)]
struct PairState {
    code: Option<(String, String)>,
    decision: Option<bool>,
}

struct Inner {
    host: Arc<dyn Host>,
    vault: Arc<dyn Vault>,
    data: Mutex<VaultData>,
    me: Keypair,
    addr: SocketAddr,
    stop: AtomicBool,
    window: Mutex<Option<PairingWindow>>,
    pair: Mutex<PairState>,
    pair_cv: Condvar,
    conns: Mutex<Vec<Conn>>,
    throttle: Mutex<Throttle>,
    threads: AtomicUsize,
    next_conn: AtomicU64,
    /// El sıkışması bitmemiş bağlantılar (yuva doldurmayı sınırlar, süresi dolanı keser).
    pending: Mutex<Vec<Pend>>,
    next_pend: AtomicU64,
    /// Kayıt silinemedi: güvenli taraf, yeni bağlantı kabul edilmez, açık oturumlar kapanır.
    lockdown: AtomicBool,
    /// M-O1: açık bütün bağlantı soketleri (bekleyen, eşleşen, bağlı); kapanışta hepsi kesilir.
    live: Mutex<Vec<(u64, TcpStream)>>,
    /// M-D2: kayıtlı cihazların son bağlandığı IP'ler (yalnız bellekte, ayrılmış el sıkışma yuvası için).
    known_ips: Mutex<VecDeque<IpAddr>>,
}

struct Pend {
    id: u64,
    ip: IpAddr,
    deadline: Instant,
    ctl: TcpStream,
}
/// El sıkışma bitince (ya da bağlantı ölünce) yuvayı bırakır.
struct PendGuard {
    inner: Arc<Inner>,
    id: u64,
}
impl PendGuard {
    fn release(&self) {
        lock(&self.inner.pending).retain(|p| p.id != self.id);
    }
}
impl Drop for PendGuard {
    fn drop(&mut self) {
        self.release();
    }
}

/// İş parçacığı bitince (panikte de) sayacı düşürür ve soketini kapanış listesinden çıkarır.
struct ThreadGuard {
    inner: Arc<Inner>,
    live: Option<u64>,
}
impl ThreadGuard {
    fn new(inner: &Arc<Inner>, live: Option<u64>) -> ThreadGuard {
        inner.threads.fetch_add(1, Ordering::SeqCst);
        ThreadGuard { inner: inner.clone(), live }
    }
}
impl Drop for ThreadGuard {
    fn drop(&mut self) {
        if let Some(id) = self.live {
            lock(&self.inner.live).retain(|x| x.0 != id);
        }
        self.inner.threads.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Sabit süreli karşılaştırma (eşleştirme kodu).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    let mut d = (a.len() != b.len()) as u8;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct Bridge {
    inner: Arc<Inner>,
    accept_thread: Option<std::thread::JoinHandle<()>>,
}

impl Bridge {
    /// Dinlemeyi başlatır. Reddedilir: ağ Özel değil, adres yerel değil.
    pub fn start(host: Arc<dyn Host>, vault: Arc<dyn Vault>, ip: IpAddr, preferred_port: u16) -> Result<Bridge, String> {
        if !host.network_ok() {
            return Err("Ağ 'Özel' değil: dinleme kapalı.".into());
        }
        if !crate::net::is_local_bind(ip) {
            return Err("Yalnız yerel ağ adresine bağlanılır.".into());
        }
        // "Kayıt yok" ile "okunamadı" ayrı: okunamazsa anahtar/cihazlar sessizce yenilenmez.
        let mut data: VaultData = match vault.load_checked()? {
            None => VaultData::default(),
            Some(b) => serde_json::from_str(&b).map_err(|_| "Telefon kaydı okunamadı; 'Hepsini kaldır' ile sıfırlayabilirsin.".to_string())?,
        };
        if hex_decode(&data.private).map(|k| k.len()) != Some(32) || hex_decode(&data.public).map(|k| k.len()) != Some(32) {
            let kp = generate_keypair();
            data.private = hex(&kp.private);
            data.public = hex(&kp.public);
            data.devices.clear();
            vault.store(&serde_json::to_string(&data).map_err(|_| "Kayıt hatası.")?)?;
        }
        let me = Keypair { private: hex_decode(&data.private).unwrap(), public: hex_decode(&data.public).unwrap() };
        let listener = TcpListener::bind((ip, preferred_port)).or_else(|_| TcpListener::bind((ip, 0))).map_err(|_| "Dinleme başlatılamadı.".to_string())?;
        let addr = listener.local_addr().map_err(|_| "Dinleme başlatılamadı.".to_string())?;
        listener.set_nonblocking(true).map_err(|_| "Dinleme başlatılamadı.".to_string())?;
        let inner = Arc::new(Inner {
            host,
            vault,
            data: Mutex::new(data),
            me,
            addr,
            stop: AtomicBool::new(false),
            window: Mutex::new(None),
            pair: Mutex::new(PairState::default()),
            pair_cv: Condvar::new(),
            conns: Mutex::new(Vec::new()),
            throttle: Mutex::new(Throttle::new()),
            threads: AtomicUsize::new(0),
            next_conn: AtomicU64::new(1),
            pending: Mutex::new(Vec::new()),
            next_pend: AtomicU64::new(1),
            lockdown: AtomicBool::new(false),
            live: Mutex::new(Vec::new()),
            known_ips: Mutex::new(VecDeque::new()),
        });
        let i2 = inner.clone();
        let accept_thread = std::thread::Builder::new()
            .name("phone-accept".into())
            .spawn(move || accept_loop(i2, listener))
            .map_err(|_| "Dinleme başlatılamadı.".to_string())?;
        Ok(Bridge { inner, accept_thread: Some(accept_thread) })
    }

    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }

    /// "Telefon ekle": tek kullanımlık QR, 2 dk. Aynı anda yalnız bir pencere.
    pub fn open_pairing(&self) -> Result<QrPayload, String> {
        if lock(&self.inner.data).devices.len() >= MAX_DEVICES {
            return Err("En çok 5 telefon eşlenebilir; önce birini kaldır.".into());
        }
        let mut w = lock(&self.inner.window);
        if w.as_ref().is_some_and(|w| !w.expired() && !w.used()) || lock(&self.inner.pair).code.is_some() {
            return Err("Bir eşleştirme zaten açık.".into());
        }
        let win = PairingWindow::open();
        let qr = win.qr(&self.inner.addr.to_string(), &self.inner.me.public);
        *w = Some(win);
        drop(w);
        self.inner.host.event(Event::Changed);
        Ok(qr)
    }

    pub fn pairing_open(&self) -> bool {
        lock(&self.inner.window).as_ref().is_some_and(|w| !w.expired() && !w.used())
    }
    /// Eşleştirme onayı bekleyen telefonun (doğrulanmamış) adı. Kod burada VERİLMEZ:
    /// kullanıcı telefonda görünen kodu masaüstüne yazar.
    pub fn pairing_name(&self) -> Option<String> {
        lock(&self.inner.pair).code.as_ref().map(|c| c.1.clone())
    }

    /// Kullanıcının masaüstüne yazdığı kod. TEK deneme: yanlışsa eşleştirme iptal olur.
    pub fn confirm_pairing_code(&self, typed: &str) -> Result<(), String> {
        let mut p = lock(&self.inner.pair);
        let Some((code, _)) = p.code.clone() else { return Err("Bekleyen eşleştirme yok.".into()) };
        if p.decision.is_some() {
            return Err("Bu eşleştirme zaten sonuçlandı.".into());
        }
        let ok = ct_eq(code.as_bytes(), typed.trim().as_bytes());
        p.decision = Some(ok);
        self.inner.pair_cv.notify_all();
        if ok { Ok(()) } else { Err("Kod yanlış; eşleştirme iptal edildi. Yeniden dene.".into()) }
    }
    /// Masaüstünde "Hayır / iptal".
    pub fn decline_pairing(&self) {
        let mut p = lock(&self.inner.pair);
        if p.code.is_some() && p.decision.is_none() {
            p.decision = Some(false);
            self.inner.pair_cv.notify_all();
        }
    }
    pub fn cancel_pairing(&self) {
        *lock(&self.inner.window) = None;
        self.decline_pairing();
        self.inner.host.event(Event::Changed);
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        let conns = lock(&self.inner.conns);
        lock(&self.inner.data)
            .devices
            .iter()
            .map(|d| DeviceInfo {
                id: d.id.clone(),
                name: d.name.clone(),
                paired_at: d.paired_at,
                last_seen: d.last_seen,
                connected: conns.iter().any(|c| c.device == d.id),
                hw: d.hw,
                hw_level: d.hw_level.clone(),
                hw_why: d.hw_why.clone(),
                expired: now_secs().saturating_sub(d.last_seen) > REPAIR_AFTER_SECS,
            })
            .collect()
    }

    /// G4: iptal listesi geldi/yenilendi. Listede seri özeti olan cihaz korumasız olur; eşleşmede liste yoktuysa
    /// (`rp`) ve listede değilse korumalı olur. Değişen cihazların açık oturumlarına yeni durum gider.
    /// Döner: değişen cihaz sayısı. Kayıt yazılamazsa revoke gibi kilitlenir.
    pub fn apply_revocations(&self, rev: &crate::attest::Revocations) -> Result<usize, String> {
        let mut changed: Vec<(String, bool, String)> = Vec::new();
        {
            let mut d = lock(&self.inner.data);
            for dev in d.devices.iter_mut() {
                if dev.rv.is_empty() {
                    // M-O3: denetlenecek seri yok (ara sertifikasız eski kayıt): sonsuza dek beklemede kalmaz,
                    // korumasız olarak sonuçlanır.
                    if dev.rp {
                        dev.rp = false;
                        dev.hw = false;
                        dev.hw_why = CHAIN_LEN_WHY.into();
                        changed.push((dev.id.clone(), false, dev.hw_level.clone()));
                    }
                    continue;
                }
                if rev.revoked(&dev.rv) {
                    if dev.hw || dev.rp {
                        dev.hw = false;
                        dev.rp = false;
                        dev.hw_why = REVOKED_WHY.into();
                        changed.push((dev.id.clone(), false, dev.hw_level.clone()));
                    }
                } else if dev.rp {
                    dev.rp = false;
                    dev.hw = !dev.hw_level.is_empty();
                    dev.hw_why = if dev.hw { String::new() } else { dev.hw_why.clone() };
                    changed.push((dev.id.clone(), dev.hw, dev.hw_level.clone()));
                }
            }
        }
        if changed.is_empty() {
            return Ok(0);
        }
        for c in lock(&self.inner.conns).iter() {
            if let Some((_, hw, level)) = changed.iter().find(|x| x.0 == c.device) {
                let _ = c.tx.send(Out::Device(*hw, level.clone()));
            }
        }
        let saved = persist(&self.inner);
        if saved.is_err() {
            self.inner.lockdown.store(true, Ordering::SeqCst);
        }
        self.inner.host.event(Event::Changed);
        saved.map(|_| changed.len())
    }

    /// Kaldır: anında güveni siler, açık bağlantıyı keser. Kayıt silinemezse (Kimlik Bilgisi
    /// Yöneticisi hatası) cihaz bellekte yine güvensiz kalır, bağlantılar kesilir, köprü kilitlenir
    /// (yeni bağlantı yok) ve hata döner: yeniden başlayınca geri gelebileceği gizlenmez.
    pub fn revoke(&self, id: &str) -> Result<bool, String> {
        let removed = {
            let mut d = lock(&self.inner.data);
            let before = d.devices.len();
            d.devices.retain(|x| x.id != id);
            d.devices.len() != before
        };
        if !removed {
            return Ok(false);
        }
        for c in lock(&self.inner.conns).iter().filter(|c| c.device == id) {
            let _ = c.tx.send(Out::Bye("revoked"));
        }
        let saved = persist(&self.inner);
        if saved.is_err() {
            self.inner.lockdown.store(true, Ordering::SeqCst);
            for c in lock(&self.inner.conns).iter() {
                let _ = c.tx.send(Out::Bye("locked"));
            }
        }
        self.inner.host.event(Event::Changed);
        saved.map(|_| true).map_err(|e| format!("Telefon kaydı silinemedi, dinleme kapatıldı: {e}"))
    }
    /// Hepsini kaldırır; ilk hatayı döndürür (diğerleri yine denenir).
    pub fn revoke_all(&self) -> Result<(), String> {
        let ids: Vec<String> = lock(&self.inner.data).devices.iter().map(|d| d.id.clone()).collect();
        let mut first = None;
        for id in ids {
            if let Err(e) = self.revoke(&id) {
                first.get_or_insert(e);
            }
        }
        first.map_or(Ok(()), Err)
    }
    /// Kayıt silinemediği için kilitlendi mi?
    pub fn locked_down(&self) -> bool {
        self.inner.lockdown.load(Ordering::SeqCst)
    }

    /// Ajan durumu değişince çağrılır; bağlı telefonlara (değiştiyse) yeni durum gider.
    pub fn push_state(&self) {
        let conns = lock(&self.inner.conns);
        if conns.is_empty() {
            return;
        }
        let msg = state_msg(&self.inner.host.state());
        for c in conns.iter() {
            let _ = c.tx.send(Out::State(msg.clone()));
        }
    }

    /// Ajanda değişince (ya da gün dönünce) çağrılır; bağlı telefonlara (değiştiyse) yeni liste gider.
    pub fn push_agenda(&self) {
        let conns = lock(&self.inner.conns);
        if conns.is_empty() {
            return;
        }
        let msg = agenda_msg(&self.inner.host.agenda());
        for c in conns.iter() {
            let _ = c.tx.send(Out::Agenda(msg.clone()));
        }
    }

    /// Sınamalar için: cihazı donanım korumalı say (gerçek kanıt zinciri Google köküne bağlanmalı; sınamada yok).
    #[doc(hidden)]
    pub fn mark_protected_for_tests(&self, id: &str) {
        if let Some(d) = lock(&self.inner.data).devices.iter_mut().find(|d| d.id == id) {
            d.hw = true;
            d.hw_level = "tee".into();
            d.hw_why.clear();
        }
    }

    /// Sınamalar için: kanıt sonucunu (seviye, seri özetleri, liste beklemesi) elle kurar.
    #[doc(hidden)]
    pub fn set_attest_for_tests(&self, id: &str, hw: bool, level: &str, rv: Vec<String>, rp: bool) {
        if let Some(d) = lock(&self.inner.data).devices.iter_mut().find(|d| d.id == id) {
            d.hw = hw;
            d.hw_level = level.into();
            d.rv = rv;
            d.rp = rp;
        }
    }

    /// Sınamalar için: kayıttaki son görülme.
    #[doc(hidden)]
    pub fn last_seen_for_tests(&self, id: &str) -> Option<u64> {
        lock(&self.inner.data).devices.iter().find(|d| d.id == id).map(|d| d.last_seen)
    }

    #[doc(hidden)]
    pub fn age_device_for_tests(&self, id: &str, secs: u64) {
        if let Some(d) = lock(&self.inner.data).devices.iter_mut().find(|d| d.id == id) {
            d.last_seen = now_secs().saturating_sub(secs);
        }
    }

    pub fn connected(&self) -> usize {
        lock(&self.inner.conns).len()
    }

    /// Sınamalar için: bağlantı iş parçacığı sayısını okuyan işlev (köprü durdurulduktan sonra da çalışır).
    #[doc(hidden)]
    pub fn thread_probe_for_tests(&self) -> impl Fn() -> usize + Send + 'static {
        let inner = self.inner.clone();
        move || inner.threads.load(Ordering::SeqCst)
    }

    pub fn stop(mut self) {
        self.shutdown();
    }
    fn shutdown(&mut self) {
        if self.inner.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        for c in lock(&self.inner.conns).iter() {
            let _ = c.tx.send(Out::Bye("closed"));
        }
        self.inner.pair_cv.notify_all();
        // M-O1: bekleyen, eşleşen ve bağlı bütün soketler kesilir; okumada bekleyen iş parçacıkları hemen biter.
        // Kayıt aynı kilit altında durdurma bayrağına baktığı için durdurmadan sonra eklenen soket kalmaz.
        for (_, s) in lock(&self.inner.live).iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
        // M-D1: kabul döngüsü bloklamaz, en geç `ACCEPT_POLL` içinde durur (kendine bağlanıp uyandırmaya gerek yok).
        if let Some(t) = self.accept_thread.take() {
            let _ = t.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// İptal edilmiş kanıt sertifikasının arayüz nedeni.
pub const REVOKED_WHY: &str = WHY_TEXTS[16];
/// Eşleşmede iptal listesi alınamadı (internet yok); liste gelince karar verilir.
pub const PENDING_WHY: &str = WHY_TEXTS[17];
/// Zincir uzunluğu geçersiz (ara sertifika yok ya da `MAX_RV`'den çok).
const CHAIN_LEN_WHY: &str = WHY_TEXTS[1];

/// G4: oturum sürerken / kapanırken `last_seen` güncellenir. Kayıt yazılamazsa sessiz kalır (yalnız tarih).
fn touch(inner: &Inner, id: &str) {
    let now = now_secs();
    let found = {
        let mut d = lock(&inner.data);
        match d.devices.iter_mut().find(|x| x.id == id) {
            Some(x) => {
                x.last_seen = now;
                true
            }
            None => false,
        }
    };
    if found {
        let _ = persist(inner);
    }
}

fn persist(inner: &Inner) -> Result<(), String> {
    let blob = serde_json::to_string(&*lock(&inner.data)).map_err(|_| "Kayıt hatası.".to_string())?;
    inner.vault.store(&blob)
}

fn agenda_msg(a: &AgendaView) -> String {
    json!({"t": "agenda", "from": a.from, "days": a.days, "items": a.items}).to_string()
}

fn state_msg(s: &StateView) -> String {
    json!({"t": "state", "sessions": s.sessions, "approvals": s.approvals}).to_string()
}

/// El sıkışma süresi dolan bağlantıları keser (yeni bağlantı gelmesini beklemeden; M-D2).
fn sweep_pending(pend: &mut Vec<Pend>) {
    let now = Instant::now();
    pend.retain(|p| {
        if p.deadline <= now {
            let _ = p.ctl.shutdown(Shutdown::Both);
            false
        } else {
            true
        }
    });
}

/// M-D2: doğrulanmış bağlantının IP'si hatırlanır (ayrılmış el sıkışma yuvası için).
fn remember_ip(inner: &Inner, ip: IpAddr) {
    let mut k = lock(&inner.known_ips);
    k.retain(|x| *x != ip);
    k.push_back(ip);
    while k.len() > MAX_KNOWN_IPS {
        k.pop_front();
    }
}

fn accept_loop(inner: Arc<Inner>, listener: TcpListener) {
    // M-D1: dinleyici bloklamaz; durdurma bayrağı en geç `ACCEPT_POLL` içinde görülür (kendine bağlanıp uyandırma,
    // Wi-Fi adresi kalkmışsa başarısız olup `join`'i sonsuza dek bekletebiliyordu).
    while !inner.stop.load(Ordering::SeqCst) {
        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    let mut pend = lock(&inner.pending);
                    if !pend.is_empty() {
                        sweep_pending(&mut pend);
                    }
                }
                std::thread::sleep(ACCEPT_POLL);
                continue;
            }
        };
        // Kabul edilen soket dinleyicinin bloklamayan kipini devralabilir (Windows).
        if stream.set_nonblocking(false).is_err() || inner.threads.load(Ordering::SeqCst) >= MAX_THREADS || inner.lockdown.load(Ordering::SeqCst) || !inner.host.network_ok() {
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        }
        let Ok(ip) = stream.peer_addr().map(|a| a.ip()) else { continue };
        if lock(&inner.throttle).blocked(ip) {
            let _ = stream.shutdown(Shutdown::Both);
            continue;
        }
        // Doğrulanmamış bağlantılar: süresi dolanı kes; toplam ve IP başına sınır. Kayıtlı cihazın son IP'si
        // için bir yuva ayrılır (yabancı IP'lerden yuva doldurma eşli telefonu dışarıda bırakmasın).
        let (Ok(ctl), Ok(live)) = (stream.try_clone(), stream.try_clone()) else { continue };
        let id = inner.next_pend.fetch_add(1, Ordering::SeqCst);
        let limit = if lock(&inner.known_ips).contains(&ip) { MAX_PENDING + RESERVED_PENDING } else { MAX_PENDING };
        {
            let mut pend = lock(&inner.pending);
            sweep_pending(&mut pend);
            if pend.len() >= limit || pend.iter().any(|p| p.ip == ip) {
                drop(pend);
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            }
            pend.push(Pend { id, ip, deadline: Instant::now() + HANDSHAKE_TOTAL, ctl });
        }
        let guard = PendGuard { inner: inner.clone(), id };
        {
            // M-O1: kapanışta kesilecek soketler listesi (durdurma bayrağı aynı kilit altında denetlenir).
            let mut l = lock(&inner.live);
            if inner.stop.load(Ordering::SeqCst) {
                let _ = stream.shutdown(Shutdown::Both);
                break;
            }
            l.push((id, live));
        }
        let tg = ThreadGuard::new(&inner, Some(id));
        let i2 = inner.clone();
        let _ = std::thread::Builder::new().name("phone-conn".into()).spawn(move || {
            let _tg = tg;
            handle(&i2, stream, ip, guard);
        });
    }
}

fn handle(inner: &Arc<Inner>, mut stream: TcpStream, ip: IpAddr, pend: PendGuard) {
    // G4: yavaşlatma: bu IP'nin önceki hataları kadar bekle (yalnız kendi yuvasını tutar; IP başına 1 yuva).
    let wait = lock(&inner.throttle).delay(ip);
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
    // M-D2: el sıkışmada her okuma kısa sürelidir (Windows'ta `shutdown` bekleyen okumayı uyandırmaz; süresi dolan
    // bağlantı ancak okuma zaman aşımıyla biter).
    let _ = stream.set_read_timeout(Some(HANDSHAKE_READ));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_nodelay(true);
    let mut mode = [0u8; 1];
    if stream.read_exact(&mut mode).is_err() {
        lock(&inner.throttle).fail(ip);
        return;
    }
    match mode[0] {
        MODE_PAIR => pair(inner, stream, ip, pend),
        MODE_NORMAL => normal(inner, stream, ip, pend),
        _ => lock(&inner.throttle).fail(ip),
    }
}

/// Veri gelene dek bekler (en çok `max`); durdurma bayrağına `STOP_POLL` aralıkla bakar. Windows'ta okuma zaman
/// aşımı tanıtıcı başınadır ve `shutdown` bekleyen okumayı uyandırmaz: uzun beklemeler bu dilimli `peek` ile,
/// okumayı yapan tanıtıcının kendisinde yapılır; çerçeve sonra kısa zaman aşımıyla okunur.
fn wait_readable(inner: &Inner, s: &TcpStream, max: Duration) -> bool {
    let until = Instant::now() + max;
    let mut one = [0u8; 1];
    let ready = loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() || inner.stop.load(Ordering::SeqCst) {
            break false;
        }
        let _ = s.set_read_timeout(Some(left.min(STOP_POLL)));
        match s.peek(&mut one) {
            Ok(n) => break n > 0,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break false,
        }
    };
    let _ = s.set_read_timeout(Some(IO_TIMEOUT));
    ready
}

fn send_json<S: std::io::Read + std::io::Write>(ch: &mut Channel<S>, v: Value) -> bool {
    ch.send(v.to_string().as_bytes()).is_ok()
}
fn recv_json<S: std::io::Read + std::io::Write>(ch: &mut Channel<S>) -> Option<Value> {
    recv_json_max(ch, MAX_INCOMING)
}
fn recv_json_max<S: std::io::Read + std::io::Write>(ch: &mut Channel<S>, max: usize) -> Option<Value> {
    let b = ch.recv().ok()?;
    if b.len() > max {
        return None;
    }
    serde_json::from_slice(&b).ok()
}

/// O9 + G4: kanıt zincirinin sonucu.
#[derive(Default)]
struct HelloAttest {
    hw: bool,
    level: String,
    why: String,
    rv: Vec<String>,
    rp: bool,
}

/// O9: `hello`'daki kanıt zincirini doğrular; G4: önyükleme şartı (attest içinde) ve Google iptal listesi.
/// Hata eşleşmeyi durdurmaz (korumasız işaretlenir).
fn attest_hello(hello: &Value, apub: &str, handshake_hash: &[u8], rev: Option<&crate::attest::Revocations>) -> HelloAttest {
    let att: Vec<String> = match hello.get("att") {
        None => Vec::new(),
        Some(Value::Array(a)) if a.len() <= crate::attest::MAX_CHAIN => a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect(),
        Some(_) => return HelloAttest { why: "Kanıt zinciri okunamadı.".into(), ..Default::default() },
    };
    match crate::attest::verify_full(&att, apub, handshake_hash, &crate::attest::GOOGLE_ROOTS) {
        Ok(a) => decide_revocation(a, rev),
        Err(why) => HelloAttest { why: why.to_owned(), ..Default::default() },
    }
}

/// Zincir geçerliyse iptal listesine göre karar: listede → korumasız; liste yok → beklemede (korumasız, `rp`).
fn decide_revocation(a: crate::attest::Attested, rev: Option<&crate::attest::Revocations>) -> HelloAttest {
    let level = a.level.as_str().to_owned();
    match rev {
        Some(r) if r.revoked(&a.rv) => HelloAttest { hw: false, level, why: REVOKED_WHY.into(), rv: a.rv, rp: false },
        Some(_) => HelloAttest { hw: true, level, why: String::new(), rv: a.rv, rp: false },
        None => HelloAttest { hw: false, level, why: PENDING_WHY.into(), rv: a.rv, rp: true },
    }
}

/// Sınamalar için: sahte kökle üretilmiş zincirin sonucunu eşleştirmedeki iptal kararından geçirir.
#[doc(hidden)]
pub fn decide_revocation_for_tests(a: crate::attest::Attested, rev: Option<&crate::attest::Revocations>) -> (bool, String, bool) {
    let h = decide_revocation(a, rev);
    (h.hw, h.why, h.rp)
}

fn pair(inner: &Arc<Inner>, stream: TcpStream, ip: IpAddr, pend: PendGuard) {
    // Pencere yalnız el sıkışma BAŞARILI olunca yanar: jetonu bilmeyen bağlantı QR'ı tüketemez.
    let token = lock(&inner.window).as_ref().and_then(|w| w.peek());
    let Some(token) = token else {
        lock(&inner.throttle).fail(ip);
        return;
    };
    let mut ch = match accept(stream, &inner.me, &token, Trust::AcceptNew) {
        Ok(c) => c,
        Err(_) => {
            lock(&inner.throttle).fail(ip);
            return;
        }
    };
    // Jeton doğrulandı: pencereyi şimdi tüket (yarışı kaybeden düşer).
    if lock(&inner.window).as_mut().and_then(|w| w.take()) != Some(token) {
        lock(&inner.throttle).fail(ip);
        return;
    }
    pend.release();
    // M-O1: uzun beklemeler durdurmayı görür (eskiden ayrı tanıtıcıda kurulan süre okumaya hiç uygulanmıyordu).
    let hello = if wait_readable(inner, ch.io(), HELLO_WAIT) { recv_json_max(&mut ch, MAX_HELLO) } else { None };
    let (name, apub) = match hello.as_ref().and_then(|h| Some((h.get("t")?.as_str()? == "hello", h.get("name")?.as_str()?.to_owned(), h.get("apub")?.as_str()?.to_owned()))) {
        Some((true, n, a)) if valid_apub(&a) => (clean_name(&n), a),
        _ => {
            lock(&inner.throttle).fail(ip);
            return;
        }
    };
    let rev = inner.host.revocations();
    let HelloAttest { hw, level: hw_level, why: hw_why, rv, rp } = hello.as_ref().map(|h| attest_hello(h, &apub, &ch.handshake_hash, rev.as_deref())).unwrap_or_default();
    {
        let mut p = lock(&inner.pair);
        *p = PairState { code: Some((ch.confirm.clone(), name.clone())), decision: None };
    }
    inner.host.event(Event::PairCode { code: ch.confirm.clone(), name: name.clone() });
    // Masaüstü onayı ("Bu telefonu eşle?") için bekle.
    let decision = {
        let mut p = lock(&inner.pair);
        let until = Instant::now() + PAIR_CONFIRM_WAIT;
        while p.decision.is_none() && !inner.stop.load(Ordering::SeqCst) {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            p = inner.pair_cv.wait_timeout(p, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        let d = p.decision.unwrap_or(false);
        *p = PairState::default();
        d
    };
    let finish = |ok: bool| {
        // G4 (D10): yalnız bu eşleştirmenin (yanmış/süresi dolmuş) penceresi kapanır; bu arada açılmış yenisi kalır.
        let mut w = lock(&inner.window);
        if w.as_ref().is_none_or(|w| w.used() || w.expired()) {
            *w = None;
        }
        drop(w);
        inner.host.event(Event::Changed);
        ok
    };
    if !decision {
        let _ = send_json(&mut ch, json!({"t": "bye", "why": "declined"}));
        finish(false);
        return;
    }
    // Telefon da kendi ekranında kodu onaylamış olmalı.
    let phone_ok = wait_readable(inner, ch.io(), PAIR_CONFIRM_WAIT) && recv_json(&mut ch).is_some_and(|m| m.get("t").and_then(Value::as_str) == Some("pc") && m.get("ok").and_then(Value::as_bool) == Some(true));
    if !phone_ok {
        let _ = send_json(&mut ch, json!({"t": "bye", "why": "declined"}));
        finish(false);
        return;
    }
    let id = {
        let mut rnd = [0u8; 4];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
        hex(&rnd)
    };
    {
        let mut d = lock(&inner.data);
        if d.devices.len() >= MAX_DEVICES {
            drop(d);
            let _ = send_json(&mut ch, json!({"t": "bye", "why": "full"}));
            finish(false);
            return;
        }
        let pk = hex(&ch.remote_static);
        d.devices.retain(|x| x.pubkey != pk); // aynı telefon yeniden eşlenirse eskisi düşer
        let now = now_secs();
        d.devices.push(Device { id: id.clone(), name, pubkey: pk, apub, paired_at: now, last_seen: now, hw, hw_level: hw_level.clone(), hw_why, rv, rp });
    }
    if persist(inner).is_err() {
        lock(&inner.data).devices.retain(|x| x.id != id);
        let _ = send_json(&mut ch, json!({"t": "bye", "why": "error"}));
        finish(false);
        return;
    }
    lock(&inner.throttle).ok(ip);
    remember_ip(inner, ip);
    let _ = send_json(&mut ch, json!({"t": "paired", "id": id, "hw": hw, "level": hw_level}));
    finish(true);
}

fn normal(inner: &Arc<Inner>, stream: TcpStream, ip: IpAddr, pend: PendGuard) {
    // T-Y2: bilinmeyen anahtarla da el sıkışma tamamlanır ve tek bir `bye unpaired` yollanır; telefon masaüstü
    // anahtarını sabitlediği için bu red doğrulanmıştır. Böylece çevrimdışıyken kaldırılan telefon da kaldırıldığını
    // öğrenir (yoksa sonsuza dek dener). Kanalda bu iletiden başka hiçbir şey yollanmaz.
    let mut ch = match accept(stream, &inner.me, b"", Trust::AcceptNew) {
        Ok(c) => c,
        Err(_) => {
            lock(&inner.throttle).fail(ip);
            return;
        }
    };
    let pk = hex(&ch.remote_static);
    if !lock(&inner.data).devices.iter().any(|d| d.pubkey == pk) {
        lock(&inner.throttle).fail(ip);
        let _ = send_json(&mut ch, json!({"t": "bye", "why": "unpaired"}));
        return;
    }
    pend.release();
    let now = now_secs();
    let dev = {
        let mut d = lock(&inner.data);
        let Some(pos) = d.devices.iter().position(|x| x.pubkey == pk) else { return };
        if now.saturating_sub(d.devices[pos].last_seen) > REPAIR_AFTER_SECS {
            d.devices.remove(pos);
            drop(d);
            if persist(inner).is_err() {
                // Silinemeyen kayıt yeniden başlayınca geri gelmesin diye iptaldeki gibi güvenli taraf.
                inner.lockdown.store(true, Ordering::SeqCst);
            }
            let _ = send_json(&mut ch, json!({"t": "bye", "why": "repair"}));
            inner.host.event(Event::Changed);
            return;
        }
        if now.saturating_sub(d.devices[pos].last_seen) > 3600 {
            d.devices[pos].last_seen = now;
            drop(d);
            let _ = persist(inner);
            lock(&inner.data).devices.iter().find(|x| x.pubkey == pk).cloned()
        } else {
            Some(d.devices[pos].clone())
        }
    };
    let Some(dev) = dev else { return };
    lock(&inner.throttle).ok(ip);
    remember_ip(inner, ip);
    let (Ok(wsock), Ok(wctl)) = (ch.io().try_clone(), ch.io().try_clone()) else { return };
    // Ayarlar tanıtıcı başına olabilir (Windows): yazan tanıtıcıya da süre konur.
    let _ = wsock.set_write_timeout(Some(IO_TIMEOUT));
    let _ = ch.io().set_read_timeout(Some(IO_TIMEOUT));

    let (tx, rx) = channel::<Out>();
    let cid = inner.next_conn.fetch_add(1, Ordering::SeqCst);
    {
        let mut conns = lock(&inner.conns);
        if conns.iter().filter(|c| c.device != dev.id).count() >= MAX_CONNS {
            drop(conns);
            let _ = send_json(&mut ch, json!({"t": "bye", "why": "busy"}));
            return;
        }
        for c in conns.iter().filter(|c| c.device == dev.id) {
            let _ = c.tx.send(Out::Bye("replaced"));
        }
        conns.push(Conn { id: cid, device: dev.id.clone(), tx: tx.clone() });
    }
    inner.host.event(Event::Changed);
    // M-D4: giden iletiler (durum, ajanda, cevaplar) kuyrukta bekleyen ayrı yazan iş parçacığından gider; okuyan
    // taraf yoklamadan, uzun beklemeyle okur. İkisi aynı Noise durumunu paylaşır.
    let hash = ch.handshake_hash.clone();
    let (mut rd, wr) = ch.split(wsock);
    let tg = ThreadGuard::new(inner, None);
    let closed = Arc::new(AtomicBool::new(false));
    let (i2, c2, hw, level) = (inner.clone(), closed.clone(), dev.hw, dev.hw_level.clone());
    let writer = std::thread::Builder::new().name("phone-send".into()).spawn(move || {
        let _tg = tg;
        writer_loop(&i2, wr, &wctl, rx, &c2, hw, level);
    });
    if writer.is_ok() {
        session_loop(inner, &mut rd, &tx, &closed, &dev, &hash, ip);
    }
    // Kuyruk kapanır: yazan taraf bekleyen son iletiyi (bye) yollar, soketi kapatır ve biter.
    drop(tx);
    lock(&inner.conns).retain(|c| c.id != cid);
    if let Ok(w) = writer {
        let _ = w.join();
    }
    // G4: bağlantı kapanırken son görülme güncellenir (uzun süren bağlantı 30 gün sayacını eskitmesin).
    touch(inner, &dev.id);
    inner.host.event(Event::Changed);
}

/// M-D4: yazan yarı. Kuyruk boşken uyanmaz; kuyruk kapanınca ya da yazma hatasında soketi kapatıp biter
/// (okuyan yarı da böylece uyanır).
fn writer_loop(inner: &Inner, mut w: SendHalf<TcpStream>, ctl: &TcpStream, rx: Receiver<Out>, closed: &AtomicBool, hw: bool, level: String) {
    let mut send = |s: &str| w.send(s.as_bytes()).is_ok();
    // O9: telefon kendi koruma durumunu bilsin (korumasızsa ekran kapalı eylemleri gizler).
    let mut ok = send(&json!({"t": "device", "hw": hw, "level": level}).to_string()) && send(&state_msg(&inner.host.state()));
    // Ajanda yalnız telefon isteyince başlar ({"t":"agenda"}); sonra değişiklikler kendiliğinden itilir.
    let mut agenda_on = false;
    let mut last_agenda = String::new();
    let mut last_state = String::new();
    while ok {
        let Ok(out) = rx.recv() else { break };
        ok = match out {
            Out::State(s) => {
                if s == last_state {
                    true
                } else {
                    let r = send(&s);
                    last_state = s;
                    r
                }
            }
            Out::Agenda(s) => {
                if agenda_on && s != last_agenda {
                    let r = send(&s);
                    last_agenda = s;
                    r
                } else {
                    true
                }
            }
            Out::AgendaNow => {
                let msg = agenda_msg(&inner.host.agenda());
                let r = send(&msg);
                last_agenda = msg;
                agenda_on = true;
                r
            }
            Out::Device(hw, level) => send(&json!({"t": "device", "hw": hw, "level": level}).to_string()),
            Out::Reply(s) => send(&s),
            Out::Bye(why) => {
                // Yazma yönü düzgün kapanır (FIN): okunmamış veri yüzünden RST gidip telefon `bye`'ı kaçırmasın.
                // Okuyan yarı telefonun kapatmasını ya da en geç `SESSION_CHECK`'te `closed` bayrağını görür.
                let _ = send(&json!({"t": "bye", "why": why}).to_string());
                closed.store(true, Ordering::SeqCst);
                let _ = ctl.shutdown(Shutdown::Write);
                return;
            }
        };
    }
    closed.store(true, Ordering::SeqCst);
    let _ = ctl.shutdown(Shutdown::Both);
}

fn recv_half(rd: &mut RecvHalf<TcpStream>) -> Option<Value> {
    let b = rd.recv().ok()?;
    if b.len() > MAX_INCOMING {
        return None;
    }
    serde_json::from_slice(&b).ok()
}

/// Okuyan yarı: telefonun iletilerini işler, cevapları yazan yarının kuyruğuna koyar.
fn session_loop(inner: &Arc<Inner>, rd: &mut RecvHalf<TcpStream>, tx: &Sender<Out>, closed: &AtomicBool, dev: &Device, hash: &[u8], ip: IpAddr) {
    let bye = |why: &'static str| {
        let _ = tx.send(Out::Bye(why));
    };
    let reply = |v: Value| tx.send(Out::Reply(v.to_string())).is_ok();
    let mut last_n = 0u64;
    let mut recent: VecDeque<Instant> = VecDeque::new();
    let mut all_recent: VecDeque<Instant> = VecDeque::new();
    let mut bad_sigs = 0u8;
    let mut last_touch = Instant::now();
    // Kayıttaki güncel cihaz (iptal ya da koruma durumu oturum sırasında değişebilir). `None`: kaldırıldı.
    let current = |inner: &Inner| lock(&inner.data).devices.iter().find(|d| d.id == dev.id && d.apub == dev.apub).map(|d| d.hw);
    loop {
        if last_touch.elapsed() >= TOUCH_EVERY {
            touch(inner, &dev.id);
            last_touch = Instant::now();
        }
        if inner.lockdown.load(Ordering::SeqCst) {
            bye("locked");
            return;
        }
        if inner.stop.load(Ordering::SeqCst) || !inner.host.network_ok() {
            bye("network");
            return;
        }
        // Yazan yarı `bye` yollayıp bitti (telefon bağlantıyı kapatmadıysa burada bırakılır).
        if closed.load(Ordering::SeqCst) {
            return;
        }
        // Veri var mı? Giden iletiler yazan yarıdan gider; bu bekleme yalnız yukarıdaki denetimler için uyanır.
        let _ = rd.io().set_read_timeout(Some(SESSION_CHECK));
        let mut one = [0u8; 1];
        match rd.io().peek(&mut one) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(_) => return,
        }
        let _ = rd.io().set_read_timeout(Some(IO_TIMEOUT));
        let Some(msg) = recv_half(rd) else { return };
        // G4 (D4): genel ileti sınırı (ping/folders/agenda da diskten okuyabilir).
        let now = Instant::now();
        while all_recent.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
            all_recent.pop_front();
        }
        all_recent.push_back(now);
        if all_recent.len() > MAX_MSGS_PER_SEC {
            lock(&inner.throttle).fail(ip);
            bye("busy");
            return;
        }
        match msg.get("t").and_then(Value::as_str) {
            Some("ping") => {
                if !reply(json!({"t": "pong"})) {
                    return;
                }
            }
            Some("agenda") => {
                if tx.send(Out::AgendaNow).is_err() {
                    return;
                }
            }
            Some("folders") => {
                // Kısıt açıksa korumasız cihaz yeni görev başlatamaz; klasör adları da gitmez.
                let list = if current(inner) == Some(true) || !RESTRICT_UNPROTECTED { inner.host.folders() } else { Vec::new() };
                if !reply(json!({"t": "folders", "list": list})) {
                    return;
                }
            }
            Some("act") => {
                // Hız sınırı: saniyede en çok 10 eylem.
                let now = Instant::now();
                while recent.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
                    recent.pop_front();
                }
                recent.push_back(now);
                let n = msg.get("n").and_then(Value::as_u64).unwrap_or(0);
                let ack = |ok: bool, err: &str| json!({"t": "ack", "n": n, "ok": ok, "err": err});
                if recent.len() > 10 {
                    if !reply(ack(false, "Çok hızlı.")) {
                        return;
                    }
                    continue;
                }
                let (a, sig) = (msg.get("a").and_then(Value::as_str), msg.get("sig").and_then(Value::as_str));
                let (Some(a), Some(sig)) = (a, sig) else {
                    let _ = reply(ack(false, "Geçersiz ileti."));
                    return;
                };
                if n <= last_n || a.len() > MAX_INCOMING || !verify_action(&dev.apub, hash, n, a, sig) {
                    bad_sigs += 1;
                    lock(&inner.throttle).fail(ip);
                    let _ = reply(ack(false, "İmza doğrulanamadı."));
                    if bad_sigs >= 3 {
                        return;
                    }
                    continue;
                }
                last_n = n;
                // İptal edilmiş cihazın (revoke ile oturum kuyruğu arasındaki pencerede) eylemi işlenmez.
                // G4: koruma durumu da kayıttan güncel okunur (iptal listesi oturum sırasında değiştirmiş olabilir).
                let Some(hw_now) = current(inner) else {
                    bye("revoked");
                    return;
                };
                let result = serde_json::from_str::<Action>(a).map_err(|_| "Geçersiz eylem.".to_string()).and_then(|act| {
                    act.validate()?;
                    // O9: kısıt açıksa korumasız cihaz yalnız bekleyen isteğe cevap verir ve durdurur.
                    let label = act.label();
                    let who = format!("{} · {}", dev.name, dev.id);
                    if !hw_now && RESTRICT_UNPROTECTED && !act.allowed_unprotected() {
                        inner.host.audit(&who, label, "reddedildi: korumasız telefon");
                        return Err(UNPROTECTED_DENIED.to_string());
                    }
                    let r = inner.host.act(&dev.name, act);
                    // G4: başarısız eylem de kayda girer (hata metni içerik taşımaz).
                    inner.host.audit(&who, label, &match &r {
                        Ok(()) => "ok".to_owned(),
                        Err(e) => format!("hata: {e}"),
                    });
                    r
                });
                let ok = result.is_ok();
                if !reply(ack(ok, &result.err().unwrap_or_default())) {
                    return;
                }
            }
            _ => {}
        }
    }
}
