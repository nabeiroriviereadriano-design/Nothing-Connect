//! Messages de contrôle (JSON, étiquetés par `"t"`).
//!
//! Règle de compatibilité : on peut **ajouter** des champs optionnels (`#[serde(default)]`)
//! sans changer `PROTOCOL_VERSION`. Supprimer/renommer un champ ou changer un type exige
//! d'incrémenter la version.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Android,
}

/// Identité publique d'un appareil (échangée après le handshake).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub platform: Platform,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Version Android (ex. "14") ou Windows (ex. "10.0.22631").
    #[serde(default)]
    pub os_version: Option<String>,
    pub app_version: String,
    /// Fonctions réellement disponibles sur cet appareil (voir [`Capability`]).
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

/// Capacités annoncées. L'interface n'affiche une fonction que si le pair l'annonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    FileTransfer,
    ShareText,
    DeviceStatus,
    Notifications,
    NotificationActions,
    MediaControl,
    Ring,
    Vibrate,
    MediaVolume,
    DoNotDisturb,
    /// Synchronisation du presse-papiers (texte).
    Clipboard,
    /// Le téléphone peut diffuser son écran (MediaProjection).
    ScreenMirror,
    /// Le téléphone accepte le contrôle à distance (service d'accessibilité activé).
    MirrorControl,
    /// Le téléphone peut diffuser son audio (AudioPlaybackCapture).
    PhoneAudio,
    /// L'appareil peut lire un flux audio reçu.
    AudioPlayback,
    /// Le PC peut créer un écran étendu (pilote installé) / le téléphone peut l'afficher.
    ExtendedDisplay,
    #[serde(other)]
    Unknown,
}

/// Flux temps réel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum StreamKind {
    /// Écran du téléphone vers le PC (H.264).
    Mirror = 1,
    /// Son du téléphone vers le PC (PCM).
    PhoneAudio = 2,
    /// Son du PC vers le téléphone (PCM).
    PcAudio = 3,
    /// Écran étendu Windows vers le téléphone.
    Display = 4,
}

impl StreamKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Mirror,
            2 => Self::PhoneAudio,
            3 => Self::PcAudio,
            4 => Self::Display,
            _ => return None,
        })
    }
}

/// Paramètres d'un flux. Champs absents = choix automatique de l'émetteur.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamParams {
    /// "h264", "jpeg", "pcm_s16le".
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Plus grand côté souhaité (mise à l'échelle par l'émetteur).
    pub max_size: Option<u32>,
    pub fps: Option<u32>,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    /// Pour le miroir : inclure aussi le son.
    pub with_audio: Option<bool>,
    /// Pour l'écran étendu : densité (dpi) et fréquence proposées par le téléphone.
    pub dpi: Option<u32>,
    pub refresh_hz: Option<u32>,
    /// Pour l'écran étendu : orientation demandée au téléphone.
    pub portrait: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhoneKey {
    Back,
    Home,
    Recents,
    Notifications,
}

/// Commande de contrôle à distance du téléphone (coordonnées normalisées 0..1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "i", rename_all = "snake_case")]
pub enum InputEvent {
    Tap {
        x: f32,
        y: f32,
    },
    LongPress {
        x: f32,
        y: f32,
    },
    Swipe {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        duration_ms: u32,
    },
    Key {
        key: PhoneKey,
    },
    Text {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerAction {
    Down,
    Move,
    Up,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DeviceStatus {
    pub battery_percent: Option<u8>,
    pub charging: Option<bool>,
    pub battery_temp_c: Option<f32>,
    pub storage_free_bytes: Option<u64>,
    pub storage_total_bytes: Option<u64>,
    pub wifi_connected: Option<bool>,
    pub wifi_ssid: Option<String>,
    pub bluetooth_enabled: Option<bool>,
    pub mobile_network: Option<String>,
    pub media_volume_percent: Option<u8>,
    pub dnd_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "c", rename_all = "snake_case")]
pub enum RemoteCommand {
    Ring,
    StopRing,
    Vibrate { millis: u32 },
    SetMediaVolume { percent: u8 },
    SetDoNotDisturb { enabled: bool },
    RefreshStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationActionInfo {
    pub index: u32,
    pub title: String,
    /// Vrai si l'action accepte une réponse texte (RemoteInput).
    pub accepts_text: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneNotification {
    /// Clé Android (`StatusBarNotification.key`), stable pour mises à jour/suppression.
    pub key: String,
    pub package: String,
    pub app_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// Millisecondes Unix.
    pub posted_at: i64,
    /// Icône PNG en base64 (petite, ≤ 8 Ko) envoyée seulement si non connue du PC.
    #[serde(default)]
    pub icon_png_b64: Option<String>,
    #[serde(default)]
    pub actions: Vec<NotificationActionInfo>,
    /// Le téléphone signale un contenu marqué privé (VISIBILITY_PRIVATE/SECRET).
    #[serde(default)]
    pub sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppEntry {
    pub package: String,
    pub app_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    Buffering,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaSessionInfo {
    pub id: String,
    pub package: String,
    pub app_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub position_ms: Option<u64>,
    pub state: PlaybackState,
    #[serde(default)]
    pub artwork_jpeg_b64: Option<String>,
    #[serde(default)]
    pub can_skip_next: bool,
    #[serde(default)]
    pub can_skip_prev: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaAction {
    Play,
    Pause,
    TogglePlayPause,
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferedFile {
    /// Chemin relatif (séparateur `/`) — conserve l'arborescence d'un dossier envoyé.
    pub rel_path: String,
    pub size: u64,
    /// Date de modification (ms Unix) pour information.
    #[serde(default)]
    pub modified_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    Text,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    UserDeclined,
    NotTrusted,
    InsufficientSpace,
    Busy,
    Invalid,
}

/// Messages de contrôle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Message {
    /// Premier message de chaque session, dans les deux sens.
    Hello {
        protocol: u16,
        device: DeviceInfo,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// Fin propre de session.
    Bye,
    /// L'expéditeur a révoqué l'association : le destinataire supprime aussi l'appareil.
    Unpaired,

    // --- association (dans le canal Noise XXpsk3) ---
    /// Le PC confirme l'association après validation par l'utilisateur.
    PairingAccepted,
    PairingRejected,

    // --- état et commandes ---
    Status(DeviceStatus),
    Command {
        id: u64,
        command: RemoteCommand,
    },
    CommandResult {
        id: u64,
        ok: bool,
        #[serde(default)]
        error: Option<String>,
    },

    // --- notifications ---
    NotificationPosted(PhoneNotification),
    NotificationRemoved {
        key: String,
    },
    /// PC → téléphone : déclencher une action (réponse rapide si `reply` présent).
    NotificationAction {
        key: String,
        action_index: u32,
        #[serde(default)]
        reply: Option<String>,
    },
    /// PC → téléphone : ignorer / supprimer la notification sur le téléphone.
    NotificationDismiss {
        key: String,
    },
    /// PC → téléphone : ouvrir l'application associée sur le téléphone.
    NotificationOpen {
        key: String,
    },
    /// Téléphone → PC : liste des applis vues par le service de notification.
    NotificationApps {
        apps: Vec<AppEntry>,
    },
    /// PC → téléphone : filtre par application, appliqué **sur le téléphone** avant envoi.
    NotificationFilter {
        package: String,
        enabled: bool,
    },

    // --- multimédia ---
    MediaSessions {
        sessions: Vec<MediaSessionInfo>,
    },
    MediaCommand {
        session_id: String,
        action: MediaAction,
    },

    // --- partage rapide ---
    Share {
        kind: ShareKind,
        content: String,
    },

    // --- transferts ---
    TransferOffer {
        id: Uuid,
        files: Vec<OfferedFile>,
    },
    /// Le récepteur accepte et indique, par fichier, l'offset déjà reçu (reprise).
    TransferAccept {
        id: Uuid,
        offsets: Vec<u64>,
    },
    TransferReject {
        id: Uuid,
        reason: RejectReason,
    },
    /// Fin d'un fichier côté émetteur, avec l'empreinte SHA-256 (hex) du fichier entier.
    TransferFileEnd {
        id: Uuid,
        file_index: u32,
        sha256: String,
    },
    /// Le récepteur confirme l'intégrité d'un fichier.
    TransferFileVerified {
        id: Uuid,
        file_index: u32,
        ok: bool,
    },
    TransferPause {
        id: Uuid,
    },
    /// Le récepteur relance l'envoi à partir des offsets indiqués.
    TransferResume {
        id: Uuid,
        offsets: Vec<u64>,
    },
    TransferCancel {
        id: Uuid,
    },

    // --- presse-papiers ---
    Clipboard {
        content: String,
    },

    // --- flux temps réel ---
    /// Demande au pair de démarrer un flux (ex. le PC demande l'écran du téléphone).
    StreamRequest {
        kind: StreamKind,
        #[serde(default)]
        params: StreamParams,
    },
    /// L'émetteur annonce le format réel du flux.
    StreamStart {
        kind: StreamKind,
        params: StreamParams,
    },
    StreamStop {
        kind: StreamKind,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Demande d'image clé (après perte ou au démarrage du décodeur).
    StreamKeyframe {
        kind: StreamKind,
    },
    /// PC → téléphone : contrôle du miroir.
    Input(InputEvent),
    /// Téléphone → PC : toucher sur l'écran étendu (coordonnées normalisées 0..1).
    DisplayPointer {
        action: PointerAction,
        pointer: u32,
        x: f32,
        y: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_capability_is_tolerated() {
        let c: Vec<Capability> = serde_json::from_str(r#"["ring","future_thing"]"#).unwrap();
        assert_eq!(c, vec![Capability::Ring, Capability::Unknown]);
    }

    #[test]
    fn tagged_json_shape() {
        let m = Message::Command {
            id: 1,
            command: RemoteCommand::Vibrate { millis: 300 },
        };
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(
            s,
            r#"{"t":"command","id":1,"command":{"c":"vibrate","millis":300}}"#
        );
    }

    #[test]
    fn stream_messages_shape() {
        let m = Message::StreamRequest {
            kind: StreamKind::Mirror,
            params: StreamParams {
                max_size: Some(1280),
                ..Default::default()
            },
        };
        let s = serde_json::to_string(&m).unwrap();
        assert!(s.starts_with(r#"{"t":"stream_request","kind":"mirror""#));
        assert_eq!(serde_json::from_str::<Message>(&s).unwrap(), m);
        let i = Message::Input(InputEvent::Tap { x: 0.5, y: 0.25 });
        assert_eq!(
            serde_json::to_string(&i).unwrap(),
            r#"{"t":"input","i":"tap","x":0.5,"y":0.25}"#
        );
    }

    #[test]
    fn optional_fields_default() {
        let m: Message =
            serde_json::from_str(r#"{"t":"command_result","id":3,"ok":true}"#).unwrap();
        assert_eq!(
            m,
            Message::CommandResult {
                id: 3,
                ok: true,
                error: None
            }
        );
    }
}
