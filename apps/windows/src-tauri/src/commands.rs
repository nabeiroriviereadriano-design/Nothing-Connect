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

/// Toutes les adresses IPv4 utilisables du PC : principale d'abord, cartes virtuelles
/// (Hyper-V, VirtualBox, VMware, WSL) en dernier. Le téléphone les essaie toutes.
fn lan_ips() -> Vec<String> {
    let mut v: Vec<(u8, String)> = Vec::new();
    let main = lan_ip();
    if let Some(m) = &main {
        v.push((0, m.clone()));
    }
    for (name, ip) in local_ip_address::list_afinet_netifas().unwrap_or_default() {
        let std::net::IpAddr::V4(ip4) = ip else {
            continue;
        };
        if ip4.is_loopback() || ip4.is_link_local() || ip4.is_unspecified() || ip4.is_multicast() {
            continue;
        }
        let s = ip4.to_string();
        if v.iter().any(|(_, x)| *x == s) {
            continue;
        }
        let n = name.to_lowercase();
        let virt = [
            "vethernet",
            "virtualbox",
            "vmware",
            "wsl",
            "hyper-v",
            "docker",
            "loopback",
        ]
        .iter()
        .any(|k| n.contains(k));
        v.push((if virt { 2 } else { 1 }, s));
    }
    v.sort_by_key(|(rank, _)| *rank);
    v.into_iter().map(|(_, s)| s).take(8).collect()
}

/// Installe les outils USB officiels de Google (adb) : liaison par câble sans démarche manuelle.
#[tauri::command]
async fn usb_tools_install() -> R<String> {
    tauri::async_runtime::spawn_blocking(crate::adb::install_tools)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn firewall_status() -> crate::firewall::FirewallStatus {
    tauri::async_runtime::spawn_blocking(crate::firewall::status)
        .await
        .unwrap_or_default()
}

#[tauri::command]
async fn firewall_allow() -> R<()> {
    tauri::async_runtime::spawn_blocking(crate::firewall::allow)
        .await
        .map_err(|e| e.to_string())?
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
    let ips = lan_ips();
    let node = s.node.lock().unwrap();
    let info = node.open_pairing(&ips).map_err(e)?;
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
    let address = node.listen_addr().filter(|_| !ips.is_empty()).map(|a| {
        ips.iter()
            .map(|ip| format!("{ip}:{}", a.port()))
            .collect::<Vec<_>>()
            .join(", ")
    });
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
    crate::adb::wake();
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

// ------------------------------------------------------------------ flux temps réel

#[tauri::command]
fn mirror_subscribe(
    s: State<AppState>,
    channel: tauri::ipc::Channel<tauri::ipc::InvokeResponseBody>,
) {
    *s.media.mirror.lock().unwrap() = Some(channel);
}

/// Demande au téléphone de diffuser son écran ; le téléphone affiche l'autorisation Android.
#[tauri::command]
fn mirror_start(
    app: tauri::AppHandle,
    s: State<AppState>,
    peer_id: String,
    max_size: u32,
    bitrate: u32,
    with_audio: bool,
) -> R<()> {
    let params = nc_core::protocol::StreamParams {
        codec: Some("h264".into()),
        max_size: Some(max_size.clamp(480, 2560)),
        bitrate: Some(bitrate.clamp(1_000_000, 40_000_000)),
        fps: Some(60),
        with_audio: Some(with_audio),
        ..Default::default()
    };
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamRequest {
                kind: nc_core::protocol::StreamKind::Mirror,
                params,
            },
        )
        .map_err(e)?;
    s.media.set_streaming(&app, true);
    Ok(())
}

#[tauri::command]
fn mirror_stop(app: tauri::AppHandle, s: State<AppState>, peer_id: String) -> R<()> {
    s.media.player.lock().unwrap().take();
    s.media.set_streaming(&app, false);
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamStop {
                kind: nc_core::protocol::StreamKind::Mirror,
                reason: None,
            },
        )
        .map_err(e)
}

#[tauri::command]
fn mirror_keyframe(s: State<AppState>, peer_id: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamKeyframe {
                kind: nc_core::protocol::StreamKind::Mirror,
            },
        )
        .map_err(e)
}

#[tauri::command]
fn mirror_input(
    s: State<AppState>,
    peer_id: String,
    event: nc_core::protocol::InputEvent,
) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(&peer_id, Message::Input(event))
        .map_err(e)
}

/// Son du téléphone sur le PC, sans l'écran (le téléphone demande l'autorisation de capture).
#[tauri::command]
fn phone_audio_start(app: tauri::AppHandle, s: State<AppState>, peer_id: String) -> R<()> {
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamRequest {
                kind: nc_core::protocol::StreamKind::PhoneAudio,
                params: Default::default(),
            },
        )
        .map_err(e)?;
    s.media.set_streaming(&app, true);
    Ok(())
}

#[tauri::command]
fn phone_audio_stop(app: tauri::AppHandle, s: State<AppState>, peer_id: String) -> R<()> {
    s.media.player.lock().unwrap().take();
    s.media.set_streaming(&app, false);
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamStop {
                kind: nc_core::protocol::StreamKind::PhoneAudio,
                reason: None,
            },
        )
        .map_err(e)
}

/// Son du PC vers le téléphone.
#[tauri::command]
fn pc_audio_start(app: tauri::AppHandle, s: State<AppState>, peer_id: String) -> R<()> {
    s.media.start_pc_audio(&app, &peer_id)?;
    s.media.set_streaming(&app, true);
    Ok(())
}

#[tauri::command]
fn pc_audio_stop(app: tauri::AppHandle, s: State<AppState>) {
    s.media.stop_pc_audio(&app);
    s.media.set_streaming(&app, false);
}

#[tauri::command]
fn pc_audio_active(s: State<AppState>) -> bool {
    s.media.capture.lock().unwrap().is_some()
}

// ------------------------------------------------------------------ USB

#[tauri::command]
fn usb_status() -> Option<crate::adb::UsbStatus> {
    crate::adb::STATUS.lock().unwrap().clone()
}

/// Installe ou met à jour l'application Android par câble (contourne le blocage Play Protect
/// des APK téléchargés, l'installation par adb n'étant pas concernée).
#[tauri::command]
async fn android_install(app: tauri::AppHandle, serial: Option<String>) -> R<String> {
    tauri::async_runtime::spawn_blocking(move || crate::apkinstall::install(&app, serial))
        .await
        .map_err(e)?
}

// ------------------------------------------------------------------ écran étendu

/// Réglages de l'interface (écran étendu, miroir…) : enregistrés à chaque changement,
/// conservés après les mises à jour.
#[tauri::command]
fn ui_prefs_get(s: State<AppState>) -> serde_json::Value {
    std::fs::read(s.data_dir.join("ui-prefs.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

#[tauri::command]
fn ui_prefs_set(s: State<AppState>, prefs: serde_json::Value) -> R<()> {
    if !prefs.is_object() {
        return Err("réglages invalides".into());
    }
    // Écriture atomique : fichier temporaire puis renommage.
    let path = s.data_dir.join("ui-prefs.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&prefs).map_err(e)?).map_err(e)?;
    std::fs::rename(&tmp, &path).map_err(e)
}

#[tauri::command]
fn display_status(app: tauri::AppHandle, s: State<AppState>) -> crate::display::DisplayStatus {
    s.media.display.status(&app)
}

/// `action` : « install », « uninstall » ou « enable-testsigning » (fenêtre UAC de Windows).
#[tauri::command]
async fn display_setup(app: tauri::AppHandle, action: String) -> R<String> {
    if !matches!(
        action.as_str(),
        "install" | "uninstall" | "enable-testsigning"
    ) {
        return Err("action inconnue".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let st = tauri::Manager::state::<AppState>(&app);
        st.media.display.setup(&app, &action)
    })
    .await
    .map_err(e)?
}

/// Demande au téléphone d'ouvrir l'écran étendu ; le moniteur est branché quand il répond.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn display_start(
    s: State<AppState>,
    peer_id: String,
    scale: f32,
    quality: u8,
    fps: u32,
    portrait: bool,
    touch: Option<bool>,
    gpu: Option<bool>,
) -> R<()> {
    s.media.display.set_quality(scale, quality, fps);
    s.media.display.set_touch(touch.unwrap_or(true));
    s.media.display.set_gpu(gpu.unwrap_or(true));
    let params = nc_core::protocol::StreamParams {
        codec: Some("jpeg".into()),
        portrait: Some(portrait),
        refresh_hz: Some(60),
        ..Default::default()
    };
    s.node
        .lock()
        .unwrap()
        .send(
            &peer_id,
            Message::StreamRequest {
                kind: nc_core::protocol::StreamKind::Display,
                params,
            },
        )
        .map_err(e)
}

/// Qualité et fréquence modifiées pendant une session : appliquées sans débrancher l'écran.
#[tauri::command]
fn display_tune(
    s: State<AppState>,
    scale: f32,
    quality: u8,
    fps: u32,
    touch: Option<bool>,
    gpu: Option<bool>,
) {
    s.media.display.set_quality(scale, quality, fps);
    if let Some(t) = touch {
        s.media.display.set_touch(t);
    }
    if let Some(g) = gpu {
        s.media.display.set_gpu(g);
    }
}

#[tauri::command]
fn display_stop(app: tauri::AppHandle, s: State<AppState>) {
    s.media.display.stop(&app);
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
        mirror_subscribe,
        mirror_start,
        mirror_stop,
        mirror_keyframe,
        mirror_input,
        phone_audio_start,
        phone_audio_stop,
        pc_audio_start,
        pc_audio_stop,
        pc_audio_active,
        usb_status,
        android_install,
        display_status,
        ui_prefs_get,
        firewall_status,
        usb_tools_install,
        firewall_allow,
        ui_prefs_set,
        display_setup,
        display_start,
        display_stop,
        display_tune,
    ]
}
