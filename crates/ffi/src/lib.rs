//! Interface Kotlin du cœur, générée par UniFFI.
//!
//! Surface volontairement réduite : les structures riches circulent en JSON (déjà
//! sérialisées par le cœur et validées à l'entrée), ce qui évite de dupliquer chaque type
//! du protocole en Kotlin et garde la compatibilité lors des mises à jour.

use nc_core::protocol::DeviceInfo;
use nc_core::{Event, EventSink, Node, NodeConfig, Role, SecretStore, Settings};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum NcError {
    #[error("{msg}")]
    Failed { msg: String },
}

impl From<nc_core::CoreError> for NcError {
    fn from(e: nc_core::CoreError) -> Self {
        NcError::Failed { msg: e.to_string() }
    }
}
impl From<serde_json::Error> for NcError {
    fn from(e: serde_json::Error) -> Self {
        NcError::Failed { msg: e.to_string() }
    }
}

type R<T> = Result<T, NcError>;

/// Reçoit chaque événement du cœur sous forme JSON (`{"type": …}`).
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, json: String);
}

/// Reçoit les unités de flux temps réel (audio du PC, écran étendu) sans passer par JSON.
#[uniffi::export(with_foreign)]
pub trait StreamListener: Send + Sync {
    fn on_stream(&self, peer_id: String, kind: u8, flags: u8, ts_us: u64, data: Vec<u8>);
}

/// Coffre de secrets implémenté en Kotlin (Android Keystore).
#[uniffi::export(with_foreign)]
pub trait SecretVault: Send + Sync {
    fn get(&self, name: String) -> Option<Vec<u8>>;
    fn set(&self, name: String, value: Vec<u8>) -> bool;
    fn delete(&self, name: String) -> bool;
}

struct Sink(Arc<dyn EventListener>, Arc<dyn StreamListener>);
impl EventSink for Sink {
    fn emit(&self, e: Event) {
        if let Ok(j) = serde_json::to_string(&e) {
            self.0.on_event(j);
        }
    }
    fn stream(&self, peer_id: &str, p: nc_core::StreamPacket) {
        self.1
            .on_stream(peer_id.to_string(), p.kind as u8, p.flags, p.ts_us, p.data);
    }
}

struct Vault(Arc<dyn SecretVault>);
impl SecretStore for Vault {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, nc_core::SecurityError> {
        Ok(self.0.get(name.into()))
    }
    fn set(&self, name: &str, value: &[u8]) -> Result<(), nc_core::SecurityError> {
        if self.0.set(name.into(), value.to_vec()) {
            Ok(())
        } else {
            Err(nc_core::SecurityError::Store("Keystore".into()))
        }
    }
    fn delete(&self, name: &str) -> Result<(), nc_core::SecurityError> {
        self.0.delete(name.into());
        Ok(())
    }
}

#[derive(uniffi::Record)]
pub struct NamedInput {
    /// Chemin réel ou `/proc/self/fd/N` pour un `content://` ouvert par Kotlin.
    pub path: String,
    pub name: Option<String>,
}

/// PC annoncés sur le réseau local (mDNS), pour l'écran d'association.
#[derive(uniffi::Record)]
pub struct DiscoveredPc {
    pub name: String,
    pub fingerprint: String,
    pub protocol: u16,
    /// Adresses `ip:port`.
    pub addrs: Vec<String>,
}

/// Recherche bloquante (`wait_ms`), à appeler hors du thread principal.
/// Android : un `WifiManager.MulticastLock` doit être tenu pendant l'appel.
#[uniffi::export]
pub fn discover_pcs(wait_ms: u32) -> Vec<DiscoveredPc> {
    nc_transport::discovery::browse(std::time::Duration::from_millis(wait_ms as u64))
        .unwrap_or_default()
        .into_iter()
        .map(|d| DiscoveredPc {
            name: d.name,
            fingerprint: d.fingerprint,
            protocol: d.protocol,
            addrs: d.addrs.iter().map(|a| a.to_string()).collect(),
        })
        .collect()
}

#[derive(uniffi::Object)]
pub struct NcNode {
    node: Mutex<Node>,
    vault: Vault,
}

fn uuid(s: &str) -> R<uuid::Uuid> {
    uuid::Uuid::parse_str(s).map_err(|e| NcError::Failed { msg: e.to_string() })
}

#[uniffi::export]
impl NcNode {
    /// `device_json` : [`DeviceInfo`] sérialisé.
    #[uniffi::constructor]
    pub fn new(
        data_dir: String,
        download_dir: String,
        device_json: String,
        vault: Arc<dyn SecretVault>,
        listener: Arc<dyn EventListener>,
        streams: Arc<dyn StreamListener>,
    ) -> R<Arc<Self>> {
        #[cfg(target_os = "android")]
        android_logger::init_once(
            android_logger::Config::default()
                .with_tag("nc-core")
                .with_max_level(log::LevelFilter::Info),
        );
        let device: DeviceInfo = serde_json::from_str(&device_json)?;
        let cfg = NodeConfig {
            role: Role::Phone,
            data_dir: PathBuf::from(data_dir),
            download_dir: PathBuf::from(download_dir),
            device,
            listen_port: 0,
            advertise: false,
        };
        let vault = Vault(vault);
        let node = Node::new(cfg, &vault, Arc::new(Sink(listener, streams)))?;
        Ok(Arc::new(Self {
            node: Mutex::new(node),
            vault,
        }))
    }

    pub fn start(&self) -> R<()> {
        Ok(self.node.lock().unwrap().start()?)
    }
    pub fn stop(&self) {
        self.node.lock().unwrap().stop();
    }
    pub fn fingerprint(&self) -> String {
        self.node.lock().unwrap().fingerprint()
    }
    pub fn protocol_version(&self) -> u16 {
        nc_core::protocol::PROTOCOL_VERSION
    }
    /// `pc_fingerprint` : empreinte annoncée en mDNS, vérifiée après le handshake.
    pub fn pair_with(&self, addr: String, code: String, pc_fingerprint: Option<String>) -> R<()> {
        Ok(self
            .node
            .lock()
            .unwrap()
            .pair_with(&addr, &code, pc_fingerprint.as_deref())?)
    }
    /// Association en essayant toutes les adresses du PC (découverte), puis le câble USB.
    pub fn pair_with_addrs(
        &self,
        addrs: Vec<String>,
        code: String,
        pc_fingerprint: Option<String>,
    ) -> R<()> {
        Ok(self
            .node
            .lock()
            .unwrap()
            .pair_with_addrs(&addrs, &code, pc_fingerprint.as_deref())?)
    }
    pub fn pair_with_qr(&self, uri: String) -> R<()> {
        Ok(self.node.lock().unwrap().pair_with_qr(&uri)?)
    }
    pub fn devices_json(&self) -> R<String> {
        Ok(serde_json::to_string(
            &self.node.lock().unwrap().devices()?,
        )?)
    }
    pub fn connected_json(&self) -> R<String> {
        Ok(serde_json::to_string(
            &self.node.lock().unwrap().connected_peers(),
        )?)
    }
    pub fn rename_device(&self, peer_id: String, name: Option<String>) -> R<()> {
        Ok(self
            .node
            .lock()
            .unwrap()
            .rename_device(&peer_id, name.as_deref())?)
    }
    pub fn set_auto_accept(&self, peer_id: String, on: bool) -> R<()> {
        Ok(self.node.lock().unwrap().set_auto_accept(&peer_id, on)?)
    }
    pub fn remove_device(&self, peer_id: String) -> R<()> {
        Ok(self.node.lock().unwrap().remove_device(&peer_id)?)
    }
    pub fn network_changed(&self) {
        self.node.lock().unwrap().network_changed();
    }
    /// Envoie un message JSON (validé) à un pair.
    pub fn send_json(&self, peer_id: String, json: String) -> R<()> {
        Ok(self.node.lock().unwrap().send_json(&peer_id, &json)?)
    }
    /// Envoie un message JSON à tous les PC connectés ; renvoie le nombre de destinataires.
    pub fn broadcast_json(&self, json: String) -> R<u32> {
        let msg = nc_core::parse_message(&json)?;
        Ok(self.node.lock().unwrap().broadcast(msg) as u32)
    }
    pub fn send_files(&self, peer_id: String, inputs: Vec<NamedInput>) -> R<String> {
        let v = inputs
            .into_iter()
            .map(|i| (PathBuf::from(i.path), i.name))
            .collect();
        Ok(self
            .node
            .lock()
            .unwrap()
            .send_named(&peer_id, v)?
            .to_string())
    }
    pub fn accept_transfer(&self, id: String, accept: bool) -> R<()> {
        Ok(self
            .node
            .lock()
            .unwrap()
            .accept_transfer(&uuid(&id)?, accept)?)
    }
    pub fn pause_transfer(&self, id: String) -> R<()> {
        Ok(self.node.lock().unwrap().pause_transfer(&uuid(&id)?)?)
    }
    pub fn resume_transfer(&self, id: String) -> R<()> {
        Ok(self.node.lock().unwrap().resume_transfer(&uuid(&id)?)?)
    }
    pub fn cancel_transfer(&self, id: String) -> R<()> {
        Ok(self.node.lock().unwrap().cancel_transfer(&uuid(&id)?)?)
    }
    pub fn transfers_json(&self, limit: u32) -> R<String> {
        Ok(serde_json::to_string(
            &self.node.lock().unwrap().transfers(limit)?,
        )?)
    }
    pub fn clear_transfer_history(&self) -> R<u32> {
        Ok(self.node.lock().unwrap().clear_transfer_history()? as u32)
    }
    pub fn settings_json(&self) -> R<String> {
        Ok(serde_json::to_string(
            &self.node.lock().unwrap().settings(),
        )?)
    }
    pub fn set_settings_json(&self, json: String) -> R<()> {
        let s: Settings = serde_json::from_str(&json)?;
        Ok(self.node.lock().unwrap().set_settings(s)?)
    }
    /// Envoie une unité de flux (1 = miroir, 2 = audio du téléphone, 3 = audio du PC,
    /// 4 = écran étendu). `false` : unité abandonnée (réseau saturé ou PC absent).
    pub fn send_stream(
        &self,
        peer_id: String,
        kind: u8,
        flags: u8,
        ts_us: u64,
        data: Vec<u8>,
    ) -> bool {
        let Some(kind) = nc_core::protocol::StreamKind::from_u8(kind) else {
            return false;
        };
        self.node
            .lock()
            .unwrap()
            .send_stream(&peer_id, kind, flags, ts_us, &data)
    }

    /// Signale un flux actif (informatif : la bascule Wi-Fi ↔ USB est sans coupure pour les flux).
    pub fn set_streaming(&self, active: bool) {
        self.node.lock().unwrap().set_streaming(active);
    }

    /// Câble USB branché : bascule sur le câble dès que le PC a créé le tunnel.
    pub fn cable_hint(&self) {
        self.node.lock().unwrap().cable_hint();
    }

    /// Efface toutes les données et l'identité de l'appareil.
    pub fn wipe(&self) -> R<()> {
        Ok(self.node.lock().unwrap().wipe(&self.vault)?)
    }
}
