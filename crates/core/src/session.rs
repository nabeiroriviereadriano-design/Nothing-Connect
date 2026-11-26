//! Session avec un pair : échange `Hello`, confirmation d'association, boucle de lecture,
//! tâche d'écriture prioritaire et maintien de connexion.

use crate::events::Event;
use crate::node::{Inner, Role};
use crate::transfers;
use nc_protocol::{check_version, Frame, Message, SessionMode, PROTOCOL_VERSION};
use nc_transport::handshake::{Established, Reader, Writer};
use nc_transport::TransportError;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Notify};

const PING_EVERY: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(90);
/// File de données : ~16 trames pleines (≈1 Mo) en vol → contre-pression vers le disque.
const DATA_QUEUE: usize = 16;
const CTRL_QUEUE: usize = 256;

pub(crate) struct Session {
    pub peer_id: String,
    pub addr: SocketAddr,
    ctrl: mpsc::Sender<Vec<u8>>,
    data: mpsc::Sender<Vec<u8>>,
    closed: AtomicBool,
    close_notify: Notify,
}

impl Session {
    /// Envoi prioritaire d'un message de contrôle. Renvoie `false` si la session est fermée.
    pub fn send(&self, m: &Message) -> bool {
        if self.closed.load(Ordering::SeqCst) {
            return false;
        }
        match Frame::Control(m.clone()).encode() {
            Ok(b) => self.ctrl.try_send(b).is_ok(),
            Err(e) => {
                tracing::error!("message trop grand non envoyé : {e}");
                false
            }
        }
    }

    /// File ordonnée (blocs de fichier et fin de fichier), depuis un thread bloquant.
    pub fn data_sender(&self) -> mpsc::Sender<Vec<u8>> {
        self.data.clone()
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.close_notify.notify_waiters();
        self.close_notify.notify_one();
    }

}

pub(crate) fn peer_id_of(key: &[u8; 32]) -> String {
    hex::encode(key)
}

/// Exécute une session jusqu'à sa fin. `initiator` : nous avons ouvert la connexion.
pub(crate) async fn run(inner: Arc<Inner>, est: Established, initiator: bool) {
    let Established { mode, peer_key, peer_addr, mut reader, mut writer } = est;
    let peer_id = peer_id_of(&peer_key);

    // 1. Hello
    let hello = Message::Hello { protocol: PROTOCOL_VERSION, device: inner.cfg.device.clone() };
    if writer.write_frame(&Frame::Control(hello)).await.is_err() || writer.flush().await.is_err() {
        return;
    }
    let remote = match tokio::time::timeout(HELLO_TIMEOUT, reader.read_frame()).await {
        Ok(Ok(Frame::Control(Message::Hello { protocol, device }))) => {
            if check_version(protocol).is_err() {
                inner.emit(Event::IncompatibleVersion { peer_id: Some(peer_id), remote: protocol, local: PROTOCOL_VERSION });
                return;
            }
            device
        }
        _ => {
            if mode == SessionMode::Pairing {
                inner.emit(Event::PairingFailed { reason: "le pair n'a pas répondu".into() });
            }
            return;
        }
    };

    // 2. Association : confirmation explicite côté PC, enregistrement des deux côtés.
    if mode == SessionMode::Pairing {
        let addr = initiator.then(|| peer_addr.to_string());
        if !pairing_confirmation(&inner, &peer_id, &peer_key, &remote, &mut reader, &mut writer, addr).await {
            return;
        }
    } else {
        // Appareil déjà associé : rafraîchit nom, modèle, version.
        if inner.store.upsert_device(&peer_key, &remote, initiator.then(|| peer_addr.to_string()).as_deref()).is_err() {
            return;
        }
    }

    // 3. Enregistrement de la session (remplace une éventuelle session précédente).
    let (ctrl_tx, ctrl_rx) = mpsc::channel(CTRL_QUEUE);
    let (data_tx, data_rx) = mpsc::channel(DATA_QUEUE);
    let session = Arc::new(Session {
        peer_id: peer_id.clone(),
        addr: peer_addr,
        ctrl: ctrl_tx,
        data: data_tx,
        closed: AtomicBool::new(false),
        close_notify: Notify::new(),
    });
    if let Some(old) = inner.sessions.lock().unwrap().insert(peer_id.clone(), session.clone()) {
        old.close();
    }
    inner.emit(Event::PeerConnected { peer_id: peer_id.clone(), addr: peer_addr.to_string(), link: "wifi" });

    let writer_task = tokio::spawn(write_loop(writer, ctrl_rx, data_rx, session.clone()));
    let ping_task = tokio::spawn(ping_loop(session.clone()));

    // Reprise automatique des transferts interrompus vers ce pair.
    transfers::on_connected(&inner, &peer_id);

    // 4. Boucle de lecture
    let reason = read_loop(&inner, &session, &mut reader).await;

    // 5. Fin de session
    session.close();
    ping_task.abort();
    let _ = writer_task.await;
    {
        let mut map = inner.sessions.lock().unwrap();
        if map.get(&peer_id).map(|s| Arc::ptr_eq(s, &session)).unwrap_or(false) {
            map.remove(&peer_id);
        }
    }
    let _ = inner.store.touch_device(&peer_id, None);
    if inner.session(&peer_id).is_none() {
        transfers::on_disconnected(&inner, &peer_id);
        inner.emit(Event::PeerDisconnected { peer_id, reason });
    }
}

async fn pairing_confirmation(
    inner: &Arc<Inner>,
    peer_id: &str,
    peer_key: &[u8; 32],
    remote: &nc_protocol::DeviceInfo,
    reader: &mut Reader,
    writer: &mut Writer,
    peer_addr: Option<String>,
) -> bool {
    match inner.cfg.role {
        Role::Pc => {
            let (tx, rx) = oneshot::channel();
            inner.pending_pairings.lock().unwrap().insert(peer_id.to_string(), tx);
            inner.emit(Event::PairingRequest {
                peer_id: peer_id.to_string(),
                device: remote.clone(),
                fingerprint: nc_security::fingerprint(peer_key),
            });
            let accepted = matches!(tokio::time::timeout(CONFIRM_TIMEOUT, rx).await, Ok(Ok(true)));
            inner.pending_pairings.lock().unwrap().remove(peer_id);
            let reply = if accepted { Message::PairingAccepted } else { Message::PairingRejected };
            let sent = writer.write_frame(&Frame::Control(reply)).await.is_ok() && writer.flush().await.is_ok();
            if !accepted || !sent {
                inner.emit(Event::PairingFailed { reason: "association refusée sur le PC".into() });
                return false;
            }
            // Une fenêtre = un appareil.
            if inner.pairing.lock().unwrap().take().is_some() {
                inner.emit(Event::PairingWindowClosed);
            }
            store_paired(inner, peer_key, remote, None)
        }
        Role::Phone => {
            let res = tokio::time::timeout(CONFIRM_TIMEOUT + Duration::from_secs(5), reader.read_frame()).await;
            match res {
                Ok(Ok(Frame::Control(Message::PairingAccepted))) => store_paired(inner, peer_key, remote, peer_addr.as_deref()),
                _ => {
                    inner.emit(Event::PairingFailed { reason: "association refusée ou délai dépassé sur le PC".into() });
                    false
                }
            }
        }
    }
}

fn store_paired(inner: &Arc<Inner>, key: &[u8; 32], remote: &nc_protocol::DeviceInfo, addr: Option<&str>) -> bool {
    match inner.store.upsert_device(key, remote, addr) {
        Ok(d) => {
            inner.refused.lock().unwrap().remove(&d.id);
            inner.emit(Event::Paired { device: d });
            true
        }
        Err(e) => {
            inner.emit(Event::Error { message: e.to_string() });
            false
        }
    }
}

async fn write_loop(mut w: Writer, mut ctrl: mpsc::Receiver<Vec<u8>>, mut data: mpsc::Receiver<Vec<u8>>, s: Arc<Session>) {
    loop {
        let first = tokio::select! {
            biased;
            _ = s.close_notify.notified() => break,
            m = ctrl.recv() => m,
            m = data.recv() => m,
        };
        let Some(first) = first else { break };
        if w.write_raw(&first).await.is_err() {
            break;
        }
        // Regroupe ce qui est déjà en file avant de vider le tampon réseau (moins d'appels
        // système), en servant toujours le contrôle en premier.
        let mut ok = true;
        for _ in 0..32 {
            let next = ctrl.try_recv().or_else(|_| data.try_recv());
            match next {
                Ok(b) => {
                    if w.write_raw(&b).await.is_err() {
                        ok = false;
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if !ok || w.flush().await.is_err() {
            break;
        }
    }
    // Vide les messages de contrôle restants (ex. `Bye`, `Unpaired`) avant fermeture.
    while let Ok(b) = ctrl.try_recv() {
        if w.write_raw(&b).await.is_err() {
            break;
        }
    }
    let _ = w.shutdown().await;
    s.close();
}

async fn ping_loop(s: Arc<Session>) {
    let mut n = 0u64;
    loop {
        tokio::time::sleep(PING_EVERY).await;
        n += 1;
        if !s.send(&Message::Ping { nonce: n }) {
            return;
        }
    }
}

async fn read_loop(inner: &Arc<Inner>, s: &Arc<Session>, r: &mut Reader) -> String {
    loop {
        let frame = tokio::select! {
            f = tokio::time::timeout(IDLE_TIMEOUT, r.read_frame()) => f,
            _ = s.close_notify.notified() => return "fermée localement".into(),
        };
        let frame = match frame {
            Err(_) => return "aucune réponse (délai dépassé)".into(),
            Ok(Err(TransportError::Closed)) => return "connexion fermée".into(),
            Ok(Err(e)) => return e.to_string(),
            Ok(Ok(f)) => f,
        };
        match frame {
            Frame::Data(chunk) => transfers::on_chunk(inner, chunk).await,
            // Doit suivre les blocs dans la même file que l'écriture disque.
            Frame::Control(Message::TransferFileEnd { id, file_index, sha256 }) => {
                transfers::on_file_end(inner, id, file_index, sha256).await
            }
            Frame::Control(m) => {
                if let Some(reason) = dispatch(inner, s, m) {
                    return reason;
                }
            }
        }
    }
}

/// Traite un message ; renvoie `Some(raison)` si la session doit se terminer.
fn dispatch(inner: &Arc<Inner>, s: &Arc<Session>, m: Message) -> Option<String> {
    let peer = s.peer_id.clone();
    match m {
        Message::Ping { nonce } => {
            s.send(&Message::Pong { nonce });
        }
        Message::Pong { .. } | Message::Hello { .. } | Message::PairingAccepted | Message::PairingRejected => {}
        Message::Bye => return Some("fermée par le pair".into()),
        Message::Unpaired => {
            transfers::abort_peer(inner, &peer);
            let _ = inner.store.remove_device(&peer);
            inner.emit(Event::DeviceRemoved { peer_id: peer });
            return Some("association révoquée par le pair".into());
        }
        Message::Status(status) => inner.emit(Event::Status { peer_id: peer, status }),
        Message::CommandResult { id, ok, error } => inner.emit(Event::CommandResult { peer_id: peer, id, ok, error }),
        Message::NotificationPosted(n) => {
            let st = inner.settings();
            if !st.notifications_enabled {
                return None;
            }
            let keep = st.store_notification_content && !(n.sensitive && st.hide_sensitive_content);
            let _ = inner.store.insert_notification(&peer, &n, keep);
            let _ = inner.store.prune_notifications(st.notification_history_limit);
            let mut n = n;
            if n.sensitive && st.hide_sensitive_content {
                n.title = Some("Contenu masqué".into());
                n.text = None;
            }
            inner.emit(Event::Notification { peer_id: peer, notification: n });
        }
        Message::NotificationRemoved { key } => {
            let _ = inner.store.mark_notification_removed(&peer, &key);
            inner.emit(Event::NotificationRemoved { peer_id: peer, key });
        }
        Message::NotificationApps { apps } => {
            let _ = inner.store.set_notification_apps(&peer, &apps);
            inner.emit(Event::NotificationApps { peer_id: peer, apps });
        }
        Message::MediaSessions { sessions } => inner.emit(Event::MediaSessions { peer_id: peer, sessions }),
        Message::Share { kind, content } => {
            if content.len() <= 1 << 20 {
                inner.emit(Event::Share { peer_id: peer, kind, content });
            }
        }
        // Demandes destinées à la plateforme (le téléphone exécute ; Kotlin répond).
        m @ (Message::Command { .. }
        | Message::NotificationAction { .. }
        | Message::NotificationDismiss { .. }
        | Message::NotificationOpen { .. }
        | Message::NotificationFilter { .. }
        | Message::MediaCommand { .. }) => inner.emit(Event::Request { peer_id: peer, message: m }),
        // Transferts
        Message::TransferOffer { id, files } => transfers::on_offer(inner, &peer, id, files),
        Message::TransferAccept { id, offsets } => transfers::on_accept(inner, &peer, id, offsets),
        Message::TransferReject { id, .. } => transfers::on_reject(inner, id),
        Message::TransferFileEnd { .. } => {}
        Message::TransferFileVerified { id, file_index, ok } => transfers::on_verified(inner, id, file_index, ok),
        Message::TransferPause { id } => transfers::on_remote_pause(inner, id),
        Message::TransferResume { id, offsets } => transfers::on_remote_resume(inner, &peer, id, offsets),
        Message::TransferCancel { id } => transfers::on_remote_cancel(inner, id),
    }
    None
}
