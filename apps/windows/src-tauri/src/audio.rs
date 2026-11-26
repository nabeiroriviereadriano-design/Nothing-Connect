//! Audio temps réel.
//!
//! * [`Player`] : lit le son du téléphone (PCM s16le 48 kHz stéréo) sur la sortie Windows par
//!   défaut, avec un tampon de gigue court (≈ 60 ms) et une latence plafonnée (≈ 250 ms).
//! * [`Capture`] : capture le son du PC (boucle de sortie WASAPI) et le convertit en PCM
//!   s16le 48 kHz stéréo, par blocs de 10 ms, pour l'envoyer au téléphone.
//!
//! Les flux cpal ne sont pas `Send` sur toutes les plateformes : chacun vit dans son thread.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub const RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// 10 ms à 48 kHz.
pub const FRAMES_PER_PACKET: usize = 480;

/// Rééchantillonneur linéaire (suffisant pour la voix et la musique à ces écarts de fréquence).
struct Resampler {
    ratio: f64,
    pos: f64,
    prev: [f32; CHANNELS],
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        Self {
            ratio: from as f64 / to as f64,
            pos: 0.0,
            prev: [0.0; CHANNELS],
        }
    }

    /// Entrée/sortie entrelacées stéréo.
    fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames = input.len() / CHANNELS;
        if (self.ratio - 1.0).abs() < 1e-9 {
            out.extend_from_slice(&input[..frames * CHANNELS]);
            return;
        }
        let get = |i: isize, c: usize, prev: &[f32; CHANNELS]| -> f32 {
            if i < 0 {
                prev[c]
            } else {
                input[i as usize * CHANNELS + c]
            }
        };
        while self.pos < frames as f64 - 1.0 + 1.0 {
            let i = self.pos.floor() as isize - 1;
            let t = (self.pos - self.pos.floor()) as f32;
            if i + 1 >= frames as isize {
                break;
            }
            for c in 0..CHANNELS {
                let a = get(i, c, &self.prev);
                let b = get(i + 1, c, &self.prev);
                out.push(a + (b - a) * t);
            }
            self.pos += self.ratio;
        }
        self.pos -= frames as f64;
        if frames > 0 {
            for c in 0..CHANNELS {
                self.prev[c] = input[(frames - 1) * CHANNELS + c];
            }
        }
    }
}

// ------------------------------------------------------------------ lecture

type Opened = (cpal::Stream, Arc<Mutex<Ring>>, u32, usize);

struct Ring {
    buf: VecDeque<f32>,
    started: bool,
    prebuffer: usize,
    max: usize,
}

pub struct Player {
    ring: Arc<Mutex<Ring>>,
    resampler: Mutex<Resampler>,
    device_channels: usize,
    stop: Arc<AtomicBool>,
}

impl Player {
    /// Ouvre la sortie audio par défaut. Erreur lisible si aucun périphérique n'est disponible.
    pub fn start() -> Result<Arc<Self>, String> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("nc-audio-out".into())
            .spawn(move || {
                let res = (|| -> Result<Opened, String> {
                    let host = cpal::default_host();
                    let dev = host.default_output_device().ok_or("aucune sortie audio")?;
                    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
                    let rate = cfg.sample_rate();
                    let ch = cfg.channels() as usize;
                    let per_ms = rate as usize * ch / 1000;
                    let ring = Arc::new(Mutex::new(Ring {
                        buf: VecDeque::with_capacity(per_ms * 500),
                        started: false,
                        prebuffer: per_ms * 60,
                        max: per_ms * 250,
                    }));
                    let stream = match cfg.sample_format() {
                        cpal::SampleFormat::F32 => {
                            build_out::<f32>(&dev, &cfg.config(), ring.clone())
                        }
                        cpal::SampleFormat::I16 => {
                            build_out::<i16>(&dev, &cfg.config(), ring.clone())
                        }
                        cpal::SampleFormat::U16 => {
                            build_out::<u16>(&dev, &cfg.config(), ring.clone())
                        }
                        cpal::SampleFormat::I32 => {
                            build_out::<i32>(&dev, &cfg.config(), ring.clone())
                        }
                        f => Err(format!("format audio non pris en charge : {f}")),
                    }?;
                    stream.play().map_err(|e| e.to_string())?;
                    Ok((stream, ring, rate, ch))
                })();
                match res {
                    Ok((stream, ring, rate, ch)) => {
                        let _ = ready_tx.send(Ok((ring, rate, ch)));
                        while !stop2.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let (ring, rate, ch) = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "sortie audio indisponible".to_string())??;
        Ok(Arc::new(Self {
            ring,
            resampler: Mutex::new(Resampler::new(RATE, rate)),
            device_channels: ch,
            stop,
        }))
    }

    /// Ajoute un paquet PCM s16le 48 kHz stéréo.
    pub fn push(&self, pcm: &[u8]) {
        let input: Vec<f32> = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        let mut stereo = Vec::with_capacity(input.len() * 2);
        self.resampler.lock().unwrap().process(&input, &mut stereo);
        let mut r = self.ring.lock().unwrap();
        for f in stereo.chunks_exact(CHANNELS) {
            for c in 0..self.device_channels {
                // Stéréo → n canaux : canaux supplémentaires = moyenne (centre, caissons).
                r.buf.push_back(if c < CHANNELS {
                    f[c]
                } else {
                    (f[0] + f[1]) * 0.5
                });
            }
        }
        // Latence plafonnée : on rattrape en jetant le plus ancien.
        let excess = r.buf.len().saturating_sub(r.max);
        r.buf.drain(..excess);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn build_out<T>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    ring: Arc<Mutex<Ring>>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    dev.build_output_stream(
        *cfg,
        move |out: &mut [T], _| {
            let mut r = ring.lock().unwrap();
            if !r.started && r.buf.len() >= r.prebuffer {
                r.started = true;
            }
            for s in out.iter_mut() {
                let v = if r.started { r.buf.pop_front() } else { None };
                *s = T::from_sample(v.unwrap_or(0.0));
            }
            if r.buf.is_empty() {
                r.started = false; // sous-alimentation : on reconstitue le tampon de gigue
            }
        },
        |e| tracing::warn!("sortie audio : {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ capture

pub struct Capture {
    stop: Arc<AtomicBool>,
}

impl Capture {
    /// Capture le son joué par Windows (boucle de la sortie par défaut).
    pub fn start(mut on_packet: impl FnMut(Vec<u8>) + Send + 'static) -> Result<Self, String> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("nc-audio-in".into())
            .spawn(move || {
                let res = (|| -> Result<cpal::Stream, String> {
                    let host = cpal::default_host();
                    let dev = host
                        .default_output_device()
                        .ok_or("aucune sortie audio à capturer")?;
                    let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
                    let ch = cfg.channels() as usize;
                    let mut res = Resampler::new(cfg.sample_rate(), RATE);
                    let mut pending: Vec<f32> =
                        Vec::with_capacity(FRAMES_PER_PACKET * CHANNELS * 4);
                    let mut handle = move |data: &[f32]| {
                        // n canaux → stéréo
                        let stereo: Vec<f32> = data
                            .chunks_exact(ch)
                            .flat_map(|f| if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] })
                            .collect();
                        res.process(&stereo, &mut pending);
                        let n = FRAMES_PER_PACKET * CHANNELS;
                        while pending.len() >= n {
                            let pcm: Vec<u8> = pending
                                .drain(..n)
                                .flat_map(|s| i16::from_sample(s.clamp(-1.0, 1.0)).to_le_bytes())
                                .collect();
                            on_packet(pcm);
                        }
                    };
                    let config = cfg.config();
                    let err = |e| tracing::warn!("capture audio : {e}");
                    let stream = match cfg.sample_format() {
                        cpal::SampleFormat::F32 => {
                            dev.build_input_stream(config, move |d: &[f32], _| handle(d), err, None)
                        }
                        cpal::SampleFormat::I16 => dev.build_input_stream(
                            config,
                            move |d: &[i16], _| {
                                handle(&d.iter().map(|s| s.to_sample::<f32>()).collect::<Vec<_>>())
                            },
                            err,
                            None,
                        ),
                        cpal::SampleFormat::I32 => dev.build_input_stream(
                            config,
                            move |d: &[i32], _| {
                                handle(&d.iter().map(|s| s.to_sample::<f32>()).collect::<Vec<_>>())
                            },
                            err,
                            None,
                        ),
                        f => return Err(format!("format audio non pris en charge : {f}")),
                    }
                    .map_err(|e| e.to_string())?;
                    stream.play().map_err(|e| e.to_string())?;
                    Ok(stream)
                })();
                match res {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        while !stop2.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_millis(100));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "capture audio indisponible".to_string())??;
        Ok(Self { stop })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampler_keeps_duration() {
        let mut r = Resampler::new(44_100, 48_000);
        let input = vec![0.5f32; 441 * CHANNELS]; // 10 ms
        let mut out = Vec::new();
        for _ in 0..100 {
            r.process(&input, &mut out);
        }
        let frames = out.len() / CHANNELS;
        assert!(
            (frames as i64 - 48_000).abs() < 10,
            "{frames} trames pour 1 s"
        );
        // Après la première trame (interpolée depuis le silence), le signal constant est conservé.
        assert!(out[2 * CHANNELS..].iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resampler_identity() {
        let mut r = Resampler::new(48_000, 48_000);
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }
}
