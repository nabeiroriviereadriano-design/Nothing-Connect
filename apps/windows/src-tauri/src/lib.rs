//! Application Windows : couche d'interface et d'intégration système uniquement.
//! Toute la logique métier est dans `nc-core`.

mod commands;
mod secrets;
mod sink;
mod tray;

use nc_core::protocol::{Capability, DeviceInfo, Platform};
use nc_core::{Node, NodeConfig, Role, Settings};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{Manager, WindowEvent};

pub struct AppState {
    pub node: Mutex<Node>,
    /// Copie des réglages lisible sans verrouiller le nœud (utilisée pendant les événements).
    pub settings: Arc<RwLock<Settings>>,
    pub data_dir: PathBuf,
    pub secrets: Box<dyn nc_core::SecretStore>,
    pub snapshot: Arc<Mutex<sink::Snapshot>>,
}

/// Argument passé par le démarrage automatique : démarrer réduit dans la zone de notification.
pub const HIDDEN_ARG: &str = "--hidden";
/// Argument utilisé par le désinstalleur : efface les secrets du Credential Manager.
pub const PURGE_ARG: &str = "--purge-secrets";

fn device_info() -> DeviceInfo {
    let name = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "PC".into());
    DeviceInfo {
        name,
        platform: Platform::Windows,
        manufacturer: None,
        model: None,
        os_version: Some(std::env::consts::OS.into()),
        app_version: env!("CARGO_PKG_VERSION").into(),
        capabilities: vec![Capability::FileTransfer, Capability::ShareText],
    }
}

fn init_logs(dir: &std::path::Path) {
    let logs = dir.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .max_log_files(7)
        .filename_prefix("nothing-connect")
        .filename_suffix("log")
        .build(&logs);
    if let Ok(appender) = appender {
        let _ = tracing_subscriber::fmt()
            .with_writer(appender)
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .try_init();
    }
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == PURGE_ARG) {
        let store = secrets::store(&std::env::temp_dir());
        let _ = nc_security::Identity::destroy(&*store);
        return;
    }
    let start_hidden = args.iter().any(|a| a == HIDDEN_ARG);
    let files_from_cli: Vec<PathBuf> = args.iter().skip(1).filter(|a| !a.starts_with("--")).map(PathBuf::from).filter(|p| p.exists()).collect();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // Deuxième lancement (ou « Envoyer vers ») : réutilise l'instance existante.
            let files: Vec<String> = argv.iter().skip(1).filter(|a| !a.starts_with("--")).filter(|p| std::path::Path::new(p).exists()).cloned().collect();
            tray::show_main(app);
            if !files.is_empty() {
                use tauri::Emitter;
                let _ = app.emit("nc-send-request", files);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![HIDDEN_ARG])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            init_logs(&data_dir);
            let download_dir = dirs::download_dir().unwrap_or_else(|| data_dir.clone()).join("NOTHING CONNECT");
            let secrets = secrets::store(&data_dir);
            let settings = Arc::new(RwLock::new(Settings::default()));
            let snapshot = Arc::new(Mutex::new(sink::Snapshot::default()));
            let sink = Arc::new(sink::TauriSink::new(app.handle().clone(), settings.clone(), snapshot.clone()));
            let cfg = NodeConfig {
                role: Role::Pc,
                data_dir: data_dir.clone(),
                download_dir,
                device: device_info(),
                listen_port: nc_core::DEFAULT_PORT,
                advertise: true,
            };
            let node = Node::new(cfg, &*secrets, sink).map_err(|e| e.to_string())?;
            *settings.write().unwrap() = node.settings();
            node.start().map_err(|e| e.to_string())?;
            tracing::info!("NOTHING CONNECT {} démarré", env!("CARGO_PKG_VERSION"));
            app.manage(AppState { node: Mutex::new(node), settings, data_dir, secrets, snapshot });
            tray::create(app.handle())?;
            if !start_hidden {
                tray::show_main(app.handle());
            }
            if !files_from_cli.is_empty() {
                use tauri::Emitter;
                let files: Vec<String> = files_from_cli.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                let h = app.handle().clone();
                // Laisse l'interface s'abonner avant d'envoyer la demande.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let _ = h.emit("nc-send-request", files);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Fermer la fenêtre ne quitte pas : l'application reste dans la zone de notification.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                    tray::first_hide_hint(window.app_handle());
                }
            }
        })
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!())
        .expect("initialisation de l'application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // « Quitter complètement » : arrêt propre des connexions et transferts.
                if let Some(state) = app.try_state::<AppState>() {
                    state.node.lock().unwrap().stop();
                }
            }
        });
}
