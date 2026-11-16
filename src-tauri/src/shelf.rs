//! Dosya rafı (Özellikler → "Dosya rafı"): adaya bırakılan dosyalar rafta durur, oradan başka bir
//! uygulamaya sürüklenir. Dosyalar kopyalanmaz ya da taşınmaz; raf yalnız yollarını `shelf.json`'da tutar.
//!
//! Bırakma hedefi adanın kendi görünmez alt penceresidir (catcher.rs); dinlemenin boşta maliyeti yok, iş
//! yalnız sürükleme olduğunda yapılır. Dışarı sürükleme kabuğun kendi veri nesnesiyle olur (Gezgin'den
//! sürüklemekle aynı) ve yalnız kopyalama/kısayol izni verilir: rafın dosyası yerinden taşınmaz.
//!
//! Raf → AI (Faz 6, K5): sürükleme sürerken imlecin yeri arayüze bildirilir (`shelf://over`); arayüz
//! imlecin altındaki AI oturumunu `shelf_target` ile seçer. Bırakılınca dosyalar rafa değil o oturuma
//! gider (send.rs). Raftaki kart adın üstüne sürüklenirse arayüz `shelf_send` çağırır.

pub mod catcher;
mod list;
mod send;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, DragDropEvent, Emitter, Manager, State};

use crate::debuglog;
pub use list::Shelf;
use list::{Drag, react};

pub struct ShelfHub {
    app: AppHandle,
    path: PathBuf,
    data: Mutex<Shelf>,
    /// Raftan dışarı sürükleme sürüyor: imleç kendi adamızın üstünden geçerken gelen olaylar yok sayılır.
    dragging_out: AtomicBool,
    /// Sürüklenen dosyalar bırakılınca gidecek AI oturumu (imleç adının üstünde); yoksa rafa eklenir.
    target: Mutex<Option<String>>,
}

impl ShelfHub {
    fn lock(&self) -> MutexGuard<'_, Shelf> {
        self.data.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn save(&self, shelf: &Shelf) {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.path.parent() {
                fs::create_dir_all(dir)?;
            }
            let tmp = self.path.with_extension("json.tmp");
            fs::write(&tmp, serde_json::to_vec_pretty(shelf)?)?;
            fs::rename(&tmp, &self.path)
        };
        if let Err(error) = write() {
            debuglog::line(format!("shelf: kaydedilemedi {error}"));
        }
    }

    fn changed(&self, shelf: &Shelf) {
        self.save(shelf);
        let _ = self.app.emit("shelf://changed", shelf);
    }
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let path = app.path().app_config_dir()?.join("shelf.json");
    let shelf = fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Shelf>(&bytes).ok()).unwrap_or_default();
    app.manage(Arc::new(ShelfHub { app: app.clone(), path, data: Mutex::new(shelf), dragging_out: AtomicBool::new(false), target: Mutex::new(None) }));
    Ok(())
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Tauri'nin (WebView2 üzerinden) sürükle-bırak olayı. Ada kapalıyken (ya da kendi bırakma penceremiz
/// sürüklemeyi yakalamışken) olayları o pencere verir; ada açıkken o pencere gizlidir ve dosya doğrudan
/// WebView2'nin üstüne bırakılır, o zaman bu yol kullanılır (önceden yok sayılıyordu: açık adaya bırakılan
/// dosya hiçbir yere gitmiyordu). `Over` her fare kımıldamasında gelir; yalnız imlecin yeri bildirilir.
pub fn on_drag(app: &AppHandle, event: &DragDropEvent) {
    if catcher::handles_drags() {
        return;
    }
    let to_logical = |p: &tauri::PhysicalPosition<f64>| {
        let scale = app.state::<crate::island::Shared>().with(|s| s.hwnd).map_or(1.0, crate::island::scale);
        (p.x / scale, p.y / scale)
    };
    let drag = match event {
        DragDropEvent::Enter { paths, position } => {
            let (x, y) = to_logical(position);
            over(app, x, y);
            Drag::Enter(paths.len())
        }
        DragDropEvent::Over { position } => {
            let (x, y) = to_logical(position);
            over(app, x, y);
            return;
        }
        DragDropEvent::Drop { paths, .. } => Drag::Drop(paths.iter().map(|p| (text(p), p.is_dir())).collect()),
        DragDropEvent::Leave => Drag::Leave,
        _ => return,
    };
    handle(app, drag);
}

/// İmlecin ada penceresindeki yeri (arayüz pikseli): arayüz altındaki oturum adını parlatır ve seçer.
/// Yalnız dosya sürüklenirken çağrılır; ayar kapalıysa bildirilmez.
pub(crate) fn over(app: &AppHandle, x: f64, y: f64) {
    let Some(hub) = app.try_state::<Arc<ShelfHub>>() else { return };
    if hub.dragging_out.load(Ordering::Relaxed) || !crate::settings::current(app).features.shelf {
        return;
    }
    let _ = app.emit("shelf://over", serde_json::json!({ "x": x.round(), "y": y.round() }));
}

/// Sürükleme olayını rafa uygular ve arayüze bildirir (rafı açar, ekler, oturuma gönderir ya da kapatır).
fn handle(app: &AppHandle, drag: Drag) {
    let Some(hub) = app.try_state::<Arc<ShelfHub>>() else { return };
    let on = crate::settings::current(app).features.shelf;
    // Hedef yalnız bu sürükleme için geçerli: giriş, bırakma ve vazgeçme onu siler.
    let target = hub.target.lock().unwrap_or_else(|p| p.into_inner()).take();
    let mut shelf = hub.lock();
    let paths = match &drag {
        Drag::Drop(paths) => Some(paths.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>()),
        _ => None,
    };
    let Some(signal) = react(&mut shelf, drag, on, hub.dragging_out.load(Ordering::Relaxed), target.is_some()) else { return };
    if signal.kind == "drop" {
        debuglog::line(format!("shelf: bırakıldı, {} yeni, rafta {}", signal.count, shelf.items.len()));
        hub.changed(&shelf);
    }
    drop(shelf);
    let _ = app.emit("shelf://drag", &signal);
    if let (Some(session), Some(paths), "send") = (target, paths, signal.kind) {
        // Dosya okuma (resim) ve pano bırakma geri çağrısını (ana iş parçacığı) bekletmesin.
        let app = app.clone();
        std::thread::spawn(move || {
            let sent = send::deliver(&app, &session, paths);
            let _ = app.emit("shelf://sent", &sent);
        });
    }
}

/// Sürükleme sürerken imlecin altındaki AI oturumu (arayüz seçer); `None`: rafa bırakılır.
#[tauri::command]
pub fn shelf_target(hub: State<'_, Arc<ShelfHub>>, session: Option<String>) {
    *hub.target.lock().unwrap_or_else(|p| p.into_inner()) = session;
}

/// Raftaki kartlar bir AI oturumunun adına bırakıldı (arayüzün kendi sürüklemesi).
#[tauri::command]
pub async fn shelf_send(app: AppHandle, session: String, ids: Vec<u64>) -> Result<send::Sent, String> {
    if !crate::settings::current(&app).features.shelf {
        return Err("Dosya rafı kapalı (Ayarlar → Özellikler).".into());
    }
    let hub = app.state::<Arc<ShelfHub>>().inner().clone();
    let paths = hub.lock().paths(&ids);
    if paths.iter().any(|p| !Path::new(p).exists()) {
        prune(&hub);
    }
    let sent = tauri::async_runtime::spawn_blocking(move || send::deliver(&app, &session, paths)).await.map_err(|e| e.to_string())?;
    Ok(sent)
}

/// Raf; yerinden silinmiş ya da taşınmış dosyalar düşülür (açılırken bir kez bakılır).
#[tauri::command]
pub fn shelf_snapshot(hub: State<'_, Arc<ShelfHub>>) -> Shelf {
    let mut shelf = hub.lock();
    if shelf.retain_existing(|p| Path::new(p).exists()) {
        hub.save(&shelf);
    }
    shelf.clone()
}

/// Bir öğeyi ya da (`id` yoksa) hepsini raftan kaldırır; dosyalar yerinde kalır.
#[tauri::command]
pub fn shelf_remove(hub: State<'_, Arc<ShelfHub>>, id: Option<u64>) -> Shelf {
    let mut shelf = hub.lock();
    shelf.remove(id);
    hub.changed(&shelf);
    shelf.clone()
}

/// Dosyayı Gezgin'de seçili gösterir.
#[tauri::command]
pub fn shelf_reveal(hub: State<'_, Arc<ShelfHub>>, id: u64) -> Result<(), String> {
    let path = hub.lock().paths(&[id]).pop().ok_or("Raf öğesi bulunamadı.")?;
    if !Path::new(&path).exists() {
        prune(&hub);
        return Err("Dosya artık yerinde değil.".into());
    }
    // Boşluklu yol ("Yeni klasör") Command::arg ile tırnaklanınca Gezgin seçimi anlamaz; ham argüman şart.
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{path}\"")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Yerinden silinmiş ya da taşınmış dosyaları raftan düşürür ve arayüze bildirir (açılışta bir kez bakmak yetmez).
fn prune(hub: &ShelfHub) {
    let mut shelf = hub.lock();
    if shelf.retain_existing(|p| Path::new(p).exists()) {
        hub.changed(&shelf);
    }
}

/// Seçili öğeleri sürüklemeye başlar (fare tuşu basılıyken çağrılır). Sürükleme bitene dek bekler;
/// `true`: bir yere bırakıldı.
#[tauri::command]
pub async fn shelf_drag_out(app: AppHandle, ids: Vec<u64>) -> Result<bool, String> {
    let hub = app.state::<Arc<ShelfHub>>().inner().clone();
    if !crate::settings::current(&app).features.shelf {
        return Err("Dosya rafı kapalı (Ayarlar → Özellikler).".into());
    }
    let paths: Vec<String> = hub.lock().paths(&ids).into_iter().filter(|p| Path::new(p).exists()).collect();
    if paths.len() < ids.len() {
        prune(&hub);
    }
    if paths.is_empty() {
        return Ok(false);
    }
    let hwnd = app.state::<crate::island::Shared>().with(|s| s.hwnd);
    let (tx, rx) = std::sync::mpsc::channel();
    // OLE sürüklemesi ana (STA) iş parçacığında, kendi ileti döngüsüyle çalışır.
    let worker = hub.clone();
    app.run_on_main_thread(move || {
        worker.dragging_out.store(true, Ordering::Relaxed);
        let result = drag::start(hwnd, &paths);
        worker.dragging_out.store(false, Ordering::Relaxed);
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .map_err(|e| {
            debuglog::line(format!("shelf: sürükleme başlamadı {e}"));
            "Dosya sürüklenemedi.".to_string()
        })
}

mod drag {
    use windows::Win32::System::Com::{CoTaskMemFree, IDataObject};
    use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_NONE, IDropSource};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{BHID_DataObject, IShellItemArray, SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHParseDisplayName};
    use windows::core::{HSTRING, Result};

    use crate::win::Hwnd;

    /// Kimlik listeleri iş bitince serbest bırakılır.
    struct Pidls(Vec<*mut ITEMIDLIST>);

    impl Drop for Pidls {
        fn drop(&mut self) {
            for pidl in &self.0 {
                unsafe { CoTaskMemFree(Some(*pidl as *const _)) };
            }
        }
    }

    /// Kabuğun veri nesnesiyle (Gezgin'in kendi biçimleri) sürükler; varsayılan bırakma kaynağı ve
    /// sürükleme görüntüsü kabuktan gelir.
    pub fn start(hwnd: Option<Hwnd>, paths: &[String]) -> Result<bool> {
        let mut pidls = Pidls(Vec::with_capacity(paths.len()));
        for path in paths {
            let mut pidl = std::ptr::null_mut();
            unsafe { SHParseDisplayName(&HSTRING::from(path.as_str()), None, &mut pidl, 0, None)? };
            pidls.0.push(pidl);
        }
        let list: Vec<*const ITEMIDLIST> = pidls.0.iter().map(|p| *p as *const _).collect();
        let items: IShellItemArray = unsafe { SHCreateShellItemArrayFromIDLists(&list)? };
        let data: IDataObject = unsafe { items.BindToHandler(None, &BHID_DataObject)? };
        let effect = unsafe { SHDoDragDrop(hwnd.map(Hwnd::raw), &data, None::<&IDropSource>, DROPEFFECT_COPY | DROPEFFECT_LINK)? };
        Ok(effect != DROPEFFECT_NONE)
    }
}
