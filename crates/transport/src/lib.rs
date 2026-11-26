//! Transport réseau local.
//!
//! * [`channel`] : trames chiffrées Noise sur un flux TCP (longueur u16 + message Noise).
//! * [`handshake`] : préambule, handshake `IK` (appareils associés) et association
//!   `SPAKE2 → XXpsk3`.
//! * [`discovery`] : annonce/recherche mDNS `_nothingconnect._tcp` sur le réseau local.
//!
//! Aucun serveur Internet n'est utilisé.

pub mod channel;
#[cfg(feature = "mdns")]
pub mod discovery;
pub mod handshake;

use thiserror::Error;

/// Port TCP par défaut (configurable). Choisi hors des plages IANA enregistrées courantes.
pub const DEFAULT_PORT: u16 = 47820;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("E/S : {0}")]
    Io(#[from] std::io::Error),
    #[error("noise : {0}")]
    Noise(#[from] snow::Error),
    #[error("protocole : {0}")]
    Protocol(#[from] nc_protocol::ProtocolError),
    #[error("association : {0}")]
    Pairing(#[from] nc_pairing::PairingError),
    #[error("appareil inconnu refusé")]
    UnknownPeer,
    #[error("l'appareil ne correspond pas à la clé attendue")]
    PeerMismatch,
    #[error("délai dépassé")]
    Timeout,
    #[error("connexion fermée par le pair")]
    Closed,
    #[error("association refusée : code incorrect, expiré, ou aucune association ouverte sur le PC")]
    PairingFailed,
    #[error("découverte : {0}")]
    Discovery(String),
}

impl From<tokio::time::error::Elapsed> for TransportError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        TransportError::Timeout
    }
}
