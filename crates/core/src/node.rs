use crate::events::{Event, EventSink};
use crate::session::{self, Session};
use crate::settings::Settings;
use crate::transfers::TransferCtl;
use crate::{CoreError, Result};
use nc_pairing::{PairingError, PairingWindow, QrPayload};
use nc_protocol::{DeviceInfo, Message, Platform, RemoteCommand};
use nc_security::{Identity, SecretStore};
use nc_storage::{DeviceRecord, NotificationRecord, Store, TransferRecord};
use nc_transport::handshake::{self, AcceptPolicy};
use nc_transport::TransportError;
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Notify};
use uuid::Uuid;

/// Rôle de l'appareil dans le réseau : le PC écoute, le téléphone se connecte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Pc,
    Phone,
}

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub role: Role,
    /// Dossier des données (base SQLite).
    pub data_dir: PathBuf,
    /// Dossier de réception par défaut.
    pub download_dir: PathBuf,
    pub device: DeviceInfo,
    /// Port d'écoute (PC). Si occupé, un port libre est choisi et annoncé en mDNS.
    pub listen_port: u16,
    /// Annonce mDNS (PC).
    pub advertise: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerLink {
    pub peer_id: String,
    pub addr: String,
    pub link: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PairingInfo {
    pub code: String,
    pub expires_in_s: u64,
    pub qr_uri: Option<String>,
    pub fingerprint: String,
}

pub(crate) struct Inner {
    pub cfg: NodeConfig,
    pub identity: Identity,
    pub store: Store,
    pub sink: Arc<dyn EventSink>,
    pub sessions: Mutex<HashMap<String, Arc<Session>>>,
    pub pairing: Mutex<Option<PairingWindow>>,
    pub pending_pairings: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub transfers: Mutex<HashMap<Uuid, Arc<TransferCtl>>>,
    pub settings: Mutex<Settings>,
    pub next_cmd: AtomicU64,
    pub listen_addr: Mutex<Option<SocketAddr>>,
    pub stopping: AtomicBool,
    pub wake_reconnect: Notify,
    pub refused: Mutex<std::collections::HashSet<String>>,
}

impl Inner {
    pub fn emit(&self, e: Event) {
        self.sink.emit(e);
    }

    pub fn session(&self, peer_id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().unwrap().get(peer_id).cloned()
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn download_dir(&self) -> PathBuf {
        self.settings()
            .download_dir
            .filter(|d| !d.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.cfg.download_dir.clone())
    }

    fn is_suspended(&self) -> bool {
        self.settings.lock().unwrap().suspended
    }
}

impl AcceptPolicy for Inner {
    fn is_trusted(&self, peer_key: &[u8; 32]) -> bool {
        matches!(self.store.device_by_key(peer_key), Ok(Some(_)))
    }

    fn pairing_attempt(&self) -> std::result::Result<String, PairingError> {
        let mut g = self.pairing.lock().unwrap();
        let w = g.as_mut().ok_or(PairingError::NotOpen)?;
        match w.begin_attempt() {
            Ok(c) => Ok(c),
            Err(e) => {
                *g = None;
                drop(g);
                self.emit(Event::PairingWindowClosed);
                Err(e)
            }
        }
    }
}

/// Instance NOTHING CONNECT.
pub struct Node {
    inner: Arc<Inner>,
    rt: Option<tokio::runtime::Runtime>,
}

impl Node {
    pub fn new(cfg: NodeConfig, secrets: &dyn SecretStore, sink: Arc<dyn EventSink>) -> Result<Self> {
        let identity = Identity::load_or_create(secrets)?;
        let store = Store::open(&cfg.data_dir.join("nothing-connect.db"))?;
        let settings = store.get_setting::<Settings>("settings")?.unwrap_or_default();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("nc-core")
            .enable_all()
            .build()?;
        let inner = Arc::new(Inner {
            cfg,
            identity,
            store,
            sink,
            sessions: Mutex::new(HashMap::new()),
            pairing: Mutex::new(None),
            pending_pairings: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            settings: Mutex::new(settings),
            next_cmd: AtomicU64::new(1),
            listen_addr: Mutex::new(None),
            stopping: AtomicBool::new(false),
            wake_reconnect: Notify::new(),
            refused: Mutex::new(Default::default()),
        });
        Ok(Self { inner, rt: Some(rt) })
    }

    fn rt(&self) -> &tokio::runtime::Runtime {
        self.rt.as_ref().expect("runtime actif")
    }

    /// Démarre l'écoute (PC) ou la reconnexion automatique (téléphone).
    pub fn start(&self) -> Result<()> {
        let inner = self.inner.clone();
        // Nettoyage contrôlé des fichiers partiels abandonnés depuis plus de 7 jours.
        let dl = inner.download_dir();
        std::thread::spawn(move || nc_transfer::purge_stale_partials(&dl, Duration::from_secs(7 * 86_400)));
        match inner.cfg.role {
            Role::Pc => {
                let port = self.rt().block_on(start_listener(inner.clone()))?;
                #[cfg(feature = "mdns")]
                if inner.cfg.advertise {
                    match nc_transport::discovery::Advertiser::start(&inner.cfg.device.name, port, &inner.identity.fingerprint()) {
                        Ok(a) => std::mem::forget(a), // vit aussi longtemps que le processus
                        Err(e) => tracing::warn!("annonce mDNS impossible : {e}"),
                    }
                }
                inner.emit(Event::Started { fingerprint: inner.identity.fingerprint(), port: Some(port) });
            }
            Role::Phone => {
                self.rt().spawn(reconnect_loop(inner.clone()));
                inner.emit(Event::Started { fingerprint: inner.identity.fingerprint(), port: None });
            }
        }
        Ok(())
    }

    /// Arrêt propre : ferme les sessions (message `Bye`) et interrompt les transferts.
    pub fn stop(&mut self) {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let sessions: Vec<_> = self.inner.sessions.lock().unwrap().values().cloned().collect();
        for s in sessions {
            s.send(&Message::Bye);
            s.close();
        }
        if let Some(rt) = self.rt.take() {
            rt.shutdown_timeout(Duration::from_secs(2));
        }
    }

    // ------------------------------------------------------------------ identité

    pub fn fingerprint(&self) -> String {
        self.inner.identity.fingerprint()
    }

    pub fn listen_addr(&self) -> Option<SocketAddr> {
        *self.inner.listen_addr.lock().unwrap()
    }

    // ------------------------------------------------------------------ association

    /// PC : ouvre une fenêtre d'association (code 6 chiffres, 120 s, 3 essais).
    /// `lan_host` sert au QR code (adresse IP du PC sur le réseau local).
    pub fn open_pairing(&self, lan_host: Option<&str>) -> Result<PairingInfo> {
        if self.inner.cfg.role != Role::Pc {
            return Err(CoreError::WrongRole("PC"));
        }
        let w = PairingWindow::open();
        let port = self.listen_addr().map(|a| a.port()).unwrap_or(self.inner.cfg.listen_port);
        let fp = self.fingerprint();
        let qr_uri = lan_host.map(|h| QrPayload { host: h.into(), port, code: w.code().into(), pc_fingerprint: fp.clone() }.to_uri());
        let info = PairingInfo { code: w.code().into(), expires_in_s: w.remaining().as_secs(), qr_uri: qr_uri.clone(), fingerprint: fp };
        *self.inner.pairing.lock().unwrap() = Some(w);
        self.inner.emit(Event::PairingWindow { code: info.code.clone(), expires_in_s: info.expires_in_s, qr_uri });
        // Fermeture automatique à l'expiration.
        let inner = self.inner.clone();
        let code = info.code.clone();
        self.rt().spawn(async move {
            tokio::time::sleep(nc_pairing::WINDOW_TTL).await;
            let mut g = inner.pairing.lock().unwrap();
            if g.as_ref().map(|w| w.code() == code).unwrap_or(false) {
                *g = None;
                drop(g);
                inner.emit(Event::PairingWindowClosed);
            }
        });
        Ok(info)
    }

    pub fn close_pairing(&self) {
        if self.inner.pairing.lock().unwrap().take().is_some() {
            self.inner.emit(Event::PairingWindowClosed);
        }
    }

    /// PC : confirme ou refuse l'appareil qui a saisi le bon code.
    pub fn confirm_pairing(&self, peer_id: &str, accept: bool) -> Result<()> {
        let tx = self.inner.pending_pairings.lock().unwrap().remove(peer_id).ok_or(CoreError::UnknownDevice)?;
        let _ = tx.send(accept);
        Ok(())
    }

    /// Téléphone : association avec le code (saisie manuelle ou QR).
    pub fn pair_with(&self, addr: &str, code: &str, pc_fingerprint: Option<&str>) -> Result<()> {
        if self.inner.cfg.role != Role::Phone {
            return Err(CoreError::WrongRole("téléphone"));
        }
        let addr: SocketAddr = addr.parse().map_err(|_| CoreError::BadAddress)?;
        let code = nc_pairing::normalize_code(code).map_err(|_| CoreError::BadAddress)?;
        let fp = pc_fingerprint.map(str::to_string);
        let inner = self.inner.clone();
        self.rt().spawn(async move {
            match handshake::connect_pairing(addr, &inner.identity, &code, fp.as_deref()).await {
                Ok(est) => session::run(inner, est, true).await,
                Err(e) => inner.emit(Event::PairingFailed { reason: e.to_string() }),
            }
        });
        Ok(())
    }

    /// Téléphone : association depuis le contenu d'un QR code.
    pub fn pair_with_qr(&self, uri: &str) -> Result<()> {
        let q = QrPayload::parse(uri).map_err(|_| CoreError::BadAddress)?;
        let host = if q.host.contains(':') { format!("[{}]", q.host) } else { q.host.clone() };
        self.pair_with(&format!("{host}:{}", q.port), &q.code, Some(&q.pc_fingerprint))
    }

    // ------------------------------------------------------------------ appareils

    pub fn devices(&self) -> Result<Vec<DeviceRecord>> {
        Ok(self.inner.store.devices()?)
    }

    /// Pairs connectés : (identifiant, adresse, type de liaison).
    pub fn connected_peers(&self) -> Vec<PeerLink> {
        self.inner
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|s| PeerLink { peer_id: s.peer_id.clone(), addr: s.addr.to_string(), link: "wifi" })
            .collect()
    }

    pub fn rename_device(&self, peer_id: &str, name: Option<&str>) -> Result<()> {
        Ok(self.inner.store.rename_device(peer_id, name.filter(|n| !n.trim().is_empty()))?)
    }

    /// Réception automatique des fichiers venant de cet appareil de confiance.
    pub fn set_auto_accept(&self, peer_id: &str, on: bool) -> Result<()> {
        Ok(self.inner.store.set_auto_accept(peer_id, on)?)
    }

    /// Révocation immédiate : prévient le pair, ferme la session et supprime la clé.
    pub fn remove_device(&self, peer_id: &str) -> Result<()> {
        if let Some(s) = self.inner.session(peer_id) {
            s.send(&Message::Unpaired);
            s.close();
        }
        crate::transfers::abort_peer(&self.inner, peer_id);
        if !self.inner.store.remove_device(peer_id)? {
            return Err(CoreError::UnknownDevice);
        }
        self.inner.emit(Event::DeviceRemoved { peer_id: peer_id.into() });
        Ok(())
    }

    /// Téléphone : force une tentative de reconnexion immédiate (changement de réseau).
    pub fn network_changed(&self) {
        self.inner.refused.lock().unwrap().clear();
        self.inner.wake_reconnect.notify_one();
    }

    // ------------------------------------------------------------------ messages

    /// Envoie un message applicatif (état, notification, réponse de commande…).
    pub fn send(&self, peer_id: &str, msg: Message) -> Result<()> {
        let s = self.inner.session(peer_id).ok_or(CoreError::NotConnected)?;
        if s.send(&msg) { Ok(()) } else { Err(CoreError::NotConnected) }
    }

    /// Variante JSON (liaisons Kotlin / TypeScript) : le message est validé avant envoi.
    pub fn send_json(&self, peer_id: &str, json: &str) -> Result<()> {
        let msg: Message = serde_json::from_str(json)?;
        self.send(peer_id, msg)
    }

    /// Envoie un message à tous les pairs connectés (ex. téléphone → PC).
    pub fn broadcast(&self, msg: Message) -> usize {
        let sessions: Vec<_> = self.inner.sessions.lock().unwrap().values().cloned().collect();
        sessions.iter().filter(|s| s.send(&msg)).count()
    }

    pub fn command(&self, peer_id: &str, command: RemoteCommand) -> Result<u64> {
        let id = self.inner.next_cmd.fetch_add(1, Ordering::Relaxed);
        self.send(peer_id, Message::Command { id, command })?;
        Ok(id)
    }

    // ------------------------------------------------------------------ transferts

    /// Envoie des fichiers et/ou dossiers.
    pub fn send_paths(&self, peer_id: &str, paths: Vec<PathBuf>) -> Result<Uuid> {
        self.send_named(peer_id, paths.into_iter().map(|p| (p, None)).collect())
    }

    /// Envoie des entrées avec un nom d'affichage (Android : descripteurs ouverts).
    pub fn send_named(&self, peer_id: &str, inputs: Vec<(PathBuf, Option<String>)>) -> Result<Uuid> {
        crate::transfers::send_named(&self.inner, peer_id, inputs)
    }

    pub fn accept_transfer(&self, id: &Uuid, accept: bool) -> Result<()> {
        crate::transfers::answer_offer(&self.inner, id, accept)
    }

    pub fn pause_transfer(&self, id: &Uuid) -> Result<()> {
        crate::transfers::pause(&self.inner, id)
    }

    pub fn resume_transfer(&self, id: &Uuid) -> Result<()> {
        crate::transfers::resume(&self.inner, id)
    }

    pub fn cancel_transfer(&self, id: &Uuid) -> Result<()> {
        crate::transfers::cancel(&self.inner, id)
    }

    pub fn transfers(&self, limit: u32) -> Result<Vec<TransferRecord>> {
        Ok(self.inner.store.transfers(limit)?)
    }

    pub fn clear_transfer_history(&self) -> Result<usize> {
        Ok(self.inner.store.clear_transfer_history()?)
    }

    // ------------------------------------------------------------------ notifications

    pub fn notifications(&self, limit: u32) -> Result<Vec<NotificationRecord>> {
        Ok(self.inner.store.notifications(limit)?)
    }

    pub fn clear_notifications(&self) -> Result<usize> {
        Ok(self.inner.store.clear_notifications()?)
    }

    pub fn notification_apps(&self, peer_id: &str) -> Result<Vec<nc_protocol::AppEntry>> {
        Ok(self.inner.store.notification_apps(peer_id)?)
    }

    /// PC : active/désactive une application ; le filtre est appliqué **sur le téléphone**.
    pub fn set_notification_app(&self, peer_id: &str, package: &str, enabled: bool) -> Result<()> {
        self.inner.store.set_notification_app_enabled(peer_id, package, enabled)?;
        self.send(peer_id, Message::NotificationFilter { package: package.into(), enabled })
    }

    // ------------------------------------------------------------------ réglages

    pub fn settings(&self) -> Settings {
        self.inner.settings()
    }

    pub fn set_settings(&self, s: Settings) -> Result<()> {
        let was_suspended = self.inner.settings().suspended;
        self.inner.store.set_setting("settings", &s)?;
        let now_suspended = s.suspended;
        *self.inner.settings.lock().unwrap() = s;
        if now_suspended && !was_suspended {
            let sessions: Vec<_> = self.inner.sessions.lock().unwrap().values().cloned().collect();
            for s in sessions {
                s.send(&Message::Bye);
                s.close();
            }
        } else if !now_suspended && was_suspended {
            self.inner.wake_reconnect.notify_one();
        }
        Ok(())
    }

    /// Supprime toutes les données locales et l'identité (réinitialisation complète).
    pub fn wipe(&mut self, secrets: &dyn SecretStore) -> Result<()> {
        let peers: Vec<String> = self.devices()?.into_iter().map(|d| d.id).collect();
        for p in peers {
            let _ = self.remove_device(&p);
        }
        self.inner.store.wipe()?;
        Identity::destroy(secrets)?;
        Ok(())
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if self.rt.is_some() {
            self.stop();
        }
    }
}

async fn start_listener(inner: Arc<Inner>) -> Result<u16> {
    let want = SocketAddr::from(([0, 0, 0, 0], inner.cfg.listen_port));
    let listener = match TcpListener::bind(want).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("port {} indisponible ({e}), choix d'un port libre", inner.cfg.listen_port);
            TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0))).await?
        }
    };
    let addr = listener.local_addr()?;
    *inner.listen_addr.lock().unwrap() = Some(addr);
    tokio::spawn(async move {
        loop {
            let Ok((stream, from)) = listener.accept().await else {
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            };
            if inner.stopping.load(Ordering::SeqCst) {
                break;
            }
            if inner.is_suspended() {
                drop(stream); // connexions suspendues : fermeture sans réponse
                continue;
            }
            let inner = inner.clone();
            tokio::spawn(async move {
                match handshake::accept(stream, &inner.identity, &*inner).await {
                    Ok(est) => session::run(inner, est, false).await,
                    Err(TransportError::Protocol(nc_protocol::ProtocolError::IncompatibleVersion { remote, .. })) => {
                        inner.emit(Event::IncompatibleVersion { peer_id: None, remote, local: nc_protocol::PROTOCOL_VERSION });
                    }
                    Err(TransportError::PairingFailed) => inner.emit(Event::PairingFailed { reason: "code incorrect".into() }),
                    Err(e) => tracing::debug!("connexion refusée depuis {from} : {e}"),
                }
            });
        }
    });
    Ok(addr.port())
}

/// Téléphone : maintient la connexion avec les PC associés (adresse connue puis mDNS).
async fn reconnect_loop(inner: Arc<Inner>) {
    let mut backoff = Duration::from_secs(2);
    loop {
        if inner.stopping.load(Ordering::SeqCst) {
            return;
        }
        let mut all_connected = true;
        if !inner.is_suspended() {
            let pcs: Vec<DeviceRecord> = inner
                .store
                .devices()
                .unwrap_or_default()
                .into_iter()
                .filter(|d| d.platform == Platform::Windows)
                .collect();
            for pc in pcs {
                if inner.session(&pc.id).is_some() || inner.refused.lock().unwrap().contains(&pc.id) {
                    continue;
                }
                all_connected = false;
                if try_connect(&inner, &pc).await {
                    backoff = Duration::from_secs(2);
                }
            }
        }
        let wait = if all_connected { Duration::from_secs(30) } else { backoff };
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = inner.wake_reconnect.notified() => backoff = Duration::from_secs(2),
        }
        if !all_connected {
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    }
}

async fn try_connect(inner: &Arc<Inner>, pc: &DeviceRecord) -> bool {
    let Ok(key): std::result::Result<[u8; 32], _> = pc.public_key.as_slice().try_into() else { return false };
    let mut candidates: Vec<SocketAddr> = pc.last_addr.as_deref().and_then(|a| a.parse().ok()).into_iter().collect();
    #[cfg(feature = "mdns")]
    {
        let fp = nc_security::fingerprint(&key);
        if let Ok(Ok(found)) = tokio::task::spawn_blocking(|| nc_transport::discovery::browse(Duration::from_millis(1500))).await {
            for d in found.into_iter().filter(|d| d.fingerprint == fp) {
                for a in d.addrs {
                    if !candidates.contains(&a) {
                        candidates.push(a);
                    }
                }
            }
        }
    }
    for addr in candidates {
        match handshake::connect_paired(addr, &inner.identity, &key).await {
            Ok(est) => {
                let inner = inner.clone();
                tokio::spawn(session::run(inner, est, true));
                return true;
            }
            Err(TransportError::UnknownPeer) => {
                // Le PC ne nous reconnaît plus (révocation) : ne pas insister.
                inner.refused.lock().unwrap().insert(pc.id.clone());
                inner.emit(Event::PeerRefused { peer_id: pc.id.clone() });
                return false;
            }
            Err(e) => tracing::debug!("connexion à {addr} impossible : {e}"),
        }
    }
    false
}
