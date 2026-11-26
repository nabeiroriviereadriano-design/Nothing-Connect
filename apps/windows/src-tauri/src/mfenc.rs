//! Encodage H.264 par la carte graphique (NVIDIA NVENC, Intel Quick Sync, AMD AMF) via les
//! encodeurs matériels de Media Foundation. Plus rapide et quasiment sans charge processeur ;
//! en cas d'échec, l'appelant revient à l'encodeur logiciel (OpenH264).
#![cfg(windows)]

use crate::h264enc::{split_annexb, Nv12, CONFIG, KEYFRAME};
use std::mem::ManuallyDrop;
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::VARIANT;

pub struct MfH264 {
    t: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    codec: Option<ICodecAPI>,
    w: u32,
    h: u32,
    fps: u32,
    need_input: u32,
    provides_samples: bool,
    out_size: u32,
    nv12: Nv12,
    index: i64,
    sent_config: bool,
    pending: Vec<(u8, Vec<u8>)>,
    /// Nom de l'encodeur (ex. « NVIDIA H.264 Encoder MFT »).
    pub name: String,
}

// Utilisé depuis un seul fil (le fil d'encodage) ; COM y est initialisé en multi-thread.
unsafe impl Send for MfH264 {}

fn e(err: windows::core::Error) -> String {
    format!("Media Foundation : {err}")
}

fn set(codec: &ICodecAPI, key: &GUID, v: VARIANT) {
    unsafe {
        let _ = codec.SetValue(key, &v);
    }
}

impl MfH264 {
    pub fn new(w: u32, h: u32, fps: u32, bitrate: u32) -> Result<Self, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(e)?;
            let input = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_NV12,
            };
            let output = MFT_REGISTER_TYPE_INFO {
                guidMajorType: MFMediaType_Video,
                guidSubtype: MFVideoFormat_H264,
            };
            let mut arr: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut n = 0u32;
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&input),
                Some(&output),
                &mut arr,
                &mut n,
            )
            .map_err(e)?;
            let mut result = Err("aucun encodeur vidéo matériel".to_string());
            if !arr.is_null() {
                let list = std::slice::from_raw_parts_mut(arr, n as usize);
                for a in list.iter() {
                    if let Some(a) = a {
                        match Self::open(a, w, h, fps, bitrate) {
                            Ok(s) => {
                                result = Ok(s);
                                break;
                            }
                            Err(err) => result = Err(err),
                        }
                    }
                }
                for a in list.iter_mut() {
                    drop(a.take());
                }
                CoTaskMemFree(Some(arr as *const _));
            }
            result
        }
    }

    unsafe fn open(
        a: &IMFActivate,
        w: u32,
        h: u32,
        fps: u32,
        bitrate: u32,
    ) -> Result<Self, String> {
        let name = {
            let mut p = PWSTR::null();
            let mut len = 0u32;
            if a.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut p, &mut len)
                .is_ok()
                && !p.is_null()
            {
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len as usize));
                CoTaskMemFree(Some(p.0 as *const _));
                s
            } else {
                "Encodeur matériel".to_string()
            }
        };
        let t: IMFTransform = a.ActivateObject().map_err(e)?;
        let attrs = t.GetAttributes().map_err(e)?;
        let is_async = attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) == 1;
        if is_async {
            attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1).map_err(e)?;
        }
        let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);

        let size = ((w as u64) << 32) | h as u64;
        let rate = ((fps.max(1) as u64) << 32) | 1;
        // Format de sortie d'abord (exigé par les encodeurs), puis l'entrée NV12.
        let out = MFCreateMediaType().map_err(e)?;
        out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(e)?;
        out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
            .map_err(e)?;
        out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).map_err(e)?;
        out.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(e)?;
        out.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(e)?;
        out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
            .map_err(e)?;
        out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(e)?;
        out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)
            .map_err(e)?;
        t.SetOutputType(0, &out, 0).map_err(e)?;
        let inp = MFCreateMediaType().map_err(e)?;
        inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(e)?;
        inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
            .map_err(e)?;
        inp.SetUINT64(&MF_MT_FRAME_SIZE, size).map_err(e)?;
        inp.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(e)?;
        inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)
            .map_err(e)?;
        inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
            .map_err(e)?;
        inp.SetUINT32(&MF_MT_DEFAULT_STRIDE, w).map_err(e)?;
        t.SetInputType(0, &inp, 0).map_err(e)?;

        // Faible latence : pas d'images B, débit régulier, images clés à la demande.
        let codec = t.cast::<ICodecAPI>().ok();
        if let Some(c) = &codec {
            set(c, &CODECAPI_AVLowLatencyMode, VARIANT::from(true));
            set(
                c,
                &CODECAPI_AVEncCommonRateControlMode,
                VARIANT::from(eAVEncCommonRateControlMode_LowDelayVBR.0 as u32),
            );
            set(c, &CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(bitrate));
            set(
                c,
                &CODECAPI_AVEncMPVDefaultBPictureCount,
                VARIANT::from(0u32),
            );
            set(c, &CODECAPI_AVEncMPVGOPSize, VARIANT::from(fps.max(1) * 10));
        }

        let info = t.GetOutputStreamInfo(0).map_err(e)?;
        let provides = info.dwFlags
            & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0)
                as u32
            != 0;
        t.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0).map_err(e)?;
        t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .map_err(e)?;
        t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .map_err(e)?;
        let events = if is_async {
            Some(t.cast::<IMFMediaEventGenerator>().map_err(e)?)
        } else {
            None
        };
        tracing::info!("encodeur matériel : {name} ({w}×{h} à {fps} i/s)");
        Ok(Self {
            t,
            events,
            codec,
            w,
            h,
            fps: fps.max(1),
            need_input: 0,
            provides_samples: provides,
            out_size: info.cbSize.max(w * h),
            nv12: Nv12::new(w, h),
            index: 0,
            sent_config: false,
            pending: Vec::new(),
            name,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    pub fn force_keyframe(&mut self) {
        if let Some(c) = &self.codec {
            set(c, &CODECAPI_AVEncVideoForceKeyFrame, VARIANT::from(1u32));
        }
    }

    pub fn encode_bgra(&mut self, bgra: &[u8], src_w: u32) -> Result<Vec<(u8, Vec<u8>)>, String> {
        self.nv12.convert(bgra, src_w)?;
        unsafe {
            let len = self.nv12.data.len() as u32;
            let buf = MFCreateMemoryBuffer(len).map_err(e)?;
            let mut p: *mut u8 = std::ptr::null_mut();
            buf.Lock(&mut p, None, None).map_err(e)?;
            std::ptr::copy_nonoverlapping(self.nv12.data.as_ptr(), p, len as usize);
            buf.Unlock().map_err(e)?;
            buf.SetCurrentLength(len).map_err(e)?;
            let sample = MFCreateSample().map_err(e)?;
            sample.AddBuffer(&buf).map_err(e)?;
            let dur = 10_000_000i64 / self.fps as i64;
            sample.SetSampleTime(self.index * dur).map_err(e)?;
            sample.SetSampleDuration(dur).map_err(e)?;
            self.index += 1;

            if let Some(ev) = self.events.clone() {
                // Mode asynchrone : on attend que l'encodeur demande une image.
                while self.need_input == 0 {
                    self.handle_event(&ev, false)?;
                }
                self.t.ProcessInput(0, &sample, 0).map_err(e)?;
                self.need_input -= 1;
                // Puis on récupère l'image encodée dès qu'elle est prête.
                self.handle_event(&ev, true)?;
                loop {
                    match ev.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                        Ok(x) => self.dispatch(&x)?,
                        Err(err) if err.code() == MF_E_NO_EVENTS_AVAILABLE => break,
                        Err(err) => return Err(e(err)),
                    }
                }
            } else {
                self.t.ProcessInput(0, &sample, 0).map_err(e)?;
                while self.drain()? {}
            }
        }
        Ok(std::mem::take(&mut self.pending))
    }

    /// Attend un événement de l'encodeur (bloquant) et le traite.
    unsafe fn handle_event(
        &mut self,
        ev: &IMFMediaEventGenerator,
        _after_input: bool,
    ) -> Result<(), String> {
        let x = ev
            .GetEvent(MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS(0))
            .map_err(e)?;
        self.dispatch(&x)
    }

    unsafe fn dispatch(&mut self, x: &IMFMediaEvent) -> Result<(), String> {
        match x.GetType().map_err(e)? {
            t if t == METransformNeedInput.0 as u32 => self.need_input += 1,
            t if t == METransformHaveOutput.0 as u32 => {
                self.drain()?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Récupère une image encodée. `false` quand l'encodeur n'a plus rien.
    unsafe fn drain(&mut self) -> Result<bool, String> {
        let own = if self.provides_samples {
            None
        } else {
            let s = MFCreateSample().map_err(e)?;
            let b = MFCreateMemoryBuffer(self.out_size).map_err(e)?;
            s.AddBuffer(&b).map_err(e)?;
            Some(s)
        };
        let mut out = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(own),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        }];
        let mut status = 0u32;
        let r = self.t.ProcessOutput(0, &mut out, &mut status);
        let sample = ManuallyDrop::take(&mut out[0].pSample);
        drop(ManuallyDrop::take(&mut out[0].pEvents));
        match r {
            Ok(()) => {}
            Err(err) if err.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(false),
            Err(err) if err.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                // Nouveau format de sortie proposé par l'encodeur : on l'accepte.
                let ty = self.t.GetOutputAvailableType(0, 0).map_err(e)?;
                self.t.SetOutputType(0, &ty, 0).map_err(e)?;
                return Ok(true);
            }
            Err(err) => return Err(e(err)),
        }
        let Some(s) = sample else { return Ok(false) };
        let b = s.ConvertToContiguousBuffer().map_err(e)?;
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut n = 0u32;
        b.Lock(&mut p, None, Some(&mut n)).map_err(e)?;
        let bytes = std::slice::from_raw_parts(p, n as usize).to_vec();
        b.Unlock().map_err(e)?;
        let mut packets = split_annexb(&bytes);
        if packets.iter().any(|(f, _)| *f == CONFIG) {
            self.sent_config = true;
        } else if !self.sent_config && packets.iter().any(|(f, _)| *f == KEYFRAME) {
            // SPS/PPS absents du flux : on les prend dans le format de sortie.
            if let Some(cfg) = self.sequence_header() {
                packets.insert(0, (CONFIG, cfg));
                self.sent_config = true;
            }
        }
        self.pending.extend(packets);
        Ok(true)
    }

    unsafe fn sequence_header(&self) -> Option<Vec<u8>> {
        let ty = self.t.GetOutputCurrentType(0).ok()?;
        let mut p: *mut u8 = std::ptr::null_mut();
        let mut n = 0u32;
        ty.GetAllocatedBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut p, &mut n)
            .ok()?;
        let v = std::slice::from_raw_parts(p, n as usize).to_vec();
        CoTaskMemFree(Some(p as *const _));
        Some(v)
    }
}

impl Drop for MfH264 {
    fn drop(&mut self) {
        unsafe {
            let _ = self.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            if let Ok(s) = self.t.cast::<IMFShutdown>() {
                let _ = s.Shutdown();
            }
        }
    }
}
