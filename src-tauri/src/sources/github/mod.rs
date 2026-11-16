//! GitHub kaynağı: kırık CI (kendi açık PR'larım ve son pushlanan depoların ana dalı), inceleme istekleri ve
//! okunmamış bildirimler. Klasik kişisel token (kapsam `notifications` + `repo`) Kimlik Bilgisi
//! Yöneticisi'nden okunur, yalnız `api.github.com`'a TLS ile gider (`http.rs`).
//!
//! Yoklama tek iş parçacığında: bildirimler `If-Modified-Since` ile (değişiklik yoksa 304, hız sınırından
//! düşmez) ve GitHub'ın `X-Poll-Interval` aralığıyla (en az 60 sn); PR/CI özeti tek GraphQL sorgusuyla 5 dk'da
//! bir, CI sürerken 1 dk'da bir. Ağ hatasında 30 sn → 1 dk → 5 dk geri çekilir; token geçersizse "Yenile"
//! ya da yeni token beklenir. `stop` iş parçacığını uyandırır, döngü hemen çıkar ve bellek boşalır. Kayda
//! yalnız sayılar (kalan istek hakkı, satır sayısı) ve durum kodları yazılır.

pub(crate) mod api;
mod http;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::debuglog;
use crate::sources::{Source, SourceState, publish};
use api::{Board, FailedRun, Notification, Summary};

const ID: &str = "github";
const TOKEN_KEY: &str = "github-token";
/// Ağ hatasında sırayla beklenen süreler.
const BACKOFF: [u64; 3] = [30, 60, 300];

#[derive(Default)]
struct Shared {
    /// Her `start` yeni nesil açar; eski iş parçacığı nesli değişince çıkar ve yayın yapmaz.
    generation: u64,
    running: bool,
    /// "Yenile" ya da bir eylemden sonra hemen yeniden sor.
    poke: bool,
    token: Option<String>,
    login: String,
    summary: Summary,
    notifications: Vec<Notification>,
    board: Board,
    /// Kırık CI satırı → eylem bilgisi.
    runs: BTreeMap<String, FailedRun>,
    /// Klonlu depolar: "sahip/depo" → klasör.
    clones: BTreeMap<String, String>,
    /// Son yayınlanan durum (satır bağlantıları için).
    state: Option<SourceState>,
    app: Option<AppHandle>,
}

struct Github {
    shared: Mutex<Shared>,
    wake: Condvar,
}

impl Github {
    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn current(&self, generation: u64) -> bool {
        let s = self.lock();
        s.running && s.generation == generation
    }

    /// Bu nesil hâlâ geçerliyse yayınlar.
    fn publish_if(&self, generation: u64, state: SourceState) {
        let mut s = self.lock();
        if !(s.running && s.generation == generation) {
            return;
        }
        s.state = Some(state.clone());
        drop(s);
        publish(state);
    }

    fn fail(&self, generation: u64, message: &str) {
        let account = {
            let s = self.lock();
            (!s.login.is_empty()).then(|| s.login.clone())
        };
        self.publish_if(generation, SourceState { error: Some(message.into()), account, ..SourceState::idle(ID, "error") });
    }

    /// `timeout` kadar ya da durdurulana / dürtülene kadar bekler. Nesil geçersizse `false`.
    fn sleep(&self, generation: u64, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut s = self.lock();
        loop {
            if !(s.running && s.generation == generation) {
                return false;
            }
            if s.poke {
                s.poke = false;
                return true;
            }
            match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return true;
                    }
                    s = self.wake.wait_timeout(s, d - now).map(|(g, _)| g).unwrap_or_else(|p| p.into_inner().0);
                }
                None => s = self.wake.wait(s).unwrap_or_else(|p| p.into_inner()),
            }
        }
    }

    fn token(&self) -> Result<String, String> {
        self.lock().token.clone().ok_or_else(|| "GitHub bağlı değil.".to_string())
    }

    /// Bellekteki satırlardan durumu yeniden kurar ve yayınlar (eylemden sonra, ağsız).
    fn rebuild(&self, generation: u64) {
        let state = {
            let mut s = self.lock();
            let clones = s.clones.clone();
            let (state, runs) = api::build(&s.login, &s.summary, &s.notifications, &s.board, &|repo| clones.contains_key(&repo.to_lowercase()));
            s.runs = runs;
            state
        };
        self.publish_if(generation, state);
    }

    fn poll(self: Arc<Self>, app: AppHandle, generation: u64) {
        let Ok(token) = self.token() else { return };
        if let Some(problem) = api::token_problem(&token) {
            self.fail(generation, problem);
            return;
        }
        let client = match http::Client::new() {
            Ok(c) => c,
            Err(e) => return self.fail(generation, &e),
        };
        let mut last_modified: Option<String> = None;
        let mut notif_secs = api::MIN_POLL_SECS;
        let mut next_notif = Instant::now();
        let mut next_summary = Instant::now();
        let mut failures = 0usize;
        let mut first = true;

        loop {
            if !self.current(generation) {
                return;
            }
            let now = Instant::now();
            let mut result: Result<(), String> = Ok(());

            if now >= next_notif {
                match client.send("GET", "/notifications?per_page=30", &token, None, last_modified.as_deref()) {
                    Ok(r) if r.status == 304 => next_notif = now + Duration::from_secs(notif_secs),
                    Ok(r) if r.status == 200 => {
                        if first && let Some(problem) = api::scopes_problem(r.scopes.as_deref()) {
                            self.fail(generation, problem);
                            if !self.sleep(generation, None) {
                                return;
                            }
                            continue;
                        }
                        match api::parse_notifications(&r.body) {
                            Ok(list) => {
                                self.lock().notifications = list;
                                last_modified = r.last_modified;
                            }
                            Err(e) => result = Err(e),
                        }
                        notif_secs = r.poll_interval.unwrap_or(api::MIN_POLL_SECS).max(api::MIN_POLL_SECS);
                        next_notif = now + Duration::from_secs(notif_secs);
                        if let Some(left) = r.rate_remaining {
                            debuglog::line(format!("github: bildirim 200, kalan hak {left}"));
                        }
                    }
                    Ok(r) if r.status == 401 => {
                        self.fail(generation, &api::status_error(401));
                        // Token düzelene kadar istek yok: yeni token ya da "Yenile" bekler.
                        if !self.sleep(generation, None) {
                            return;
                        }
                        last_modified = None;
                        first = true;
                        continue;
                    }
                    Ok(r) => result = Err(api::status_error(r.status)),
                    Err(e) => result = Err(e),
                }
            }

            if result.is_ok() && now >= next_summary {
                match client.send("POST", "/graphql", &token, Some(&api::query_body()), None) {
                    Ok(r) if r.status == 200 => match api::parse_summary(&r.body) {
                        Ok(summary) => {
                            let busy = summary.busy();
                            let clones = clones(&app, &summary);
                            let mut s = self.lock();
                            s.board.track(&summary, now_ms());
                            s.login = summary.login.clone();
                            s.summary = summary;
                            s.clones = clones;
                            drop(s);
                            let secs = if busy { api::SUMMARY_BUSY_SECS } else { api::SUMMARY_SECS };
                            next_summary = now + Duration::from_secs(secs);
                            if let Some(left) = r.rate_remaining {
                                debuglog::line(format!("github: özet 200, kalan hak {left}"));
                            }
                        }
                        Err(e) => result = Err(e),
                    },
                    Ok(r) => result = Err(api::status_error(r.status)),
                    Err(e) => result = Err(e),
                }
            }

            let wait = match result {
                Ok(()) => {
                    failures = 0;
                    first = false;
                    self.lock().board.expire(now_ms());
                    self.rebuild(generation);
                    let next = next_notif.min(next_summary);
                    next.saturating_duration_since(Instant::now()).max(Duration::from_secs(1))
                }
                Err(e) => {
                    debuglog::line("github: istek başarısız");
                    self.fail(generation, &e);
                    let secs = BACKOFF[failures.min(BACKOFF.len() - 1)];
                    failures += 1;
                    next_notif = Instant::now() + Duration::from_secs(secs);
                    next_summary = next_summary.max(next_notif);
                    Duration::from_secs(secs)
                }
            };
            if !self.sleep(generation, Some(wait)) {
                return;
            }
            // Dürtülünce (Yenile, eylem) ikisi de hemen sorulur; bildirimler yine koşullu.
            let now = Instant::now();
            if now < next_notif.min(next_summary) {
                next_notif = now;
                next_summary = now;
            }
        }
    }

    fn open(&self, item_id: &str) -> Result<(), String> {
        let url = {
            let s = self.lock();
            s.state.as_ref().and_then(|st| st.items.iter().find(|i| i.id == item_id)).and_then(|i| i.url.clone())
        };
        let url = url.and_then(|u| api::safe_url(&u)).ok_or("Bağlantı bulunamadı.")?;
        if crate::win::shell_open(&url) { Ok(()) } else { Err("Tarayıcı açılamadı.".into()) }
    }

    /// "Depolarım"daki depo: yalnız son yayınlanan listede olan depo açılır.
    fn open_repo(&self, slug: &str) -> Result<(), String> {
        let url = {
            let s = self.lock();
            s.state.as_ref().and_then(|st| st.repos.iter().find(|r| r.slug == slug)).map(|r| r.url.clone())
        };
        let url = url.and_then(|u| api::safe_url(&u)).ok_or("Depo bulunamadı.")?;
        if crate::win::shell_open(&url) { Ok(()) } else { Err("Tarayıcı açılamadı.".into()) }
    }

    fn mark_read(&self, item_id: &str) -> Result<(), String> {
        let thread = item_id.strip_prefix("n:").filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())).ok_or("Bu satır bir bildirim değil.")?;
        let token = self.token()?;
        let r = http::Client::new()?.send("PATCH", &format!("/notifications/threads/{thread}"), &token, None, None)?;
        if !matches!(r.status, 200 | 205 | 304) {
            return Err(api::status_error(r.status));
        }
        let generation = {
            let mut s = self.lock();
            s.notifications.retain(|n| n.thread_id != thread);
            s.generation
        };
        self.rebuild(generation);
        Ok(())
    }

    fn rerun(&self, item_id: &str) -> Result<(), String> {
        let run = self.lock().runs.get(item_id).cloned().ok_or("Bu iş artık kırık değil.")?;
        let run_id = run.run_id.ok_or("Bu iş GitHub Actions'ta değil; yeniden çalıştırılamaz.")?;
        if !valid_repo(&run.repo) {
            return Err("Geçersiz depo.".into());
        }
        let token = self.token()?;
        let r = http::Client::new()?.send("POST", &format!("/repos/{}/actions/runs/{run_id}/rerun-failed-jobs", run.repo), &token, None, None)?;
        if !matches!(r.status, 201 | 200 | 204) {
            return Err(match r.status {
                403 => "Yeniden çalıştırma izni yok (tokenda \"repo\" kapsamı gerekli).".into(),
                s => api::status_error(s),
            });
        }
        // CI yeniden sürüyor: özet hemen sorulsun (sonra 1 dk aralıkla).
        self.lock().poke = true;
        self.wake.notify_all();
        Ok(())
    }

    fn fix_with_claude(&self, item_id: &str) -> Result<(), String> {
        let (run, folder, app) = {
            let s = self.lock();
            let run = s.runs.get(item_id).cloned().ok_or("Bu iş artık kırık değil.")?;
            let folder = s.clones.get(&run.repo.to_lowercase()).cloned().ok_or("Bu depo bilgisayarda klonlu değil.")?;
            (run, folder, s.app.clone().ok_or("GitHub bağlı değil.")?)
        };
        let tasks = app.state::<Arc<crate::tasks::TaskHub>>();
        crate::tasks::start_task(app.clone(), tasks, folder, api::fix_prompt(&run), "default".into()).map(|_| ())
    }
}

fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')));
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Kırık CI'ı olan depolardan bilgisayarda klonlu olanlar: "Yeni görev"in son klasörlerinde `.git/config`
/// uzak adresleri. Git çalıştırılmaz, yalnız dosya okunur; kırık CI yoksa hiç bakılmaz.
fn clones(app: &AppHandle, summary: &Summary) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if !summary.cis.iter().any(|c| c.state == api::CiState::Failed) {
        return out;
    }
    let Ok(recent) = tauri::async_runtime::block_on(crate::tasks::list_recent_projects(app.clone())) else { return out };
    for project in recent {
        let Ok(config) = std::fs::read_to_string(Path::new(&project.path).join(".git").join("config")) else { continue };
        for slug in api::config_remotes(&config) {
            out.entry(slug).or_insert_with(|| project.path.clone());
        }
    }
    out
}

impl Source for Github {
    fn id(&self) -> &'static str {
        ID
    }

    fn start(&self, app: AppHandle) {
        let token = crate::secrets::read(TOKEN_KEY);
        let generation = {
            let mut s = self.lock();
            *s = Shared { generation: s.generation + 1, running: token.is_some(), token, app: Some(app.clone()), ..Shared::default() };
            s.generation
        };
        if !self.current(generation) {
            return;
        }
        publish(SourceState::idle(ID, "connecting"));
        let Some(this) = SELF.get().cloned() else { return };
        std::thread::Builder::new().name("zodyvel-github".into()).spawn(move || this.poll(app, generation)).ok();
    }

    fn stop(&self) {
        let mut s = self.lock();
        let generation = s.generation;
        *s = Shared { generation, ..Shared::default() };
        drop(s);
        self.wake.notify_all();
    }

    fn action(&self, item_id: &str, action: &str) -> Result<(), String> {
        match action {
            "open" => self.open(item_id),
            "open_repo" => self.open_repo(item_id),
            "mark_read" => self.mark_read(item_id),
            "rerun" => self.rerun(item_id),
            "fix_with_claude" => self.fix_with_claude(item_id),
            _ => Err("Bu eylem GitHub'da yok.".into()),
        }
    }

    /// "Yenile": bağlantıyı baştan kurmadan hemen sorar. Token yeni girildiyse merkez zaten baştan başlatır.
    fn refresh(&self, app: AppHandle) {
        let running = {
            let mut s = self.lock();
            s.poke = true;
            s.running
        };
        if running {
            self.wake.notify_all();
        } else {
            self.start(app);
        }
    }
}

static SELF: std::sync::OnceLock<Arc<Github>> = std::sync::OnceLock::new();

pub fn source() -> Arc<dyn Source> {
    SELF.get_or_init(|| Arc::new(Github { shared: Mutex::new(Shared::default()), wake: Condvar::new() })).clone()
}
