const COMMANDS: &[&str] = &["device_info", "create_signing_key", "sign", "authenticate", "wrap", "unwrap", "delete_keys", "set_events", "service_start", "service_stop", "post_notice", "battery_exempt", "reminders_sync", "multicast", "set_core_dir"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).android_path("android").build();
}
