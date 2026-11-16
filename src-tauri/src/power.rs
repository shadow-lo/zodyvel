//! Pil ve güç durumu: Bilgisayar ekranındaki pil, şarj ve düşük pil bildirimi.
//!
//! Yoklama yok. Windows güç değişikliğini (şarja takma/çıkarma, pil yüzdesi, pil tasarrufu) ada
//! penceresine `WM_POWERBROADCAST` ile bildirir; durum yalnız o an okunur (`GetSystemPowerStatus`, ucuz
//! bir çekirdek çağrısı). Özellik kapalıyken kayıt bırakılır, gelen ileti yok sayılır ve hiçbir şey
//! okunmaz. Pili olmayan bilgisayarda arayüze hiçbir şey gitmez (pil görünmez).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Power::{
    GetSystemPowerStatus, HPOWERNOTIFY, RegisterPowerSettingNotification, SYSTEM_POWER_STATUS, UnregisterPowerSettingNotification,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_WINDOW_HANDLE, PBT_APMPOWERSTATUSCHANGE, PBT_POWERSETTINGCHANGE, WM_POWERBROADCAST};
use windows::core::GUID;

use crate::win::Hwnd;

/// Pil yüzdesi her değiştiğinde (Windows'un `GUID_BATTERY_PERCENTAGE_REMAINING`'i).
const BATTERY_PERCENT: GUID = GUID::from_u128(0xa7ad8041_b45a_4cae_87a3_eecbb468a9e1);
/// Güç kaynağı değişince (`GUID_ACDC_POWER_SOURCE`).
const POWER_SOURCE: GUID = GUID::from_u128(0x5d3e9a59_e9d5_4b00_a6bd_ff34ff516548);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    /// 0–100
    pub percent: u8,
    /// Prize takılı.
    pub plugged: bool,
    /// Şu an şarj oluyor (takılı ama dolu olabilir).
    pub charging: bool,
    /// Windows pil tasarrufu açık.
    pub saver: bool,
    /// Windows'un tahmini kalan süresi (yalnız pilden çalışırken ve biliniyorsa).
    pub minutes_left: Option<u32>,
}

/// `GetSystemPowerStatus` sonucunu çevirir; pil yoksa ya da bilinmiyorsa `None`.
pub fn battery_from(status: &SYSTEM_POWER_STATUS) -> Option<Battery> {
    const NO_BATTERY: u8 = 128;
    const UNKNOWN: u8 = 255;
    const CHARGING: u8 = 8;
    if status.BatteryFlag == UNKNOWN || status.BatteryFlag & NO_BATTERY != 0 || status.BatteryLifePercent > 100 {
        return None;
    }
    let plugged = status.ACLineStatus == 1;
    Some(Battery {
        percent: status.BatteryLifePercent,
        plugged,
        charging: status.BatteryFlag & CHARGING != 0,
        saver: status.SystemStatusFlag == 1,
        minutes_left: (!plugged && status.BatteryLifeTime != u32::MAX).then_some(status.BatteryLifeTime / 60),
    })
}

fn read() -> Option<Battery> {
    let mut status = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    battery_from(&status)
}

struct Power {
    app: AppHandle,
    hwnd: Hwnd,
    /// Kayıtlı bildirimler (sayı olarak; iş parçacıkları arasında taşınır).
    handles: Mutex<Vec<isize>>,
    last: Mutex<Option<Battery>>,
}

static POWER: OnceLock<Power> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Ada penceresine güç iletisi dinleyicisi takar; kayıt yalnız özellik açıkken yapılır.
pub fn init(app: &AppHandle, hwnd: Hwnd, enabled: bool) {
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_POWERBROADCAST
            && matches!(wparam.0 as u32, PBT_APMPOWERSTATUSCHANGE | PBT_POWERSETTINGCHANGE)
            && ENABLED.load(Ordering::Relaxed)
        {
            publish();
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }
    if POWER.set(Power { app: app.clone(), hwnd, handles: Mutex::new(Vec::new()), last: Mutex::new(None) }).is_err() {
        return;
    }
    unsafe {
        let _ = SetWindowSubclass(hwnd.raw(), Some(proc), 2, 0);
    }
    set_enabled(app, enabled);
}

/// Pil durumunu okur ve değiştiyse arayüze gönderir.
fn publish() {
    let Some(power) = POWER.get() else { return };
    let next = read();
    let mut last = power.last.lock().unwrap_or_else(|p| p.into_inner());
    if *last == next {
        return;
    }
    *last = next;
    let _ = power.app.emit("power://battery", next);
}

/// Ayarlardan: pil özelliklerinden biri açıksa Windows'a kayıt olunur, hepsi kapalıysa kayıt bırakılır.
pub fn set_enabled(_app: &AppHandle, on: bool) {
    let Some(power) = POWER.get() else { return };
    if ENABLED.swap(on, Ordering::Relaxed) == on {
        return;
    }
    let mut handles = power.handles.lock().unwrap_or_else(|p| p.into_inner());
    if on {
        for guid in [BATTERY_PERCENT, POWER_SOURCE] {
            let recipient = HANDLE(power.hwnd.raw().0);
            if let Ok(handle) = unsafe { RegisterPowerSettingNotification(recipient, &guid, DEVICE_NOTIFY_WINDOW_HANDLE) } {
                handles.push(handle.0);
            }
        }
        drop(handles);
        publish();
    } else {
        for handle in handles.drain(..) {
            let _ = unsafe { UnregisterPowerSettingNotification(HPOWERNOTIFY(handle as _)) };
        }
        *power.last.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

/// Ayarlar → Özellikler: bu bilgisayarda pil var mı (sayfa açılınca bir kez okunur).
#[tauri::command]
pub fn power_present() -> bool {
    read().is_some()
}

/// Geçerli pil durumu; özellik kapalıysa ya da pil yoksa `None`.
#[tauri::command]
pub fn power_status() -> Option<Battery> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let next = read();
    if let Some(power) = POWER.get() {
        *power.last.lock().unwrap_or_else(|p| p.into_inner()) = next;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(ac: u8, flag: u8, percent: u8, saver: u8, life: u32) -> SYSTEM_POWER_STATUS {
        SYSTEM_POWER_STATUS {
            ACLineStatus: ac,
            BatteryFlag: flag,
            BatteryLifePercent: percent,
            SystemStatusFlag: saver,
            BatteryLifeTime: life,
            BatteryFullLifeTime: u32::MAX,
        }
    }

    #[test]
    fn desktop_without_battery_has_no_battery() {
        assert_eq!(battery_from(&status(1, 128, 255, 0, u32::MAX)), None);
        assert_eq!(battery_from(&status(1, 255, 255, 0, u32::MAX)), None);
    }

    #[test]
    fn laptop_on_battery_and_charging() {
        let on_battery = battery_from(&status(0, 1, 64, 0, 2 * 3600 + 15 * 60)).unwrap();
        assert_eq!(on_battery, Battery { percent: 64, plugged: false, charging: false, saver: false, minutes_left: Some(135) });
        let charging = battery_from(&status(1, 8, 40, 0, u32::MAX)).unwrap();
        assert!(charging.plugged && charging.charging && charging.minutes_left.is_none());
        let saver = battery_from(&status(0, 2, 18, 1, u32::MAX)).unwrap();
        assert!(saver.saver && saver.minutes_left.is_none());
    }
}
