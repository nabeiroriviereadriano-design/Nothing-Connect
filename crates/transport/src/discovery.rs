//! Découverte mDNS / DNS-SD sur le réseau local.
//!
//! Le PC annonce `_nothingconnect._tcp.local.` avec, en TXT : `fp` (empreinte publique),
//! `pv` (version de protocole) et `n` (nom). Aucun secret n'est publié. Côté Android,
//! l'application peut utiliser `NsdManager` (natif) ou ce module.

use crate::TransportError;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

pub const SERVICE_TYPE: &str = "_nothingconnect._tcp.local.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    pub name: String,
    pub fingerprint: String,
    pub protocol: u16,
    pub addrs: Vec<SocketAddr>,
}

pub struct Advertiser {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Advertiser {
    pub fn start(instance: &str, port: u16, fingerprint: &str) -> Result<Self, TransportError> {
        let daemon = ServiceDaemon::new().map_err(|e| TransportError::Discovery(e.to_string()))?;
        // Instance unique et stable par PC : empreinte courte.
        let instance_name = format!("{}-{}", sanitize(instance), &fingerprint.replace('-', "")[..8]);
        let host = format!("nc-{}.local.", fingerprint.replace('-', "")[..8].to_lowercase());
        let pv = nc_protocol::PROTOCOL_VERSION.to_string();
        let props = [("fp", fingerprint), ("pv", pv.as_str()), ("n", instance)];
        let info = ServiceInfo::new(SERVICE_TYPE, &instance_name, &host, (), port, &props[..])
            .map_err(|e| TransportError::Discovery(e.to_string()))?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info).map_err(|e| TransportError::Discovery(e.to_string()))?;
        Ok(Self { daemon, fullname })
    }

    pub fn stop(self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Recherche pendant `wait` et renvoie les PC trouvés.
pub fn browse(wait: Duration) -> Result<Vec<Discovered>, TransportError> {
    let daemon = ServiceDaemon::new().map_err(|e| TransportError::Discovery(e.to_string()))?;
    let rx = daemon.browse(SERVICE_TYPE).map_err(|e| TransportError::Discovery(e.to_string()))?;
    let deadline = std::time::Instant::now() + wait;
    let mut found: Vec<Discovered> = Vec::new();
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(ServiceEvent::ServiceResolved(s)) => {
                let Some(fp) = s.get_property_val_str("fp") else { continue };
                let addrs: Vec<SocketAddr> = s
                    .get_addresses_v4()
                    .into_iter()
                    .map(|ip| SocketAddr::new(IpAddr::V4(ip), s.get_port()))
                    .collect();
                let d = Discovered {
                    name: s.get_property_val_str("n").unwrap_or("PC").to_string(),
                    fingerprint: fp.to_string(),
                    protocol: s.get_property_val_str("pv").and_then(|v| v.parse().ok()).unwrap_or(0),
                    addrs,
                };
                if let Some(e) = found.iter_mut().find(|e| e.fingerprint == d.fingerprint) {
                    *e = d;
                } else {
                    found.push(d);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.stop_browse(SERVICE_TYPE);
    let _ = daemon.shutdown();
    Ok(found)
}

fn sanitize(s: &str) -> String {
    let v: String = s.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == ' ').take(40).collect();
    if v.trim().is_empty() { "PC".into() } else { v }
}
