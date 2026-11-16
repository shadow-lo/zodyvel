//! Yerel ağda duyuru (`_zodyvel._tcp`). Yalnız köprü dinlerken çalışır; kapanınca duyuru geri çekilir.
//! Duyuruda kimlik/ad/anahtar yok: yalnız rastgele bir örnek adı ve port (telefon el sıkışmada doğrular).
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::net::IpAddr;

pub struct Announcement {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Announcement {
    pub fn start(ip: IpAddr, port: u16) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|_| "Yerel ağ duyurusu başlatılamadı.".to_string())?;
        // G4: yalnız dinlenen arayüzde duyur (VPN/sanal bağdaştırıcılara gitmesin).
        let _ = daemon.disable_interface(mdns_sd::IfKind::All);
        let _ = daemon.enable_interface(mdns_sd::IfKind::Addr(ip));
        let mut rnd = [0u8; 3];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut rnd);
        let inst = format!("zodyvel-{}", crate::pairing::hex(&rnd));
        let host = format!("{inst}.local.");
        let info = ServiceInfo::new("_zodyvel._tcp.local.", &inst, &host, ip, port, &[("v", "1")][..])
            .map_err(|_| "Yerel ağ duyurusu kurulamadı.".to_string())?;
        let fullname = info.get_fullname().to_owned();
        daemon.register(info).map_err(|_| "Yerel ağ duyurusu kaydedilemedi.".to_string())?;
        Ok(Self { daemon, fullname })
    }
}
impl Drop for Announcement {
    fn drop(&mut self) {
        // G4: önce "gitti" duyurusu (TTL 0), yoksa eski adres ağdaki önbelleklerde TTL boyunca kalır.
        if let Ok(rx) = self.daemon.unregister(&self.fullname) {
            let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
        }
        let _ = self.daemon.shutdown();
    }
}

/// QR yükünü SVG metni olarak çizer (arayüz `{@html}` ile gösterir; veri dışarı çıkmaz).
pub fn qr_svg(data: &str) -> Result<String, String> {
    use qrcode::{render::svg, QrCode};
    let code = QrCode::new(data.as_bytes()).map_err(|_| "QR üretilemedi.".to_string())?;
    Ok(code.render::<svg::Color>().min_dimensions(220, 220).quiet_zone(true).build())
}
