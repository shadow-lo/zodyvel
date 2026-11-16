use crate::Keystore;
use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "app.zodyvel.zkeystore";

#[cfg(target_os = "android")]
pub fn init<R: Runtime, C: DeserializeOwned>(_app: &AppHandle<R>, api: PluginApi<R, C>) -> Result<Keystore<R>, Box<dyn std::error::Error>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "KeystorePlugin")?;
    Ok(Keystore(handle))
}

#[cfg(not(target_os = "android"))]
pub fn init<R: Runtime, C: DeserializeOwned>(_app: &AppHandle<R>, _api: PluginApi<R, C>) -> Result<Keystore<R>, Box<dyn std::error::Error>> {
    Err("Keystore eklentisi yalnız Android'i destekler (iOS sonra).".into())
}
