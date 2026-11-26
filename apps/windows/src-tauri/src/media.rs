//! Flux temps réel côté Windows : miroir de l'écran du téléphone (vidéo décodée dans
//! l'interface par WebCodecs), son du téléphone (lecture), son du PC (capture → téléphone).

use crate::audio::{Capture, Player};
use crate::AppState;
use nc_core::protocol::{Message, StreamKind, StreamParams};
use nc_core::StreamPacket;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct Media {
    /// Canal binaire vers l'interface pour les images du miroir.
    pub mirror: Mutex<Option<Channel<InvokeResponseBody>>>,
    pub player: Mutex<Option<Arc<Player>>>,
    pub capture: Mutex<Option<(String, Capture)>>,
    started: Mutex<Option<Instant>>,
    pub display: crate::display::Display,
}

impl Media {
    /// Appelé sur le fil réseau : copie et rend la main immédiatement.
    pub fn on_stream(&self, _peer: &str, p: StreamPacket) {
        match p.kind {
            StreamKind::Mirror => {
                if let Some(ch) = self.mirror.lock().unwrap().as_ref() {
                    // En-tête : drapeaux (1) + horodatage µs (8, little-endian) + données.
                    let mut buf = Vec::with_capacity(9 + p.data.len());
                    buf.push(p.flags);
                    buf.extend_from_slice(&p.ts_us.to_le_bytes());
                    buf.extend_from_slice(&p.data);
                    let _ = ch.send(InvokeResponseBody::Raw(buf));
                }
            }
            StreamKind::PhoneAudio => {
                if let Some(pl) = self.player.lock().unwrap().as_ref() {
                    pl.push(&p.data);
                }
            }
            // Flux émis par le PC : rien à recevoir.
            StreamKind::PcAudio | StreamKind::Display => {}
        }
    }

    /// Réagit aux annonces du téléphone (format des flux, arrêt).
    pub fn on_message(&self, app: &AppHandle, peer: &str, m: &Message) {
        match m {
            Message::StreamStart {
                kind: StreamKind::PhoneAudio,
                ..
            } => {
                if self.player.lock().unwrap().is_none() {
                    match Player::start() {
                        Ok(p) => *self.player.lock().unwrap() = Some(p),
                        Err(e) => tracing::warn!("lecture du son du téléphone impossible : {e}"),
                    }
                }
            }
            Message::StreamStop {
                kind: StreamKind::PhoneAudio,
                ..
            } => {
                self.player.lock().unwrap().take();
            }
            Message::StreamStop {
                kind: StreamKind::Mirror,
                ..
            } => {
                self.player.lock().unwrap().take();
                self.set_streaming(app, false);
            }
            Message::StreamRequest {
                kind: StreamKind::PcAudio,
                ..
            } => {
                if let Err(e) = self.start_pc_audio(app, peer) {
                    let _ = app.state::<AppState>().node.lock().unwrap().send(
                        peer,
                        Message::StreamStop {
                            kind: StreamKind::PcAudio,
                            reason: Some(e),
                        },
                    );
                }
            }
            Message::StreamStop {
                kind: StreamKind::PcAudio,
                ..
            } => {
                self.capture.lock().unwrap().take();
            }
            // Écran étendu : le téléphone annonce la taille de son écran.
            Message::StreamStart {
                kind: StreamKind::Display,
                params,
            } => {
                let (w, h) = (params.width.unwrap_or(1920), params.height.unwrap_or(1080));
                if let Err(e) =
                    self.display
                        .on_phone_ready(app, peer, w, h, params.refresh_hz.unwrap_or(60))
                {
                    tracing::warn!("écran étendu : {e}");
                    let _ = app.state::<AppState>().node.lock().unwrap().send(
                        peer,
                        Message::StreamStop {
                            kind: StreamKind::Display,
                            reason: Some(e.clone()),
                        },
                    );
                    use tauri::Emitter;
                    let _ = app.emit("nc-display-error", e);
                }
            }
            Message::StreamStop {
                kind: StreamKind::Display,
                ..
            } => self.display.stop(app),
            Message::StreamKeyframe {
                kind: StreamKind::Display,
            } => self.display.request_frame(),
            _ => {}
        }
    }

    pub fn set_streaming(&self, app: &AppHandle, on: bool) {
        app.state::<AppState>()
            .node
            .lock()
            .unwrap()
            .set_streaming(on);
        *self.started.lock().unwrap() = on.then(Instant::now);
    }

    /// Capture le son du PC et l'envoie au téléphone (PCM s16le 48 kHz stéréo, blocs de 10 ms).
    pub fn start_pc_audio(&self, app: &AppHandle, peer: &str) -> Result<(), String> {
        self.capture.lock().unwrap().take();
        let params = StreamParams {
            codec: Some("pcm_s16le".into()),
            sample_rate: Some(crate::audio::RATE),
            channels: Some(crate::audio::CHANNELS as u16),
            ..Default::default()
        };
        app.state::<AppState>()
            .node
            .lock()
            .unwrap()
            .send(
                peer,
                Message::StreamStart {
                    kind: StreamKind::PcAudio,
                    params,
                },
            )
            .map_err(|e| e.to_string())?;
        let h = app.clone();
        let p = peer.to_string();
        let t0 = Instant::now();
        let cap = Capture::start(move |pcm| {
            let ts = t0.elapsed().as_micros() as u64;
            let st = h.state::<AppState>();
            let node = st.node.lock().unwrap();
            node.send_stream(&p, StreamKind::PcAudio, 0, ts, &pcm);
        })?;
        *self.capture.lock().unwrap() = Some((peer.to_string(), cap));
        Ok(())
    }

    pub fn stop_pc_audio(&self, app: &AppHandle) {
        if let Some((peer, _cap)) = self.capture.lock().unwrap().take() {
            let _ = app.state::<AppState>().node.lock().unwrap().send(
                &peer,
                Message::StreamStop {
                    kind: StreamKind::PcAudio,
                    reason: None,
                },
            );
        }
    }
}
