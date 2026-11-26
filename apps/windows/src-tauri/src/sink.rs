//! Relais des événements du cœur vers l'interface + intégrations Windows (toasts).
//! Ne verrouille jamais le nœud (les événements peuvent être émis pendant un appel).

use nc_core::protocol::{DeviceStatus, MediaSessionInfo, PhoneNotification};
use nc_core::{Event, EventSink, Settings, TransferState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// Dernier état connu par téléphone : permet à l'interface de se reconstruire après un
/// rechargement sans attendre le prochain envoi du téléphone.
#[derive(Default, Clone, Serialize)]
pub struct Snapshot {
    pub status: HashMap<String, DeviceStatus>,
    pub media: HashMap<String, Vec<MediaSessionInfo>>,
    pub live: Vec<PhoneNotification>,
}
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

pub struct TauriSink {
    app: AppHandle,
    settings: Arc<RwLock<Settings>>,
    pub snapshot: Arc<Mutex<Snapshot>>,
    media: Arc<crate::media::Media>,
}

impl TauriSink {
    pub fn new(
        app: AppHandle,
        settings: Arc<RwLock<Settings>>,
        snapshot: Arc<Mutex<Snapshot>>,
        media: Arc<crate::media::Media>,
    ) -> Self {
        Self {
            app,
            settings,
            snapshot,
            media,
        }
    }

    fn remember(&self, e: &Event) {
        let mut snap = self.snapshot.lock().unwrap();
        match e {
            Event::Status { peer_id, status } => {
                snap.status.insert(peer_id.clone(), status.clone());
            }
            Event::MediaSessions { peer_id, sessions } => {
                snap.media.insert(peer_id.clone(), sessions.clone());
            }
            Event::Notification { notification, .. } => {
                snap.live.retain(|n| n.key != notification.key);
                snap.live.insert(0, notification.clone());
                snap.live.truncate(200);
            }
            Event::NotificationRemoved { key, .. } => snap.live.retain(|n| &n.key != key),
            Event::PeerDisconnected { peer_id, .. } | Event::DeviceRemoved { peer_id } => {
                snap.media.remove(peer_id);
                if matches!(e, Event::DeviceRemoved { .. }) {
                    snap.status.remove(peer_id);
                }
            }
            _ => {}
        }
    }

    fn toast(&self, title: &str, body: Option<&str>) {
        let mut b = self.app.notification().builder().title(title);
        if let Some(body) = body {
            b = b.body(body);
        }
        if let Err(e) = b.show() {
            tracing::warn!("notification Windows impossible : {e}");
        }
    }
}

impl EventSink for TauriSink {
    fn stream(&self, peer_id: &str, packet: nc_core::StreamPacket) {
        self.media.on_stream(peer_id, packet);
    }

    fn emit(&self, e: Event) {
        self.remember(&e);
        let s = self.settings.read().unwrap().clone();
        match &e {
            Event::Notification {
                notification: n, ..
            } if s.notifications_enabled && s.notification_popups => {
                let title = match &n.title {
                    Some(t) => format!("{} — {}", n.app_name, t),
                    None => n.app_name.clone(),
                };
                self.toast(&title, n.text.as_deref());
            }
            Event::TransferIncoming { files, .. } => {
                let body = if files.len() == 1 {
                    files[0].rel_path.clone()
                } else {
                    format!("{} fichiers", files.len())
                };
                self.toast(
                    "Fichiers entrants — ouvrez NOTHING CONNECT pour accepter",
                    Some(&body),
                );
                crate::tray::show_main(&self.app);
            }
            Event::PairingRequest { device, .. } => {
                self.toast("Demande d'association", Some(&device.name));
                crate::tray::show_main(&self.app);
            }
            Event::TransferFinished {
                outgoing: false,
                state: TransferState::Completed,
                ..
            } => {
                self.toast("Réception terminée", None);
            }
            Event::Clipboard { content, .. } if s.clipboard_sync => {
                crate::clipboard::apply_remote(content)
            }
            // Touchers sur l'écran étendu : traités immédiatement (pas de thread par événement).
            Event::Request {
                message:
                    nc_core::protocol::Message::DisplayPointer {
                        action,
                        x,
                        y,
                        pointer,
                    },
                ..
            } => {
                if *pointer == 0 {
                    self.media.display.pointer(*action, *x, *y);
                }
            }
            Event::PeerDisconnected { peer_id, .. } | Event::DeviceRemoved { peer_id } => {
                self.media.display.stop_if_peer(&self.app, peer_id);
                self.media.capture.lock().unwrap().take();
            }
            Event::Request { peer_id, message } => {
                // Hors du fil réseau : l'ouverture d'un périphérique audio peut prendre du temps.
                let (media, app, peer, msg) = (
                    self.media.clone(),
                    self.app.clone(),
                    peer_id.clone(),
                    message.clone(),
                );
                std::thread::spawn(move || media.on_message(&app, &peer, &msg));
            }
            Event::Share { content, .. } => {
                self.toast(
                    "Partage reçu du téléphone",
                    Some(&content.chars().take(120).collect::<String>()),
                );
            }
            _ => {}
        }
        if let Err(err) = self.app.emit("nc-event", &e) {
            tracing::warn!("événement non transmis à l'interface : {err}");
        }
    }
}
