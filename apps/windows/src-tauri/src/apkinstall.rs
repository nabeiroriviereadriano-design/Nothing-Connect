//! Installation / mise à jour de l'application Android par câble USB (adb).
//!
//! Une installation par adb n'est pas concernée par le blocage automatique de Play Protect
//! qui vise les APK téléchargés par un navigateur, une messagerie ou un gestionnaire de
//! fichiers. L'APK de la dernière version publiée est téléchargé depuis GitHub Releases,
//! son empreinte SHA-256 est vérifiée, puis il est installé par-dessus l'éventuelle version
//! existante (`adb install -r` : données et associations conservées).

use crate::adb;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use tauri::{AppHandle, Emitter};

const API: &str =
    "https://api.github.com/repos/nabeiroriviereadriano-design/Nothing-Connect/releases/latest";

#[derive(Serialize, Clone)]
struct Progress {
    stage: &'static str,
    percent: u32,
}

fn get(url: &str) -> Result<ureq::Response, String> {
    ureq::get(url)
        .set(
            "User-Agent",
            concat!("NothingConnect/", env!("CARGO_PKG_VERSION")),
        )
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("téléchargement impossible : {e}"))
}

/// Renvoie la version installée.
pub fn install(app: &AppHandle, serial: Option<String>) -> Result<String, String> {
    let adb = adb::find_adb().ok_or(
        "adb introuvable. Installez « SDK Platform-Tools » de Google (gratuit) puis relancez NOTHING CONNECT.",
    )?;
    let devices = adb::run(&adb, &["devices"])
        .map(|o| adb::parse_devices(&o))
        .unwrap_or_default();
    if devices.iter().any(|(_, s)| s == "unauthorized")
        && !devices.iter().any(|(_, s)| s == "device")
    {
        return Err("Le téléphone demande d'autoriser ce PC : acceptez « Autoriser le débogage USB » sur son écran.".into());
    }
    let serial = match serial {
        Some(s) => s,
        None => devices
            .iter()
            .find(|(_, s)| s == "device")
            .map(|(d, _)| d.clone())
            .ok_or("Aucun téléphone détecté. Branchez-le en USB et activez le « débogage USB » (Options pour les développeurs).")?,
    };

    let emit = |stage, percent| {
        let _ = app.emit("nc-apk", Progress { stage, percent });
    };
    emit("release", 0);
    let mut json = String::new();
    get(API)?
        .into_reader()
        .read_to_string(&mut json)
        .map_err(|e| e.to_string())?;
    let rel: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let version = rel["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    let assets = rel["assets"].as_array().cloned().unwrap_or_default();
    let url_of = |pred: &dyn Fn(&str) -> bool| {
        assets
            .iter()
            .find(|a| a["name"].as_str().map(pred).unwrap_or(false))
            .and_then(|a| a["browser_download_url"].as_str().map(str::to_string))
    };
    let apk_url = url_of(&|n: &str| n.ends_with(".apk"))
        .ok_or("aucun APK dans la dernière version publiée")?;
    let sums_url = url_of(&|n: &str| n == "SHA256SUMS.txt")
        .ok_or("empreintes introuvables : installation annulée")?;

    let mut sums = String::new();
    get(&sums_url)?
        .into_reader()
        .read_to_string(&mut sums)
        .map_err(|e| e.to_string())?;
    let expected = sums
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let h = it.next()?;
            let n = it.next()?;
            n.trim_start_matches('*')
                .ends_with(".apk")
                .then(|| h.to_lowercase())
        })
        .next()
        .ok_or("empreinte de l'APK introuvable : installation annulée")?;

    emit("download", 0);
    let resp = get(&apk_url)?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let path = std::env::temp_dir().join(format!("NOTHING-CONNECT-{version}.apk"));
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if let Some(pct) = (done * 100).checked_div(total) {
            emit("download", pct as u32);
        }
    }
    drop(file);
    let digest = hex::encode(hasher.finalize());
    if digest != expected {
        let _ = std::fs::remove_file(&path);
        return Err("Empreinte différente : fichier corrompu, installation annulée.".into());
    }

    emit("install", 100);
    let p = path.to_string_lossy().into_owned();
    let out = adb::run(&adb, &["-s", &serial, "install", "-r", &p]).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    if out.contains("Success") {
        Ok(version)
    } else {
        Err(format!(
            "adb a refusé l'installation. Si une version signée différemment est déjà installée, désinstallez-la d'abord. {}",
            out.trim()
        ))
    }
}
