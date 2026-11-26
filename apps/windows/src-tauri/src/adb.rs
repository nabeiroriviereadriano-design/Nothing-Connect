//! Liaison USB par adb.
//!
//! Quand le téléphone est branché avec le « débogage USB » activé et qu'adb est installé
//! (Android SDK Platform-Tools, gratuit, non redistribué avec l'application), on crée un
//! tunnel `adb reverse` : le téléphone joint alors le PC par le câble (127.0.0.1:47820),
//! et la liaison est prioritaire sur le Wi-Fi. Sans adb, rien ne change : le Wi-Fi est utilisé.

use crate::AppState;
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct UsbStatus {
    /// Chemin d'adb trouvé, ou null.
    pub adb: Option<String>,
    /// Téléphones vus par adb : (numéro de série, état « device » / « unauthorized » / …).
    pub devices: Vec<(String, String)>,
    /// Numéros de série dont le tunnel est actif.
    pub tunneled: Vec<String>,
}

pub static STATUS: Mutex<Option<UsbStatus>> = Mutex::new(None);

/// Dossier des outils USB installés par l'application (voir `install_tools`).
static TOOLS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Réveil immédiat de la surveillance USB (périphérique branché ou débranché, réglage).
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

pub fn wake() {
    *WAKE.0.lock().unwrap() = true;
    WAKE.1.notify_all();
}

fn wait(timeout: Duration) {
    let g = WAKE.0.lock().unwrap();
    let (mut g, _) = WAKE.1.wait_timeout_while(g, timeout, |w| !*w).unwrap();
    *g = false;
}

fn candidates() -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    let mut v: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).collect())
        .unwrap_or_default();
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(d) = std::env::var_os(var) {
            v.push(PathBuf::from(d).join("platform-tools").join(exe));
        }
    }
    if let Some(d) = dirs::data_local_dir() {
        v.push(
            d.join("Android")
                .join("Sdk")
                .join("platform-tools")
                .join(exe),
        );
    }
    v.push(PathBuf::from(r"C:\platform-tools").join(exe));
    if let Some(d) = TOOLS_DIR.get() {
        v.push(d.join(exe));
    }
    v
}

pub fn find_adb() -> Option<PathBuf> {
    candidates().into_iter().find(|p| p.is_file())
}

pub fn run(adb: &PathBuf, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(adb);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW : pas de fenêtre console
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn parse_devices(out: &str) -> Vec<(String, String)> {
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.to_string()))
        })
        .collect()
}

pub fn start(app: AppHandle, tools: PathBuf) {
    let _ = TOOLS_DIR.set(tools);
    watch_devices();
    std::thread::Builder::new()
        .name("nc-usb".into())
        .spawn(move || {
            let mut tunneled: HashSet<String> = HashSet::new();
            let mut last_refresh = Instant::now();
            let mut pending = false;
            loop {
                // Branchement détecté par Windows : réaction immédiate. Téléphone branché mais
                // pas encore autorisé : on revérifie chaque seconde.
                wait(if pending {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(3)
                });
                let state = app.state::<AppState>();
                if !state.settings.read().unwrap().usb_adb {
                    continue;
                }
                let port = state
                    .node
                    .lock()
                    .unwrap()
                    .listen_addr()
                    .map(|a| a.port())
                    .unwrap_or(nc_core::DEFAULT_PORT);
                let adb = find_adb();
                let mut status = UsbStatus {
                    adb: adb.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    ..Default::default()
                };
                if let Some(adb) = &adb {
                    status.devices = run(adb, &["devices"])
                        .map(|o| parse_devices(&o))
                        .unwrap_or_default();
                    let ready: HashSet<String> = status
                        .devices
                        .iter()
                        .filter(|(_, s)| s == "device")
                        .map(|(d, _)| d.clone())
                        .collect();
                    tunneled.retain(|d| ready.contains(d));
                    // Le tunnel est recréé régulièrement (il disparaît si adb redémarre).
                    if last_refresh.elapsed() > Duration::from_secs(30) {
                        tunneled.clear();
                        last_refresh = Instant::now();
                    }
                    for d in ready.difference(&tunneled.clone()) {
                        let spec_phone = format!("tcp:{}", nc_core::DEFAULT_PORT);
                        let spec_pc = format!("tcp:{port}");
                        if run(adb, &["-s", d, "reverse", &spec_phone, &spec_pc]).is_some() {
                            tunneled.insert(d.clone());
                        }
                    }
                    status.tunneled = tunneled.iter().cloned().collect();
                    pending = status
                        .devices
                        .iter()
                        .any(|(d, s)| s != "device" || !tunneled.contains(d));
                }
                let changed = STATUS.lock().unwrap().as_ref() != Some(&status);
                if changed {
                    *STATUS.lock().unwrap() = Some(status.clone());
                    let _ = app.emit("nc-usb", &status);
                }
            }
        })
        .expect("thread USB");
}

/// Windows signale chaque branchement USB : la surveillance repart aussitôt (au lieu
/// d'attendre jusqu'à 3 s).
#[cfg(windows)]
fn watch_devices() {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Register_Notification, CM_NOTIFY_ACTION, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
        CM_NOTIFY_FILTER_0, CM_NOTIFY_FILTER_0_0, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        CR_SUCCESS, HCMNOTIFICATION,
    };
    unsafe extern "system" fn on_change(
        _h: HCMNOTIFICATION,
        _ctx: *const core::ffi::c_void,
        _action: CM_NOTIFY_ACTION,
        _data: *const CM_NOTIFY_EVENT_DATA,
        _size: u32,
    ) -> u32 {
        // Le tunnel ne peut être créé qu'une fois le téléphone énuméré par adb : petit délai.
        std::thread::spawn(|| {
            for _ in 0..3 {
                wake();
                std::thread::sleep(Duration::from_millis(700));
            }
        });
        0
    }
    // GUID_DEVINTERFACE_USB_DEVICE
    const USB: windows::core::GUID =
        windows::core::GUID::from_u128(0xa5dcbf10_6530_11d2_901f_00c04fb951ed);
    let filter = CM_NOTIFY_FILTER {
        cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
        Flags: 0,
        FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
        Reserved: 0,
        u: CM_NOTIFY_FILTER_0 {
            DeviceInterface: CM_NOTIFY_FILTER_0_0 { ClassGuid: USB },
        },
    };
    let mut h = HCMNOTIFICATION::default();
    let r = unsafe { CM_Register_Notification(&filter, None, Some(on_change), &mut h) };
    if r != CR_SUCCESS {
        tracing::debug!("notifications USB indisponibles : {r:?}");
    }
    // La notification reste active pendant toute la vie de l'application.
    std::mem::forget(h);
}

#[cfg(not(windows))]
fn watch_devices() {}

/// Télécharge les outils USB officiels de Google (adb) dans le dossier de l'application.
pub fn install_tools() -> Result<String, String> {
    let dir = TOOLS_DIR.get().ok_or("dossier inconnu")?.clone();
    let url = if cfg!(windows) {
        "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"
    } else {
        "https://dl.google.com/android/repository/platform-tools-latest-linux.zip"
    };
    let resp = ureq::get(url)
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| format!("téléchargement impossible : {e}"))?;
    let mut bytes = Vec::new();
    let mut reader = std::io::Read::take(resp.into_reader(), 64 << 20);
    std::io::Read::read_to_end(&mut reader, &mut bytes).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("archive illisible : {e}"))?;
    let tmp = dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        // Seuls adb et ses bibliothèques sont utiles ; chemins de l'archive vérifiés.
        let Some(rel) = f.enclosed_name() else {
            continue;
        };
        let Some(name) = rel.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        let keep = name == "adb"
            || name == "adb.exe"
            || name.to_lowercase().starts_with("adbwin")
            || name == "NOTICE.txt";
        if !keep || f.is_dir() {
            continue;
        }
        let mut out = std::fs::File::create(tmp.join(&name)).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(tmp.join(&name), std::fs::Permissions::from_mode(0o755));
        }
    }
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    if !tmp.join(exe).is_file() {
        return Err("adb absent de l'archive téléchargée".into());
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir).map_err(|e| e.to_string())?;
    wake();
    Ok(dir.join(exe).to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_adb_devices() {
        let out = "List of devices attached\nR58N123\tdevice\nemulator-5554\tunauthorized\n\n";
        assert_eq!(
            super::parse_devices(out),
            vec![
                ("R58N123".into(), "device".into()),
                ("emulator-5554".into(), "unauthorized".into())
            ]
        );
    }
}
