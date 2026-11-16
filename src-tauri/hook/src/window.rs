//! Olayı gönderen aracın penceresi: adadaki "Terminale git" bu pencereyi öne getirir.
//!
//! Köprüyü araç (Claude Code, Codex) çalıştırır; araç da bir terminalde (Windows Terminal, konsol,
//! VS Code…) ya da bir masaüstü uygulamasında çalışır. Köprü üst süreç zincirinde yukarı çıkar:
//! 1. Zincirdeki ilk konsola bağlanıp konsol penceresini alır. Windows Terminal'de bu gizli sözde
//!    konsoldur; sahibi Terminal penceresidir (`GA_ROOTOWNER`).
//! 2. Konsol yoksa ya da görünmüyorsa zincirde görünür üst düzey penceresi olan ilk süreç seçilir.
//!
//! Yalnız pencere tutamacı ve uygulamanın exe adı gönderilir. Bulunamazsa hiçbir şey eklenmez.

use std::collections::HashMap;

use serde_json::{Value, json};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM};
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleWindow, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GA_ROOTOWNER, GW_OWNER, GetAncestor, GetWindow, GetWindowThreadProcessId, IsWindowVisible};
use windows::core::BOOL;

/// Üst süreç zincirinde en çok bu kadar yukarı çıkılır.
const MAX_DEPTH: usize = 10;

struct Process {
    parent: u32,
    exe: String,
}

fn processes() -> HashMap<u32, Process> {
    let mut out = HashMap::new();
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return out };
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while ok {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        out.insert(entry.th32ProcessID, Process { parent: entry.th32ParentProcessID, exe: String::from_utf16_lossy(&entry.szExeFile[..len]) });
        ok = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snapshot) };
    out
}

/// Sürecin başlama anı (100 ns birimi); okunamazsa `None`.
fn started(pid: u32) -> Option<u64> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    let ok = unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }.is_ok();
    let _ = unsafe { CloseHandle(process) };
    ok.then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Köprünün üst süreçleri, yakından uzağa. Kapanmış bir üst sürecin numarası yeniden
/// kullanılmış olabilir: çocuğundan sonra başlamış "üst" süreçte zincir kesilir.
fn ancestors(procs: &HashMap<u32, Process>) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut pid = unsafe { GetCurrentProcessId() };
    let mut born = started(pid);
    while chain.len() < MAX_DEPTH {
        let Some(parent) = procs.get(&pid).map(|p| p.parent).filter(|&p| p != 0 && p != pid && !chain.contains(&p)) else { break };
        let parent_born = started(parent);
        if let (Some(child), Some(mine)) = (born, parent_born)
            && mine > child
        {
            break;
        }
        chain.push(parent);
        (pid, born) = (parent, parent_born);
    }
    chain
}

fn visible(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { IsWindowVisible(hwnd) }.as_bool()
}

fn owner_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Zincirdeki ilk konsolun penceresi (ya da sahibi). Bağlanmak köprünün stdin/stdout'unu
/// değiştirebilir: aracın borularına geri dönülür.
fn console_window(chain: &[u32]) -> Option<HWND> {
    let kinds: [STD_HANDLE; 3] = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];
    let saved: Vec<Option<HANDLE>> = kinds.iter().map(|&k| unsafe { GetStdHandle(k) }.ok()).collect();
    let mut found = None;
    for &pid in chain {
        if unsafe { AttachConsole(pid) }.is_err() {
            continue;
        }
        let console = unsafe { GetConsoleWindow() };
        let _ = unsafe { FreeConsole() };
        if !console.is_invalid() {
            let root = unsafe { GetAncestor(console, GA_ROOTOWNER) };
            found = Some(if root.is_invalid() { console } else { root });
        }
        break;
    }
    for (&kind, handle) in kinds.iter().zip(saved) {
        if let Some(handle) = handle {
            let _ = unsafe { SetStdHandle(kind, handle) };
        }
    }
    found.filter(|&hwnd| visible(hwnd))
}

struct Search {
    pid: u32,
    found: HWND,
}

unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    // Sahipsiz (asıl) ve görünür ilk pencere.
    let owned = unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|o| !o.is_invalid());
    if !owned && visible(hwnd) && owner_pid(hwnd) == search.pid {
        search.found = hwnd;
        return false.into();
    }
    true.into()
}

fn top_window(pid: u32) -> Option<HWND> {
    let mut search = Search { pid, found: HWND::default() };
    let _ = unsafe { EnumWindows(Some(each), LPARAM(&mut search as *mut Search as isize)) };
    (!search.found.is_invalid()).then_some(search.found)
}

/// `{ "hwnd": sayı, "app": "WindowsTerminal.exe" }`; bulunamazsa `None`.
pub fn find() -> Option<Value> {
    let procs = processes();
    let chain = ancestors(&procs);
    let hwnd = console_window(&chain).or_else(|| chain.iter().find_map(|&pid| top_window(pid)))?;
    let app = procs.get(&owner_pid(hwnd)).map(|p| p.exe.clone()).unwrap_or_default();
    Some(json!({ "hwnd": hwnd.0 as usize as u64, "app": app }))
}
