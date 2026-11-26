//! Téléphone simulé pour développer et tester l'application Windows sans appareil réel.
//! Outil de développement uniquement (non livré).
//!
//! Usage : `cargo run -p nc-core --example fake_phone -- <adresse:port> <code>`
//! Les données du faux téléphone sont dans `target/fake-phone` (association conservée).

use nc_core::protocol::*;
use nc_core::{Event, EventSink, MemoryStore, Node, NodeConfig, Role, SecretStore};
use std::sync::{mpsc, Arc};
use std::time::Duration;

struct Sink(mpsc::Sender<Event>);
impl EventSink for Sink {
    fn emit(&self, e: Event) {
        let _ = self.0.send(e);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = std::path::PathBuf::from("target/fake-phone");
    std::fs::create_dir_all(&dir).unwrap();
    let key_file = dir.join("identity.key");
    let secrets = MemoryStore::default();
    if let Ok(k) = std::fs::read(&key_file) {
        secrets.set("identity.x25519", &k).unwrap();
    }
    let (tx, rx) = mpsc::channel();
    let cfg = NodeConfig {
        role: Role::Phone,
        data_dir: dir.join("data"),
        download_dir: dir.join("recus"),
        device: DeviceInfo {
            name: "Phone (3a) simulé".into(),
            platform: Platform::Android,
            manufacturer: Some("Nothing".into()),
            model: Some("A059".into()),
            os_version: Some("15".into()),
            app_version: "0.1.0".into(),
            capabilities: vec![
                Capability::FileTransfer, Capability::ShareText, Capability::DeviceStatus, Capability::Notifications,
                Capability::NotificationActions, Capability::MediaControl, Capability::Ring, Capability::Vibrate,
                Capability::MediaVolume, Capability::DoNotDisturb,
            ],
        },
        listen_port: 0,
        advertise: false,
    };
    let node = Node::new(cfg, &secrets, Arc::new(Sink(tx))).unwrap();
    std::fs::write(&key_file, secrets.get("identity.x25519").unwrap().unwrap()).unwrap();
    node.start().unwrap();
    if args.len() >= 3 {
        node.pair_with(&args[1], &args[2], None).unwrap();
    }
    let mut volume = 60u8;
    let mut dnd = false;
    let mut pc = None::<String>;
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Event::PeerConnected { peer_id, .. }) => {
                println!("connecté à {peer_id}");
                let p = peer_id.clone();
                pc = Some(peer_id);
                send_all(&node, &p, volume, dnd);
            }
            Ok(Event::Request { peer_id, message }) => {
                println!("demande : {message:?}");
                match message {
                    Message::Command { id, command } => {
                        match command {
                            RemoteCommand::SetMediaVolume { percent } => volume = percent,
                            RemoteCommand::SetDoNotDisturb { enabled } => dnd = enabled,
                            _ => {}
                        }
                        let _ = node.send(&peer_id, Message::CommandResult { id, ok: true, error: None });
                        send_all(&node, &peer_id, volume, dnd);
                    }
                    Message::NotificationDismiss { key } => {
                        let _ = node.send(&peer_id, Message::NotificationRemoved { key });
                    }
                    _ => {}
                }
            }
            Ok(Event::TransferIncoming { id, .. }) => {
                let _ = node.accept_transfer(&id, true);
            }
            Ok(Event::PairingFailed { reason }) => println!("échec : {reason}"),
            Ok(e) => println!("{e:?}"),
            Err(_) => {
                if let Some(p) = &pc {
                    send_all(&node, p, volume, dnd);
                }
            }
        }
    }
}

fn send_all(node: &Node, peer: &str, volume: u8, dnd: bool) {
    let _ = node.send(peer, Message::Status(DeviceStatus {
        battery_percent: Some(78),
        charging: Some(true),
        battery_temp_c: Some(31.4),
        storage_free_bytes: Some(87_400_000_000),
        storage_total_bytes: Some(256_000_000_000),
        wifi_connected: Some(true),
        wifi_ssid: Some("Maison".into()),
        bluetooth_enabled: Some(true),
        mobile_network: Some("Free 5G".into()),
        media_volume_percent: Some(volume),
        dnd_enabled: Some(dnd),
    }));
    let _ = node.send(peer, Message::NotificationApps { apps: vec![
        AppEntry { package: "com.whatsapp".into(), app_name: "WhatsApp".into(), enabled: true },
        AppEntry { package: "com.discord".into(), app_name: "Discord".into(), enabled: true },
        AppEntry { package: "com.instagram.android".into(), app_name: "Instagram".into(), enabled: false },
        AppEntry { package: "com.google.android.gm".into(), app_name: "Gmail".into(), enabled: true },
    ]});
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let _ = node.send(peer, Message::NotificationPosted(PhoneNotification {
        key: "0|com.whatsapp|1".into(), package: "com.whatsapp".into(), app_name: "WhatsApp".into(),
        title: Some("Léa".into()), text: Some("On se retrouve à 19 h devant le cinéma ?".into()), posted_at: now - 60_000,
        icon_png_b64: None,
        actions: vec![NotificationActionInfo { index: 0, title: "Répondre".into(), accepts_text: true }, NotificationActionInfo { index: 1, title: "Marquer comme lu".into(), accepts_text: false }],
        sensitive: false,
    }));
    let _ = node.send(peer, Message::NotificationPosted(PhoneNotification {
        key: "0|com.google.android.gm|7".into(), package: "com.google.android.gm".into(), app_name: "Gmail".into(),
        title: Some("Votre colis arrive demain".into()), text: Some("Livraison prévue entre 9 h et 13 h.".into()), posted_at: now - 600_000,
        icon_png_b64: None, actions: vec![], sensitive: false,
    }));
    let _ = node.send(peer, Message::MediaSessions { sessions: vec![MediaSessionInfo {
        id: "spotify".into(), package: "com.spotify.music".into(), app_name: "Spotify".into(),
        title: Some("Morceau de démonstration".into()), artist: Some("Artiste".into()), album: Some("Album".into()),
        duration_ms: Some(214_000), position_ms: Some(81_000), state: PlaybackState::Playing, artwork_jpeg_b64: None,
        can_skip_next: true, can_skip_prev: true,
    }]});
}
