//! TLS soket: WinRT `StreamSocket` (`SocketProtectionLevel::Tls12`, Windows'un kendi TLS'i ve sertifika
//! doğrulaması; yeni paket yok, exe büyümez). Okuma ayrı bir iş parçacığında sürer ve gelen her parçayı
//! `on_data`'ya verir; böylece IDLE beklerken bir istek (önizleme, durdur) kanaldan araya girebilir.
//! `close` bekleyen okumayı ve bağlantı kurmayı da keser.

use std::thread;

use windows::Networking::HostName;
use windows::Networking::Sockets::{SocketProtectionLevel, StreamSocket};
use windows::Storage::Streams::{Buffer, DataReader, DataWriter, InputStreamOptions};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::HSTRING;

/// Tek okumada en çok bu kadar bayt.
const CHUNK: u32 = 16 * 1024;

/// Okuma iş parçacığından gelen: veri ya da bağlantının bittiği (iç ayrıntı yalnız kayıt için).
pub type Chunk = Result<Vec<u8>, String>;

/// Kurulmakta olan ya da kurulmuş bağlantı. `close` başka bir iş parçacığından da çağrılabilir.
/// (Akış arayüzleri iş parçacıkları arasında taşınamaz; her seferinde soketten istenir.)
#[derive(Clone)]
pub struct Socket {
    socket: StreamSocket,
}

impl Socket {
    pub fn new() -> Result<Self, String> {
        let socket = StreamSocket::new().map_err(code)?;
        if let Ok(control) = socket.Control() {
            let _ = control.SetKeepAlive(true);
            let _ = control.SetNoDelay(true);
        }
        Ok(Self { socket })
    }

    /// Bağlanır (TLS el sıkışması ve sertifika doğrulaması dahil) ve okuma iş parçacığını başlatır.
    pub fn connect(&self, host: &str, port: &str, on_data: impl Fn(Chunk) + Send + 'static) -> Result<(), String> {
        let host = HostName::CreateHostName(&HSTRING::from(host)).map_err(code)?;
        self.socket
            .ConnectWithProtectionLevelAsync(&host, &HSTRING::from(port), SocketProtectionLevel::Tls12)
            .and_then(|op| op.join())
            .map_err(code)?;
        let socket = self.socket.clone();
        thread::Builder::new()
            .name("zodyvel-mail-read".into())
            .spawn(move || {
                let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
                let input = match socket.InputStream() {
                    Ok(input) => input,
                    Err(e) => return on_data(Err(code(e))),
                };
                loop {
                    let read = Buffer::Create(CHUNK).and_then(|buffer| input.ReadAsync(&buffer, CHUNK, InputStreamOptions::Partial)?.join());
                    match read {
                        Ok(filled) => {
                            let len = filled.Length().unwrap_or(0) as usize;
                            if len == 0 {
                                on_data(Err("bağlantı kapandı".into()));
                                return;
                            }
                            let mut bytes = vec![0u8; len];
                            if let Err(e) = DataReader::FromBuffer(&filled).and_then(|r| r.ReadBytes(&mut bytes)) {
                                on_data(Err(code(e)));
                                return;
                            }
                            on_data(Ok(bytes));
                        }
                        Err(e) => {
                            on_data(Err(code(e)));
                            return;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn send(&self, data: &[u8]) -> Result<(), String> {
        let output = self.socket.OutputStream().map_err(code)?;
        let writer = DataWriter::new().map_err(code)?;
        writer.WriteBytes(data).map_err(code)?;
        let buffer = writer.DetachBuffer().map_err(code)?;
        output.WriteAsync(&buffer).and_then(|op| op.join()).map_err(code)?;
        output.FlushAsync().and_then(|op| op.join()).map_err(code)?;
        Ok(())
    }

    /// Bağlantıyı kapatır; bekleyen bağlanma/okuma hatayla biter.
    pub fn close(&self) {
        let _ = self.socket.Close();
    }
}

/// Hata kodu (kayıt için; içerik yok).
fn code(e: windows::core::Error) -> String {
    format!("{:#x}", e.code().0)
}
