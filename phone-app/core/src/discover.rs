//! G3 · IP değişince bilgisayarı yerel ağda bulma (`_zodyvel._tcp`, mDNS).
//!
//! Güvenlik: duyuruda kimlik yok, yani duyuru KİMSEYE güvenilmeden yalnız aday adres verir. Telefon her adaya kayıtlı
//! masaüstü anahtarıyla Noise el sıkışması yapar; anahtarı tutmayan (sahte) aday bağlanamaz. Yeni adres ancak doğrulanmış
//! bağlantıdan sonra kaydedilir. Yalnız özel ağ adresleri (10/8, 172.16/12, 192.168/16, 169.254/16, fc00::/7) aday olur.
//! Arama yalnız bağlantı başarısız olunca, en çok `SEARCH_EVERY`'de bir ve `BROWSE_FOR` süreyle yapılır (pil).
use std::net::IpAddr;
use std::time::{Duration, Instant};

pub const SERVICE: &str = "_zodyvel._tcp.local.";
/// Tek aramanın süresi.
pub const BROWSE_FOR: Duration = Duration::from_secs(3);
/// T-D3: ilk aday bulunduktan sonra aramanın sürdüğü en kısa ek süre.
pub const SETTLE: Duration = Duration::from_millis(300);
/// En çok kaç aday denenir.
pub const MAX_CANDIDATES: usize = 5;
/// İki arama arası en az (T-Y2: her başarısız aramada ikiye katlanır, en çok `live::SEARCH_MAX`).
pub const SEARCH_EVERY: Duration = Duration::from_secs(60);
/// Bu kadar art arda bağlantı hatasından sonra aranır (tek geçici hata için ağ taranmaz).
pub const FAILS_BEFORE_SEARCH: u32 = 2;

pub fn allowed(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_link_local(),
        // Yerel benzersiz adres (fc00::/7). Bağlantı yerel (fe80::) kapsam kimliği ister; atlanır.
        IpAddr::V6(v) => (v.segments()[0] & 0xfe00) == 0xfc00,
    }
}

pub fn fmt_addr(ip: &IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v) => format!("{v}:{port}"),
        IpAddr::V6(v) => format!("[{v}]:{port}"),
    }
}

/// Yerel ağda `_zodyvel._tcp` arar; bulunan özel ağ adreslerini (en çok `max`) döndürür. Hata olursa boş liste.
pub fn browse(wait: Duration, max: usize) -> Vec<String> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    let Ok(d) = ServiceDaemon::new() else {
        return vec![];
    };
    let mut out: Vec<String> = Vec::new();
    if let Ok(rx) = d.browse(SERVICE) {
        let mut end = Instant::now() + wait;
        while out.len() < max {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let port = info.get_port();
                    for ip in info.get_addresses() {
                        let a = fmt_addr(ip, port);
                        if allowed(ip) && !out.contains(&a) && out.len() < max {
                            out.push(a);
                        }
                    }
                    // T-D3: ilk sonuçtan sonra yalnız kısa bir süre daha dinlenir (aynı anda duyuran diğer
                    // adresler için); 3 sn'nin tamamı beklenmez.
                    if !out.is_empty() {
                        end = end.min(Instant::now() + SETTLE);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        let _ = d.stop_browse(SERVICE);
    }
    let _ = d.shutdown();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_private_addresses() {
        let ok = ["192.168.1.20", "10.0.0.5", "172.20.1.1", "169.254.3.4", "fd12::1"];
        let no = ["8.8.8.8", "172.32.0.1", "127.0.0.1", "fe80::1", "2001:db8::1", "0.0.0.0"];
        for a in ok {
            assert!(allowed(&a.parse().unwrap()), "{a}");
        }
        for a in no {
            assert!(!allowed(&a.parse().unwrap()), "{a}");
        }
        assert_eq!(fmt_addr(&"fd12::1".parse().unwrap(), 4000), "[fd12::1]:4000");
        assert_eq!(fmt_addr(&"10.0.0.5".parse().unwrap(), 4000), "10.0.0.5:4000");
    }
}
