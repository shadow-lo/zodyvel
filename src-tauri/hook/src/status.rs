//! Durum satırı kipi: Claude Code durum satırı girdisindeki plan limitlerini `~/.zodyvel/limits.json`'a
//! yazar, sonra kullanıcının önceki durum satırı komutunu aynı girdiyle çalıştırıp çıktısını basar.
//! Önceki komut yoksa kısa bir limit satırı basar. Ne olursa olsun 0 ile çıkar; hata sessizdir.

use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

use serde_json::Value;
use zodyvel_hook::limits::{self, CHAIN_FLAG, CHAIN_TIMEOUT_SECS, LIMITS_FILE};

const MAX_INPUT: u64 = 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn run(args: &[String]) -> Option<()> {
    let mut input = Vec::new();
    std::io::stdin().take(MAX_INPUT).read_to_end(&mut input).ok()?;
    let json = input.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&input);
    let windows = serde_json::from_slice::<Value>(json).ok().and_then(|p| limits::windows(&p));
    if let Some(windows) = &windows {
        save(windows);
    }

    let chain = args.iter().position(|a| a == CHAIN_FLAG).and_then(|i| args.get(i + 1)).and_then(|h| limits::hex_decode(h));
    let output = match chain {
        Some(command) => run_chained(&command, &input).unwrap_or_default(),
        None => windows.as_ref().map(limits::line).unwrap_or_default().into_bytes(),
    };
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&output).ok()?;
    stdout.flush().ok()
}

fn limits_path() -> PathBuf {
    let home = env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    home.join(".zodyvel").join(LIMITS_FILE)
}

/// Pencereler değiştiyse dosyayı yeniden yazar (geçici dosya + ad değiştirme: okuyan yarım dosya görmez).
fn save(windows: &serde_json::Map<String, Value>) {
    let path = limits_path();
    if fs::read_to_string(&path).is_ok_and(|old| limits::same_windows(&old, windows)) {
        return;
    }
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if fs::write(&tmp, limits::record(windows, now)).is_err() || fs::rename(&tmp, &path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Claude Code'un yaptığı gibi: Git Bash varsa onunla, yoksa PowerShell ile çalıştırır.
fn run_chained(command: &str, input: &[u8]) -> Option<Vec<u8>> {
    let mut process = match git_bash() {
        Some(bash) => {
            let mut c = Command::new(bash);
            c.arg("-c").arg(command);
            c
        }
        None => {
            let mut c = Command::new("powershell.exe");
            c.args(["-NoProfile", "-NonInteractive", "-Command", command]);
            c
        }
    };
    let mut child = process
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let data = input.to_vec();
        thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.by_ref().take(MAX_OUTPUT).read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + Duration::from_secs(CHAIN_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

fn git_bash() -> Option<PathBuf> {
    let existing = |p: PathBuf| p.is_file().then_some(p);
    if let Some(path) = env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from).and_then(existing) {
        return Some(path);
    }
    // PATH'teki git.exe'nin kurulumu: Git\cmd\git.exe → Git\bin\bash.exe.
    for dir in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        if !dir.join("git.exe").is_file() {
            continue;
        }
        let root = dir.parent().unwrap_or(Path::new(""));
        for candidate in [root.join("bin").join("bash.exe"), root.join("usr").join("bin").join("bash.exe"), dir.join("bash.exe")] {
            if let Some(path) = existing(candidate) {
                return Some(path);
            }
        }
    }
    ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"]
        .iter()
        .filter_map(env::var_os)
        .map(|p| PathBuf::from(p).join("Git").join("bin").join("bash.exe"))
        .chain(env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join("Git").join("bin").join("bash.exe")))
        .find_map(existing)
}
