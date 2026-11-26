use nc_transfer::ConflictPolicy;
use serde::{Deserialize, Serialize};

/// Préférences persistées (clé `settings` en base). Les champs ajoutés plus tard doivent
/// avoir une valeur par défaut : une mise à jour ne perd jamais les réglages existants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Dossier de réception (vide = dossier par défaut de la plateforme).
    pub download_dir: Option<String>,
    pub conflict_policy: ConflictPolicy,
    pub notifications_enabled: bool,
    pub notification_popups: bool,
    pub popup_duration_s: u32,
    /// Masquer titre et texte des notifications marquées sensibles.
    pub hide_sensitive_content: bool,
    /// Conserver le contenu des notifications dans l'historique local.
    pub store_notification_content: bool,
    pub notification_history_limit: u32,
    /// Suspend toutes les connexions (bouton « pause » global).
    pub suspended: bool,
    /// Synchronisation automatique du texte copié (désactivée par défaut : activation explicite).
    pub clipboard_sync: bool,
    /// Liaison USB via adb (débogage USB activé sur le téléphone).
    pub usb_adb: bool,
    /// Annonce Bluetooth Low Energy pour aider le téléphone à trouver le PC.
    pub ble_advertise: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: None,
            conflict_policy: ConflictPolicy::KeepBoth,
            notifications_enabled: true,
            notification_popups: true,
            popup_duration_s: 5,
            hide_sensitive_content: true,
            store_notification_content: true,
            notification_history_limit: 500,
            suspended: false,
            clipboard_sync: false,
            usb_adb: true,
            ble_advertise: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_json_gets_new_defaults() {
        // Réglages enregistrés par la 0.1.0 (sans les nouveaux champs).
        let s: Settings =
            serde_json::from_str(r#"{"notifications_enabled":false,"suspended":true}"#).unwrap();
        assert!(!s.notifications_enabled && s.suspended);
        assert!(!s.clipboard_sync && s.usb_adb && s.ble_advertise);
    }
}
