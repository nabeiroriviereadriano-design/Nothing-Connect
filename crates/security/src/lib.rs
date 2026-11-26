//! Identité cryptographique d'un appareil NOTHING CONNECT.
//!
//! * Chaque installation possède une paire de clés **X25519 statique** (clé Noise).
//! * La clé privée vit **uniquement** dans un [`SecretStore`] fourni par la plateforme
//!   (Windows Credential Manager, Android Keystore via Kotlin). Elle n'est jamais écrite
//!   dans SQLite ni dans les journaux.
//! * La clé publique est l'identifiant permanent ; son empreinte courte sert à la
//!   vérification visuelle et au QR code.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;
use zeroize::Zeroizing;

/// Motif Noise des sessions entre appareils déjà associés.
pub const NOISE_PAIRED: &str = "Noise_IK_25519_ChaChaPoly_BLAKE2s";
/// Motif Noise de l'association (psk dérivée de SPAKE2).
pub const NOISE_PAIRING: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";
/// Prologue lié au protocole : empêche de rejouer un handshake dans un autre contexte.
pub const NOISE_PROLOGUE: &[u8] = b"nothing-connect/1";

const IDENTITY_KEY: &str = "identity.x25519";

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("coffre de secrets indisponible : {0}")]
    Store(String),
    #[error("clé invalide")]
    BadKey,
    #[error("noise : {0}")]
    Noise(#[from] snow::Error),
}

/// Coffre de secrets de la plateforme.
pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, SecurityError>;
    fn set(&self, name: &str, value: &[u8]) -> Result<(), SecurityError>;
    fn delete(&self, name: &str) -> Result<(), SecurityError>;
}

/// Coffre en mémoire (tests, ou appareil qui refuse tout stockage persistant).
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, Vec<u8>>>);

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, SecurityError> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn set(&self, name: &str, value: &[u8]) -> Result<(), SecurityError> {
        self.0.lock().unwrap().insert(name.into(), value.to_vec());
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<(), SecurityError> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

/// Coffre Windows : Credential Manager, chiffré par DPAPI pour l'utilisateur courant.
#[cfg(all(windows, feature = "os-keyring"))]
pub struct WindowsCredentialStore {
    pub service: String,
}

#[cfg(all(windows, feature = "os-keyring"))]
impl SecretStore for WindowsCredentialStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, SecurityError> {
        let e = keyring::Entry::new(&self.service, name)
            .map_err(|e| SecurityError::Store(e.to_string()))?;
        match e.get_secret() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SecurityError::Store(e.to_string())),
        }
    }
    fn set(&self, name: &str, value: &[u8]) -> Result<(), SecurityError> {
        keyring::Entry::new(&self.service, name)
            .and_then(|e| e.set_secret(value))
            .map_err(|e| SecurityError::Store(e.to_string()))
    }
    fn delete(&self, name: &str) -> Result<(), SecurityError> {
        let e = keyring::Entry::new(&self.service, name)
            .map_err(|e| SecurityError::Store(e.to_string()))?;
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(SecurityError::Store(e.to_string())),
        }
    }
}

/// Paire de clés statique de l'appareil.
pub struct Identity {
    private: Zeroizing<Vec<u8>>,
    public: [u8; 32],
}

impl Identity {
    /// Charge l'identité depuis le coffre, ou la crée au premier lancement.
    pub fn load_or_create(store: &dyn SecretStore) -> Result<Self, SecurityError> {
        if let Some(sk) = store.get(IDENTITY_KEY)? {
            return Self::from_private(Zeroizing::new(sk));
        }
        let id = Self::generate()?;
        store.set(IDENTITY_KEY, &id.private)?;
        Ok(id)
    }

    pub fn generate() -> Result<Self, SecurityError> {
        let kp = snow::Builder::new(NOISE_PAIRED.parse()?).generate_keypair()?;
        let public: [u8; 32] = kp
            .public
            .as_slice()
            .try_into()
            .map_err(|_| SecurityError::BadKey)?;
        Ok(Self {
            private: Zeroizing::new(kp.private),
            public,
        })
    }

    /// Reconstruit une identité depuis une clé privée (fournie par le coffre de la plateforme).
    pub fn from_secret(sk: Vec<u8>) -> Result<Self, SecurityError> {
        Self::from_private(Zeroizing::new(sk))
    }

    fn from_private(sk: Zeroizing<Vec<u8>>) -> Result<Self, SecurityError> {
        if sk.len() != 32 {
            return Err(SecurityError::BadKey);
        }
        // Recalcul via une session Noise locale : évite une dépendance x25519 supplémentaire.
        let public = derive_public(&sk)?;
        Ok(Self {
            private: sk,
            public,
        })
    }

    /// Efface l'identité (réinitialisation complète : toutes les associations deviennent caduques).
    pub fn destroy(store: &dyn SecretStore) -> Result<(), SecurityError> {
        store.delete(IDENTITY_KEY)
    }

    pub fn public(&self) -> &[u8; 32] {
        &self.public
    }
    pub fn private(&self) -> &[u8] {
        &self.private
    }
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.public)
    }
}

/// Recalcule la clé publique X25519 à partir de la privée : handshake Noise `X` (one-way)
/// vers un répondeur éphémère local, qui lit la clé statique transmise.
fn derive_public(sk: &[u8]) -> Result<[u8; 32], SecurityError> {
    // Pattern X : l'initiateur transmet sa clé statique chiffrée au répondeur.
    let params: snow::params::NoiseParams = "Noise_X_25519_ChaChaPoly_BLAKE2s".parse()?;
    let responder_kp = snow::Builder::new(params.clone()).generate_keypair()?;
    let mut init = snow::Builder::new(params.clone())
        .local_private_key(sk)?
        .remote_public_key(&responder_kp.public)?
        .build_initiator()?;
    let mut resp = snow::Builder::new(params)
        .local_private_key(&responder_kp.private)?
        .build_responder()?;
    let mut buf = [0u8; 256];
    let mut out = [0u8; 256];
    let n = init.write_message(&[], &mut buf)?;
    resp.read_message(&buf[..n], &mut out)?;
    resp.get_remote_static()
        .and_then(|k| k.try_into().ok())
        .ok_or(SecurityError::BadKey)
}

/// Empreinte lisible : 16 premiers caractères hex du SHA-256, par groupes de 4.
pub fn fingerprint(public: &[u8]) -> String {
    let h = hex::encode_upper(&Sha256::digest(public)[..8]);
    h.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

/// Comparaison en temps constant (évite les fuites par mesure de temps).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_persists_in_store() {
        let store = MemoryStore::default();
        let a = Identity::load_or_create(&store).unwrap();
        let b = Identity::load_or_create(&store).unwrap();
        assert_eq!(a.public(), b.public());
        Identity::destroy(&store).unwrap();
        let c = Identity::load_or_create(&store).unwrap();
        assert_ne!(a.public(), c.public());
    }

    #[test]
    fn derived_public_matches_generated() {
        let id = Identity::generate().unwrap();
        assert_eq!(&derive_public(id.private()).unwrap(), id.public());
    }

    #[test]
    fn fingerprint_format() {
        let f = fingerprint(&[1u8; 32]);
        assert_eq!(f.len(), 19);
        assert_eq!(f.matches('-').count(), 3);
    }

    #[test]
    fn constant_time_eq() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
    }
}
