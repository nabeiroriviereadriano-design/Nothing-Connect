//! Mesure de débit lissée (moyenne exponentielle) et estimation du temps restant.

use std::time::{Duration, Instant};

pub(crate) struct SpeedMeter {
    last_t: Instant,
    last_bytes: u64,
    bps: f64,
    last_emit: Option<Instant>,
}

impl SpeedMeter {
    pub fn new(bytes: u64) -> Self {
        Self { last_t: Instant::now(), last_bytes: bytes, bps: 0.0, last_emit: None }
    }

    pub fn reset(&mut self, bytes: u64) {
        *self = Self::new(bytes);
    }

    /// Met à jour la mesure ; renvoie `true` si un événement de progression doit être émis
    /// (au plus 4 par seconde pour ne pas saturer l'interface).
    pub fn update(&mut self, bytes: u64) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_t).as_secs_f64();
        if dt >= 0.25 {
            let inst = bytes.saturating_sub(self.last_bytes) as f64 / dt;
            self.bps = if self.bps == 0.0 { inst } else { 0.7 * self.bps + 0.3 * inst };
            self.last_t = now;
            self.last_bytes = bytes;
        }
        match self.last_emit {
            Some(t) if now.duration_since(t) < Duration::from_millis(250) => false,
            _ => {
                self.last_emit = Some(now);
                true
            }
        }
    }

    pub fn bps(&self) -> u64 {
        self.bps as u64
    }

    pub fn eta(&self, remaining: u64) -> Option<u64> {
        (self.bps > 1.0).then(|| (remaining as f64 / self.bps).ceil() as u64)
    }
}
