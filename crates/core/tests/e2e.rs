//! Tests de bout en bout : un PC et un téléphone simulés dans le même processus,
//! reliés en TCP local avec le vrai protocole chiffré.

use nc_core::protocol::{DeviceInfo, Message, Platform, RemoteCommand};
use nc_core::{Event, EventSink, MemoryStore, Node, NodeConfig, Role, Settings, TransferState};
use rand::RngCore;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Collector {
    events: Mutex<Vec<Event>>,
    cv: Condvar,
}

impl EventSink for Collector {
    fn emit(&self, e: Event) {
        self.events.lock().unwrap().push(e);
        self.cv.notify_all();
    }
}

impl Collector {
    fn wait<T>(&self, timeout: Duration, mut f: impl FnMut(&Event) -> Option<T>) -> T {
        let deadline = Instant::now() + timeout;
        let mut g = self.events.lock().unwrap();
        let mut seen = 0;
        loop {
            while seen < g.len() {
                if let Some(v) = f(&g[seen]) {
                    return v;
                }
                seen += 1;
            }
            let left = deadline
                .checked_duration_since(Instant::now())
                .unwrap_or_else(|| {
                    panic!("délai dépassé ; événements : {:#?}", *g);
                });
            g = self.cv.wait_timeout(g, left).unwrap().0;
        }
    }
    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

struct Peer {
    node: Node,
    ev: Arc<Collector>,
    _secrets: MemoryStore,
    _dir: tempfile::TempDir,
    downloads: PathBuf,
}

fn peer(role: Role) -> Peer {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("recus");
    let secrets = MemoryStore::default();
    let ev = Arc::new(Collector::default());
    let cfg = NodeConfig {
        role,
        data_dir: dir.path().join("data"),
        download_dir: downloads.clone(),
        device: DeviceInfo {
            name: if role == Role::Pc {
                "PC-Test".into()
            } else {
                "Phone-Test".into()
            },
            platform: if role == Role::Pc {
                Platform::Windows
            } else {
                Platform::Android
            },
            manufacturer: None,
            model: None,
            os_version: None,
            app_version: "0.1.0".into(),
            capabilities: vec![],
        },
        listen_port: 0,
        advertise: false,
    };
    let node = Node::new(cfg, &secrets, ev.clone()).unwrap();
    node.start().unwrap();
    Peer {
        node,
        ev,
        _secrets: secrets,
        _dir: dir,
        downloads,
    }
}

const T: Duration = Duration::from_secs(20);

/// Associe un téléphone à un PC et renvoie (id du téléphone vu du PC, id du PC vu du téléphone).
fn pair(pc: &Peer, phone: &Peer) -> (String, String) {
    let info = pc.node.open_pairing(Some("127.0.0.1")).unwrap();
    phone
        .node
        .pair_with_qr(info.qr_uri.as_deref().unwrap())
        .unwrap();
    let phone_id = pc.ev.wait(T, |e| match e {
        Event::PairingRequest { peer_id, .. } => Some(peer_id.clone()),
        _ => None,
    });
    pc.node.confirm_pairing(&phone_id, true).unwrap();
    let pc_id = phone.ev.wait(T, |e| match e {
        Event::Paired { device } => Some(device.id.clone()),
        _ => None,
    });
    pc.ev.wait(T, |e| {
        matches!(e, Event::PeerConnected { .. }).then_some(())
    });
    phone.ev.wait(T, |e| {
        matches!(e, Event::PeerConnected { .. }).then_some(())
    });
    (phone_id, pc_id)
}

fn random_file(dir: &Path, name: &str, size: usize) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let mut v = vec![0u8; size];
    rand::thread_rng().fill_bytes(&mut v);
    let p = dir.join(name);
    std::fs::write(&p, v).unwrap();
    p
}

fn wait_finished(p: &Peer, id: uuid::Uuid) -> TransferState {
    p.ev.wait(T, |e| match e {
        Event::TransferFinished { id: i, state, .. } if *i == id => Some(*state),
        _ => None,
    })
}

#[test]
fn pairing_with_code_and_confirmation() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    assert_eq!(pc.node.devices().unwrap()[0].id, phone_id);
    assert_eq!(phone.node.devices().unwrap()[0].id, pc_id);
    assert_eq!(phone.node.devices().unwrap()[0].name, "PC-Test");
    // La fenêtre d'association est refermée après usage.
    pc.ev
        .wait(T, |e| matches!(e, Event::PairingWindowClosed).then_some(()));
}

#[test]
fn pairing_rejected_by_user() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let info = pc.node.open_pairing(None).unwrap();
    let addr = pc.node.listen_addr().unwrap();
    phone
        .node
        .pair_with(&format!("127.0.0.1:{}", addr.port()), &info.code, None)
        .unwrap();
    let id = pc.ev.wait(T, |e| match e {
        Event::PairingRequest { peer_id, .. } => Some(peer_id.clone()),
        _ => None,
    });
    pc.node.confirm_pairing(&id, false).unwrap();
    phone.ev.wait(T, |e| {
        matches!(e, Event::PairingFailed { .. }).then_some(())
    });
    assert!(pc.node.devices().unwrap().is_empty());
    assert!(phone.node.devices().unwrap().is_empty());
}

#[test]
fn wrong_code_is_refused() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let info = pc.node.open_pairing(None).unwrap();
    let wrong = if info.code == "000000" {
        "111111"
    } else {
        "000000"
    };
    let addr = pc.node.listen_addr().unwrap();
    phone
        .node
        .pair_with(&format!("127.0.0.1:{}", addr.port()), wrong, None)
        .unwrap();
    phone.ev.wait(T, |e| {
        matches!(e, Event::PairingFailed { .. }).then_some(())
    });
    assert!(pc.node.devices().unwrap().is_empty());
}

#[test]
fn files_and_folders_both_directions_with_sha256() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    let src = tempfile::tempdir().unwrap();
    // PC → téléphone : un gros fichier + un dossier.
    let big = random_file(src.path(), "video.bin", 5_000_000);
    let folder = src.path().join("Dossier");
    random_file(&folder.join("sous"), "a.txt", 1234);
    random_file(&folder, "vide.txt", 0);
    let id = pc
        .node
        .send_paths(&phone_id, vec![big.clone(), folder])
        .unwrap();
    let inc = phone.ev.wait(T, |e| match e {
        Event::TransferIncoming {
            id, total_bytes, ..
        } => Some((*id, *total_bytes)),
        _ => None,
    });
    assert_eq!(inc, (id, 5_000_000 + 1234));
    phone.node.accept_transfer(&id, true).unwrap();
    assert_eq!(wait_finished(&phone, id), TransferState::Completed);
    assert_eq!(wait_finished(&pc, id), TransferState::Completed);
    assert_eq!(
        std::fs::read(phone.downloads.join("video.bin")).unwrap(),
        std::fs::read(&big).unwrap()
    );
    assert!(phone.downloads.join("Dossier/sous/a.txt").exists());
    assert!(phone.downloads.join("Dossier/vide.txt").exists());
    // Historique
    let h = pc.node.transfers(10).unwrap();
    assert_eq!(h[0].state, "completed");
    // Téléphone → PC avec réception automatique activée.
    pc.node.set_auto_accept(&phone_id, true).unwrap();
    let p = random_file(src.path(), "photo.jpg", 300_000);
    let id2 = phone.node.send_paths(&pc_id, vec![p.clone()]).unwrap();
    assert_eq!(wait_finished(&pc, id2), TransferState::Completed);
    assert_eq!(
        std::fs::read(pc.downloads.join("photo.jpg")).unwrap(),
        std::fs::read(&p).unwrap()
    );
    // Renvoi du même fichier : identique → pas de doublon.
    let id3 = phone.node.send_paths(&pc_id, vec![p]).unwrap();
    assert_eq!(wait_finished(&pc, id3), TransferState::Completed);
    assert!(!pc.downloads.join("photo (1).jpg").exists());
}

#[test]
fn reject_and_cancel() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, _) = pair(&pc, &phone);
    let src = tempfile::tempdir().unwrap();
    let f = random_file(src.path(), "x.bin", 1000);
    let id = pc.node.send_paths(&phone_id, vec![f.clone()]).unwrap();
    phone.ev.wait(T, |e| {
        matches!(e, Event::TransferIncoming { .. }).then_some(())
    });
    phone.node.accept_transfer(&id, false).unwrap();
    assert_eq!(wait_finished(&pc, id), TransferState::Rejected);

    let big = random_file(src.path(), "big.bin", 60_000_000);
    let id = pc.node.send_paths(&phone_id, vec![big]).unwrap();
    phone.ev.wait(T, |e| {
        matches!(e, Event::TransferIncoming { id: i, .. } if *i == id).then_some(())
    });
    phone.node.accept_transfer(&id, true).unwrap();
    phone.ev.wait(T, |e| matches!(e, Event::TransferProgress { id: i, bytes_done, .. } if *i == id && *bytes_done > 0).then_some(()));
    phone.node.cancel_transfer(&id).unwrap();
    assert_eq!(wait_finished(&pc, id), TransferState::Cancelled);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!phone.downloads.join("big.bin").exists());
    assert!(
        !phone.downloads.join(".nc-partial").exists(),
        "partiels supprimés"
    );
}

#[test]
fn pause_and_resume_keeps_integrity() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    pc.node.set_auto_accept(&phone_id, true).unwrap();
    let src = tempfile::tempdir().unwrap();
    let big = random_file(src.path(), "archive.zip", 30_000_000);
    let id = phone.node.send_paths(&pc_id, vec![big.clone()]).unwrap();
    pc.ev.wait(T, |e| matches!(e, Event::TransferProgress { id: i, bytes_done, .. } if *i == id && *bytes_done > 0).then_some(()));
    // Pause côté récepteur, puis reprise côté émetteur.
    pc.node.pause_transfer(&id).unwrap();
    phone.ev.wait(T, |e| matches!(e, Event::TransferProgress { id: i, state: TransferState::Paused, .. } if *i == id).then_some(()));
    std::thread::sleep(Duration::from_millis(300));
    phone.node.resume_transfer(&id).unwrap();
    assert_eq!(wait_finished(&pc, id), TransferState::Completed);
    assert!(
        std::fs::read(pc.downloads.join("archive.zip")).unwrap() == std::fs::read(&big).unwrap()
    );
}

#[test]
fn messages_commands_and_notifications() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    // Commande PC → téléphone, réponse téléphone → PC.
    let cid = pc.node.command(&phone_id, RemoteCommand::Ring).unwrap();
    let (got, from) = phone.ev.wait(T, |e| match e {
        Event::Request {
            peer_id,
            message: Message::Command { id, command },
        } => Some(((*id, command.clone()), peer_id.clone())),
        _ => None,
    });
    assert_eq!(got, (cid, RemoteCommand::Ring));
    phone
        .node
        .send(
            &from,
            Message::CommandResult {
                id: cid,
                ok: true,
                error: None,
            },
        )
        .unwrap();
    pc.ev.wait(T, |e| {
        matches!(e, Event::CommandResult { ok: true, id, .. } if *id == cid).then_some(())
    });

    // Notification sensible : contenu masqué et non stocké.
    let n = nc_core::protocol::PhoneNotification {
        key: "0|com.bank|1".into(),
        package: "com.bank".into(),
        app_name: "Banque".into(),
        title: Some("Code 123456".into()),
        text: Some("secret".into()),
        posted_at: 1,
        icon_png_b64: None,
        actions: vec![],
        sensitive: true,
    };
    phone
        .node
        .send_json(&pc_id, &serde_json_string(&Message::NotificationPosted(n)))
        .unwrap();
    let shown = pc.ev.wait(T, |e| match e {
        Event::Notification { notification, .. } => Some(notification.clone()),
        _ => None,
    });
    assert_eq!(shown.title.as_deref(), Some("Contenu masqué"));
    assert!(shown.text.is_none());
    assert!(pc.node.notifications(10).unwrap()[0].title.is_none());

    // Filtre par application : transmis au téléphone.
    phone
        .node
        .send(
            &pc_id,
            Message::NotificationApps {
                apps: vec![nc_core::protocol::AppEntry {
                    package: "com.tiktok".into(),
                    app_name: "TikTok".into(),
                    enabled: true,
                }],
            },
        )
        .unwrap();
    pc.ev.wait(T, |e| {
        matches!(e, Event::NotificationApps { .. }).then_some(())
    });
    pc.node
        .set_notification_app(&phone_id, "com.tiktok", false)
        .unwrap();
    phone.ev.wait(T, |e| {
        matches!(
            e,
            Event::Request {
                message: Message::NotificationFilter { enabled: false, .. },
                ..
            }
        )
        .then_some(())
    });
    assert!(!pc.node.notification_apps(&phone_id).unwrap()[0].enabled);

    // Un message JSON invalide est refusé avant envoi.
    assert!(phone.node.send_json(&pc_id, "{\"t\":\"nope\"}").is_err());
}

fn serde_json_string(m: &Message) -> String {
    serde_json::to_string(m).unwrap()
}

#[test]
fn reconnects_after_restart_and_revocation_is_immediate() {
    let pc = peer(Role::Pc);
    let mut phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    // Le téléphone se ferme proprement…
    phone.node.stop();
    pc.ev.wait(T, |e| {
        matches!(e, Event::PeerDisconnected { .. }).then_some(())
    });
    // …et une nouvelle instance avec les mêmes données se reconnecte seule.
    let cfg_dir = phone._dir.path().to_path_buf();
    let secrets = std::mem::take(&mut phone._secrets);
    let ev = Arc::new(Collector::default());
    let cfg = NodeConfig {
        role: Role::Phone,
        data_dir: cfg_dir.join("data"),
        download_dir: cfg_dir.join("recus"),
        device: DeviceInfo {
            name: "Phone-Test".into(),
            platform: Platform::Android,
            manufacturer: None,
            model: None,
            os_version: None,
            app_version: "0.2.0".into(),
            capabilities: vec![],
        },
        listen_port: 0,
        advertise: false,
    };
    pc.ev.clear();
    let phone2 = Node::new(cfg, &secrets, ev.clone()).unwrap();
    phone2.start().unwrap();
    ev.wait(T, |e| {
        matches!(e, Event::PeerConnected { peer_id, .. } if *peer_id == pc_id).then_some(())
    });
    pc.ev.wait(T, |e| {
        matches!(e, Event::PeerConnected { .. }).then_some(())
    });
    // Le PC voit la nouvelle version de l'application (mise à jour conservant l'association).
    assert_eq!(
        pc.node.devices().unwrap()[0].app_version.as_deref(),
        Some("0.2.0")
    );
    // Révocation côté PC : le téléphone supprime aussi l'association.
    pc.node.remove_device(&phone_id).unwrap();
    ev.wait(T, |e| {
        matches!(e, Event::DeviceRemoved { .. }).then_some(())
    });
    assert!(phone2.devices().unwrap().is_empty());
    assert!(pc.node.devices().unwrap().is_empty());
}

#[test]
fn revoked_phone_is_refused() {
    let pc = peer(Role::Pc);
    let mut phone = peer(Role::Phone);
    let (phone_id, _pc_id) = pair(&pc, &phone);
    // Suppression locale côté PC sans pouvoir prévenir (téléphone éteint).
    phone.node.stop();
    pc.ev.wait(T, |e| {
        matches!(e, Event::PeerDisconnected { .. }).then_some(())
    });
    pc.node.remove_device(&phone_id).unwrap();
    let secrets = std::mem::take(&mut phone._secrets);
    let ev = Arc::new(Collector::default());
    let dir = phone._dir.path().to_path_buf();
    let cfg = NodeConfig {
        role: Role::Phone,
        data_dir: dir.join("data"),
        download_dir: dir.join("recus"),
        device: DeviceInfo {
            name: "Phone-Test".into(),
            platform: Platform::Android,
            manufacturer: None,
            model: None,
            os_version: None,
            app_version: "0.1.0".into(),
            capabilities: vec![],
        },
        listen_port: 0,
        advertise: false,
    };
    let phone2 = Node::new(cfg, &secrets, ev.clone()).unwrap();
    phone2.start().unwrap();
    ev.wait(T, |e| matches!(e, Event::PeerRefused { .. }).then_some(()));
    assert!(pc.node.connected_peers().is_empty());
}

#[test]
fn suspend_closes_connections() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    pair(&pc, &phone);
    pc.node
        .set_settings(Settings {
            suspended: true,
            ..pc.node.settings()
        })
        .unwrap();
    phone.ev.wait(T, |e| {
        matches!(e, Event::PeerDisconnected { .. }).then_some(())
    });
    pc.ev.wait(T, |e| {
        matches!(e, Event::PeerDisconnected { .. }).then_some(())
    });
    // Le téléphone ne parvient pas à se reconnecter tant que le PC est suspendu.
    std::thread::sleep(Duration::from_secs(3));
    assert!(pc.node.connected_peers().is_empty());
    // Les réglages survivent (persistés en base).
    assert!(pc.node.settings().suspended);
}

#[test]
fn transfer_resumes_after_connection_loss() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    pc.node.set_auto_accept(&phone_id, true).unwrap();
    let src = tempfile::tempdir().unwrap();
    let big = random_file(
        src.path(),
        "film.mkv",
        if cfg!(debug_assertions) {
            40_000_000
        } else {
            150_000_000
        },
    );
    let id = phone.node.send_paths(&pc_id, vec![big.clone()]).unwrap();
    pc.ev.wait(T, |e| matches!(e, Event::TransferProgress { id: i, bytes_done, .. } if *i == id && *bytes_done > 5_000_000).then_some(()));
    // Coupure : le PC suspend toutes les connexions puis les réautorise.
    pc.node
        .set_settings(Settings {
            suspended: true,
            ..pc.node.settings()
        })
        .unwrap();
    phone.ev.wait(T, |e| matches!(e, Event::TransferProgress { id: i, state: TransferState::Interrupted, .. } if *i == id).then_some(()));
    pc.node
        .set_settings(Settings {
            suspended: false,
            ..pc.node.settings()
        })
        .unwrap();
    phone.node.network_changed();
    // Reconnexion automatique, nouvelle offre avec le même identifiant, reprise à l'offset.
    let st = pc.ev.wait(Duration::from_secs(60), |e| match e {
        Event::TransferFinished { id: i, state, .. } if *i == id => Some(*state),
        _ => None,
    });
    assert_eq!(st, TransferState::Completed);
    assert!(std::fs::read(pc.downloads.join("film.mkv")).unwrap() == std::fs::read(&big).unwrap());
    assert!(!pc.downloads.join("film (1).mkv").exists());
}

/// Mesure de débit : significative seulement en `--release`.
#[test]
#[cfg_attr(debug_assertions, ignore)]
fn throughput_loopback() {
    let pc = peer(Role::Pc);
    let phone = peer(Role::Phone);
    let (phone_id, pc_id) = pair(&pc, &phone);
    pc.node.set_auto_accept(&phone_id, true).unwrap();
    let src = tempfile::tempdir().unwrap();
    let size = 300_000_000;
    let big = random_file(src.path(), "debit.bin", size);
    let t = Instant::now();
    let id = phone.node.send_paths(&pc_id, vec![big]).unwrap();
    assert_eq!(wait_finished(&pc, id), TransferState::Completed);
    let mbps = size as f64 / t.elapsed().as_secs_f64() / 1e6;
    eprintln!("DEBIT boucle locale (chiffré + SHA-256 des deux côtés + disque) : {mbps:.0} Mo/s");
    assert!(mbps > 20.0, "débit anormalement faible : {mbps:.1} Mo/s");
}
