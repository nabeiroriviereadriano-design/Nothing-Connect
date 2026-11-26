//! Établissement des sessions.
//!
//! ```text
//! Initiateur (téléphone)                     Répondeur (PC)
//!   préambule NCN1 | version | mode  ───────►
//! mode Paired (Noise IK, clé du PC épinglée) :
//!   -> e, es, s, ss                  ───────►  vérifie que `s` est un appareil associé
//!                                    ◄───────  <- e, ee, se
//! mode Pairing :
//!   SPAKE2(B, code saisi)            ───────►  SPAKE2(A, code affiché) si fenêtre ouverte
//!                                    ◄───────  SPAKE2(A)
//!   Noise XXpsk3 (psk = HKDF(clé SPAKE2)) ; échoue si les codes diffèrent
//! ```

use crate::channel::{read_plain_msg, split, write_plain_msg, SecureReader, SecureWriter};
use crate::TransportError;
use nc_pairing::{Role, Spake, SPAKE2_MSG_LEN};
use nc_protocol::{Preamble, SessionMode, NOISE_MAX_MSG};
use nc_security::{ct_eq, Identity, NOISE_PAIRED, NOISE_PAIRING, NOISE_PROLOGUE};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub type Reader = SecureReader<OwnedReadHalf>;
pub type Writer = SecureWriter<OwnedWriteHalf>;

/// Session chiffrée établie.
pub struct Established {
    pub mode: SessionMode,
    /// Clé publique statique authentifiée du pair.
    pub peer_key: [u8; 32],
    pub peer_addr: std::net::SocketAddr,
    pub reader: Reader,
    pub writer: Writer,
}

/// Politique du répondeur.
pub trait AcceptPolicy: Send + Sync {
    /// Le pair est-il un appareil associé et non révoqué ?
    fn is_trusted(&self, peer_key: &[u8; 32]) -> bool;
    /// Une association est-elle ouverte ? Consomme un essai et renvoie le code.
    fn pairing_attempt(&self) -> Result<String, nc_pairing::PairingError>;
}

fn tune(stream: &TcpStream) {
    // Faible latence pour les messages de contrôle ; le débit est assuré par les tampons.
    let _ = stream.set_nodelay(true);
}

fn key32(k: Option<&[u8]>) -> Result<[u8; 32], TransportError> {
    k.and_then(|k| k.try_into().ok())
        .ok_or(TransportError::UnknownPeer)
}

/// Côté initiateur : connexion à un appareil associé dont on connaît la clé.
pub async fn connect_paired(
    addr: std::net::SocketAddr,
    identity: &Identity,
    expected_peer: &[u8; 32],
) -> Result<Established, TransportError> {
    let stream = timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(addr)).await??;
    timeout(
        HANDSHAKE_TIMEOUT,
        initiate_paired(stream, identity, expected_peer),
    )
    .await?
}

async fn initiate_paired(
    stream: TcpStream,
    identity: &Identity,
    expected: &[u8; 32],
) -> Result<Established, TransportError> {
    tune(&stream);
    let peer_addr = stream.peer_addr()?;
    let (mut r, mut w) = stream.into_split();
    w.write_all(&Preamble::new(SessionMode::Paired).encode())
        .await?;

    let mut hs = snow::Builder::new(NOISE_PAIRED.parse()?)
        .prologue(NOISE_PROLOGUE)?
        .local_private_key(identity.private())?
        .remote_public_key(expected)?
        .build_initiator()?;
    let mut msg = vec![0u8; NOISE_MAX_MSG];
    let mut tmp = vec![0u8; NOISE_MAX_MSG];
    let n = hs.write_message(&[], &mut msg)?;
    write_plain_msg(&mut w, &msg[..n]).await?;
    let n = read_plain_msg(&mut r, &mut msg).await?;
    if n == 0 {
        // Refus explicite : le répondeur ne nous connaît pas (ou plus).
        return Err(TransportError::UnknownPeer);
    }
    hs.read_message(&msg[..n], &mut tmp)?;
    let peer_key = key32(hs.get_remote_static())?;
    if !ct_eq(&peer_key, expected) {
        return Err(TransportError::PeerMismatch);
    }
    let (reader, writer) = split(r, w, hs.into_stateless_transport_mode()?);
    Ok(Established {
        mode: SessionMode::Paired,
        peer_key,
        peer_addr,
        reader,
        writer,
    })
}

/// Côté initiateur : association avec le code saisi par l'utilisateur.
/// `expected_fingerprint` (QR code) permet de vérifier qu'on parle bien au bon PC.
pub async fn connect_pairing(
    addr: std::net::SocketAddr,
    identity: &Identity,
    code: &str,
    expected_fingerprint: Option<&str>,
) -> Result<Established, TransportError> {
    let stream = timeout(HANDSHAKE_TIMEOUT, TcpStream::connect(addr)).await??;
    pairing_over(stream, identity, code, expected_fingerprint).await
}

/// Association sur une connexion TCP déjà ouverte (le téléphone essaie plusieurs adresses
/// du PC et garde la première qui répond).
pub async fn pairing_over(
    stream: TcpStream,
    identity: &Identity,
    code: &str,
    expected_fingerprint: Option<&str>,
) -> Result<Established, TransportError> {
    let code = nc_pairing::normalize_code(code)?;
    let est = timeout(HANDSHAKE_TIMEOUT, initiate_pairing(stream, identity, &code)).await??;
    if let Some(fp) = expected_fingerprint {
        if nc_security::fingerprint(&est.peer_key) != fp {
            return Err(TransportError::PeerMismatch);
        }
    }
    Ok(est)
}

async fn initiate_pairing(
    stream: TcpStream,
    identity: &Identity,
    code: &str,
) -> Result<Established, TransportError> {
    tune(&stream);
    let peer_addr = stream.peer_addr()?;
    let (mut r, mut w) = stream.into_split();
    w.write_all(&Preamble::new(SessionMode::Pairing).encode())
        .await?;

    let (spake, out) = Spake::start(Role::Phone, code);
    write_plain_msg(&mut w, &out).await?;
    let mut msg = vec![0u8; NOISE_MAX_MSG];
    let mut tmp = vec![0u8; NOISE_MAX_MSG];
    let n = read_plain_msg(&mut r, &mut msg)
        .await
        .map_err(|_| TransportError::PairingFailed)?;
    let psk = spake.finish(&msg[..n])?;

    let mut hs = snow::Builder::new(NOISE_PAIRING.parse()?)
        .prologue(NOISE_PROLOGUE)?
        .local_private_key(identity.private())?
        .psk(3, &psk)?
        .build_initiator()?;
    let n = hs.write_message(&[], &mut msg)?; // -> e
    write_plain_msg(&mut w, &msg[..n]).await?;
    let n = read_plain_msg(&mut r, &mut msg)
        .await
        .map_err(|_| TransportError::PairingFailed)?;
    hs.read_message(&msg[..n], &mut tmp)?; // <- e, ee, s, es
    let n = hs.write_message(&[], &mut msg)?; // -> s, se, psk
    write_plain_msg(&mut w, &msg[..n]).await?;
    let peer_key = key32(hs.get_remote_static())?;

    // Confirmation : le répondeur renvoie un octet chiffré seulement si la psk concorde.
    let n = read_plain_msg(&mut r, &mut msg)
        .await
        .map_err(|_| TransportError::PairingFailed)?;
    let state = hs.into_stateless_transport_mode()?;
    state
        .read_message(0, &msg[..n], &mut tmp)
        .map_err(|_| TransportError::PairingFailed)?;
    let (mut reader, writer) = split(r, w, state);
    reader_skip_nonce(&mut reader);
    Ok(Established {
        mode: SessionMode::Pairing,
        peer_key,
        peer_addr,
        reader,
        writer,
    })
}

/// Le message de confirmation a consommé le nonce 0 de réception.
fn reader_skip_nonce<R: tokio::io::AsyncRead + Unpin>(r: &mut SecureReader<R>) {
    r.advance_nonce(1);
}

/// Côté répondeur : accepte une connexion entrante selon la politique.
pub async fn accept(
    stream: TcpStream,
    identity: &Identity,
    policy: &dyn AcceptPolicy,
) -> Result<Established, TransportError> {
    timeout(HANDSHAKE_TIMEOUT, accept_inner(stream, identity, policy)).await?
}

async fn accept_inner(
    stream: TcpStream,
    identity: &Identity,
    policy: &dyn AcceptPolicy,
) -> Result<Established, TransportError> {
    tune(&stream);
    let peer_addr = stream.peer_addr()?;
    let (mut r, mut w) = stream.into_split();
    let mut pre = [0u8; Preamble::LEN];
    r.read_exact(&mut pre).await?;
    let pre = Preamble::decode(&pre)?;
    pre.check_version()?;

    let mut msg = vec![0u8; NOISE_MAX_MSG];
    let mut tmp = vec![0u8; NOISE_MAX_MSG];

    match pre.mode {
        SessionMode::Paired => {
            let mut hs = snow::Builder::new(NOISE_PAIRED.parse()?)
                .prologue(NOISE_PROLOGUE)?
                .local_private_key(identity.private())?
                .build_responder()?;
            let n = read_plain_msg(&mut r, &mut msg).await?;
            hs.read_message(&msg[..n], &mut tmp)?;
            let peer_key = key32(hs.get_remote_static())?;
            if !policy.is_trusted(&peer_key) {
                // Refus par défaut, signalé par un message vide (ne révèle aucune donnée).
                let _ = write_plain_msg(&mut w, &[]).await;
                return Err(TransportError::UnknownPeer);
            }
            let n = hs.write_message(&[], &mut msg)?;
            write_plain_msg(&mut w, &msg[..n]).await?;
            let (reader, writer) = split(r, w, hs.into_stateless_transport_mode()?);
            Ok(Established {
                mode: SessionMode::Paired,
                peer_key,
                peer_addr,
                reader,
                writer,
            })
        }
        SessionMode::Pairing => {
            let code = policy.pairing_attempt()?;
            let n = read_plain_msg(&mut r, &mut msg).await?;
            if n != SPAKE2_MSG_LEN {
                return Err(TransportError::PairingFailed);
            }
            let (spake, out) = Spake::start(Role::Pc, &code);
            write_plain_msg(&mut w, &out).await?;
            let psk = spake.finish(&msg[..n])?;

            let mut hs = snow::Builder::new(NOISE_PAIRING.parse()?)
                .prologue(NOISE_PROLOGUE)?
                .local_private_key(identity.private())?
                .psk(3, &psk)?
                .build_responder()?;
            let n = read_plain_msg(&mut r, &mut msg).await?;
            hs.read_message(&msg[..n], &mut tmp)?; // -> e
            let n = hs.write_message(&[], &mut msg)?; // <- e, ee, s, es
            write_plain_msg(&mut w, &msg[..n]).await?;
            let n = read_plain_msg(&mut r, &mut msg).await?;
            // Échoue ici si le code saisi est différent (psk différente).
            hs.read_message(&msg[..n], &mut tmp)
                .map_err(|_| TransportError::PairingFailed)?;
            let peer_key = key32(hs.get_remote_static())?;
            let state = hs.into_stateless_transport_mode()?;
            let n = state.write_message(0, &[1], &mut msg)?;
            write_plain_msg(&mut w, &msg[..n]).await?;
            let (reader, mut writer) = split(r, w, state);
            writer.advance_nonce(1);
            Ok(Established {
                mode: SessionMode::Pairing,
                peer_key,
                peer_addr,
                reader,
                writer,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nc_protocol::{Frame, Message};
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    struct Policy {
        trusted: Vec<[u8; 32]>,
        window: Mutex<Option<nc_pairing::PairingWindow>>,
    }
    impl AcceptPolicy for Policy {
        fn is_trusted(&self, k: &[u8; 32]) -> bool {
            self.trusted.iter().any(|t| t == k)
        }
        fn pairing_attempt(&self) -> Result<String, nc_pairing::PairingError> {
            self.window
                .lock()
                .unwrap()
                .as_mut()
                .ok_or(nc_pairing::PairingError::NotOpen)?
                .begin_attempt()
        }
    }

    async fn server(
        policy: Policy,
    ) -> (
        std::net::SocketAddr,
        Identity,
        tokio::task::JoinHandle<Result<Established, TransportError>>,
        Option<String>,
    ) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let id = Identity::generate().unwrap();
        let me = Identity::from_secret(id.private().to_vec()).unwrap();
        let code = policy
            .window
            .lock()
            .unwrap()
            .as_ref()
            .map(|w| w.code().to_string());
        let h = tokio::spawn(async move {
            let (s, _) = l.accept().await.unwrap();
            accept(s, &me, &policy).await
        });
        (addr, id, h, code)
    }

    #[tokio::test]
    async fn pairing_then_encrypted_exchange() {
        let phone = Identity::generate().unwrap();
        let (addr, pc, h, code) = server(Policy {
            trusted: vec![],
            window: Mutex::new(Some(nc_pairing::PairingWindow::open())),
        })
        .await;
        let fp = pc.fingerprint();
        let mut c = connect_pairing(addr, &phone, &code.unwrap(), Some(&fp))
            .await
            .unwrap();
        let mut s = h.await.unwrap().unwrap();
        assert_eq!(&c.peer_key, pc.public());
        assert_eq!(&s.peer_key, phone.public());
        c.writer
            .write_frame(&Frame::Control(Message::Ping { nonce: 7 }))
            .await
            .unwrap();
        c.writer.flush().await.unwrap();
        assert_eq!(
            s.reader.read_frame().await.unwrap(),
            Frame::Control(Message::Ping { nonce: 7 })
        );
        s.writer
            .write_frame(&Frame::Control(Message::Pong { nonce: 7 }))
            .await
            .unwrap();
        s.writer.flush().await.unwrap();
        assert_eq!(
            c.reader.read_frame().await.unwrap(),
            Frame::Control(Message::Pong { nonce: 7 })
        );
    }

    #[tokio::test]
    async fn wrong_code_fails_on_both_sides() {
        let phone = Identity::generate().unwrap();
        let (addr, _pc, h, code) = server(Policy {
            trusted: vec![],
            window: Mutex::new(Some(nc_pairing::PairingWindow::open())),
        })
        .await;
        let wrong = if code.as_deref() == Some("000000") {
            "111111"
        } else {
            "000000"
        };
        assert!(matches!(
            connect_pairing(addr, &phone, wrong, None).await,
            Err(TransportError::PairingFailed)
        ));
        assert!(h.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn pairing_refused_when_window_closed() {
        let phone = Identity::generate().unwrap();
        let (addr, _pc, h, _) = server(Policy {
            trusted: vec![],
            window: Mutex::new(None),
        })
        .await;
        assert!(connect_pairing(addr, &phone, "123456", None).await.is_err());
        assert!(h.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn paired_session_for_trusted_peer() {
        let phone = Identity::generate().unwrap();
        let (addr, pc, h, _) = server(Policy {
            trusted: vec![*phone.public()],
            window: Mutex::new(None),
        })
        .await;
        let c = connect_paired(addr, &phone, pc.public()).await.unwrap();
        let s = h.await.unwrap().unwrap();
        assert_eq!(&s.peer_key, phone.public());
        assert_eq!(&c.peer_key, pc.public());
    }

    #[tokio::test]
    async fn unknown_peer_refused_by_default() {
        let phone = Identity::generate().unwrap();
        let (addr, pc, h, _) = server(Policy {
            trusted: vec![],
            window: Mutex::new(None),
        })
        .await;
        assert!(matches!(
            connect_paired(addr, &phone, pc.public()).await,
            Err(TransportError::UnknownPeer)
        ));
        assert!(matches!(h.await.unwrap(), Err(TransportError::UnknownPeer)));
    }

    #[tokio::test]
    async fn impostor_pc_detected() {
        let phone = Identity::generate().unwrap();
        let (addr, _pc, h, _) = server(Policy {
            trusted: vec![*phone.public()],
            window: Mutex::new(None),
        })
        .await;
        let other = Identity::generate().unwrap();
        // Le téléphone attend la clé d'un autre PC : le handshake IK échoue.
        assert!(connect_paired(addr, &phone, other.public()).await.is_err());
        let _ = h.await;
    }
}
