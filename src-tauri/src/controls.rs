//! Kontroller (müzik ayrıntısı): Wi-Fi ve Bluetooth radyoları, ses düzeyi, koyu mod.
//!
//! - Radyolar: `Windows.Devices.Radios` (Ayarlar'daki anahtarlarla aynı; uçak modu değişmez).
//! - Ses: Core Audio, varsayılan çıkış aygıtının `IAudioEndpointVolume`'u.
//! - Koyu mod: kişiselleştirme kaydı + `WM_SETTINGCHANGE "ImmersiveColorSet"` (Ayarlar'ın yaptığı gibi).
//! - Ses çıkışı: etkin çıkış aygıtları `EnumAudioEndpoints` ile listelenir; varsayılanı değiştirmek için
//!   belgelenmemiş `IPolicyConfig` (Windows'un ses ayarlarının kullandığı arayüz) üç rol için çağrılır.
//! - Gece ışığı ve rahatsız etme için resmi API yok: Windows'un ayar sayfası açılır.
//!
//! Arka planda iş yok: durum yalnız Kontroller ekranı açılınca ve bir ayar değiştirilince okunur.
//! WinRT/COM çağrıları ana iş parçacığını bekletmesin diye engelleyici havuzda, MTA'da yapılır.

use std::ffi::c_void;

use serde::Serialize;
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Networking::Connectivity::NetworkInformation;
use windows::Win32::Devices::FunctionDiscovery::{PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName};
use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, PROPERTYKEY, WPARAM};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToBSTR};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW};
use windows::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, w};

use crate::win;

const PERSONALIZE: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
const APPS_LIGHT: PCWSTR = w!("AppsUseLightTheme");
const SYSTEM_LIGHT: PCWSTR = w!("SystemUsesLightTheme");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    /// 0–100
    pub level: u8,
    pub muted: bool,
    /// Varsayılan çıkış aygıtının gerçek adı ("Realtek(R) Audio", "Razer Barracuda X (BT)"…).
    pub device: Option<String>,
}

/// `None`: bu bilgisayarda yok ya da okunamadı (anahtar pasif görünür).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Controls {
    pub wifi: Option<bool>,
    /// Bağlı kablosuz ağın adı (Windows izin vermezse profil adı, o da yoksa boş).
    pub wifi_name: Option<String>,
    pub bluetooth: Option<bool>,
    pub volume: Option<Volume>,
    pub dark: Option<bool>,
}

/// Bir çıkış aygıtı (liste için). `id`: Core Audio uç nokta kimliği.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub id: String,
    /// Aygıtın gerçek adı ("Razer Barracuda X (BT)").
    pub name: String,
    /// Aygıt türü ("Kulaklıklar", "Hoparlör"); addan ayrı gösterilir.
    pub kind: Option<String>,
    /// Şu an varsayılan olan.
    pub default: bool,
}

/// Çıkış değişince hem yeni durum (ses düzeyi aygıta göre değişir) hem güncel liste döner.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSwitch {
    pub controls: Controls,
    pub outputs: Vec<Output>,
}

/// Windows'un ses ayarlarının kullandığı belgelenmemiş `IPolicyConfig` (Vista sonrası düzen).
/// Yalnız `SetDefaultEndpoint` çağrılır; ondan önceki 10 metot (GetMixFormat … SetPropertyValue)
/// vtable sırasını korumak için yer tutar. Makro `windows_core` bağımlılığı istediğinden vtable elle yazıldı.
#[repr(C)]
struct PolicyConfigVtbl {
    base: IUnknown_Vtbl,
    _unused: [usize; 10],
    set_default_endpoint: unsafe extern "system" fn(this: *mut c_void, device: PCWSTR, role: ERole) -> HRESULT,
}

/// IPolicyConfig
const POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
/// PolicyConfigClient
const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// `IPolicyConfig` işaretçisi; bırakılınca `Release` çağrılır.
struct PolicyConfig(IUnknown);

impl PolicyConfig {
    fn create() -> windows::core::Result<Self> {
        unsafe {
            let unknown: IUnknown = CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
            let mut raw = std::ptr::null_mut();
            unknown.query(&POLICY_CONFIG, &mut raw).ok()?;
            Ok(Self(IUnknown::from_raw(raw)))
        }
    }

    fn set_default(&self, id: &[u16], role: ERole) -> windows::core::Result<()> {
        debug_assert_eq!(id.last(), Some(&0));
        unsafe {
            let this = self.0.as_raw();
            let vtbl = *this.cast::<*const PolicyConfigVtbl>();
            ((*vtbl).set_default_endpoint)(this, PCWSTR(id.as_ptr()), role).ok()
        }
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Havuzdaki iş parçacığı daha önce katıldıysa çağrı zararsızdır.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        f()
    })
    .await
    .map_err(|_| "İşlem tamamlanamadı.".to_string())
}

fn radios() -> Vec<Radio> {
    let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.join()) else { return Vec::new() };
    (0..list.Size().unwrap_or(0)).filter_map(|i| list.GetAt(i).ok()).collect()
}

fn of_kind(radios: &[Radio], kind: RadioKind) -> Vec<&Radio> {
    radios.iter().filter(|r| r.Kind().is_ok_and(|k| k == kind)).collect()
}

/// Aynı türden birden çok radyo varsa biri açıksa açık sayılır.
fn radio_on(radios: &[Radio], kind: RadioKind) -> Option<bool> {
    let states: Vec<RadioState> = of_kind(radios, kind).iter().filter_map(|r| r.State().ok()).collect();
    (!states.is_empty()).then(|| states.contains(&RadioState::On))
}

fn wifi_name() -> Option<String> {
    let profile = NetworkInformation::GetInternetConnectionProfile().ok()?;
    if !profile.IsWlanConnectionProfile().ok()? {
        return None;
    }
    let ssid = profile.WlanConnectionProfileDetails().and_then(|d| d.GetConnectedSsid()).map(|s| s.to_string());
    ssid.ok().filter(|s| !s.is_empty()).or_else(|| profile.ProfileName().ok().map(|s| s.to_string()).filter(|s| !s.is_empty()))
}

fn endpoint() -> windows::core::Result<(IMMDevice, IAudioEndpointVolume)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)?;
        Ok((device, volume))
    }
}

fn text_property(device: &IMMDevice, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(key).ok()?;
        let text = PropVariantToBSTR(&value).ok().map(|b| b.to_string());
        let _ = PropVariantClear(&mut value);
        text.filter(|t| !t.is_empty())
    }
}

/// Aygıtın gerçek adı ("Razer Barracuda X (BT)") ve altına yazılacak türü ("Kulaklıklar").
/// Windows tam adı "tür (ad)" biçiminde verir; parantez içi gerçek addır.
fn split_name(kind: Option<&str>, full: Option<&str>) -> (String, Option<String>) {
    match (kind, full) {
        (Some(kind), Some(full)) => {
            let name = full.strip_prefix(kind).and_then(|r| r.strip_prefix(" (")).and_then(|r| r.strip_suffix(')')).filter(|n| !n.is_empty());
            match name {
                Some(name) => (name.to_string(), Some(kind.to_string())),
                None if full == kind => (full.to_string(), None),
                None => (full.to_string(), Some(kind.to_string())),
            }
        }
        (None, Some(text)) | (Some(text), None) => (text.to_string(), None),
        (None, None) => ("Ses aygıtı".to_string(), None),
    }
}

fn device_labels(device: &IMMDevice) -> (String, Option<String>) {
    let kind = text_property(device, &PKEY_Device_DeviceDesc);
    let full = text_property(device, &PKEY_Device_FriendlyName);
    split_name(kind.as_deref(), full.as_deref())
}

/// Varsayılan çıkış aygıtının gerçek adı (karonun alt satırı, oynatıcı şeridi).
fn device_name(device: &IMMDevice) -> String {
    device_labels(device).0
}

fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let raw = device.GetId().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.as_ptr().cast()));
        id.filter(|i| !i.is_empty())
    }
}

fn outputs() -> windows::core::Result<Vec<Output>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| device_id(&d));
        let list = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut outputs = Vec::new();
        for i in 0..list.GetCount()? {
            let Ok(device) = list.Item(i) else { continue };
            let Some(id) = device_id(&device) else { continue };
            let (name, kind) = device_labels(&device);
            let default = default.as_deref() == Some(id.as_str());
            outputs.push(Output { id, name, kind, default });
        }
        Ok(outputs)
    }
}

/// Yalnız listedeki (etkin) bir aygıt seçilebilir; başka dizeler Windows'a hiç ulaşmaz.
fn listed<'a>(outputs: &'a [Output], id: &str) -> Option<&'a Output> {
    outputs.iter().find(|o| o.id == id)
}

fn set_output(id: &str) -> Result<(), String> {
    let list = outputs().map_err(|_| "Ses aygıtları okunamadı.".to_string())?;
    if listed(&list, id).is_none() {
        return Err("Bu ses aygıtı artık kullanılamıyor.".into());
    }
    let failed = |_| "Ses çıkışı değiştirilemedi.".to_string();
    let policy = PolicyConfig::create().map_err(failed)?;
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    for role in [eConsole, eMultimedia, eCommunications] {
        policy.set_default(&wide, role).map_err(failed)?;
    }
    Ok(())
}

fn volume() -> Option<Volume> {
    let (device, endpoint) = endpoint().ok()?;
    unsafe {
        let level = endpoint.GetMasterVolumeLevelScalar().ok()?;
        let muted = endpoint.GetMute().ok()?.as_bool();
        Some(Volume { level: (level * 100.0).round().clamp(0.0, 100.0) as u8, muted, device: Some(device_name(&device)) })
    }
}

fn read_dword(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, name, RRF_RT_REG_DWORD, None, Some((&raw mut value).cast()), Some(&mut size))
    };
    (status == ERROR_SUCCESS).then_some(value)
}

fn dark() -> Option<bool> {
    read_dword(APPS_LIGHT).map(|light| light == 0)
}

fn read() -> Controls {
    let radios = radios();
    let wifi = radio_on(&radios, RadioKind::WiFi);
    Controls {
        wifi,
        wifi_name: if wifi == Some(true) { wifi_name() } else { None },
        bluetooth: radio_on(&radios, RadioKind::Bluetooth),
        volume: volume(),
        dark: dark(),
    }
}

fn set_radio(kind: RadioKind, on: bool) -> Result<(), String> {
    let denied = || "Windows bu ayarı değiştirmeye izin vermedi.".to_string();
    let access = Radio::RequestAccessAsync().and_then(|op| op.join()).map_err(|_| denied())?;
    if access != RadioAccessStatus::Allowed {
        return Err(denied());
    }
    let radios = radios();
    let targets = of_kind(&radios, kind);
    if targets.is_empty() {
        return Err("Bu bilgisayarda bulunamadı.".into());
    }
    let state = if on { RadioState::On } else { RadioState::Off };
    for radio in targets {
        let status = radio.SetStateAsync(state).and_then(|op| op.join()).map_err(|_| denied())?;
        if status != RadioAccessStatus::Allowed {
            return Err(denied());
        }
    }
    Ok(())
}

fn set_volume(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
    let (_, endpoint) = endpoint().map_err(|_| "Ses aygıtı bulunamadı.".to_string())?;
    unsafe {
        if let Some(level) = level {
            endpoint.SetMasterVolumeLevelScalar(f32::from(level.min(100)) / 100.0, std::ptr::null()).map_err(|e| e.message())?;
        }
        if let Some(muted) = muted {
            endpoint.SetMute(muted, std::ptr::null()).map_err(|e| e.message())?;
        }
    }
    Ok(())
}

fn set_dark(on: bool) -> Result<(), String> {
    let light: u32 = if on { 0 } else { 1 };
    for name in [APPS_LIGHT, SYSTEM_LIGHT] {
        let status = unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, name, REG_DWORD.0, Some((&raw const light).cast()), 4) };
        if status != ERROR_SUCCESS {
            return Err("Tema ayarı yazılamadı.".into());
        }
    }
    // Açık pencereler (Explorer, görev çubuğu, uygulamalar) yeni temaya geçsin; asılı pencere beklenmez.
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("ImmersiveColorSet").as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
    Ok(())
}

fn settings_page(page: &str) -> Option<&'static str> {
    Some(match page {
        "wifi" => "ms-settings:network-wifi",
        "bluetooth" => "ms-settings:bluetooth",
        "sound" => "ms-settings:sound",
        "nightlight" => "ms-settings:nightlight",
        "notifications" => "ms-settings:notifications",
        _ => return None,
    })
}

#[tauri::command]
pub async fn controls_read() -> Result<Controls, String> {
    blocking(read).await
}

#[tauri::command]
pub async fn controls_radio(kind: String, on: bool) -> Result<Controls, String> {
    let kind = match kind.as_str() {
        "wifi" => RadioKind::WiFi,
        "bluetooth" => RadioKind::Bluetooth,
        _ => return Err("Geçersiz radyo.".into()),
    };
    blocking(move || set_radio(kind, on).map(|()| read())).await?
}

#[tauri::command]
pub async fn controls_volume(level: Option<u8>, muted: Option<bool>) -> Result<(), String> {
    blocking(move || set_volume(level, muted)).await?
}

#[tauri::command]
pub async fn controls_dark(on: bool) -> Result<Controls, String> {
    blocking(move || set_dark(on).map(|()| read())).await?
}

#[tauri::command]
pub async fn controls_outputs() -> Result<Vec<Output>, String> {
    blocking(|| outputs().map_err(|_| "Ses aygıtları okunamadı.".to_string())).await?
}

#[tauri::command]
pub async fn controls_output(id: String) -> Result<OutputSwitch, String> {
    blocking(move || {
        set_output(&id)?;
        Ok(OutputSwitch { controls: read(), outputs: outputs().unwrap_or_default() })
    })
    .await?
}

#[tauri::command]
pub fn controls_open(page: String) -> Result<(), String> {
    let uri = settings_page(&page).ok_or("Geçersiz sayfa.")?;
    if win::shell_open(uri) { Ok(()) } else { Err("Ayarlar açılamadı.".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_settings_pages_open() {
        assert_eq!(settings_page("nightlight"), Some("ms-settings:nightlight"));
        assert_eq!(settings_page("sound"), Some("ms-settings:sound"));
        assert_eq!(settings_page("notifications"), Some("ms-settings:notifications"));
        assert_eq!(settings_page("ms-settings:privacy"), None);
        assert_eq!(settings_page(""), None);
    }

    fn out(id: &str, name: &str) -> Output {
        Output { id: id.into(), name: name.into(), kind: None, default: false }
    }

    #[test]
    fn only_listed_outputs_are_selectable() {
        let list = vec![out("{0.0.0.00000000}.{aaa}", "Hoparlör"), out("{0.0.0.00000000}.{bbb}", "Kulaklık")];
        assert!(listed(&list, "{0.0.0.00000000}.{bbb}").is_some());
        assert!(listed(&list, "{0.0.0.00000000}.{ccc}").is_none());
        assert!(listed(&list, "").is_none());
        assert!(listed(&[], "{0.0.0.00000000}.{aaa}").is_none());
    }

    #[test]
    fn real_device_name_is_split_from_kind() {
        let check = |kind, full, name: &str, expected: Option<&str>| {
            assert_eq!(split_name(kind, full), (name.to_string(), expected.map(String::from)));
        };
        check(Some("Speaker"), Some("Speaker (Realtek(R) Audio)"), "Realtek(R) Audio", Some("Speaker"));
        check(Some("Kulaklıklar"), Some("Kulaklıklar (Razer Barracuda X (BT))"), "Razer Barracuda X (BT)", Some("Kulaklıklar"));
        // Parantezsiz ya da tür adıyla başlamayan tam ad olduğu gibi kalır.
        check(Some("Hoparlör"), Some("Hoparlör"), "Hoparlör", None);
        check(Some("Hoparlör"), Some("USB Ses Aygıtı"), "USB Ses Aygıtı", Some("Hoparlör"));
        check(Some("Hoparlör"), Some("Hoparlör ()"), "Hoparlör ()", Some("Hoparlör"));
        check(None, Some("Monitör (HDMI)"), "Monitör (HDMI)", None);
        check(Some("Hoparlör"), None, "Hoparlör", None);
        check(None, None, "Ses aygıtı", None);
    }

    /// Çıkış aygıtlarını yalnız listeler (varsayılanı DEĞİŞTİRMEZ); adlar yazdırılır, kimlikler yazdırılmaz.
    #[test]
    #[ignore]
    fn lists_real_outputs() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let list = outputs().expect("aygıtlar listelenmeli");
        println!("çıkış_sayısı={} varsayılan_var={}", list.len(), list.iter().any(|o| o.default));
        for o in &list {
            println!("aygıt ad={:?} tür={:?} varsayılan={}", o.name, o.kind, o.default);
        }
        assert!(list.iter().filter(|o| o.default).count() <= 1);
        // Arayüz kaydı var mı: nesne yaratılır, hiçbir metot çağrılmaz.
        let created = PolicyConfig::create().is_ok();
        println!("policy_config_oluştu={created}");
    }

    /// Bu bilgisayardaki durumu yalnız okur (hiçbir ayarı değiştirmez); ağ adı yazdırılmaz.
    #[test]
    #[ignore]
    fn reads_real_controls() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let c = read();
        // İzin istemek bir şey değiştirmez; masaüstü uygulamasına Windows'un ne dediğini gösterir.
        let access = Radio::RequestAccessAsync().and_then(|op| op.join()).map(|a| a == RadioAccessStatus::Allowed);
        println!("radyo_izni={access:?}");
        println!(
            "wifi={:?} ağ_adı_var={} bluetooth={:?} ses={:?} koyu={:?}",
            c.wifi,
            c.wifi_name.is_some(),
            c.bluetooth,
            c.volume.as_ref().map(|v| (v.level, v.muted, v.device.clone())),
            c.dark
        );
    }
}
