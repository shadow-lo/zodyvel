//! Git'i çalıştırma: `git.exe`'yi bulma, zaman sınırlı çalıştırma, durum okuma, commit ve push.
//! Tauri'ye bağlı değil; Linux'ta gerçek git ile geçici depolarda sınanır.
//!
//! Güvenlik: kabuk yok, her argüman ayrı verilir; yollar `--pathspec-from-file` ile girdiden ve
//! `--literal-pathspecs` ile (joker karakter yorumlanmaz). Soru penceresi açılmaz (`GIT_TERMINAL_PROMPT=0`);
//! giriş gerekirse git hata verir. Yalnız `status`, `add`, `commit`, `push` (zorlamasız, dalın kendi uzak
//! dalına), `fetch`, yalnız ileri saran pull, `restore` (geri al; dosya önce Geri Dönüşüm Kutusu'na) ve
//! okuma komutları; reset, checkout, rebase, birleştirme commit'i yok.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::core::{self, Status};

pub const STATUS_TIMEOUT: Duration = Duration::from_secs(20);
pub const COMMIT_TIMEOUT: Duration = Duration::from_secs(90);
pub const PUSH_TIMEOUT: Duration = Duration::from_secs(120);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(120);

/// Bilgisayardaki `git.exe`: PATH, Git for Windows'un olağan yeri, sonra GitHub Desktop'un içindeki git.
pub fn find_git() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if !cfg!(windows) {
        return None;
    }
    for var in ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else { continue };
        let candidate = if var == "LOCALAPPDATA" { base.join("Programs").join("Git").join("cmd").join(exe) } else { base.join("Git").join("cmd").join(exe) };
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let desktop = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("GitHubDesktop");
    let mut apps: Vec<PathBuf> = std::fs::read_dir(&desktop)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("app-")))
        .collect();
    // En yeni sürüm (app-3.4.9 > app-3.4.10 sözlük sırasıyla değil, sayılarla).
    apps.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().trim_start_matches("app-").split('.').map(|x| x.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>()));
    apps.into_iter().rev().map(|a| a.join("resources").join("app").join("git").join("cmd").join(exe)).find(|p| p.is_file())
}

pub struct Output {
    pub ok: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// `git <args>` deponun klasöründe. `input` girdiye yazılır. Süre dolarsa süreç öldürülür.
pub fn run(git: &Path, repo: &Path, args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Result<Output, String> {
    run_env(git, repo, args, input, timeout, &[])
}

/// `run`, ek ortam değişkenleriyle (GitHub girişi için, bkz. `core::auth_env`).
pub fn run_env(git: &Path, repo: &Path, args: &[&str], input: Option<&[u8]>, timeout: Duration, env: &[(String, String)]) -> Result<Output, String> {
    let mut command = Command::new(git);
    command.envs(env.iter().map(|(k, v)| (k, v)));
    command
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never", "--literal-pathspecs"])
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("LANGUAGE", "C")
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Konsol penceresi açılmasın.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|_| "Git başlatılamadı.".to_string())?;
    if let (Some(bytes), Some(mut stdin)) = (input, child.stdin.take()) {
        let bytes = bytes.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut bytes);
        }
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(e) = err.as_mut() {
            let _ = e.take(64 * 1024).read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Git zamanında bitmedi; işlem durduruldu.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => return Err("Git beklenirken hata oldu.".into()),
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    Ok(Output { ok: status.success(), stdout, stderr })
}

fn checked(out: Output) -> Result<Output, String> {
    if out.ok { Ok(out) } else { Err(core::explain(&out.stderr)) }
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Deponun durumu; büyük dosyalar (50 MB üstü) işaretlenir.
pub fn status(git: &Path, repo: &Path) -> Result<Status, String> {
    let out = checked(run(git, repo, &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"], None, STATUS_TIMEOUT)?)?;
    let mut st = core::parse_status(&out.stdout);
    for f in &mut st.files {
        f.big = f.kind != "deleted" && std::fs::metadata(repo.join(&f.path)).is_ok_and(|m| m.len() > core::BIG_FILE);
    }
    Ok(st)
}

/// Birleştirme, rebase ya da cherry-pick ortasında mı (o an commit GitHub Desktop'a bırakılır).
fn in_progress(git: &Path, repo: &Path) -> bool {
    let Ok(out) = run(git, repo, &["rev-parse", "--absolute-git-dir"], None, STATUS_TIMEOUT) else { return false };
    let dir = PathBuf::from(text(&out));
    ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply"].iter().any(|n| dir.join(n).exists())
}

/// Seçilen dosyaları commitler (başka hazırlanmış değişiklik commit'e girmez, hazırlıkta kalır).
/// Dönüş: kısa commit kimliği.
pub fn commit(git: &Path, repo: &Path, message: &str, picked: &[String], allow_sensitive: &[String]) -> Result<String, String> {
    let message = core::clean_message(message)?;
    if in_progress(git, repo) {
        return Err("Depoda yarım kalmış birleştirme ya da rebase var; önce GitHub Desktop'ta bitir.".into());
    }
    let st = status(git, repo)?;
    let (add, paths) = core::plan_commit(&st, picked, allow_sensitive)?;
    if !add.is_empty() {
        checked(run(git, repo, &["add", "--pathspec-from-file=-", "--pathspec-file-nul"], Some(&core::pathspec(&add)), COMMIT_TIMEOUT)?)?;
    }
    // Yollarla commit: yalnız bu yolların çalışma ağacındaki hâli girer (`--only`).
    checked(run(git, repo, &["commit", "--quiet", "--only", "-m", &message, "--pathspec-from-file=-", "--pathspec-file-nul"], Some(&core::pathspec(&paths)), COMMIT_TIMEOUT)?)?;
    let head = checked(run(git, repo, &["rev-parse", "--short", "HEAD"], None, STATUS_TIMEOUT)?)?;
    Ok(text(&head))
}

/// Seçilen dosyaların değişikliklerini geri alır (discard). Her dosyanın şimdiki hâli önce Geri Dönüşüm
/// Kutusu'na taşınır (kalıcı silme yok); sonra izlenen yollar son commit'teki hâline döner. Kutuya
/// taşınamayan dosyaya dokunulmaz ve iş orada durur. Dönüş: geri alınan dosya sayısı.
pub fn discard(git: &Path, repo: &Path, picked: &[String]) -> Result<usize, String> {
    if in_progress(git, repo) {
        return Err("Depoda yarım kalmış birleştirme ya da rebase var; önce GitHub Desktop'ta bitir.".into());
    }
    let st = status(git, repo)?;
    // Önce tüm seçimi denetle (geçersiz yol varsa hiçbir şey değişmez).
    core::plan_discard(&st, picked)?;
    let mut restore = Vec::new();
    let mut done = 0;
    let mut failed = None;
    for p in picked {
        let plan = core::plan_discard(&st, std::slice::from_ref(p))?;
        let moved = plan.trash.iter().try_for_each(|t| {
            let file = repo.join(t.replace('/', std::path::MAIN_SEPARATOR_STR));
            if file.symlink_metadata().is_err() { Ok(()) } else { to_trash(&file) }
        });
        if let Err(e) = moved {
            failed = Some(format!("{p}: {e}"));
            break;
        }
        restore.extend(plan.restore);
        done += 1;
    }
    if !restore.is_empty() {
        let args: &[&str] = if st.initial {
            &["rm", "--cached", "--quiet", "--ignore-unmatch", "--pathspec-from-file=-", "--pathspec-file-nul"]
        } else {
            &["restore", "--source=HEAD", "--staged", "--worktree", "--pathspec-from-file=-", "--pathspec-file-nul"]
        };
        checked(run(git, repo, args, Some(&core::pathspec(&restore)), COMMIT_TIMEOUT)?)?;
    }
    match failed {
        Some(e) if done == 0 => Err(format!("Geri Dönüşüm Kutusu'na taşınamadı, hiçbir şey değişmedi. {e}")),
        Some(e) => Err(format!("{done} dosya geri alındı; sonrakilere dokunulmadı. Taşınamayan: {e}")),
        None => Ok(done),
    }
}

/// Dosyayı Geri Dönüşüm Kutusu'na taşır.
#[cfg(windows)]
fn to_trash(file: &Path) -> Result<(), String> {
    if crate::win::recycle(file) { Ok(()) } else { Err("Geri Dönüşüm Kutusu'na taşınamadı (dosya açık olabilir).".into()) }
}

/// Windows dışı (yalnız testler): geçici bir "çöp" klasörüne taşır.
#[cfg(not(windows))]
fn to_trash(file: &Path) -> Result<(), String> {
    let bin = std::env::temp_dir().join(format!("zodyvel-trash-{}", std::process::id()));
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = format!("{stamp}-{}", file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    std::fs::rename(file, bin.join(name)).map_err(|e| e.to_string())
}

/// GitHub'daki durumu indirir (dosyalar değişmez); ardından "↓n GitHub'da yeni" görünür.
pub fn fetch(git: &Path, repo: &Path, token: Option<&str>) -> Result<(), String> {
    net(git, repo, &["fetch", "--quiet", "--all", "--no-recurse-submodules"], token, FETCH_TIMEOUT).map(|_| ())
}

/// GitHub'daki yeni commitleri alır, yalnız ileri sararak (fast-forward): birleştirme commit'i yapmaz,
/// commitlenmemiş değişikliğin üzerine yazmaz (öyleyse git durur, hiçbir şey değişmez).
pub fn pull(git: &Path, repo: &Path, token: Option<&str>) -> Result<(), String> {
    if in_progress(git, repo) {
        return Err("Depoda yarım kalmış birleştirme ya da rebase var; önce GitHub Desktop'ta bitir.".into());
    }
    let (remote, _) = push_target(git, repo)?;
    net(git, repo, &["fetch", "--quiet", "--no-recurse-submodules", "--", &remote], token, FETCH_TIMEOUT)?;
    checked(run(git, repo, &["merge", "--ff-only", "--quiet", "@{upstream}"], None, COMMIT_TIMEOUT)?).map(|_| ())
}

/// Gönderilecek yer: dalın izlediği uzak ve uzak dal. Yoksa dal hiç yayınlanmamış.
fn push_target(git: &Path, repo: &Path) -> Result<(String, String), String> {
    let branch = run(git, repo, &["symbolic-ref", "--quiet", "--short", "HEAD"], None, STATUS_TIMEOUT)?;
    let branch = text(&branch);
    if branch.is_empty() {
        return Err("Şu an bir dalda değilsin (ayrık HEAD); GitHub Desktop'ta bir dala geç.".into());
    }
    let get = |key: String| run(git, repo, &["config", "--get", &key], None, STATUS_TIMEOUT).map(|o| text(&o));
    let remote = get(format!("branch.{branch}.remote"))?;
    let merge = get(format!("branch.{branch}.merge"))?;
    if remote.is_empty() || remote == "." || !merge.starts_with("refs/heads/") {
        return Err("Bu dal GitHub'da yok. GitHub Desktop'ta bir kez \"Publish branch\" de.".into());
    }
    Ok((remote, merge))
}

/// Dalı kendi uzak dalına gönderir; zorlama yok (GitHub'da yeni commit varsa reddedilir).
pub fn push(git: &Path, repo: &Path, token: Option<&str>) -> Result<(), String> {
    let (remote, merge) = push_target(git, repo)?;
    let refspec = format!("HEAD:{merge}");
    net(git, repo, &["push", "--porcelain", "--", &remote, &refspec], token, PUSH_TIMEOUT).map(|_| ())
}

/// Ağ komutu (push, fetch): önce Zodyvel'e bağlı GitHub tokenıyla; giriş hatası olursa (ya da token yoksa)
/// bilgisayarın kendi git girişiyle (Git Credential Manager vb.).
fn net(git: &Path, repo: &Path, args: &[&str], token: Option<&str>, timeout: Duration) -> Result<Output, String> {
    let env = core::auth_env(token);
    if !env.is_empty() {
        let out = run_env(git, repo, args, None, timeout, &env)?;
        if out.ok || !core::auth_failed(&out.stderr) {
            return checked(out);
        }
    }
    checked(run(git, repo, args, None, timeout)?)
}

/// Depo uzak adreslerinden GitHub "sahip/depo" (ilk bulunan).
pub fn github_remote(git: &Path, repo: &Path) -> Option<String> {
    let out = run(git, repo, &["remote", "-v"], None, STATUS_TIMEOUT).ok()?;
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split_whitespace().nth(1)).find_map(crate::sources::github::api::github_slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_NOSYSTEM", "1").status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zodyvel-exec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Uzak (bare) depo + klon; klonda bir ilk commit gönderilmiş.
    fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp(name);
        let remote = root.join("remote.git");
        let work = root.join("work dir");
        sh(&root, &["init", "--quiet", "--bare", "-b", "main", remote.to_str().unwrap()]);
        sh(&root, &["clone", "--quiet", remote.to_str().unwrap(), work.to_str().unwrap()]);
        sh(&work, &["config", "user.name", "Test"]);
        sh(&work, &["config", "user.email", "t@example.com"]);
        // Git for Windows sistem ayarı (autocrlf) çıkışta \r\n yazar; içerik karşılaştırmaları için kapalı.
        sh(&work, &["config", "core.autocrlf", "false"]);
        sh(&work, &["checkout", "--quiet", "-b", "main"]);
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        sh(&work, &["add", "a.txt"]);
        sh(&work, &["commit", "--quiet", "-m", "ilk"]);
        sh(&work, &["push", "--quiet", "-u", "origin", "main"]);
        (root, remote, work)
    }

    #[test]
    fn commit_only_picked_files_then_push() {
        let git = find_git().expect("git");
        let (root, remote, work) = setup("commit");
        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        // Windows dosya adında * kabul etmez; [1] yine de yol-desen (pathspec) karakteri.
        let odd = if cfg!(windows) { "b [1].txt" } else { "b [1]*.txt" };
        std::fs::create_dir_all(work.join("yeni klasör")).unwrap();
        std::fs::write(work.join("yeni klasör").join(odd), "b\n").unwrap();
        std::fs::write(work.join(".env"), "TOKEN=x\n").unwrap();
        std::fs::write(work.join("c.txt"), "c\n").unwrap();
        // Kullanıcının elle hazırladığı (staged) başka bir dosya commit'e girmemeli.
        std::fs::write(work.join("d.txt"), "d\n").unwrap();
        sh(&work, &["add", "d.txt"]);

        let st = status(&git, &work).unwrap();
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        let env = st.files.iter().find(|f| f.path == ".env").unwrap();
        assert!(env.sensitive && env.kind == "untracked");

        let picked = vec!["a.txt".to_string(), format!("yeni klasör/{odd}"), ".env".to_string()];
        assert!(commit(&git, &work, "deneme", &picked, &[]).is_err(), "hassas dosya onaysız gitmez");
        let picked = &picked[..2];
        let sha = commit(&git, &work, "  Düzeltme: ada\r\n", picked, &[]).unwrap();
        assert!(sha.len() >= 7);

        let st = status(&git, &work).unwrap();
        let left: Vec<_> = st.files.iter().map(|f| f.path.as_str()).collect();
        assert!(left.contains(&".env") && left.contains(&"c.txt") && left.contains(&"d.txt"), "{left:?}");
        assert!(!left.contains(&"a.txt"));
        assert_eq!(st.ahead, 1);
        let shown = Command::new("git").args(["log", "-1", "--format=%s", "--name-only"]).current_dir(&work).output().unwrap();
        let shown = String::from_utf8_lossy(&shown.stdout).into_owned();
        assert!(shown.starts_with("Düzeltme: ada\n") && shown.contains(odd) && !shown.contains("d.txt"), "{shown}");

        push(&git, &work, None).unwrap();
        assert_eq!(status(&git, &work).unwrap().ahead, 0);
        let remote_head = Command::new("git").args(["rev-parse", "--short", "main"]).current_dir(&remote).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&remote_head.stdout).trim(), sha);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn push_never_forces_and_unpublished_branch_is_refused() {
        let git = find_git().expect("git");
        let (root, remote, work) = setup("push");
        // Başka bir yerden uzak dala commit gelmiş.
        let other = root.join("other");
        sh(&root, &["clone", "--quiet", remote.to_str().unwrap(), other.to_str().unwrap()]);
        sh(&other, &["-c", "user.name=O", "-c", "user.email=o@x", "commit", "--quiet", "--allow-empty", "-m", "uzak"]);
        sh(&other, &["push", "--quiet"]);
        std::fs::write(work.join("a.txt"), "yerel\n").unwrap();
        commit(&git, &work, "yerel", &["a.txt".into()], &[]).unwrap();
        let err = push(&git, &work, None).unwrap_err();
        assert!(err.contains("Pull"), "{err}");

        sh(&work, &["checkout", "--quiet", "-b", "yeni-dal"]);
        let err = push(&git, &work, None).unwrap_err();
        assert!(err.contains("Publish"), "{err}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn discard_moves_to_trash_and_restores_head() {
        let git = find_git().expect("git");
        let (root, _remote, work) = setup("discard");
        std::fs::write(work.join("a.txt"), "değişti\n").unwrap();
        std::fs::write(work.join("b.txt"), "b\n").unwrap();
        std::fs::write(work.join("yeni.txt"), "y\n").unwrap();
        sh(&work, &["add", "yeni.txt"]);
        std::fs::write(work.join("kalsın.txt"), "k\n").unwrap();
        assert!(discard(&git, &work, &["yok.txt".into()]).is_err());
        let n = discard(&git, &work, &["a.txt".into(), "b.txt".into(), "yeni.txt".into()]).unwrap();
        assert_eq!(n, 3);
        assert_eq!(std::fs::read_to_string(work.join("a.txt")).unwrap(), "1\n");
        assert!(!work.join("b.txt").exists() && !work.join("yeni.txt").exists());
        let left: Vec<_> = status(&git, &work).unwrap().files.into_iter().map(|f| f.path).collect();
        assert_eq!(left, vec!["kalsın.txt".to_string()]);
        // Windows'ta gerçek Geri Dönüşüm Kutusu kullanılır; geçici çöp klasörü yalnız diğer sistemlerde.
        if !cfg!(windows) {
            let bin = std::env::temp_dir().join(format!("zodyvel-trash-{}", std::process::id()));
            let kept: Vec<String> = std::fs::read_dir(&bin).unwrap().flatten().map(|e| std::fs::read_to_string(e.path()).unwrap()).collect();
            assert!(kept.contains(&"değişti\n".to_string()) && kept.contains(&"b\n".to_string()), "{kept:?}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn fetch_then_fast_forward_pull() {
        let git = find_git().expect("git");
        let (root, remote, work) = setup("pull");
        let other = root.join("other");
        sh(&root, &["clone", "--quiet", remote.to_str().unwrap(), other.to_str().unwrap()]);
        std::fs::write(other.join("c.txt"), "uzak\n").unwrap();
        sh(&other, &["add", "c.txt"]);
        sh(&other, &["-c", "user.name=O", "-c", "user.email=o@x", "commit", "--quiet", "-m", "uzak"]);
        sh(&other, &["push", "--quiet"]);
        // Commitlenmemiş başka bir değişiklik çekmeyi engellemez ve korunur.
        std::fs::write(work.join("a.txt"), "yerel\n").unwrap();
        fetch(&git, &work, None).unwrap();
        assert_eq!(status(&git, &work).unwrap().behind, 1);
        pull(&git, &work, None).unwrap();
        let st = status(&git, &work).unwrap();
        assert_eq!((st.behind, st.ahead), (0, 0));
        assert_eq!(std::fs::read_to_string(work.join("c.txt")).unwrap(), "uzak\n");
        assert_eq!(std::fs::read_to_string(work.join("a.txt")).unwrap(), "yerel\n");

        // Ayrışmış dal: birleştirme yapılmaz.
        std::fs::write(other.join("c.txt"), "uzak 2\n").unwrap();
        sh(&other, &["-c", "user.name=O", "-c", "user.email=o@x", "commit", "--quiet", "-am", "uzak 2"]);
        sh(&other, &["push", "--quiet"]);
        commit(&git, &work, "yerel", &["a.txt".into()], &[]).unwrap();
        let err = pull(&git, &work, None).unwrap_err();
        assert!(err.contains("ayrışmış"), "{err}");
        let st = status(&git, &work).unwrap();
        assert_eq!((st.ahead, st.behind), (1, 1));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn identity_and_github_remote() {
        let git = find_git().expect("git");
        let (root, _remote, work) = setup("ident");
        assert_eq!(github_remote(&git, &work), None);
        sh(&work, &["remote", "add", "gh", "https://github.com/Ornek/Zodyvel.git"]);
        assert_eq!(github_remote(&git, &work).as_deref(), Some("ornek/zodyvel"));
        let st = status(&git, &work).unwrap();
        assert!(st.files.is_empty() && st.ahead == 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn token_reaches_git_only_as_config() {
        let git = find_git().expect("git");
        let (root, _remote, work) = setup("token");
        let env = core::auth_env(Some("ghp_test"));
        let out = run_env(&git, &work, &["config", "--get", "http.https://github.com/.extraheader"], None, STATUS_TIMEOUT, &env).unwrap();
        assert_eq!(text(&out), env[2].1);
        let plain = run(&git, &work, &["config", "--get", "http.https://github.com/.extraheader"], None, STATUS_TIMEOUT).unwrap();
        assert!(!plain.ok, "token kalıcı ayara yazılmaz");
        // Yerel (github.com olmayan) uzakta token işi bozmaz.
        fetch(&git, &work, Some("ghp_test")).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
