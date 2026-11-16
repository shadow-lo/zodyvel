//! Noise XX el sıkışması ve şifreli çerçeve kanalı (std::net üstünde, 2 baytlık uzunluk ön eki).
use crate::{pairing, MAX_FRAME, NOISE_PARAMS};
use snow::{Builder, HandshakeState, TransportState};
use std::io::{Read, Write};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Noise(snow::Error),
    /// Karşı taraf eşli değil / parmak izi uyuşmuyor.
    Untrusted,
    Frame,
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<snow::Error> for Error {
    fn from(e: snow::Error) -> Self {
        Error::Noise(e)
    }
}

pub struct Keypair {
    pub private: Vec<u8>,
    pub public: Vec<u8>,
}
impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keypair(..)")
    }
}
pub fn generate_keypair() -> Keypair {
    let kp = Builder::new(NOISE_PARAMS.parse().unwrap()).generate_keypair().unwrap();
    Keypair { private: kp.private, public: kp.public }
}

fn write_frame<W: Write>(w: &mut W, data: &[u8]) -> Result<(), Error> {
    if data.len() > MAX_FRAME {
        return Err(Error::Frame);
    }
    w.write_all(&(data.len() as u16).to_be_bytes())?;
    w.write_all(data)?;
    Ok(())
}
fn read_frame<R: Read>(r: &mut R) -> Result<Vec<u8>, Error> {
    let mut l = [0u8; 2];
    r.read_exact(&mut l)?;
    let n = u16::from_be_bytes(l) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

/// Eşleştirilmiş, şifreli kanal.
pub struct Channel<S: Read + Write> {
    io: S,
    t: TransportState,
    /// Karşı tarafın kalıcı açık anahtarı (eşleşme kaydı için).
    pub remote_static: Vec<u8>,
    /// 6 haneli doğrulama kodu.
    pub confirm: String,
    /// El sıkışma özeti: eylem imzaları bu oturuma bağlanır (başka oturumda tekrar oynatılamaz).
    pub handshake_hash: Vec<u8>,
}

impl<S: Read + Write> Channel<S> {
    pub fn send(&mut self, msg: &[u8]) -> Result<(), Error> {
        let mut out = vec![0u8; msg.len() + 16];
        let n = self.t.write_message(msg, &mut out)?;
        write_frame(&mut self.io, &out[..n])
    }
    pub fn recv(&mut self) -> Result<Vec<u8>, Error> {
        let f = read_frame(&mut self.io)?;
        let mut out = vec![0u8; f.len()];
        // Noise sayacı: tekrar oynatılan / sırası bozuk çerçeve burada hata verir.
        let n = self.t.read_message(&f, &mut out)?;
        out.truncate(n);
        Ok(out)
    }
}

impl<S: Read + Write> Channel<S> {
    /// Alttaki akış (ör. zaman aşımı ayarı ya da `peek` için). Windows'ta soket ayarları tanıtıcı başınadır:
    /// okuma zaman aşımı, okumayı yapan bu tanıtıcıda ayarlanmalıdır.
    pub fn io(&self) -> &S {
        &self.io
    }
    /// Kanalı okuyan ve yazan iki yarıya böler (iki ayrı iş parçacığı için). Noise durumu paylaşılır; kilit
    /// yalnız şifreleme/çözme sırasında tutulur, soket beklemesi kilitsizdir. `writer`: aynı soketin kopyası.
    pub fn split<W: Write>(self, writer: W) -> (RecvHalf<S>, SendHalf<W>) {
        let t = std::sync::Arc::new(std::sync::Mutex::new(self.t));
        (RecvHalf { io: self.io, t: t.clone() }, SendHalf { io: writer, t })
    }
}

type SharedTransport = std::sync::Arc<std::sync::Mutex<TransportState>>;

fn lock_t(t: &SharedTransport) -> std::sync::MutexGuard<'_, TransportState> {
    t.lock().unwrap_or_else(|p| p.into_inner())
}

/// Kanalın okuyan yarısı.
pub struct RecvHalf<S: Read> {
    io: S,
    t: SharedTransport,
}
impl<S: Read> RecvHalf<S> {
    pub fn io(&self) -> &S {
        &self.io
    }
    pub fn recv(&mut self) -> Result<Vec<u8>, Error> {
        let f = read_frame(&mut self.io)?;
        let mut out = vec![0u8; f.len()];
        let n = lock_t(&self.t).read_message(&f, &mut out)?;
        out.truncate(n);
        Ok(out)
    }
}

/// Kanalın yazan yarısı (tek yazar: çerçeve sırası Noise sayacıyla aynı kalır).
pub struct SendHalf<W: Write> {
    io: W,
    t: SharedTransport,
}
impl<W: Write> SendHalf<W> {
    pub fn send(&mut self, msg: &[u8]) -> Result<(), Error> {
        let mut out = vec![0u8; msg.len() + 16];
        let n = lock_t(&self.t).write_message(msg, &mut out)?;
        write_frame(&mut self.io, &out[..n])
    }
}

fn finish<S: Read + Write>(io: S, hs: HandshakeState) -> Result<Channel<S>, Error> {
    let hash = hs.get_handshake_hash().to_vec();
    let remote_static = hs.get_remote_static().ok_or(Error::Untrusted)?.to_vec();
    let t = hs.into_transport_mode()?;
    Ok(Channel { io, t, remote_static, confirm: pairing::confirm_code(&hash), handshake_hash: hash })
}

/// Karşı tarafın anahtarına güven kararı.
pub enum Trust<'a> {
    /// Eşleştirme: jeton prologue'da; QR'daki parmak izine (ya da telefonda henüz yok) bakılır.
    PinnedKey(&'a [u8]),
    /// Eşleşmiş cihaz listesi (masaüstü, normal bağlantı).
    Known(&'a [Vec<u8>]),
    /// Masaüstü: eşleştirmede telefonun anahtarı yeni (jeton zaten doğrulandı); normal bağlantıda anahtar el
    /// sıkışmadan sonra eşli listeye bakılarak denetlenir (bilinmeyene yalnız `bye unpaired` gider).
    AcceptNew,
}

fn trusted(t: &Trust, remote: &[u8]) -> bool {
    match t {
        Trust::PinnedKey(k) => *k == remote,
        Trust::Known(list) => list.iter().any(|k| k == remote),
        Trust::AcceptNew => true,
    }
}

/// `prologue`: eşleştirmede QR jetonu; normal bağlantıda boş.
/// Masaüstü (dinleyen) = responder, telefon = initiator.
pub fn accept<S: Read + Write>(
    mut io: S,
    me: &Keypair,
    prologue: &[u8],
    trust: Trust,
) -> Result<Channel<S>, Error> {
    let mut hs = Builder::new(NOISE_PARAMS.parse().unwrap())
        .local_private_key(&me.private)
        .prologue(prologue)
        .build_responder()?;
    let mut buf = vec![0u8; MAX_FRAME];
    let m1 = read_frame(&mut io)?;
    hs.read_message(&m1, &mut buf)?;
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(&mut io, &buf[..n])?;
    let m3 = read_frame(&mut io)?;
    hs.read_message(&m3, &mut buf)?;
    let rs = hs.get_remote_static().ok_or(Error::Untrusted)?.to_vec();
    if !trusted(&trust, &rs) {
        return Err(Error::Untrusted);
    }
    finish(io, hs)
}

pub fn connect<S: Read + Write>(
    mut io: S,
    me: &Keypair,
    prologue: &[u8],
    trust: Trust,
) -> Result<Channel<S>, Error> {
    let mut hs = Builder::new(NOISE_PARAMS.parse().unwrap())
        .local_private_key(&me.private)
        .prologue(prologue)
        .build_initiator()?;
    let mut buf = vec![0u8; MAX_FRAME];
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(&mut io, &buf[..n])?;
    let m2 = read_frame(&mut io)?;
    hs.read_message(&m2, &mut buf)?;
    let rs = hs.get_remote_static().ok_or(Error::Untrusted)?.to_vec();
    if !trusted(&trust, &rs) {
        return Err(Error::Untrusted);
    }
    let n = hs.write_message(&[], &mut buf)?;
    write_frame(&mut io, &buf[..n])?;
    finish(io, hs)
}
