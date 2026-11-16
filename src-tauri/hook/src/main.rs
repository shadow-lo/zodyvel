//! zodyvel-hook.exe — Claude Code / Codex hook köprüsü.
//!
//! Kullanım (ayar dosyasına Zodyvel yazar): `zodyvel-hook.exe --zodyvel-hook-v1 --provider claude --mode interactive`
//! Durum satırı kipi (`--zodyvel-statusline-v1`): Claude plan limitlerini yazar, önceki durum satırını çalıştırır (`status.rs`).
//! 0 ile çıkar; karar yoksa stdout'a hiçbir şey yazmaz (araç normal akışına döner). Tek istisna
//! `--await-task`: adadan görev gelince 2 ile çıkıp Claude'u uyandırır.

// GUI alt sistemi: araç bu süreci başlattığında konsol penceresi açılmaz; stdin/stdout yine
// aracın verdiği borulardan çalışır.
#![windows_subsystem = "windows"]

mod status;
mod window;

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::time::Duration;
use std::{env, process, thread};

use serde_json::{Map, Value};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::FlushFileBuffers;
use windows::Win32::System::Pipes::{GetNamedPipeServerProcessId, WaitNamedPipeW};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{HSTRING, PWSTR};
use zodyvel_hook::{AWAIT_FLAG, AWAIT_TIMEOUT_SECS, Decision, MARKER, Mode, Provider};

const MAX_INPUT: u64 = 8 * 1024 * 1024;
const ERROR_PIPE_BUSY: i32 = 231;

fn main() {
    let _ = run();
    process::exit(0);
}

/// Unix ms (ölçüm damgaları için).
fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn run() -> Option<()> {
    let spawned = now_ms();
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == zodyvel_hook::limits::STATUSLINE_MARKER) {
        return status::run(&args);
    }
    if !args.iter().any(|a| a == MARKER) {
        return None;
    }
    let provider = Provider::parse(arg_value(&args, "--provider")?)?;
    let mode = arg_value(&args, "--mode").and_then(Mode::parse).unwrap_or(Mode::Observe);

    let mut input = Vec::new();
    std::io::stdin().take(MAX_INPUT).read_to_end(&mut input).ok()?;
    // Bazı çağıranlar (ör. UTF-8 BOM'lu konsol kodlamasıyla .NET) stdin'in başına BOM koyar.
    let json = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&input);
    let payload: Value = serde_json::from_slice(json).ok()?;
    let event = payload.get("hook_event_name")?.as_str()?;
    if args.iter().any(|a| a == AWAIT_FLAG) {
        return await_task(provider, mode, &payload, event);
    }
    let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let wants_reply = zodyvel_hook::wants_reply(mode, event, tool);
    // Tur sonu ve araç çağrısında sıradaki not sorulur; Zodyvel yanıtı hemen verir (yoksa bağlantıyı kapatır).
    let wants_note = !wants_reply && zodyvel_hook::wants_note(provider, mode, event, tool);

    // Zodyvel çalışmıyorsa pipe yoktur: hemen, sessizce çık.
    let mut pipe = connect()?;
    if !server_is_zodyvel(&pipe) {
        return None;
    }
    // Ne olursa olsun aracı süresiz bekletme: yanıt beklenmeyen olaylarda 2 sn, onayda karar süresi kadar
    // (plan okumak için daha uzun).
    let limit = if wants_reply {
        zodyvel_hook::wait_limit(event, tool)
    } else if wants_note {
        zodyvel_hook::NOTE_WAIT_SECS
    } else {
        2
    };
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(limit));
        process::exit(0);
    });
    // Ekler kırpılmadan önce, tam olaydan: onaydaki "+N −M" ve "Terminale git" için pencere.
    let mut extra: Map<String, Value> = zodyvel_hook::extras(&payload);
    if zodyvel_hook::wants_window(event, tool)
        && let Some(found) = window::find()
    {
        extra.insert("window".to_owned(), found);
    }
    // "Bitti" gecikmesi ölçümü (Faz 7.2): köprünün başladığı ve gönderdiği an. Zodyvel yalnız tanı
    // kaydı açıkken yazar.
    extra.insert("zodyvel_spawned".to_owned(), Value::from(spawned));
    extra.insert("zodyvel_sent".to_owned(), Value::from(now_ms()));
    if wants_note {
        // Zodyvel notu yalnız yanıtı okuyacak köprüye verir (eski köprüde not kaybolmasın).
        extra.insert("zodyvel_notes".to_owned(), Value::Bool(true));
    }
    pipe.write_all(zodyvel_hook::envelope(provider, mode, &payload, extra).as_bytes()).ok()?;
    if !wants_reply && !wants_note {
        // Sunucu satırı okuyana kadar bekle; okunmadan kapanan pipe'taki veri kaybolur.
        let _ = unsafe { FlushFileBuffers(HANDLE(pipe.as_raw_handle())) };
        return Some(());
    }

    let mut line = String::new();
    BufReader::new(pipe).read_line(&mut line).ok()?;
    let decision = zodyvel_hook::parse_reply(&line)?;
    let output = zodyvel_hook::decision_output(event, &decision, &payload)?;
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes()).ok()?;
    stdout.flush().ok()
}

/// Tur sonunda arka planda adadan görev bekler (`asyncRewake`). Görev gelirse stderr'e yazar ve 2 ile
/// çıkar: Claude uyanıp görevi okur. Yeni tur başlarsa ya da süre dolarsa Zodyvel bağlantıyı boş
/// yanıtla kapatır, köprü 0 ile çıkar.
fn await_task(provider: Provider, mode: Mode, payload: &Value, event: &str) -> Option<()> {
    if provider != Provider::Claude || mode != Mode::Interactive || event != "Stop" {
        return None;
    }
    let mut pipe = connect()?;
    if !server_is_zodyvel(&pipe) {
        return None;
    }
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(AWAIT_TIMEOUT_SECS + 60));
        process::exit(0);
    });
    let mut extra = Map::new();
    extra.insert("zodyvel_await".to_owned(), Value::Bool(true));
    pipe.write_all(zodyvel_hook::envelope(provider, mode, payload, extra).as_bytes()).ok()?;
    let mut line = String::new();
    BufReader::new(pipe).read_line(&mut line).ok()?;
    let Some(Decision::Note(task)) = zodyvel_hook::parse_reply(&line) else { return None };
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(zodyvel_hook::task_message(&task).as_bytes()).ok()?;
    stderr.flush().ok()?;
    process::exit(2);
}

fn arg_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn connect() -> Option<File> {
    let name = zodyvel_hook::current_pipe_name();
    for _ in 0..2 {
        match OpenOptions::new().read(true).write(true).open(&name) {
            Ok(file) => return Some(file),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                let _ = unsafe { WaitNamedPipeW(&HSTRING::from(name.as_str()), 300) };
            }
            Err(_) => return None,
        }
    }
    None
}

/// Pipe'ın karşısındaki süreç gerçekten Zodyvel mi? Başka bir süreç aynı adla pipe açıp
/// sahte "onay" vermesin diye kontrol edilir.
fn server_is_zodyvel(pipe: &File) -> bool {
    let handle = HANDLE(pipe.as_raw_handle());
    let mut pid = 0u32;
    if unsafe { GetNamedPipeServerProcessId(handle, &mut pid) }.is_err() {
        return false;
    }
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else { return false };
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len) }.is_ok();
    let _ = unsafe { CloseHandle(process) };
    if !ok {
        return false;
    }
    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    path.rsplit(['\\', '/']).next().is_some_and(|name| name.eq_ignore_ascii_case("zodyvel.exe"))
}
