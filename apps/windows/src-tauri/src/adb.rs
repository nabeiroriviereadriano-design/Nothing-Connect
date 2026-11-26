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
use std::sync::Mutex;
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

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("nc-usb".into())
        .spawn(move || {
            let mut tunneled: HashSet<String> = HashSet::new();
            let mut last_refresh = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(3));
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
