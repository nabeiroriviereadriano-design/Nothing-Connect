//! Cœur NOTHING CONNECT, partagé par Windows (Tauri) et Android (UniFFI).
//!
//! [`Node`] possède son propre runtime tokio : l'API publique est **synchrone** et
//! non bloquante (les opérations longues tournent en tâche de fond et rendent compte via
//! des [`Event`]), ce qui simplifie les liaisons Kotlin et Tauri.

mod events;
mod node;
mod session;
mod settings;
mod speed;
mod transfers;

pub use events::{Event, EventSink, NullSink, TransferFileView, TransferState};
pub use nc_protocol as protocol;
pub use nc_security::{MemoryStore, SecretStore, SecurityError};
pub use nc_storage::{DeviceRecord, NotificationRecord, TransferRecord};
pub use nc_transfer::ConflictPolicy;
pub use node::{Node, NodeConfig, PairingInfo, PeerLink, Role};
pub use nc_transport::DEFAULT_PORT;
pub use settings::Settings;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{0}")]
    Storage(#[from] nc_storage::StorageError),
    #[error("{0}")]
    Security(#[from] nc_security::SecurityError),
    #[error("{0}")]
    Transport(#[from] nc_transport::TransportError),
    #[error("{0}")]
    Transfer(#[from] nc_transfer::TransferError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("message invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("appareil non connecté")]
    NotConnected,
    #[error("appareil inconnu")]
    UnknownDevice,
    #[error("transfert inconnu")]
    UnknownTransfer,
    #[error("opération impossible dans l'état actuel")]
    InvalidState,
    #[error("adresse invalide")]
    BadAddress,
    #[error("fonction réservée au rôle {0}")]
    WrongRole(&'static str),
}

pub type Result<T> = std::result::Result<T, CoreError>;
