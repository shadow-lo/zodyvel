//! Hook köprüsünün bağlandığı named pipe sunucusu.
//!
//! - Pipe yalnız bu Windows kullanıcısına açıktır (DACL) ve ağdan gelen bağlantıları reddeder.
//! - İlk örnek `FILE_FLAG_FIRST_PIPE_INSTANCE` ile açılır: başka bir süreç adı önceden kapmışsa çalışmayız.
//! - Bağlantı bekleme engelleyicidir; boştayken bu iş parçacıkları CPU harcamaz.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, PeekNamedPipe,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};
use zodyvel_hook::{Mode, PROTOCOL_VERSION, Provider};

use super::AgentHub;
use crate::debuglog;

const MAX_LINE: u64 = 1024 * 1024;
const BUFFER: u32 = 64 * 1024;

pub fn spawn(hub: Arc<AgentHub>) {
    let _ = thread::Builder::new().name("zodyvel-hooks".into()).spawn(move || serve(hub));
}

fn serve(hub: Arc<AgentHub>) {
    let Some(security) = UserOnly::new() else {
        eprintln!("Zodyvel: pipe güvenlik tanımı oluşturulamadı; ajan bağlantısı kapalı.");
        return;
    };
    let name = HSTRING::from(zodyvel_hook::current_pipe_name());
    let mut first = true;
    loop {
        let mode = if first { PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE } else { PIPE_ACCESS_DUPLEX };
        let handle = unsafe {
            CreateNamedPipeW(
                &name,
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                Some(&security.attributes),
            )
        };
        if handle.is_invalid() {
            if first {
                eprintln!("Zodyvel: hook pipe'ı açılamadı (başka bir süreç kullanıyor olabilir).");
                return;
            }
            thread::sleep(Duration::from_secs(1));
            continue;
        }
        first = false;
        let result = unsafe { ConnectNamedPipe(handle, None) };
        let connected = result.is_ok() || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !connected {
            debuglog::line(format!("pipe: bağlantı kurulamadı {result:?}"));
            let _ = unsafe { CloseHandle(handle) };
            continue;
        }
        let hub = hub.clone();
        let raw = handle.0 as usize;
        let _ = thread::Builder::new().name("zodyvel-hook-client".into()).spawn(move || client(hub, raw));
    }
}

fn client(hub: Arc<AgentHub>, raw: usize) {
    // Tek tutamak: okuma, yazma ve "bağlantı canlı mı" denetimi aynı açık tutamak üzerinden.
    // `file` fonksiyon sonunda düşünce pipe örneği kapanır.
    let file = unsafe { File::from_raw_handle(raw as RawHandle) };
    let handle = HANDLE(raw as *mut _);
    let mut line = String::new();
    let read = BufReader::new((&file).take(MAX_LINE)).read_line(&mut line);
    debuglog::line(format!("pipe: okundu {:?} etkin={}", read.as_ref().map_err(|e| e.raw_os_error()), hub.enabled()));
    if read.is_err() || !hub.enabled() {
        return;
    }
    let Ok(message) = serde_json::from_str::<Value>(&line) else {
        debuglog::line("pipe: JSON çözülemedi");
        return;
    };
    debuglog::line(format!(
        "pipe: olay {} ({})",
        message["payload"]["hook_event_name"].as_str().unwrap_or("?"),
        message["mode"].as_str().unwrap_or("?")
    ));
    if message.get("v").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
        return;
    }
    let provider = message.get("provider").and_then(Value::as_str).and_then(Provider::parse);
    let mode = message.get("mode").and_then(Value::as_str).and_then(Mode::parse);
    let (Some(provider), Some(mode), Some(payload)) = (provider, mode, message.get("payload")) else { return };

    let Some(pending) = hub.ingest(provider, mode, payload) else { return };
    let deadline = Instant::now() + Duration::from_secs(pending.timeout_secs);
    // Saatlerce süren görev beklemesinde köprü daha seyrek denetlenir (boşta uyanma az olsun).
    let poll = Duration::from_secs(if pending.timeout_secs > 3600 { 5 } else { 1 });
    let decision = loop {
        match pending.receiver.recv_timeout(poll) {
            Ok(decision) => break decision,
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) => {
                if Instant::now() >= deadline || !client_alive(handle) {
                    hub.expire(&pending.id);
                    break None;
                }
            }
        }
    };
    let _ = (&file).write_all(zodyvel_hook::reply(decision.as_ref()).as_bytes());
}

/// Araç köprüyü sonlandırdıysa (kendi zaman aşımı, kesinti) pipe kopar.
fn client_alive(handle: HANDLE) -> bool {
    let mut available = 0u32;
    unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut available), None) }.is_ok()
}

/// Yalnız geçerli kullanıcının erişebildiği güvenlik tanımı.
struct UserOnly {
    attributes: SECURITY_ATTRIBUTES,
    descriptor: PSECURITY_DESCRIPTOR,
}

impl UserOnly {
    fn new() -> Option<Self> {
        let sid = current_user_sid()?;
        let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut descriptor, None) }.ok()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        Some(Self { attributes, descriptor })
    }
}

impl Drop for UserOnly {
    fn drop(&mut self) {
        let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
    }
}

fn current_user_sid() -> Option<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        let mut buffer = vec![0u8; len as usize];
        let ok = GetTokenInformation(token, TokenUser, Some(buffer.as_mut_ptr().cast()), len, &mut len).is_ok();
        let _ = CloseHandle(token);
        if !ok {
            return None;
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut text).ok()?;
        let sid = text.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        sid
    }
}
