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
        }
    }
}
