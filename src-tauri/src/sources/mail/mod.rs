//! Mail kaynağı: kişisel Gmail, IMAP + uygulama şifresi (`imap.gmail.com:993`, yalnız TLS).
//!
//! Birden çok hesap (en çok `MAIL_SLOTS`): her hesap kendi bağlantısı ve iş parçacığıyla aşağıdaki gibi
//! çalışır; durumları `board`'da toplanır ve `model::merge_mail` ile tek Mail durumu olarak yayınlanır
//! (adada tek kutucuk, tek Zody). Satır kimliği "yuva:UID" (ör. "1:4821"); eylemler o hesabın iş
//! parçacığına gider. Bir hesabın hatası diğerlerini durdurmaz.
//!
//! Hesap başına tek bağlantı, tek iş parçacığı: giriş → `SELECT INBOX` → okunmamışlar (Gmail'de "Birincil" sekmesi) →
//! `IDLE`. Sunucu değişiklik söyleyince yeniden çekilir; sıkı yoklama yok. IDLE 9 dakikada bir yenilenir
//! (ölü bağlantı fark edilsin diye), uyku dönüşü duvar saatinden anlaşılır ve bağlantı baştan kurulur.
//! Önizleme ve "okundu yap" istekleri kanaldan gelir, IDLE'dan `DONE` ile çıkılıp aynı bağlantıda yapılır.
//!
//! Güvenlik (bkz. `sources/model.rs`): adres ve uygulama şifresi yalnız `crate::secrets::read` ile okunur,
//! yalnız iş parçacığının belleğinde durur. Satırlar (gönderen, konu) ve önizleme yalnız bellekte; kayda
//! yalnız sayılar ve hata kodları yazılır. Giriş reddedilince kendiliğinden yeniden denenmez (hesap kilidi
//! riski); "Yenile" ya da yeniden bağlanma ile denenir. Bağlantı yeniden kurma: 30 sn → 1 dk → 5 dk.

mod imap;
mod mime;
mod tls;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use tauri::AppHandle;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

use crate::sources::model::{AccountLine, CAT_ITEMS, MAIL_CATS, MAIL_KEYS, MAX_ITEMS, MailAccount, MailCat, merge_mail};
use crate::sources::{Source, SourceItem, publish};
use crate::{debuglog, secrets};
use imap::{Client, Fail, IdleEnd, Recv, Transport};

const ID: &str = "mail";
const HOST: &str = "imap.gmail.com";
const PORT: &str = "993";
/// IDLE bu kadar sürünce `DONE` + yeniden `IDLE` (RFC sınırı 29 dk; Gmail'de ölü bağlantıyı erken görmek için kısa).
const IDLE_MAX: Duration = Duration::from_secs(9 * 60);
/// IDLE beklerken uyku dönüşüne bakma aralığı (ağ trafiği yok).
const IDLE_SLICE: Duration = Duration::from_secs(2 * 60);
/// IDLE desteklemeyen sunucuda (Gmail destekler) yoklama aralığı.
const POLL: Duration = Duration::from_secs(5 * 60);
/// Bağlantı kopunca yeniden deneme aralıkları.
const BACKOFF: [Duration; 3] = [Duration::from_secs(30), Duration::from_secs(60), Duration::from_secs(300)];
/// Önizleme / okundu yap yanıtı en çok bu kadar beklenir.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Bu oturumda okundu yapılıp listede soluk kalan en çok satır.
const KEEP_READ: usize = 3;

const UNREACHABLE: &str = "Gmail'e ulaşılamıyor. Bağlantı gelince yeniden denenecek.";
const OFFLINE: &str = "Gmail'e şu an bağlı değil.";

enum Req {
    Preview(u32, Sender<Result<String, String>>),
    MarkRead(u32, Sender<Result<(), String>>),
    /// "Yenile": bağlıysa yeniden çeker, beklemedeyse hemen yeniden dener.
    Refresh,
}

enum Ev {
    /// (bağlantı no, veri) — okuma iş parçacığından.
    Data(u64, tls::Chunk),
    Req(Req),
    Stop,
}

/// Bir hesabın çalışan iş parçacığına ulaşmak için.
struct Run {
    slot: usize,
    tx: Sender<Ev>,
    socket: Arc<Mutex<Option<tls::Socket>>>,
}

/// Hesapların son durumu ve satırları (yuva sırasıyla). "Tarayıcıda aç" ağ beklemeden adresi buradan alır.
type Board = Arc<Mutex<Vec<MailAccount>>>;

#[derive(Default)]
struct Mail {
    runs: Mutex<Vec<Run>>,
    /// Her `start` bir artırır; eski iş parçacığı yeni çalışmanın adına yayın yapmasın.
    generation: Arc<AtomicU64>,
    board: Board,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Mail {
    fn request<T>(&self, slot: usize, make: impl FnOnce(Sender<Result<T, String>>) -> Req) -> Result<T, String> {
        let (reply, answer) = mpsc::channel();
        {
            let runs = lock(&self.runs);
            let run = runs.iter().find(|r| r.slot == slot).ok_or(OFFLINE)?;
            run.tx.send(Ev::Req(make(reply))).map_err(|_| OFFLINE)?;
        }
        answer.recv_timeout(REQUEST_TIMEOUT).map_err(|_| "Gmail yanıt vermedi.".to_owned())?
    }
}

/// Satır kimliği "yuva:UID".
fn item_id(slot: usize, uid: u32) -> String {
    format!("{slot}:{uid}")
}

fn parse_id(item_id: &str) -> Result<(usize, u32), String> {
    let unknown = || "Bilinmeyen ileti.".to_owned();
    let (slot, uid) = item_id.split_once(':').ok_or_else(unknown)?;
    Ok((slot.parse().map_err(|_| unknown())?, uid.parse().map_err(|_| unknown())?))
}

impl Source for Mail {
    fn id(&self) -> &'static str {
        ID
    }

    fn start(&self, _app: AppHandle) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let accounts: Vec<(usize, String, String)> = MAIL_KEYS
            .iter()
            .enumerate()
            .filter_map(|(slot, (address, password))| Some((slot, secrets::read(address)?, secrets::read(password)?)))
            .collect();
        {
            let mut board = lock(&self.board);
            *board = accounts
                .iter()
                .map(|(slot, address, _)| MailAccount {
                    line: AccountLine { slot: *slot as u8, address: address.clone(), status: "connecting".into(), ..AccountLine::default() },
                    items: Vec::new(),
                })
                .collect();
            publish(merge_mail(&board));
        }
        let mut runs = Vec::new();
        for (slot, address, password) in accounts {
            let (tx, rx) = mpsc::channel();
            let socket = Arc::new(Mutex::new(None));
            let worker = Worker {
                generation,
                current: self.generation.clone(),
                board: self.board.clone(),
                slot,
                tx: tx.clone(),
                socket: socket.clone(),
                address,
                password,
                conn: 0,
                primary: true,
            };
            match thread::Builder::new().name(format!("zodyvel-mail-{}", slot + 1)).spawn(move || worker.run(rx)) {
                Ok(_) => runs.push(Run { slot, tx, socket }),
                Err(_) => {
                    let mut board = lock(&self.board);
                    if let Some(a) = board.iter_mut().find(|a| a.line.slot as usize == slot) {
                        a.line.status = "error".into();
                        a.line.error = Some(UNREACHABLE.into());
                    }
                    publish(merge_mail(&board));
                }
            }
        }
        *lock(&self.runs) = runs;
    }

    fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        for run in std::mem::take(&mut *lock(&self.runs)) {
            let _ = run.tx.send(Ev::Stop);
            if let Some(socket) = lock(&run.socket).take() {
                socket.close();
            }
        }
        lock(&self.board).clear();
    }

    fn action(&self, item_id: &str, action: &str) -> Result<(), String> {
        match action {
            "open" => {
                let board = lock(&self.board);
                // "yuva:kategori" (Mail sekmesinde kartın ya da hesabın "Gmail ↗"): o hesabın o sekmesi.
                // "yuva:compose" ("Yeni mail"): o hesapla Gmail'in yazma penceresi.
                let category = item_id.split_once(':').and_then(|(slot, key)| {
                    let account = board.iter().find(|a| slot.parse::<u8>().ok() == Some(a.line.slot))?;
                    let address = &account.line.address;
                    match key {
                        "compose" => Some(imap::compose_url(address)),
                        _ => (key == "inbox" || MAIL_CATS.contains(&key)).then(|| imap::category_url(address, key)),
                    }
                });
                let url = category
                    .or_else(|| {
                        board
                            .iter()
                            .flat_map(|a| a.items.iter().chain(a.line.cats.iter().flat_map(|c| c.items.iter())))
                            .find(|i| i.id == item_id)
                            .and_then(|i| i.url.clone())
                    })
                    .ok_or("Bilinmeyen ileti.")?;
                drop(board);
                // Adresi biz kurduk (web_url); yine de yalnız Gmail'in kendi adresi açılır.
                if !url.starts_with("https://mail.google.com/") || !crate::win::shell_open(&url) {
                    return Err("Tarayıcı açılamadı.".into());
                }
                Ok(())
            }
            "mark_read" => {
                let (slot, uid) = parse_id(item_id)?;
                self.request(slot, |reply| Req::MarkRead(uid, reply))
            }
            _ => Err("Bu eylem mailde yok.".into()),
        }
    }

    fn preview(&self, item_id: &str) -> Result<String, String> {
        let (slot, uid) = parse_id(item_id)?;
        self.request(slot, |reply| Req::Preview(uid, reply))
    }

    fn refresh(&self, app: AppHandle) {
        let runs = lock(&self.runs);
        // Hepsine gönderilir (`all` ilk başarısızda durmasın diye önce topla).
        let sent: Vec<bool> = runs.iter().map(|run| run.tx.send(Ev::Req(Req::Refresh)).is_ok()).collect();
        drop(runs);
        if sent.is_empty() || !sent.iter().all(|ok| *ok) {
            self.stop();
            self.start(app);
        }
    }
}

pub fn source() -> Arc<dyn Source> {
    Arc::new(Mail::default())
}

// ─── İş parçacığı ───────────────────────────────────────────────────────────────────────────────

struct Worker {
    generation: u64,
    current: Arc<AtomicU64>,
    board: Board,
    /// Hesap yuvası (`MAIL_KEYS` sırası).
    slot: usize,
    tx: Sender<Ev>,
    socket: Arc<Mutex<Option<tls::Socket>>>,
    address: String,
    password: String,
    /// Bağlantı numarası (eski bağlantının geç gelen verisi yok sayılır).
    conn: u64,
    /// Gmail "Birincil" süzgeci (sunucu reddederse kapanır).
    primary: bool,
}

/// Oturumun nasıl bittiği.
enum End {
    Stopped,
    /// Bağlantı koptu; `bool`: daha önce "ok" yayınlanmıştı (bir kez sessizce hemen yeniden dene).
    Dropped(bool, String),
    Auth(String),
}

impl Worker {
    fn live(&self) -> bool {
        self.current.load(Ordering::SeqCst) == self.generation
    }

    /// Bu hesabın kaydını değiştirir ve tüm hesapların birleşik durumunu yayınlar (kilit içinde: iki hesabın
    /// yayını birbirini eskisiyle ezmesin).
    fn update(&self, change: impl FnOnce(&mut MailAccount)) {
        let mut board = lock(&self.board);
        if !self.live() {
            return;
        }
        if let Some(account) = board.iter_mut().find(|a| a.line.slot as usize == self.slot) {
            change(account);
        }
        publish(merge_mail(&board));
    }

    fn set_status(&self, status: &str, error: Option<&str>) {
        self.update(|a| {
            a.line.status = status.into();
            a.line.error = error.map(Into::into);
            if status != "ok" {
                a.line.badge = 0;
                a.items.clear();
                a.line.cats.clear();
            }
        });
    }

    fn error(&self, text: &str) {
        self.set_status("error", Some(text));
    }

    /// Kayıtta hangi hesap olduğu (adres değil, yuva numarası).
    fn log(&self, text: impl std::fmt::Display) {
        debuglog::line(format!("mail {}: {text}", self.slot + 1));
    }

    fn run(mut self, rx: Receiver<Ev>) {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let mut rx = rx;
        let mut failures = 0usize;
        let mut quiet_retry = false;
        loop {
            if !self.live() {
                return;
            }
            let (end, back) = self.session(rx);
            rx = back;
            let wait = match end {
                End::Stopped => return,
                End::Auth(text) => {
                    self.log("giriş reddedildi");
                    self.error(&text);
                    None
                }
                End::Dropped(was_ok, why) => {
                    self.log(format_args!("bağlantı bitti ({why})"));
                    if was_ok {
                        failures = 0;
                    }
                    if was_ok && !quiet_retry {
                        quiet_retry = true;
                        continue;
                    }
                    let wait = BACKOFF[failures.min(BACKOFF.len() - 1)];
                    failures += 1;
                    self.error(UNREACHABLE);
                    Some(wait)
                }
            };
            quiet_retry = false;
            if !self.pause(&rx, wait) {
                return;
            }
            self.set_status("connecting", None);
        }
    }

    /// Bağlantısızken bekler (`None`: "Yenile"ye kadar). İstekler "bağlı değil" ile yanıtlanır. Durdurulunca `false`.
    fn pause(&self, rx: &Receiver<Ev>, wait: Option<Duration>) -> bool {
        let until = wait.map(|w| Instant::now() + w);
        loop {
            let ev = match until {
                Some(t) => match rx.recv_timeout(t.saturating_duration_since(Instant::now())) {
                    Ok(ev) => ev,
                    Err(RecvTimeoutError::Timeout) => return true,
                    Err(RecvTimeoutError::Disconnected) => return false,
                },
                None => match rx.recv() {
                    Ok(ev) => ev,
                    Err(_) => return false,
                },
            };
            match ev {
                Ev::Stop => return false,
                Ev::Req(Req::Refresh) => return true,
                Ev::Req(req) => refuse(req),
                Ev::Data(..) => {}
            }
        }
    }

    /// Tek bağlantı ömrü. Kanalı geri verir (bağlantı nesnesi kanalı ödünç alır).
    fn session(&mut self, rx: Receiver<Ev>) -> (End, Receiver<Ev>) {
        self.conn += 1;
        let conn = self.conn;
        let socket = match tls::Socket::new() {
            Ok(s) => s,
            Err(e) => return (End::Dropped(false, e), rx),
        };
        *lock(&self.socket) = Some(socket.clone());
        if !self.live() {
            return (End::Stopped, rx);
        }
        let tx = self.tx.clone();
        if let Err(e) = socket.connect(HOST, PORT, move |chunk| {
            let _ = tx.send(Ev::Data(conn, chunk));
        }) {
            socket.close();
            return (if self.live() { End::Dropped(false, e) } else { End::Stopped }, rx);
        }
        let mut client = Client::new(Link { rx, conn, socket: socket.clone(), stash: VecDeque::new() });
        let mut ok = false;
        let result = self.serve(&mut client, &mut ok);
        socket.close();
        lock(&self.socket).take();
        let Link { rx, stash, .. } = client.io;
        stash.into_iter().for_each(refuse);
        let end = match result {
            Err(Fail::Stopped) => End::Stopped,
            Err(Fail::Auth(text)) => End::Auth(text),
            Err(Fail::Net(why)) => End::Dropped(ok, why),
            Err(Fail::Timeout) => End::Dropped(ok, "zaman aşımı".into()),
            Err(Fail::No(_)) => End::Dropped(ok, "komut reddedildi".into()),
            Ok(()) => End::Stopped,
        };
        (if self.live() { end } else { End::Stopped }, rx)
    }

    fn serve(&mut self, client: &mut Client<Link>, ok: &mut bool) -> Result<(), Fail> {
        client.greeting()?;
        client.login(&self.address, &self.password)?;
        client.select_inbox()?;
        if !client.has("X-GM-EXT-1") {
            self.primary = false;
        }
        self.sync(client)?;
        *ok = true;
        self.log("bağlandı");
        let idle = client.has("IDLE");
        loop {
            while let Some(req) = client.io.stash.pop_front() {
                self.handle(client, req)?;
            }
            let end = if idle { client.idle(IDLE_MAX, IDLE_SLICE)? } else { client.wait(POLL)? };
            match end {
                IdleEnd::Changed => self.sync(client)?,
                IdleEnd::Expired if !idle => self.sync(client)?,
                IdleEnd::Expired | IdleEnd::Interrupt => {}
                IdleEnd::Slept => return Err(Fail::Net("uyku dönüşü".into())),
            }
        }
    }

    fn handle(&mut self, client: &mut Client<Link>, req: Req) -> Result<(), Fail> {
        match req {
            Req::Refresh => self.sync(client),
            Req::Preview(uid, reply) => match client.preview(uid) {
                Ok(text) => {
                    let _ = reply.send(Ok(text));
                    Ok(())
                }
                Err(Fail::No(_)) => {
                    let _ = reply.send(Err("İleti bulunamadı (silinmiş olabilir).".into()));
                    Ok(())
                }
                Err(e) => {
                    let _ = reply.send(Err("Önizleme alınamadı.".into()));
                    Err(e)
                }
            },
            Req::MarkRead(uid, reply) => match client.mark_seen(uid) {
                Ok(()) => {
                    let _ = reply.send(Ok(()));
                    let id = item_id(self.slot, uid);
                    self.update(|a| {
                        let cats = a.line.cats.iter_mut().flat_map(|c| c.items.iter_mut());
                        a.items.iter_mut().chain(cats).filter(|i| i.id == id).for_each(|i| i.unread = false);
                    });
                    // Satır listede okunmuş olarak kalır; bir sonraki çekişte düşer.
                    self.sync(client)
                }
                Err(Fail::No(_)) => {
                    let _ = reply.send(Err("Okundu yapılamadı.".into()));
                    Ok(())
                }
                Err(e) => {
                    let _ = reply.send(Err("Okundu yapılamadı.".into()));
                    Err(e)
                }
            },
        }
    }

    /// Okunmamışları çeker ve yayınlar. Bu oturumda okundu yapılan son birkaç ileti soluk olarak kalır,
    /// kullanıcı dokunduğu satırın birden kaybolduğunu görmesin.
    fn sync(&mut self, client: &mut Client<Link>) -> Result<(), Fail> {
        let uids = match client.unseen(self.primary) {
            Err(Fail::No(_)) if self.primary => {
                self.primary = false;
                client.unseen(false)?
            }
            other => other?,
        };
        let badge = uids.len() as u32;
        let newest = &uids[uids.len().saturating_sub(MAX_ITEMS)..];
        let mut items: Vec<SourceItem> = client.summaries(newest)?.into_iter().map(|m| self.item(m)).collect();
        let cats = if self.primary { self.categories(client, badge, &items)? } else { Vec::new() };
        self.log(format_args!("{badge} okunmamış"));
        let synced = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
        self.update(|a| {
            a.line.cats = cats;
            a.line.synced_ms = synced;
            let keep = MAX_ITEMS.saturating_sub(items.len()).min(KEEP_READ);
            let stale = a.items.iter().filter(|p| !p.unread && !items.iter().any(|i| i.id == p.id)).take(keep).cloned().collect::<Vec<_>>();
            items.extend(stale);
            items.sort_by_key(|i| std::cmp::Reverse(i.time_ms));
            items.truncate(MAX_ITEMS);
            a.items = items;
            a.line.status = "ok".into();
            a.line.error = None;
            a.line.badge = badge;
        });
        Ok(())
    }

    /// Mail sekmesi: Birincil (zaten çekildi) + Güncellemeler + Tanıtımlar okunmamış sayıları ve her birinin en
    /// yeni `CAT_ITEMS` iletisi (kartta üçü, kategori listesinde hepsi; gövde yalnız açılınca `preview` ile).
    /// Aynı bağlantıda, yalnız değişiklik gelince (IDLE) çalışır. Sunucu kategori
    /// aramasını reddederse o kategori atlanır.
    fn categories(&self, client: &mut Client<Link>, primary: u32, items: &[SourceItem]) -> Result<Vec<MailCat>, Fail> {
        let mut cats = vec![MailCat { key: "primary".into(), unread: primary, items: items.iter().take(CAT_ITEMS).cloned().collect() }];
        for key in MAIL_CATS.into_iter().skip(1) {
            let uids = match client.unseen_in(Some(key)) {
                Err(Fail::No(_)) => continue,
                other => other?,
            };
            let newest = &uids[uids.len().saturating_sub(CAT_ITEMS)..];
            let items = client.summaries(newest)?.into_iter().map(|m| self.item(m)).collect();
            cats.push(MailCat { key: key.into(), unread: uids.len() as u32, items });
        }
        Ok(cats)
    }

    fn item(&self, m: imap::Message) -> SourceItem {
        SourceItem {
            id: item_id(self.slot, m.uid),
            title: if m.from.is_empty() { "(gönderen yok)".into() } else { m.from },
            summary: if m.subject.is_empty() { "(konu yok)".into() } else { m.subject },
            time_ms: m.time_ms,
            unread: !m.seen,
            tag: None,
            url: Some(imap::web_url(&self.address, m.message_id.as_deref())),
            actions: vec!["open".into(), "mark_read".into(), "preview".into()],
            account: Some(self.address.clone()),
        }
    }
}

/// Bağlantı yokken gelen isteği yanıtlar.
fn refuse(req: Req) {
    match req {
        Req::Preview(_, reply) => drop(reply.send(Err(OFFLINE.into()))),
        Req::MarkRead(_, reply) => drop(reply.send(Err(OFFLINE.into()))),
        Req::Refresh => {}
    }
}

/// `imap::Transport`: soket + olay kanalı. İstekler IDLE'ı keser, komut ortasında sıraya alınır.
struct Link {
    rx: Receiver<Ev>,
    conn: u64,
    socket: tls::Socket,
    stash: VecDeque<Req>,
}

impl Transport for Link {
    fn send(&mut self, data: &[u8]) -> Result<(), Fail> {
        self.socket.send(data).map_err(Fail::Net)
    }

    fn recv(&mut self, timeout: Duration, interruptible: bool) -> Result<Recv, Fail> {
        if interruptible && !self.stash.is_empty() {
            return Ok(Recv::Interrupt);
        }
        let until = Instant::now() + timeout;
        loop {
            match self.rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(Ev::Data(conn, chunk)) if conn == self.conn => return chunk.map(Recv::Data).map_err(Fail::Net),
                Ok(Ev::Data(..)) => {}
                Ok(Ev::Req(req)) => {
                    self.stash.push_back(req);
                    if interruptible {
                        return Ok(Recv::Interrupt);
                    }
                }
                Ok(Ev::Stop) | Err(RecvTimeoutError::Disconnected) => return Err(Fail::Stopped),
                Err(RecvTimeoutError::Timeout) => return Ok(Recv::Timeout),
            }
        }
    }
}
