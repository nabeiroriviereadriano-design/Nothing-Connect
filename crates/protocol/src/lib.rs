//! Protocole NOTHING CONNECT.
//!
//! Deux couches :
//! * **préambule en clair** (8 octets) envoyé par l'initiateur avant le handshake Noise :
//!   `b"NCN1"` + version de protocole (u16 BE) + mode (u8) + réservé (u8) ;
//! * **trames chiffrées** : chaque message Noise transporte un [`Frame`] dont le premier
//!   octet indique le type (`1` = contrôle JSON, `2` = bloc de données binaire).
//!
//! Les blocs de fichiers ne passent jamais par JSON : ils sont copiés tels quels après un
//! en-tête fixe de 28 octets (voir [`DataChunk`]).

mod messages;
pub use messages::*;

use thiserror::Error;
use uuid::Uuid;

/// Version courante du protocole. Incrémentée à chaque changement incompatible.
pub const PROTOCOL_VERSION: u16 = 1;
/// Plus ancienne version encore acceptée par cette build.
pub const MIN_PROTOCOL_VERSION: u16 = 1;
/// Magie du préambule.
pub const MAGIC: [u8; 4] = *b"NCN1";
/// Taille maximale d'un message Noise (spécification Noise).
pub const NOISE_MAX_MSG: usize = 65_535;
/// Surcoût AEAD Noise (tag Poly1305).
pub const NOISE_TAG: usize = 16;
/// Taille maximale d'un texte clair de trame.
pub const MAX_FRAME_PLAINTEXT: usize = NOISE_MAX_MSG - NOISE_TAG;
/// En-tête d'un bloc de données : type(1) + transfert(16) + index(4) + offset(8).
pub const DATA_HEADER: usize = 1 + 16 + 4 + 8;
/// Charge utile maximale d'un bloc de fichier.
pub const MAX_CHUNK: usize = MAX_FRAME_PLAINTEXT - DATA_HEADER;
/// Taille maximale acceptée pour un message de contrôle JSON (protection entrées malformées).
pub const MAX_CONTROL_JSON: usize = MAX_FRAME_PLAINTEXT - 1;

const KIND_CONTROL: u8 = 1;
const KIND_DATA: u8 = 2;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("trame vide")]
    Empty,
    #[error("type de trame inconnu : {0}")]
    UnknownKind(u8),
    #[error("trame tronquée")]
    Truncated,
    #[error("trame trop grande ({0} octets)")]
    TooLarge(usize),
    #[error("JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("préambule invalide")]
    BadPreamble,
    #[error("version de protocole incompatible : distante {remote}, acceptée {min}..={max}")]
    IncompatibleVersion { remote: u16, min: u16, max: u16 },
}

/// Mode demandé par l'initiateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    /// Appareils déjà associés : Noise IK avec clés épinglées.
    Paired = 0,
    /// Association : SPAKE2 puis Noise XXpsk3.
    Pairing = 1,
}

/// Préambule en clair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preamble {
    pub version: u16,
    pub mode: SessionMode,
}

impl Preamble {
    pub const LEN: usize = 8;

    pub fn new(mode: SessionMode) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            mode,
        }
    }

    pub fn encode(&self) -> [u8; Self::LEN] {
        let v = self.version.to_be_bytes();
        [
            MAGIC[0],
            MAGIC[1],
            MAGIC[2],
            MAGIC[3],
            v[0],
            v[1],
            self.mode as u8,
            0,
        ]
    }

    pub fn decode(b: &[u8; Self::LEN]) -> Result<Self, ProtocolError> {
        if b[..4] != MAGIC {
            return Err(ProtocolError::BadPreamble);
        }
        let version = u16::from_be_bytes([b[4], b[5]]);
        let mode = match b[6] {
            0 => SessionMode::Paired,
            1 => SessionMode::Pairing,
            _ => return Err(ProtocolError::BadPreamble),
        };
        Ok(Self { version, mode })
    }

    /// Vérifie que la version distante est acceptée.
    pub fn check_version(&self) -> Result<(), ProtocolError> {
        check_version(self.version)
    }
}

pub fn check_version(remote: u16) -> Result<(), ProtocolError> {
    if (MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION).contains(&remote) {
        Ok(())
    } else {
        Err(ProtocolError::IncompatibleVersion {
            remote,
            min: MIN_PROTOCOL_VERSION,
            max: PROTOCOL_VERSION,
        })
    }
}

/// Bloc de données d'un fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataChunk {
    pub transfer: Uuid,
    pub file_index: u32,
    pub offset: u64,
    pub data: Vec<u8>,
}

/// Trame déchiffrée.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Control(Message),
    Data(DataChunk),
}

impl Frame {
    /// Encode dans `out` (vidé au préalable) pour éviter les allocations répétées.
    pub fn encode_into(&self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
        out.clear();
        match self {
            Frame::Control(m) => {
                out.push(KIND_CONTROL);
                serde_json::to_writer(&mut *out, m)?;
            }
            Frame::Data(c) => {
                encode_data_header(out, &c.transfer, c.file_index, c.offset);
                out.extend_from_slice(&c.data);
            }
        }
        if out.len() > MAX_FRAME_PLAINTEXT {
            return Err(ProtocolError::TooLarge(out.len()));
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let mut v = Vec::new();
        self.encode_into(&mut v)?;
        Ok(v)
    }

    pub fn decode(b: &[u8]) -> Result<Self, ProtocolError> {
        if b.len() > MAX_FRAME_PLAINTEXT {
            return Err(ProtocolError::TooLarge(b.len()));
        }
        let (&kind, rest) = b.split_first().ok_or(ProtocolError::Empty)?;
        match kind {
            KIND_CONTROL => Ok(Frame::Control(serde_json::from_slice(rest)?)),
            KIND_DATA => {
                if b.len() < DATA_HEADER {
                    return Err(ProtocolError::Truncated);
                }
                let transfer = Uuid::from_slice(&b[1..17]).map_err(|_| ProtocolError::Truncated)?;
                let file_index = u32::from_be_bytes(b[17..21].try_into().unwrap());
                let offset = u64::from_be_bytes(b[21..29].try_into().unwrap());
                Ok(Frame::Data(DataChunk {
                    transfer,
                    file_index,
                    offset,
                    data: b[DATA_HEADER..].to_vec(),
                }))
            }
            k => Err(ProtocolError::UnknownKind(k)),
        }
    }
}

/// Écrit l'en-tête d'un bloc ; l'appelant ajoute ensuite les données (zéro copie intermédiaire).
pub fn encode_data_header(out: &mut Vec<u8>, transfer: &Uuid, file_index: u32, offset: u64) {
    out.push(KIND_DATA);
    out.extend_from_slice(transfer.as_bytes());
    out.extend_from_slice(&file_index.to_be_bytes());
    out.extend_from_slice(&offset.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_roundtrip() {
        let p = Preamble::new(SessionMode::Pairing);
        assert_eq!(Preamble::decode(&p.encode()).unwrap(), p);
        assert!(p.check_version().is_ok());
    }

    #[test]
    fn preamble_rejects_bad_magic_and_mode() {
        let mut b = Preamble::new(SessionMode::Paired).encode();
        b[0] = b'X';
        assert!(Preamble::decode(&b).is_err());
        let mut b = Preamble::new(SessionMode::Paired).encode();
        b[6] = 9;
        assert!(Preamble::decode(&b).is_err());
    }

    #[test]
    fn version_window() {
        assert!(check_version(PROTOCOL_VERSION + 1).is_err());
        assert!(check_version(0).is_err());
    }

    #[test]
    fn control_roundtrip() {
        let f = Frame::Control(Message::Ping { nonce: 42 });
        assert_eq!(Frame::decode(&f.encode().unwrap()).unwrap(), f);
    }

    #[test]
    fn data_roundtrip_max_chunk() {
        let f = Frame::Data(DataChunk {
            transfer: Uuid::new_v4(),
            file_index: 3,
            offset: 1 << 40,
            data: vec![7u8; MAX_CHUNK],
        });
        let enc = f.encode().unwrap();
        assert_eq!(enc.len(), MAX_FRAME_PLAINTEXT);
        assert_eq!(Frame::decode(&enc).unwrap(), f);
    }

    #[test]
    fn rejects_malformed() {
        assert!(matches!(Frame::decode(&[]), Err(ProtocolError::Empty)));
        assert!(matches!(
            Frame::decode(&[9]),
            Err(ProtocolError::UnknownKind(9))
        ));
        assert!(matches!(
            Frame::decode(&[2, 1, 2]),
            Err(ProtocolError::Truncated)
        ));
        assert!(Frame::decode(b"\x01{not json").is_err());
        let big = vec![1u8; MAX_FRAME_PLAINTEXT + 1];
        assert!(matches!(
            Frame::decode(&big),
            Err(ProtocolError::TooLarge(_))
        ));
    }

    #[test]
    fn oversized_chunk_refused_on_encode() {
        let f = Frame::Data(DataChunk {
            transfer: Uuid::nil(),
            file_index: 0,
            offset: 0,
            data: vec![0; MAX_CHUNK + 1],
        });
        assert!(f.encode().is_err());
    }
}
