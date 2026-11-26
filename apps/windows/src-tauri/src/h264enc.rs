#![cfg_attr(not(windows), allow(dead_code))]
//! Encodage H.264 de l'écran étendu (OpenH264, logiciel). Par rapport au JPEG image par image,
//! seules les différences entre images sont transmises : le débit chute d'un facteur 10 à 100
//! sur un bureau, et le téléphone décode avec son circuit vidéo matériel.
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, RateControlMode,
    UsageType,
};
use openh264::formats::{BgraSliceU8, YUVBuffer};
use openh264::OpenH264API;

/// Drapeaux des fragments (identiques au protocole).
pub const CONFIG: u8 = 1;
pub const KEYFRAME: u8 = 2;

/// Débit visé selon la définition, la fréquence et le réglage « qualité » (30 à 95).
pub fn bitrate_for(w: u32, h: u32, fps: u32, quality: u8) -> u32 {
    let q = (quality.clamp(30, 95) as f64 - 30.0) / 65.0;
    // 0,03 à 0,16 bit par pixel et par image : texte net dès le milieu de l'échelle.
    let bpp = 0.03 + q * 0.13;
    let bps = w as f64 * h as f64 * fps.max(1) as f64 * bpp;
    bps.clamp(2_000_000.0, 60_000_000.0) as u32
}

pub struct H264 {
    enc: Encoder,
    yuv: YUVBuffer,
    w: u32,
    h: u32,
    out: Vec<u8>,
    crop: Vec<u8>,
}

impl H264 {
    pub fn new(w: u32, h: u32, fps: u32, bitrate: u32) -> Result<Self, String> {
        if !w.is_multiple_of(2) || !h.is_multiple_of(2) || w < 16 || h < 16 {
            return Err(format!("définition non encodable : {w}×{h}"));
        }
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 4) as u16;
        let cfg = EncoderConfig::new()
            // Mode « vidéo temps réel » : deux fois plus rapide que le mode « écran » ici,
            // pour une qualité équivalente à débit égal.
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            .bitrate(BitRate::from_bps(bitrate))
            .max_frame_rate(FrameRate::from_hz(fps.clamp(5, 240) as f32))
            // Jamais d'image sautée : le téléphone doit voir chaque changement.
            .skip_frames(false)
            .complexity(Complexity::Low)
            // Images clés à la demande du téléphone ; une toutes les 10 s par sécurité.
            .intra_frame_period(IntraFramePeriod::from_num_frames(fps.clamp(5, 240) * 10))
            .num_threads(threads)
            // Plusieurs tranches par image : encodage réparti sur plusieurs cœurs.
            .max_slice_len(if threads > 1 { 6000 } else { 0 });
        let enc = Encoder::with_api_config(OpenH264API::from_source(), cfg)
            .map_err(|e| format!("encodeur H.264 : {e}"))?;
        Ok(Self {
            enc,
            yuv: YUVBuffer::new(w as usize, h as usize),
            w,
            h,
            out: Vec::with_capacity(256 * 1024),
            crop: Vec::new(),
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    pub fn force_keyframe(&mut self) {
        self.enc.force_intra_frame();
    }

    /// Taille encodable (paire) pour une image source.
    pub fn even(w: u32, h: u32) -> (u32, u32) {
        (w & !1, h & !1)
    }

    /// Encode une image BGRA de largeur `src_w` (recadrée si besoin à la taille de l'encodeur) ;
    /// renvoie les paquets à envoyer : configuration SPS/PPS séparée, puis l'image avec le
    /// drapeau « image clé » si elle en est une.
    pub fn encode_bgra(&mut self, bgra: &[u8], src_w: u32) -> Result<Vec<(u8, Vec<u8>)>, String> {
        let (w, h, sw) = (self.w as usize, self.h as usize, src_w as usize);
        if sw < w || bgra.len() < sw * h * 4 {
            return Err("image incomplète".into());
        }
        let src: &[u8] = if sw == w {
            &bgra[..w * h * 4]
        } else {
            self.crop.resize(w * h * 4, 0);
            for y in 0..h {
                self.crop[y * w * 4..(y + 1) * w * 4]
                    .copy_from_slice(&bgra[y * sw * 4..y * sw * 4 + w * 4]);
            }
            &self.crop
        };
        self.yuv.read_bgra8(BgraSliceU8::new(src, (w, h)));
        self.out.clear();
        self.enc
            .encode(&self.yuv)
            .map_err(|e| format!("encodage H.264 : {e}"))?
            .write_vec(&mut self.out);
        Ok(split_annexb(&self.out))
    }
}

/// Positions (début du code de départ, type NAL) dans un flux Annex B.
fn nal_starts(b: &[u8]) -> Vec<(usize, u8)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i + 3 < b.len() {
        if b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1 {
            let start = if i > 0 && b[i - 1] == 0 { i - 1 } else { i };
            v.push((start, b[i + 3] & 0x1f));
            i += 3;
        } else {
            i += 1;
        }
    }
    v
}

/// Sépare SPS/PPS (configuration du décodeur) du reste de l'image.
pub fn split_annexb(b: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let nals = nal_starts(b);
    let mut config = Vec::new();
    let mut frame = Vec::new();
    let mut key = false;
    for (k, &(start, ty)) in nals.iter().enumerate() {
        let end = nals.get(k + 1).map(|n| n.0).unwrap_or(b.len());
        let unit = &b[start..end];
        match ty {
            7 | 8 => config.extend_from_slice(unit),
            // Délimiteurs, SEI : inutiles au téléphone.
            6 | 9 => {}
            t => {
                if t == 5 {
                    key = true;
                }
                frame.extend_from_slice(unit);
            }
        }
    }
    let mut out = Vec::new();
    if !config.is_empty() {
        out.push((CONFIG, config));
    }
    if !frame.is_empty() {
        out.push((if key { KEYFRAME } else { 0 }, frame));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::decoder::Decoder;

    fn frame(w: usize, h: usize, k: usize) -> Vec<u8> {
        let mut v = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let on = ((x + k * 4) / 16 + y / 16).is_multiple_of(2);
                v[i] = if on { 230 } else { 20 };
                v[i + 1] = (x % 256) as u8;
                v[i + 2] = (y % 256) as u8;
                v[i + 3] = 255;
            }
        }
        v
    }

    #[test]
    fn bitrate_scales_with_quality_and_size() {
        assert!(bitrate_for(2400, 1080, 60, 90) > bitrate_for(2400, 1080, 60, 50));
        assert!(bitrate_for(2400, 1080, 60, 70) > bitrate_for(1200, 540, 60, 70));
        assert_eq!(bitrate_for(100, 100, 1, 30), 2_000_000);
    }

    #[test]
    fn first_frame_is_config_then_keyframe_and_decodes() {
        let (w, h) = (320, 192);
        let mut enc = H264::new(w, h, 60, 4_000_000).unwrap();
        let mut dec = Decoder::new().unwrap();
        let p = enc
            .encode_bgra(&frame(w as usize, h as usize, 0), w)
            .unwrap();
        assert_eq!(p[0].0, CONFIG);
        assert_eq!(p[1].0, KEYFRAME);
        let mut decoded = 0;
        // Le téléphone reçoit la configuration puis l'image clé, puis des images P.
        let mut stream = p[0].1.clone();
        stream.extend_from_slice(&p[1].1);
        if dec.decode(&stream).unwrap().is_some() {
            decoded += 1;
        }
        for k in 1..10 {
            for (flags, data) in enc
                .encode_bgra(&frame(w as usize, h as usize, k), w)
                .unwrap()
            {
                assert_eq!(flags, 0, "images suivantes : ni config ni image clé");
                if dec.decode(&data).unwrap().is_some() {
                    decoded += 1;
                }
            }
        }
        assert!(decoded >= 9, "images décodées : {decoded}");
        // Image clé à la demande.
        enc.force_keyframe();
        let p = enc
            .encode_bgra(&frame(w as usize, h as usize, 3), w)
            .unwrap();
        assert!(p.iter().any(|(f, _)| *f == KEYFRAME));
    }

    #[test]
    fn odd_source_is_cropped() {
        let (sw, sh) = (321u32, 193u32);
        let (w, h) = H264::even(sw, sh);
        let mut enc = H264::new(w, h, 30, 2_000_000).unwrap();
        let p = enc
            .encode_bgra(&frame(sw as usize, sh as usize, 0), sw)
            .unwrap();
        assert!(p.iter().any(|(f, _)| *f == KEYFRAME));
    }

    #[test]
    fn static_screen_costs_almost_nothing() {
        let (w, h) = (640, 360);
        let mut enc = H264::new(w, h, 60, 8_000_000).unwrap();
        let f = frame(w as usize, h as usize, 0);
        let first: usize = enc
            .encode_bgra(&f, w)
            .unwrap()
            .iter()
            .map(|p| p.1.len())
            .sum();
        let mut later = 0;
        for _ in 0..10 {
            later += enc
                .encode_bgra(&f, w)
                .unwrap()
                .iter()
                .map(|p| p.1.len())
                .sum::<usize>();
        }
        assert!(
            later / 10 < first / 20,
            "image fixe : {} o contre {} o",
            later / 10,
            first
        );
    }
}
