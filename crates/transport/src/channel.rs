//! Canal chiffré : chaque trame = `u16 BE longueur` + message Noise (≤ 65 535 octets).
//!
//! Le canal est séparé en deux moitiés indépendantes (lecture / écriture) qui partagent un
//! état Noise *stateless* : chaque moitié gère son propre compteur de nonce, ce qui permet
//! de lire et d'écrire en parallèle sans verrou.

use crate::TransportError;
use nc_protocol::{Frame, MAX_FRAME_PLAINTEXT, NOISE_MAX_MSG};
use snow::StatelessTransportState;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};

/// Taille des tampons d'E/S : plusieurs trames pleines pour limiter les appels système.
const IO_BUF: usize = 256 * 1024;

pub struct SecureReader<R> {
    inner: BufReader<R>,
    state: Arc<StatelessTransportState>,
    nonce: u64,
    cipher: Vec<u8>,
    plain: Vec<u8>,
}

pub struct SecureWriter<W> {
    inner: BufWriter<W>,
    state: Arc<StatelessTransportState>,
    nonce: u64,
    plain: Vec<u8>,
    cipher: Vec<u8>,
}

pub fn split<R, W>(r: R, w: W, state: StatelessTransportState) -> (SecureReader<R>, SecureWriter<W>)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let state = Arc::new(state);
    (
        SecureReader {
            inner: BufReader::with_capacity(IO_BUF, r),
            state: state.clone(),
            nonce: 0,
            cipher: vec![0; NOISE_MAX_MSG],
            plain: vec![0; NOISE_MAX_MSG],
        },
        SecureWriter {
            inner: BufWriter::with_capacity(IO_BUF, w),
            state,
            nonce: 0,
            plain: Vec::with_capacity(MAX_FRAME_PLAINTEXT),
            cipher: vec![0; NOISE_MAX_MSG],
        },
    )
}

impl<R: AsyncRead + Unpin> SecureReader<R> {
    /// Lit et déchiffre une trame brute. Renvoie une tranche valable jusqu'au prochain appel.
    pub async fn read_raw(&mut self) -> Result<&[u8], TransportError> {
        let len = match self.inner.read_u16().await {
            Ok(n) => n as usize,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(TransportError::Closed)
            }
            Err(e) => return Err(e.into()),
        };
        self.inner.read_exact(&mut self.cipher[..len]).await?;
        let n = self
            .state
            .read_message(self.nonce, &self.cipher[..len], &mut self.plain)?;
        self.nonce += 1;
        Ok(&self.plain[..n])
    }

    pub(crate) fn advance_nonce(&mut self, n: u64) {
        self.nonce += n;
    }

    pub async fn read_frame(&mut self) -> Result<Frame, TransportError> {
        let raw = self.read_raw().await?;
        Ok(Frame::decode(raw)?)
    }
}

impl<W: AsyncWrite + Unpin> SecureWriter<W> {
    /// Chiffre et met en tampon un texte clair (sans vider le tampon réseau).
    pub async fn write_raw(&mut self, plain: &[u8]) -> Result<(), TransportError> {
        let n = self
            .state
            .write_message(self.nonce, plain, &mut self.cipher)?;
        self.nonce += 1;
        self.inner.write_u16(n as u16).await?;
        self.inner.write_all(&self.cipher[..n]).await?;
        Ok(())
    }

    pub async fn write_frame(&mut self, f: &Frame) -> Result<(), TransportError> {
        let mut buf = std::mem::take(&mut self.plain);
        let res = match f.encode_into(&mut buf) {
            Ok(()) => self.write_raw(&buf).await,
            Err(e) => Err(e.into()),
        };
        self.plain = buf;
        res
    }

    /// Accès au tampon interne pour construire un bloc de données sans copie supplémentaire.
    pub fn scratch(&mut self) -> &mut Vec<u8> {
        self.plain.clear();
        &mut self.plain
    }

    /// Chiffre le contenu du tampon `scratch`.
    pub async fn write_scratch(&mut self) -> Result<(), TransportError> {
        let buf = std::mem::take(&mut self.plain);
        let res = self.write_raw(&buf).await;
        self.plain = buf;
        res
    }

    pub async fn flush(&mut self) -> Result<(), TransportError> {
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        self.inner.flush().await?;
        self.inner.shutdown().await?;
        Ok(())
    }

    pub(crate) fn advance_nonce(&mut self, n: u64) {
        self.nonce += n;
    }

    pub fn messages_sent(&self) -> u64 {
        self.nonce
    }
}

// ---- E/S en clair utilisées pendant les handshakes ----

pub(crate) async fn write_plain_msg<W: AsyncWrite + Unpin>(
    w: &mut W,
    b: &[u8],
) -> Result<(), TransportError> {
    w.write_u16(b.len() as u16).await?;
    w.write_all(b).await?;
    w.flush().await?;
    Ok(())
}

pub(crate) async fn read_plain_msg<R: AsyncRead + Unpin>(
    r: &mut R,
    buf: &mut [u8],
) -> Result<usize, TransportError> {
    let len = match r.read_u16().await {
        Ok(n) => n as usize,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(TransportError::Closed)
        }
        Err(e) => return Err(e.into()),
    };
    if len > buf.len() {
        return Err(TransportError::Protocol(
            nc_protocol::ProtocolError::TooLarge(len),
        ));
    }
    r.read_exact(&mut buf[..len]).await?;
    Ok(len)
}
