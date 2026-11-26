//! Commandes appelées par l'interface (invoke). Chaque commande renvoie une erreur lisible.

use crate::AppState;
use nc_core::protocol::{MediaAction, Message, RemoteCommand, ShareKind};
use nc_core::{DeviceRecord, NotificationRecord, PeerLink, Settings, TransferRecord};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

fn uuid(s: &str) -> R<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(e)
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    protocol: u16,
    fingerprint: String,
    port: Option<u16>,
    lan_ip: Option<String>,
    data_dir: String,
    logs_dir: String,
}

fn lan_ip() -> Option<String> {
    local_ip_address::local_ip().ok().map(|ip| ip.to_string())
}

#[tauri::command]
fn app_info(s: State<AppState>) -> AppInfo {
    let node = s.node.lock().unwrap();
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        protocol: nc_core::protocol::PROTOCOL_VERSION,
        fingerprint: node.fingerprint(),
        port: node.listen_addr().map(|a| a.port()),
        lan_ip: lan_ip(),
        data_dir: s.data_dir.to_string_lossy().into_owned(),
        logs_dir: s.data_dir.join("logs").to_string_lossy().into_owned(),
    }
}

#[derive(Serialize)]
pub struct DevicesView {
    devices: Vec<DeviceRecord>,
    connected: Vec<PeerLink>,
}

#[tauri::command]
fn devices(s: State<AppState>) -> R<DevicesView> {
    let node = s.node.lock().unwrap();
    Ok(DevicesView {
        devices: node.devices().map_err(e)?,
        connected: node.connected_peers(),
    })
}

#[derive(Serialize)]
pub struct PairingView {
    code: String,
    expires_in_s: u64,
    qr_svg: Option<String>,
    fingerprint: String,
    address: Option<String>,
}

#[tauri::command]
fn open_pairing(s: State<AppState>) -> R<PairingView> {
    let ip = lan_ip();
    let node = s.node.lock().unwrap();
    let info = node.open_pairing(ip.as_deref()).map_err(e)?;
    let qr_svg = info.qr_uri.as_deref().and_then(|uri| {
        qrcode::QrCode::new(uri.as_bytes()).ok().map(|q| {
            q.render::<qrcode::render::svg::Color>()
                .min_dimensions(220, 220)
                .quiet_zone(true)
                .dark_color(qrcode::render::svg::Color("#000000"))
                .light_color(qrcode::render::svg::Color("#ffffff"))
                .build()
        })
    });
    let address = ip
        .zip(node.listen_addr())
        .map(|(ip, a)| format!("{ip}:{}", a.port()));
    Ok(PairingView {
        code: info.code,
        expires_in_s: info.expires_in_s,
        qr_svg,
        fingerprint: info.fingerprint,
        address,
    })
}

#[tauri::command]
fn close_pairing(s: State<AppState>) {
    s.node.lock().unwrap().close_pairing();
}

#[tauri::command]
fn confirm_pairing(s: State<AppState>, peer_id: String, accept: bool) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .confirm_pairing(&peer_id, accept)
        .map_err(e)
}

#[tauri::command]
fn rename_device(s: State<AppState>, peer_id: String, name: Option<String>) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .rename_device(&peer_id, name.as_deref())
        .map_err(e)
}

#[tauri::command]
fn set_auto_accept(s: State<AppState>, peer_id: String, on: bool) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .set_auto_accept(&peer_id, on)
        .map_err(e)
}

#[tauri::command]
fn remove_device(s: State<AppState>, peer_id: String) -> R<()> {
    s.node.lock().unwrap().remove_device(&peer_id).map_err(e)
}

#[tauri::command]
fn send_paths(s: State<AppState>, peer_id: String, paths: Vec<String>) -> R<String> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    s.node
        .lock()
        .unwrap()
        .send_paths(&peer_id, paths)
        .map(|id| id.to_string())
        .map_err(e)
}

#[tauri::command]
fn send_text(s: State<AppState>, peer_id: String, content: String) -> R<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Texte vide".into());
    }
    let is_link = !trimmed.contains(char::is_whitespace)
        && (trimmed.starts_with("http://") || trimmed.starts_with("https://"));
    let kind = if is_link {
        ShareKind::Link
    } else {
        ShareKind::Text
    };
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::Share {
                kind,
                content: trimmed.into(),
            },
        )
        .map_err(e)
}

#[tauri::command]
fn accept_transfer(s: State<AppState>, id: String, accept: bool) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .accept_transfer(&uuid(&id)?, accept)
        .map_err(e)
}

#[tauri::command]
fn pause_transfer(s: State<AppState>, id: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .pause_transfer(&uuid(&id)?)
        .map_err(e)
}

#[tauri::command]
fn resume_transfer(s: State<AppState>, id: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .resume_transfer(&uuid(&id)?)
        .map_err(e)
}

#[tauri::command]
fn cancel_transfer(s: State<AppState>, id: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .cancel_transfer(&uuid(&id)?)
        .map_err(e)
}

#[tauri::command]
fn transfers(s: State<AppState>, limit: u32) -> R<Vec<TransferRecord>> {
    s.node.lock().unwrap().transfers(limit).map_err(e)
}

#[tauri::command]
fn clear_transfer_history(s: State<AppState>) -> R<usize> {
    s.node.lock().unwrap().clear_transfer_history().map_err(e)
}

#[tauri::command]
fn notifications(s: State<AppState>, limit: u32) -> R<Vec<NotificationRecord>> {
    s.node.lock().unwrap().notifications(limit).map_err(e)
}

#[tauri::command]
fn clear_notifications(s: State<AppState>) -> R<usize> {
    s.node.lock().unwrap().clear_notifications().map_err(e)
}

#[tauri::command]
fn notification_apps(s: State<AppState>, peer_id: String) -> R<Vec<nc_core::protocol::AppEntry>> {
    s.node
        .lock()
        .unwrap()
        .notification_apps(&peer_id)
        .map_err(e)
}

#[tauri::command]
fn set_notification_app(
    s: State<AppState>,
    peer_id: String,
    package: String,
    enabled: bool,
) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .set_notification_app(&peer_id, &package, enabled)
        .map_err(e)
}

#[tauri::command]
fn notification_action(
    s: State<AppState>,
    peer_id: String,
    key: String,
    action_index: u32,
    reply: Option<String>,
) -> R<()> {
    let reply = reply.filter(|r| !r.trim().is_empty());
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::NotificationAction {
                key,
                action_index,
                reply,
            },
        )
        .map_err(e)
}

#[tauri::command]
fn notification_dismiss(s: State<AppState>, peer_id: String, key: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(&peer_id, Message::NotificationDismiss { key })
        .map_err(e)
}

#[tauri::command]
fn notification_open(s: State<AppState>, peer_id: String, key: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(&peer_id, Message::NotificationOpen { key })
        .map_err(e)
}

#[tauri::command]
fn remote_command(s: State<AppState>, peer_id: String, command: RemoteCommand) -> R<u64> {
    s.node.lock().unwrap().command(&peer_id, command).map_err(e)
}

#[tauri::command]
fn media_command(
    s: State<AppState>,
    peer_id: String,
    session_id: String,
    action: MediaAction,
) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(&peer_id, Message::MediaCommand { session_id, action })
        .map_err(e)
}

#[tauri::command]
fn get_settings(s: State<AppState>) -> Settings {
    s.node.lock().unwrap().settings()
}

#[tauri::command]
fn set_settings(s: State<AppState>, settings: Settings) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .set_settings(settings.clone())
        .map_err(e)?;
    *s.settings.write().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn snapshot(s: State<AppState>) -> crate::sink::Snapshot {
    s.snapshot.lock().unwrap().clone()
}

#[tauri::command]
fn default_download_dir() -> String {
    dirs::download_dir()
        .unwrap_or_default()
        .join("NOTHING CONNECT")
        .to_string_lossy()
        .into_owned()
}

/// Efface appareils, historique, préférences et identité, puis redémarre l'application.
#[tauri::command]
fn wipe_all(app: tauri::AppHandle, s: State<AppState>) -> R<()> {
    s.node.lock().unwrap().wipe(&*s.secrets).map_err(e)?;
    app.restart();
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        devices,
        open_pairing,
        close_pairing,
        confirm_pairing,
        rename_device,
        set_auto_accept,
        remove_device,
        send_paths,
        send_text,
        accept_transfer,
        pause_transfer,
        resume_transfer,
        cancel_transfer,
        transfers,
        clear_transfer_history,
        notifications,
        clear_notifications,
        notification_apps,
        set_notification_app,
        notification_action,
        notification_dismiss,
        notification_open,
        remote_command,
        media_command,
        get_settings,
        set_settings,
        default_download_dir,
        snapshot,
        wipe_all,
    ]
}
