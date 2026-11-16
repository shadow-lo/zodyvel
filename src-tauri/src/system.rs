//! Sistem yükü modülü: Görev Yöneticisi ile aynı ölçüler.
//!
//! - CPU: "% Processor Utility" (Görev Yöneticisi bunu gösterir; yoksa "% Processor Time").
//! - RAM: kullanımdaki bellek = toplam − kullanılabilir.
//! - GPU: her motorun süreçler üzerinden toplamı; en yüksek motor (Görev Yöneticisi'nin özeti gibi).
//! - Uygulamalar: süreç adına göre gruplanmış özel çalışma kümesi.
//!
//! Örnekleme yalnız ada bu bilgileri gösterirken yapılır (`system_watch`); diğer zamanlarda iş
//! parçacığı uyur ve hiçbir sayaç okunmaz. Sayaç adları İngilizce eklenir: Türkçe Windows'ta da çalışır.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Win32::System::Performance::{
    PDH_FMT, PDH_FMT_COUNTERVALUE, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY,
    PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::core::{PCWSTR, w};

use crate::win;

/// 100'ün üstündeki değerleri kesme (turbo frekansta yardımcı program %100'ü aşabilir; biz sonra sınırlarız).
const PDH_FMT_NOCAP100: u32 = 0x8000;
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub name: String,
    pub ram_mb: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLoad {
    pub cpu: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub gpu: Option<f64>,
    pub top_apps: Vec<AppUsage>,
    pub demo: bool,
}

pub struct SystemHub {
    /// 0: kimse bakmıyor · 1: CPU/RAM/GPU · 2: + en çok bellek kullanan uygulamalar
    watch: AtomicU8,
    worker: thread::Thread,
}

pub fn init(app: &AppHandle) {
    let handle = app.clone();
    let worker = thread::Builder::new().name("zodyvel-system".into()).spawn(move || run(handle)).expect("sistem iş parçacığı");
    app.manage(Arc::new(SystemHub { watch: AtomicU8::new(0), worker: worker.thread().clone() }));
}

#[tauri::command]
pub fn system_watch(hub: State<'_, Arc<SystemHub>>, level: u8) {
    let level = level.min(2);
    if hub.watch.swap(level, Ordering::SeqCst) != level {
        hub.worker.unpark();
    }
}

#[tauri::command]
pub fn open_task_manager() -> Result<(), String> {
    if win::shell_open("taskmgr.exe") { Ok(()) } else { Err("Görev Yöneticisi açılamadı".into()) }
}

fn run(app: AppHandle) {
    let level = || app.try_state::<Arc<SystemHub>>().map(|h| h.watch.load(Ordering::SeqCst)).unwrap_or(0);
    let mut sampler: Option<Sampler> = None;
    loop {
        if level() == 0 {
            // Ada kapanınca sorgu 30 sn açık kalır (yeniden açılınca değer hemen gelsin), sonra kapanır.
            if sampler.is_some() {
                thread::park_timeout(Duration::from_secs(30));
                if level() == 0 {
                    sampler = None;
                }
            } else {
                thread::park_timeout(Duration::from_secs(3600));
            }
            continue;
        }
        if sampler.is_none() {
            // CPU/GPU oran sayaçları iki ölçüm arasındaki farkı kullanır: ilk değerden önce kısa bekleme.
            sampler = Some(Sampler::new());
            thread::sleep(Duration::from_millis(300));
        }
        let full = level() >= 2;
        if let Some(load) = sampler.as_mut().and_then(|s| s.sample(full)) {
            let _ = app.emit("system://load", &load);
        }
        thread::park_timeout(Duration::from_millis(if full { 2000 } else { 1000 }));
    }
}

struct Query(PDH_HQUERY);

impl Query {
    fn open() -> Option<Self> {
        let mut query = PDH_HQUERY::default();
        (unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } == 0).then_some(Self(query))
    }
    fn add(&self, path: PCWSTR) -> Option<PDH_HCOUNTER> {
        let mut counter = PDH_HCOUNTER::default();
        (unsafe { PdhAddEnglishCounterW(self.0, path, 0, &mut counter) } == 0).then_some(counter)
    }
    fn collect(&self) -> bool {
        unsafe { PdhCollectQueryData(self.0) == 0 }
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.0);
        }
    }
}

struct Sampler {
    basic: Option<(Query, PDH_HCOUNTER, Option<PDH_HCOUNTER>)>,
    apps: Option<(Query, PDH_HCOUNTER)>,
}

impl Sampler {
    fn new() -> Self {
        let basic = Query::open().and_then(|q| {
            let cpu = q
                .add(w!("\\Processor Information(_Total)\\% Processor Utility"))
                .or_else(|| q.add(w!("\\Processor(_Total)\\% Processor Time")))?;
            let gpu = q.add(w!("\\GPU Engine(*)\\Utilization Percentage"));
            q.collect();
            Some((q, cpu, gpu))
        });
        Self { basic, apps: None }
    }

    fn sample(&mut self, with_apps: bool) -> Option<SystemLoad> {
        let (query, cpu_counter, gpu_counter) = self.basic.as_ref()?;
        query.collect();
        let cpu = value(*cpu_counter).unwrap_or(0.0).clamp(0.0, 100.0);
        let gpu = gpu_counter.map(|c| gpu_percent(&array(c, PDH_FMT_DOUBLE)));
        let (ram_used_gb, ram_total_gb) = memory();

        let top_apps = if with_apps {
            if self.apps.is_none() {
                self.apps = Query::open().and_then(|q| {
                    let counter = q.add(w!("\\Process(*)\\Working Set - Private"))?;
                    Some((q, counter))
                });
            }
            self.apps.as_ref().map(|(q, c)| if q.collect() { top_apps(&array(*c, PDH_FMT_LARGE), 6) } else { Vec::new() }).unwrap_or_default()
        } else {
            self.apps = None;
            Vec::new()
        };
        Some(SystemLoad { cpu, ram_used_gb, ram_total_gb, gpu, top_apps, demo: false })
    }
}

fn value(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut out = PDH_FMT_COUNTERVALUE::default();
    let status = unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100), None, &mut out) };
    (status == 0 && out.CStatus == 0).then_some(unsafe { out.Anonymous.doubleValue })
}

enum Num {
    F(f64),
    I(i64),
}

fn array(counter: PDH_HCOUNTER, format: PDH_FMT) -> Vec<(String, Num)> {
    let format = PDH_FMT(format.0 | PDH_FMT_NOCAP100);
    let (mut size, mut count) = (0u32, 0u32);
    if unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, None) } != PDH_MORE_DATA || size == 0 {
        return Vec::new();
    }
    // 8 bayt hizalı tampon: öğeler ve ad metinleri aynı blokta.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    if unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, Some(items)) } != 0 {
        return Vec::new();
    }
    let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
    items
        .iter()
        .filter(|item| item.FmtValue.CStatus == 0)
        .map(|item| {
            let name = unsafe { item.szName.to_string() }.unwrap_or_default();
            let num = if format.0 & PDH_FMT_LARGE.0 != 0 {
                Num::I(unsafe { item.FmtValue.Anonymous.largeValue })
            } else {
                Num::F(unsafe { item.FmtValue.Anonymous.doubleValue })
            };
            (name, num)
        })
        .collect()
}

/// "pid_1234_luid_0x..._phys_0_eng_3_engtype_3D" → motor anahtarı "luid_..._eng_3_engtype_3D".
pub fn gpu_percent_from(samples: &[(&str, f64)]) -> f64 {
    let mut engines: HashMap<&str, f64> = HashMap::new();
    for (name, value) in samples {
        let key = name.find("luid_").map(|i| &name[i..]).unwrap_or(name);
        *engines.entry(key).or_default() += value;
    }
    engines.values().copied().fold(0.0, f64::max).clamp(0.0, 100.0)
}

fn gpu_percent(samples: &[(String, Num)]) -> f64 {
    let flat: Vec<(&str, f64)> = samples
        .iter()
        .map(|(n, v)| (n.as_str(), match v {
            Num::F(f) => *f,
            Num::I(i) => *i as f64,
        }))
        .collect();
    gpu_percent_from(&flat)
}

/// Süreç örneklerini ada göre toplar ("chrome", "chrome#1"…), en çok bellek kullanan `limit` tanesi.
pub fn top_apps_from(samples: &[(&str, i64)], limit: usize) -> Vec<AppUsage> {
    // Uygulama sayılmayan sistem süreçleri. svchost onlarca ayrı hizmetin toplamıdır; tek satırda yanıltır.
    const HIDDEN: &[&str] = &[
        "_Total",
        "Idle",
        "System",
        "Registry",
        "Secure System",
        "Memory Compression",
        "vmmem",
        "vmmemWSL",
        "svchost",
        "csrss",
        "lsass",
        "services",
        "smss",
        "wininit",
        "winlogon",
        "fontdrvhost",
    ];
    let mut totals: HashMap<String, i64> = HashMap::new();
    for (name, bytes) in samples {
        let base = name.split('#').next().unwrap_or(name);
        if base.is_empty() || HIDDEN.iter().any(|h| base.eq_ignore_ascii_case(h)) {
            continue;
        }
        *totals.entry(base.to_owned()).or_default() += bytes;
    }
    let mut list: Vec<(String, i64)> = totals.into_iter().collect();
    list.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
    list.into_iter()
        .take(limit)
        .map(|(name, bytes)| AppUsage { name: display_name(&name), ram_mb: (bytes as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0 })
        .collect()
}

fn top_apps(samples: &[(String, Num)], limit: usize) -> Vec<AppUsage> {
    let flat: Vec<(&str, i64)> = samples
        .iter()
        .map(|(n, v)| (n.as_str(), match v {
            Num::I(i) => *i,
            Num::F(f) => *f as i64,
        }))
        .collect();
    top_apps_from(&flat, limit)
}

/// Bilinen süreç adlarına okunur ad; diğerlerinde ilk harf büyük.
fn display_name(process: &str) -> String {
    let known = [
        ("chrome", "Google Chrome"),
        ("msedge", "Microsoft Edge"),
        ("opera", "Opera"),
        ("firefox", "Firefox"),
        ("code", "Visual Studio Code"),
        ("spotify", "Spotify"),
        ("discord", "Discord"),
        ("explorer", "Dosya Gezgini"),
        ("msedgewebview2", "WebView2"),
        ("claude", "Claude"),
        ("codex", "Codex"),
        ("devenv", "Visual Studio"),
        ("steam", "Steam"),
        ("unity", "Unity"),
        ("msmpeng", "Microsoft Defender"),
        ("searchhost", "Windows Arama"),
        ("ms-teams", "Microsoft Teams"),
        ("onedrive", "OneDrive"),
        ("whatsapp", "WhatsApp"),
        ("telegram", "Telegram"),
        ("obs64", "OBS Studio"),
        ("zodyvel", "Zodyvel"),
    ];
    let lower = process.to_ascii_lowercase();
    if let Some((_, name)) = known.iter().find(|(k, _)| lower == *k) {
        return (*name).to_owned();
    }
    let mut chars = process.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

fn memory() -> (f64, f64) {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return (0.0, 0.0);
    }
    let total = status.ullTotalPhys as f64;
    let used = total - status.ullAvailPhys as f64;
    ((used / GIB * 10.0).round() / 10.0, (total / GIB * 10.0).round() / 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_uses_busiest_engine_summed_over_processes() {
        let samples = [
            ("pid_10_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 20.0),
            ("pid_11_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 15.0),
            ("pid_10_luid_0x0_0x1_phys_0_eng_3_engtype_VideoDecode", 30.0),
        ];
        assert_eq!(gpu_percent_from(&samples), 35.0);
        assert_eq!(gpu_percent_from(&[]), 0.0);
        assert_eq!(gpu_percent_from(&[("pid_1_luid_x_eng_0_engtype_3D", 180.0)]), 100.0);
    }

    #[test]
    fn apps_are_grouped_by_process_name() {
        let mb = 1024 * 1024;
        let samples = [
            ("chrome", 300 * mb),
            ("chrome#1", 200 * mb),
            ("code", 400 * mb),
            ("_Total", 9000 * mb),
            ("Idle", 0),
            ("Memory Compression", 800 * mb),
            ("notepad", 10 * mb),
        ];
        let top = top_apps_from(&samples, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].name, "Google Chrome");
        assert_eq!(top[0].ram_mb, 500.0);
        assert_eq!(top[1].name, "Visual Studio Code");
    }

    /// Bu bilgisayarın gerçek sayaçlarını okur (Windows'a özgü proje).
    #[test]
    fn real_counters_are_readable() {
        let mut sampler = Sampler::new();
        std::thread::sleep(Duration::from_millis(500));
        let load = sampler.sample(true).expect("PDH sorgusu açılmalı");
        println!(
            "cpu=%{:.1} ram={}/{} GB gpu={:?} uygulamalar={:?}",
            load.cpu,
            load.ram_used_gb,
            load.ram_total_gb,
            load.gpu.map(|g| (g * 10.0).round() / 10.0),
            load.top_apps.iter().map(|a| format!("{} {} MB", a.name, a.ram_mb)).collect::<Vec<_>>()
        );
        assert!((0.0..=100.0).contains(&load.cpu));
        assert!(load.ram_total_gb > 0.5);
        assert!(!load.top_apps.is_empty());
    }

    #[test]
    fn memory_is_sane() {
        let (used, total) = memory();
        assert!(total > 0.5 && used > 0.0 && used <= total);
    }
}
