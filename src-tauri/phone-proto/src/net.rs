//! Dinleme yüzeyi kuralları: yalnız Özel ağ + yalnız yerel arayüz + hatalı denemede yavaşlatma.
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Category {
    Private,
    Public,
    DomainAuthenticated,
    Unknown,
}

/// `Get-NetConnectionProfile | Select -Expand NetworkCategory` çıktısını ayrıştırır.
/// Bağlı birden çok profil olabilir (satır satır): HEPSİ Private değilse dinleme açılmaz.
pub fn parse_profiles(out: &str) -> Vec<Category> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.to_ascii_lowercase().as_str() {
            "private" => Category::Private,
            "public" => Category::Public,
            "domainauthenticated" => Category::DomainAuthenticated,
            _ => Category::Unknown,
        })
        .collect()
}

/// Dinleme izni: en az bir profil var ve hepsi Özel. (Kafe/okul = Ortak → dinleme yok.)
pub fn listening_allowed(profiles: &[Category]) -> bool {
    !profiles.is_empty() && profiles.iter().all(|c| *c == Category::Private)
}

/// Windows'ta canlı sorgu (PowerShell). Diğer sistemlerde None.
#[cfg(windows)]
pub fn query_profiles() -> Option<Vec<Category>> {
    use std::os::windows::process::CommandExt;
    // Tam yol: uygulama klasörüne bırakılmış sahte `powershell.exe` çalışmasın.
    let ps = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    let o = std::process::Command::new(ps)
        .args(["-NoProfile", "-Command", "Get-NetConnectionProfile | ForEach-Object { $_.NetworkCategory }"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    Some(parse_profiles(&String::from_utf8_lossy(&o.stdout)))
}
#[cfg(not(windows))]
pub fn query_profiles() -> Option<Vec<Category>> {
    None
}

/// RFC1918 + link-local: yalnız bu adreslere bağlanılır; 0.0.0.0 ve genel adres reddedilir.
pub fn is_local_bind(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4 == Ipv4Addr::LOCALHOST,
        IpAddr::V6(_) => false,
    }
}

/// Hatalı denemelerde yavaşlatma ve geçici engel (IP başına).
/// G4: eşik altındaki hatalar `decay` sonra unutulur (aylar içinde biriken rastgele hatalar gerçek telefonu
/// engellemesin), tablo `MAX_TRACKED` ile sınırlı, her hatada artan bekleme (`delay`).
pub struct Throttle {
    fails: HashMap<IpAddr, (u32, Instant)>,
    pub max_fails: u32,
    pub ban: Duration,
    pub decay: Duration,
}
/// İzlenen en çok IP (tablo sınırsız büyümesin).
pub const MAX_TRACKED: usize = 256;
impl Throttle {
    pub fn new() -> Self {
        Self { fails: HashMap::new(), max_fails: 5, ban: Duration::from_secs(300), decay: Duration::from_secs(600) }
    }
    pub fn blocked(&mut self, ip: IpAddr) -> bool {
        match self.fails.get(&ip) {
            Some((n, t)) if *n >= self.max_fails => {
                if t.elapsed() > self.ban {
                    self.fails.remove(&ip);
                    false
                } else {
                    true
                }
            }
            Some((_, t)) if t.elapsed() > self.decay => {
                self.fails.remove(&ip);
                false
            }
            _ => false,
        }
    }
    pub fn fail(&mut self, ip: IpAddr) {
        if !self.fails.contains_key(&ip) && self.fails.len() >= MAX_TRACKED {
            let (ban, decay, max) = (self.ban, self.decay, self.max_fails);
            self.fails.retain(|_, (n, t)| if *n >= max { t.elapsed() <= ban } else { t.elapsed() <= decay });
            if self.fails.len() >= MAX_TRACKED {
                // Hâlâ doluysa yasaklı olmayanlar unutulur (yasaklar korunur).
                self.fails.retain(|_, (n, _)| *n >= max);
            }
        }
        let decay = self.decay;
        let e = self.fails.entry(ip).or_insert((0, Instant::now()));
        if e.1.elapsed() > decay {
            e.0 = 0;
        }
        e.0 += 1;
        e.1 = Instant::now();
    }
    /// Yavaşlatma: önceki her hata beklemeyi ikiye katlar (250 ms, 500 ms, 1 sn, en çok 2 sn).
    pub fn delay(&self, ip: IpAddr) -> Duration {
        match self.fails.get(&ip) {
            Some((n, t)) if *n > 0 && t.elapsed() <= self.decay => Duration::from_millis(250u64 << (n - 1).min(3)),
            _ => Duration::ZERO,
        }
    }
    pub fn tracked(&self) -> usize {
        self.fails.len()
    }
    pub fn ok(&mut self, ip: IpAddr) {
        self.fails.remove(&ip);
    }
}
impl Default for Throttle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g4_throttle_decays_caps_and_slows_down() {
        let ip: IpAddr = "192.168.1.9".parse().unwrap();
        let mut t = Throttle::new();
        assert_eq!(t.delay(ip), Duration::ZERO);
        t.fail(ip);
        assert_eq!(t.delay(ip), Duration::from_millis(250));
        t.fail(ip);
        t.fail(ip);
        assert_eq!(t.delay(ip), Duration::from_millis(1000));
        // Eşik altındaki hatalar unutulur.
        t.decay = Duration::ZERO;
        std::thread::sleep(Duration::from_millis(5));
        assert!(!t.blocked(ip));
        assert_eq!(t.tracked(), 0);
        t.fail(ip);
        assert_eq!(t.fails.get(&ip).unwrap().0, 1, "eski sayaç sıfırlandı");
        // Yasak korunur, tablo sınırlı.
        let mut t = Throttle::new();
        for _ in 0..5 {
            t.fail(ip);
        }
        assert!(t.blocked(ip));
        for i in 0..(MAX_TRACKED as u32 * 2) {
            t.fail(IpAddr::V4(Ipv4Addr::from(0x0a00_0000 + i)));
        }
        assert!(t.tracked() <= MAX_TRACKED);
        assert!(t.blocked(ip), "yasaklı IP tablo dolunca da yasaklı kalır");
    }
}
