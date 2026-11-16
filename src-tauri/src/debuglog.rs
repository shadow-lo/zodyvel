//! Ayrıntılı kayıt: `ZODYVEL_DEBUG=1` ile başlatılınca ya da Hakkında → Sorun bildir'den açılınca
//! `%LOCALAPPDATA%\Zodyvel\debug.log` dosyasına kısa tanı satırları yazar. Varsayılan kapalı; olay içerikleri
//! (komut, dosya yolu, mesaj) yazılmaz. Sorun bildir'den açılış `debug.on` işaret dosyasıyla kalıcıdır ve
//! yeniden başlatma gerektirmez.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static ON: OnceLock<AtomicBool> = OnceLock::new();

/// `%LOCALAPPDATA%\Zodyvel`; kayıt başlarken yoksa oluşturulur.
pub fn dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join("Zodyvel")
}

pub fn path() -> PathBuf {
    dir().join("debug.log")
}

fn marker() -> PathBuf {
    dir().join("debug.on")
}

fn state() -> &'static AtomicBool {
    ON.get_or_init(|| AtomicBool::new(std::env::var("ZODYVEL_DEBUG").as_deref() == Ok("1") || marker().exists()))
}

pub fn enabled() -> bool {
    state().load(Ordering::Relaxed)
}

/// Kaydı bu oturum için ve sonraki açılışlar için aç/kapat.
pub fn set_enabled(on: bool) -> std::io::Result<()> {
    if on {
        fs::create_dir_all(dir())?;
        fs::write(marker(), b"")?;
    } else {
        match fs::remove_file(marker()) {
            Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    state().store(on, Ordering::Relaxed);
    Ok(())
}

pub fn line(text: impl AsRef<str>) {
    if !enabled() {
        return;
    }
    write_row(text.as_ref());
}

fn write_row(text: &str) {
    let row = format!("{} {}\n", crate::agents::now_ms(), text);
    let open = || OpenOptions::new().create(true).append(true).open(path());
    let mut file = match open() {
        Ok(file) => file,
        // Klasör yoksa dosya hiç yazılmıyordu: bir kez oluşturup yeniden dene.
        Err(e) if e.kind() == ErrorKind::NotFound && fs::create_dir_all(dir()).is_ok() => match open() {
            Ok(file) => file,
            Err(_) => return,
        },
        Err(_) => return,
    };
    let _ = file.write_all(row.as_bytes());
}

/// Dosyanın son `lines` satırı; dosya yoksa ya da okunamıyorsa None. Büyük dosyada yalnız sonu okunur.
pub fn tail(lines: usize) -> Option<String> {
    const WINDOW: u64 = 256 * 1024;
    let mut file = fs::File::open(path()).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(WINDOW);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    // Pencere satırın ortasından başlayabilir: ilk (yarım) satırı at.
    let text = if start > 0 { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &text };
    let all: Vec<&str> = text.lines().collect();
    Some(all[all.len().saturating_sub(lines)..].join("\n"))
}
