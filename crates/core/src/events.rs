//! Événements émis vers l'interface (Tauri ou Android), sérialisés en JSON (`"type"`).

use nc_protocol::{AppEntry, DeviceInfo, DeviceStatus, MediaSessionInfo, Message, PhoneNotification, ShareKind};
use nc_storage::DeviceRecord;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Offered,
    Waiting,
    Running,
    Paused,
    Interrupted,
    Completed,
    Failed,
    Cancelled,
    Rejected,
}

impl TransferState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Offered => "offered",
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Interrupted => "interrupted",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
        }
    }
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled | Self::Rejected)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferFileView {
    pub rel_path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Started { fingerprint: String, port: Option<u16> },
    PairingWindow { code: String, expires_in_s: u64, qr_uri: Option<String> },
    PairingWindowClosed,
    /// Côté PC : un téléphone a prouvé le code, l'utilisateur doit confirmer.
    PairingRequest { peer_id: String, device: DeviceInfo, fingerprint: String },
    Paired { device: DeviceRecord },
    PairingFailed { reason: String },
    PeerConnected { peer_id: String, addr: String, link: &'static str },
    PeerDisconnected { peer_id: String, reason: String },
    /// Le pair a refusé notre clé : association révoquée de son côté.
    PeerRefused { peer_id: String },
    DeviceRemoved { peer_id: String },
    IncompatibleVersion { peer_id: Option<String>, remote: u16, local: u16 },
    Status { peer_id: String, status: DeviceStatus },
    Notification { peer_id: String, notification: PhoneNotification },
    NotificationRemoved { peer_id: String, key: String },
    NotificationApps { peer_id: String, apps: Vec<AppEntry> },
    MediaSessions { peer_id: String, sessions: Vec<MediaSessionInfo> },
    Share { peer_id: String, kind: ShareKind, content: String },
    CommandResult { peer_id: String, id: u64, ok: bool, error: Option<String> },
    /// Demande que la plateforme doit exécuter (commande, action de notification, média…).
    Request { peer_id: String, message: Message },
    TransferIncoming { id: Uuid, peer_id: String, files: Vec<TransferFileView>, total_bytes: u64 },
    TransferProgress {
        id: Uuid,
        outgoing: bool,
        state: TransferState,
        bytes_done: u64,
        total_bytes: u64,
        speed_bps: u64,
        eta_s: Option<u64>,
        current_file: Option<String>,
    },
    TransferFileDone { id: Uuid, index: u32, path: Option<String>, outcome: &'static str },
    TransferFinished { id: Uuid, outgoing: bool, state: TransferState, error: Option<String> },
    Error { message: String },
}

/// Récepteur des événements (implémenté par la couche d'application).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: Event);
}

/// Récepteur qui ignore tout (tests).
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _: Event) {}
}
