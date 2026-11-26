//! Synchronisation du presse-papiers (texte), activée explicitement dans les Paramètres.
//!
//! * PC → téléphone : surveillance du numéro de séquence du presse-papiers Windows (aucune
//!   lecture tant qu'il ne change pas), envoi du texte aux téléphones connectés.
//! * Téléphone → PC : le texte reçu est placé dans le presse-papiers ; son empreinte est
//!   mémorisée pour ne pas le renvoyer (pas de boucle).

use crate::AppState;
use nc_core::protocol::Message;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const MAX_LEN: usize = 1 << 20;
static LAST: AtomicU64 = AtomicU64::new(0);

fn digest(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[cfg(windows)]
fn sequence() -> u32 {
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn sequence() -> u32 {
    // Hors Windows (développement) : on compare simplement le contenu.
    0
}

/// Place un texte reçu du téléphone dans le presse-papiers.
pub fn apply_remote(content: &str) {
    if content.len() > MAX_LEN {
        return;
    }
    LAST.store(digest(content), Ordering::SeqCst);
    match arboard::Clipboard::new().and_then(|mut c| c.set_text(content.to_owned())) {
        Ok(()) => {}
        Err(e) => tracing::warn!("presse-papiers inaccessible : {e}"),
    }
}

pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("nc-clipboard".into())
        .spawn(move || {
            let mut last_seq = sequence();
            let mut cb = arboard::Clipboard::new().ok();
            loop {
                std::thread::sleep(Duration::from_millis(400));
                let state = app.state::<AppState>();
                if !state.settings.read().unwrap().clipboard_sync {
                    continue;
                }
                let seq = sequence();
                if cfg!(windows) && seq == last_seq {
                    continue;
                }
                last_seq = seq;
                if cb.is_none() {
                    cb = arboard::Clipboard::new().ok();
                }
                let Some(text) = cb.as_mut().and_then(|c| c.get_text().ok()) else {
                    continue;
                };
                if text.is_empty() || text.len() > MAX_LEN {
                    continue;
                }
                let h = digest(&text);
                if LAST.swap(h, Ordering::SeqCst) == h {
                    continue; // inchangé, ou texte qui vient du téléphone
                }
                let n = state
                    .node
                    .lock()
                    .unwrap()
                    .broadcast(Message::Clipboard { content: text });
                tracing::debug!("presse-papiers envoyé à {n} appareil(s)");
            }
        })
        .expect("thread presse-papiers");
}
