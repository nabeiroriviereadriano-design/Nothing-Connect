//! Association de deux appareils.
//!
//! Déroulé (le PC est **A**, le téléphone **B**) :
//! 1. le PC ouvre une [`PairingWindow`] : code à 6 chiffres, valable 120 s, 3 essais ;
//! 2. le téléphone se connecte en mode `Pairing` et lance SPAKE2 avec le code saisi ;
//! 3. la clé SPAKE2 est dérivée (HKDF) en une `psk` Noise ; le handshake `XXpsk3` échoue
//!    si les codes diffèrent — c'est la preuve de connaissance du code, sans jamais
//!    transmettre ce code ;
//! 4. dans le canal Noise, les clés statiques sont échangées et authentifiées ; le PC
//!    demande confirmation à l'utilisateur, puis enregistre l'appareil.
//!
//! Le code n'est **jamais** une clé permanente : il n'existe que pendant la fenêtre.
//! SPAKE2 rend une attaque hors ligne sur le code impossible ; les essais en ligne sont
//! limités par la fenêtre.

use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use std::time::{Duration, Instant};
use thiserror::Error;

pub const CODE_DIGITS: usize = 6;
pub const WINDOW_TTL: Duration = Duration::from_secs(120);
pub const MAX_ATTEMPTS: u32 = 3;
/// Taille d'un message SPAKE2 Ed25519 (1 octet de rôle + 32 octets).
pub const SPAKE2_MSG_LEN: usize = 33;

const ID_A: &[u8] = b"nothing-connect/pc";
const ID_B: &[u8] = b"nothing-connect/phone";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PairingError {
    #[error("aucune association en cours")]
    NotOpen,
    #[error("code expiré")]
    Expired,
    #[error("trop d'essais, recommencez l'association")]
    TooManyAttempts,
    #[error("message SPAKE2 invalide")]
    BadMessage,
    #[error("format de code ou de QR invalide")]
    BadFormat,
}

/// Génère un code à 6 chiffres uniformément (rejet des biais modulo).
pub fn generate_code() -> String {
    let n: u32 = rand::thread_rng().gen_range(0..1_000_000);
    format!("{n:06}")
}

/// Normalise une saisie utilisateur ("123 456", "123-456" → "123456").
pub fn normalize_code(input: &str) -> Result<String, PairingError> {
    let digits: String = input.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if digits.len() == CODE_DIGITS && digits.chars().all(|c| c.is_ascii_digit()) {
        Ok(digits)
    } else {
        Err(PairingError::BadFormat)
    }
}

/// Fenêtre d'association ouverte côté PC.
#[derive(Debug)]
pub struct PairingWindow {
    code: String,
    opened: Instant,
    ttl: Duration,
    attempts: u32,
}

impl PairingWindow {
    pub fn open() -> Self {
        Self::with_ttl(WINDOW_TTL)
    }
    pub fn with_ttl(ttl: Duration) -> Self {
        Self { code: generate_code(), opened: Instant::now(), ttl, attempts: 0 }
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn remaining(&self) -> Duration {
        self.ttl.saturating_sub(self.opened.elapsed())
    }
    /// Consomme un essai. Renvoie le code à utiliser pour SPAKE2.
    pub fn begin_attempt(&mut self) -> Result<String, PairingError> {
        if self.opened.elapsed() >= self.ttl {
            return Err(PairingError::Expired);
        }
        if self.attempts >= MAX_ATTEMPTS {
            return Err(PairingError::TooManyAttempts);
        }
        self.attempts += 1;
        Ok(self.code.clone())
    }
}

/// Rôle SPAKE2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Pc,
    Phone,
}

/// Étape SPAKE2 en cours.
pub struct Spake {
    state: Spake2<Ed25519Group>,
}

impl Spake {
    /// Démarre SPAKE2 ; renvoie l'état et le message à envoyer au pair.
    pub fn start(role: Role, code: &str) -> (Self, Vec<u8>) {
        let pw = Password::new(code.as_bytes());
        let (a, b) = (Identity::new(ID_A), Identity::new(ID_B));
        let (state, msg) = match role {
            Role::Pc => Spake2::<Ed25519Group>::start_a(&pw, &a, &b),
            Role::Phone => Spake2::<Ed25519Group>::start_b(&pw, &a, &b),
        };
        (Self { state }, msg)
    }

    /// Termine SPAKE2 et dérive la psk Noise (32 octets).
    pub fn finish(self, peer_msg: &[u8]) -> Result<[u8; 32], PairingError> {
        if peer_msg.len() != SPAKE2_MSG_LEN {
            return Err(PairingError::BadMessage);
        }
        let key = self.state.finish(peer_msg).map_err(|_| PairingError::BadMessage)?;
        let hk = Hkdf::<Sha256>::new(Some(b"nothing-connect/pairing/v1"), &key);
        let mut psk = [0u8; 32];
        hk.expand(b"noise-psk", &mut psk).expect("32 octets valides pour HKDF");
        Ok(psk)
    }
}

/// Contenu d'un QR code d'association : `ncpair://<hôte>:<port>?c=<code>&f=<empreinte PC>`.
/// Ne contient que des données temporaires + l'empreinte publique du PC (non secrète).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrPayload {
    pub host: String,
    pub port: u16,
    pub code: String,
    pub pc_fingerprint: String,
}

impl QrPayload {
    pub fn to_uri(&self) -> String {
        format!("ncpair://{}:{}?c={}&f={}", self.host, self.port, self.code, self.pc_fingerprint)
    }

    pub fn parse(uri: &str) -> Result<Self, PairingError> {
        let rest = uri.strip_prefix("ncpair://").ok_or(PairingError::BadFormat)?;
        let (hostport, query) = rest.split_once('?').ok_or(PairingError::BadFormat)?;
        let (host, port) = hostport.rsplit_once(':').ok_or(PairingError::BadFormat)?;
        let port: u16 = port.parse().map_err(|_| PairingError::BadFormat)?;
        let (mut code, mut fp) = (None, None);
        for kv in query.split('&') {
            match kv.split_once('=') {
                Some(("c", v)) => code = Some(normalize_code(v)?),
                Some(("f", v)) if v.len() <= 64 => fp = Some(v.to_string()),
                _ => {}
            }
        }
        if host.is_empty() || host.len() > 253 {
            return Err(PairingError::BadFormat);
        }
        Ok(Self {
            host: host.trim_start_matches('[').trim_end_matches(']').to_string(),
            port,
            code: code.ok_or(PairingError::BadFormat)?,
            pc_fingerprint: fp.ok_or(PairingError::BadFormat)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_code_same_psk() {
        let (a, ma) = Spake::start(Role::Pc, "123456");
        let (b, mb) = Spake::start(Role::Phone, "123456");
        assert_eq!(a.finish(&mb).unwrap(), b.finish(&ma).unwrap());
    }

    #[test]
    fn different_code_different_psk() {
        let (a, ma) = Spake::start(Role::Pc, "123456");
        let (b, mb) = Spake::start(Role::Phone, "654321");
        assert_ne!(a.finish(&mb).unwrap(), b.finish(&ma).unwrap());
    }

    #[test]
    fn bad_message_rejected() {
        let (a, _) = Spake::start(Role::Pc, "123456");
        assert_eq!(a.finish(&[0u8; 5]), Err(PairingError::BadMessage));
    }

    #[test]
    fn window_limits_attempts_and_time() {
        let mut w = PairingWindow::open();
        assert_eq!(w.code().len(), 6);
        for _ in 0..MAX_ATTEMPTS {
            assert!(w.begin_attempt().is_ok());
        }
        assert_eq!(w.begin_attempt(), Err(PairingError::TooManyAttempts));
        let mut w = PairingWindow::with_ttl(Duration::ZERO);
        assert_eq!(w.begin_attempt(), Err(PairingError::Expired));
    }

    #[test]
    fn code_normalization() {
        assert_eq!(normalize_code(" 123-456 ").unwrap(), "123456");
        assert!(normalize_code("12345").is_err());
        assert!(normalize_code("12345a").is_err());
    }

    #[test]
    fn qr_roundtrip() {
        let q = QrPayload { host: "192.168.1.20".into(), port: 47820, code: "004217".into(), pc_fingerprint: "AB12-CD34-EF56-7890".into() };
        assert_eq!(QrPayload::parse(&q.to_uri()).unwrap(), q);
        assert!(QrPayload::parse("http://x").is_err());
        assert!(QrPayload::parse("ncpair://h:1?c=12").is_err());
    }

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..200 {
            let c = generate_code();
            assert!(c.len() == 6 && c.chars().all(|c| c.is_ascii_digit()));
        }
    }
}
