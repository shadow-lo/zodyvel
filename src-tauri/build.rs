fn main() {
    // Bildirimdeki Windows 8+ beyanı olmadan katmanlı alt pencere (dosya rafının bırakma penceresi) kurulamaz.
    // Bildirim yalnız ASCII olmalı; Türkçe harf Windows'un okuyamadığı bir bildirim üretiyor.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("tauri build betiği çalışamadı");
}
