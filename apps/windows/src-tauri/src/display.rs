//! Écran étendu : le téléphone devient un moniteur supplémentaire de Windows.
//!
//! 1. Le PC demande au téléphone d'ouvrir l'affichage (`StreamRequest{Display}`).
//! 2. Le téléphone répond avec la taille réelle de son écran (`StreamStart{Display}`).
//! 3. Le PC demande au pilote NcDisplay (IddCx) de brancher un moniteur virtuel de cette taille :
//!    Windows le voit comme un vrai écran (Paramètres › Système › Écran, mode « Étendre »).
//! 4. Chaque image composée par Windows pour ce moniteur arrive en mémoire partagée ; elle est
//!    encodée en JPEG et envoyée au téléphone (une image n'est émise que si elle a changé).
//! 5. Les touchers sur le téléphone (`DisplayPointer`) deviennent des clics/glissers Windows
//!    positionnés sur ce moniteur.

use serde::Serialize;

/// Encode une image BGRA (sans marge de ligne) en JPEG.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn encode_jpeg(
    bgra: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    out.clear();
    let enc = jpeg_encoder::Encoder::new(&mut *out, quality);
    enc.encode(
        bgra,
        width as u16,
        height as u16,
        jpeg_encoder::ColorType::Bgra,
    )
    .map_err(|e| e.to_string())
}

/// Convertit une position normalisée (0..1) sur le moniteur virtuel en coordonnées absolues
/// SendInput (0..65535 sur l'ensemble du bureau virtuel).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn to_virtual_desk(
    x: f32,
    y: f32,
    mon: (i32, i32, i32, i32),
    desk: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (ml, mt, mw, mh) = mon;
    let (dl, dt, dw, dh) = desk;
    let px = ml as f64 + x.clamp(0.0, 1.0) as f64 * (mw - 1).max(1) as f64;
    let py = mt as f64 + y.clamp(0.0, 1.0) as f64 * (mh - 1).max(1) as f64;
    let ax = ((px - dl as f64) * 65535.0 / (dw - 1).max(1) as f64).round() as i32;
    let ay = ((py - dt as f64) * 65535.0 / (dh - 1).max(1) as f64).round() as i32;
    (ax.clamp(0, 65535), ay.clamp(0, 65535))
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DisplayStatus {
    /// Fonction disponible sur ce système (Windows uniquement).
    pub supported: bool,
    /// Pilote installé et périphérique présent.
    pub installed: bool,
    /// Signature de test activée (nécessaire tant que le pilote n'est pas signé par Microsoft).
    pub testsigning: bool,
    /// Téléphone qui affiche actuellement l'écran étendu.
    pub active_peer: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Outil d'installation présent dans l'application.
    pub setup_available: bool,
}

#[cfg(windows)]
pub use win::Display;

#[cfg(not(windows))]
pub use stub::Display;

#[cfg(not(windows))]
mod stub {
    use super::DisplayStatus;
    use tauri::AppHandle;

    #[derive(Default)]
    pub struct Display;

    impl Display {
        pub fn status(&self, _app: &AppHandle) -> DisplayStatus {
            DisplayStatus::default()
        }
        pub fn setup(&self, _app: &AppHandle, _arg: &str) -> Result<String, String> {
            Err("L'écran étendu n'est disponible que sous Windows.".into())
        }
        pub fn on_phone_ready(
            &self,
            _app: &AppHandle,
            _peer: &str,
            _w: u32,
            _h: u32,
            _hz: u32,
        ) -> Result<(), String> {
            Err("L'écran étendu n'est disponible que sous Windows.".into())
        }
        pub fn stop(&self, _app: &AppHandle) {}
        pub fn stop_if_peer(&self, _app: &AppHandle, _peer: &str) {}
        pub fn pointer(&self, _action: nc_core::protocol::PointerAction, _x: f32, _y: f32) {}
        pub fn request_frame(&self) {}
        pub fn set_quality(&self, _scale: f32, _jpeg_quality: u8, _fps: u32) {}
    }
}

#[cfg(windows)]
mod win {
    use super::{encode_jpeg, to_virtual_desk, DisplayStatus};
    use crate::AppState;
    use nc_core::protocol::{Message, PointerAction, StreamKind, StreamParams};
    use std::ffi::c_void;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Manager};
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW,
        CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CR_SUCCESS,
    };
    use windows::Win32::Foundation::{CloseHandle, HANDLE, LPARAM, RECT, WAIT_OBJECT_0};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
        MONITORINFO, MONITORINFOEXW,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Memory::{
        MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS,
    };
    use windows::Win32::System::Threading::{
        OpenEventW, WaitForSingleObject, SYNCHRONIZATION_SYNCHRONIZE,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    // Doit rester identique à native/windows/display-driver/shared/NcDisplayShared.h.
    const IFACE: GUID = GUID::from_values(
        0x6f7c2a1e,
        0x4b3d,
        0x4e8a,
        [0x9c, 0x51, 0x2d, 0x7b, 0x8e, 0x0f, 0x4a, 0x63],
    );
    const fn ioctl(f: u32) -> u32 {
        (0x22 << 16) | (f << 2)
    }
    const IOCTL_ADD: u32 = ioctl(0x800);
    const IOCTL_REMOVE: u32 = ioctl(0x801);
    const HEADER: usize = 64;
    const MAX_W: usize = 2560;
    const MAX_H: usize = 2560;
    const SHARED_SIZE: usize = HEADER + MAX_W * MAX_H * 4;
    const MAGIC: u32 = 0x4644_434E;
    /// Nom d'adaptateur déclaré dans NcDisplay.inf (DeviceName).
    const ADAPTER_NAME: &str = "NOTHING CONNECT";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn interface_path() -> Option<Vec<u16>> {
        unsafe {
            let mut len = 0u32;
            if CM_Get_Device_Interface_List_SizeW(
                &mut len,
                &IFACE,
                PCWSTR::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            ) != CR_SUCCESS
                || len <= 1
            {
                return None;
            }
            let mut buf = vec![0u16; len as usize];
            if CM_Get_Device_Interface_ListW(
                &IFACE,
                PCWSTR::null(),
                &mut buf,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            ) != CR_SUCCESS
            {
                return None;
            }
            let end = buf.iter().position(|c| *c == 0)?;
            (end > 0).then(|| {
                let mut p = buf[..end].to_vec();
                p.push(0);
                p
            })
        }
    }

    fn send_ioctl(code: u32, input: &[u8]) -> Result<(), String> {
        let path = interface_path().ok_or("pilote d'écran non installé")?;
        unsafe {
            let h = CreateFileW(
                PCWSTR(path.as_ptr()),
                0xC000_0000, // GENERIC_READ | GENERIC_WRITE
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
            .map_err(|e| format!("ouverture du pilote impossible : {e}"))?;
            let mut ret = 0u32;
            let r = DeviceIoControl(
                h,
                code,
                (!input.is_empty()).then_some(input.as_ptr() as *const c_void),
                input.len() as u32,
                None,
                0,
                Some(&mut ret),
                None,
            );
            let _ = CloseHandle(h);
            r.map_err(|e| format!("le pilote a refusé la demande : {e}"))
        }
    }

    struct FrameReader {
        map: HANDLE,
        view: MEMORY_MAPPED_VIEW_ADDRESS,
        event: HANDLE,
        last_seq: i64,
    }

    // Les handles et la vue sont utilisés depuis un seul thread à la fois.
    unsafe impl Send for FrameReader {}

    impl FrameReader {
        fn open() -> Option<Self> {
            unsafe {
                let map = OpenFileMappingW(
                    FILE_MAP_READ.0,
                    false,
                    PCWSTR(wide("Global\\NothingConnectDisplayFrame").as_ptr()),
                )
                .ok()?;
                let view = MapViewOfFile(map, FILE_MAP_READ, 0, 0, SHARED_SIZE);
                if view.Value.is_null() {
                    let _ = CloseHandle(map);
                    return None;
                }
                let event = match OpenEventW(
                    SYNCHRONIZATION_SYNCHRONIZE,
                    false,
                    PCWSTR(wide("Global\\NothingConnectDisplayEvent").as_ptr()),
                ) {
                    Ok(e) => e,
                    Err(_) => {
                        let _ = UnmapViewOfFile(view);
                        let _ = CloseHandle(map);
                        return None;
                    }
                };
                Some(Self {
                    map,
                    view,
                    event,
                    last_seq: 0,
                })
            }
        }

        /// Attend une nouvelle image (jusqu'à `timeout`) et la copie dans `buf` (BGRA compact).
        fn next(
            &mut self,
            buf: &mut Vec<u8>,
            timeout: Duration,
            force: bool,
        ) -> Option<(u32, u32)> {
            unsafe {
                let waited =
                    WaitForSingleObject(self.event, timeout.as_millis() as u32) == WAIT_OBJECT_0;
                let base = self.view.Value as *const u8;
                let rd32 = |off: usize| std::ptr::read_volatile(base.add(off) as *const u32);
                if rd32(0) != MAGIC {
                    return None;
                }
                let seq_ptr = base.add(8) as *const i64;
                for _ in 0..3 {
                    let s1 = std::ptr::read_volatile(seq_ptr);
                    if s1 & 1 == 1 {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    if s1 == self.last_seq && !force && !waited {
                        return None;
                    }
                    let (w, h, stride) = (rd32(16) as usize, rd32(20) as usize, rd32(24) as usize);
                    if w == 0 || h == 0 || w > MAX_W || h > MAX_H || stride < w * 4 {
                        return None;
                    }
                    buf.resize(w * h * 4, 0);
                    for y in 0..h {
                        std::ptr::copy_nonoverlapping(
                            base.add(HEADER + y * stride),
                            buf.as_mut_ptr().add(y * w * 4),
                            w * 4,
                        );
                    }
                    std::sync::atomic::fence(Ordering::Acquire);
                    if std::ptr::read_volatile(seq_ptr) == s1 {
                        self.last_seq = s1;
                        return Some((w as u32, h as u32));
                    }
                }
                None
            }
        }
    }

    impl Drop for FrameReader {
        fn drop(&mut self) {
            unsafe {
                let _ = UnmapViewOfFile(self.view);
                let _ = CloseHandle(self.map);
                let _ = CloseHandle(self.event);
            }
        }
    }

    /// Rectangle (gauche, haut, largeur, hauteur) du moniteur virtuel sur le bureau.
    fn monitor_rect() -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let mut device_name: Option<[u16; 32]> = None;
            let mut i = 0;
            loop {
                let mut dd = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                if !EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0).as_bool() {
                    break;
                }
                let desc = String::from_utf16_lossy(&dd.DeviceString);
                if desc.contains(ADAPTER_NAME) && dd.StateFlags & 0x1 != 0 {
                    device_name = Some(dd.DeviceName);
                    break;
                }
                i += 1;
            }
            let name = device_name?;
            struct Ctx {
                name: [u16; 32],
                rect: Option<RECT>,
            }
            unsafe extern "system" fn cb(
                m: HMONITOR,
                _: HDC,
                _: *mut RECT,
                data: LPARAM,
            ) -> windows::core::BOOL {
                let ctx = &mut *(data.0 as *mut Ctx);
                let mut mi = MONITORINFOEXW {
                    monitorInfo: MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                if GetMonitorInfoW(m, &mut mi as *mut _ as *mut MONITORINFO).as_bool()
                    && mi.szDevice == ctx.name
                {
                    ctx.rect = Some(mi.monitorInfo.rcMonitor);
                    return false.into();
                }
                true.into()
            }
            let mut ctx = Ctx { name, rect: None };
            let _ =
                EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut ctx as *mut Ctx as isize));
            ctx.rect
                .map(|r| (r.left, r.top, r.right - r.left, r.bottom - r.top))
        }
    }

    fn send_mouse(
        flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
        x: i32,
        y: i32,
    ) {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: x,
                    dy: y,
                    mouseData: 0,
                    dwFlags: flags
                        | MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_VIRTUALDESK
                        | MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    struct Active {
        peer: String,
        stop: Arc<AtomicBool>,
        width: u32,
        height: u32,
    }

    pub struct Display {
        active: Mutex<Option<Active>>,
        force_frame: Arc<AtomicBool>,
        /// Échelle de la résolution (100 = taille réelle du téléphone), qualité JPEG, images/s.
        scale_pct: AtomicU32,
        quality: Arc<AtomicU8>,
        fps: Arc<AtomicU32>,
        rect: Mutex<Option<(i32, i32, i32, i32)>>,
    }

    impl Default for Display {
        fn default() -> Self {
            Self {
                active: Mutex::new(None),
                force_frame: Arc::new(AtomicBool::new(false)),
                scale_pct: AtomicU32::new(75),
                quality: Arc::new(AtomicU8::new(70)),
                fps: Arc::new(AtomicU32::new(30)),
                rect: Mutex::new(None),
            }
        }
    }

    fn setup_exe(app: &AppHandle) -> Option<PathBuf> {
        let p = app
            .path()
            .resource_dir()
            .ok()?
            .join("driver")
            .join("NcDisplaySetup.exe");
        p.is_file().then_some(p)
    }

    impl Display {
        pub fn set_quality(&self, scale: f32, jpeg_quality: u8, fps: u32) {
            self.scale_pct
                .store((scale.clamp(0.25, 1.0) * 100.0) as u32, Ordering::SeqCst);
            self.quality
                .store(jpeg_quality.clamp(30, 95), Ordering::SeqCst);
            self.fps.store(fps.clamp(5, 60), Ordering::SeqCst);
        }

        pub fn status(&self, app: &AppHandle) -> DisplayStatus {
            let exe = setup_exe(app);
            // « status » sort avec le code 2 si le pilote est absent : on lit la sortie quel que soit le code.
            let testsigning = exe
                .as_ref()
                .and_then(|e| {
                    let mut c = std::process::Command::new(e);
                    c.arg("status");
                    {
                        use std::os::windows::process::CommandExt;
                        c.creation_flags(0x0800_0000);
                    }
                    c.output().ok()
                })
                .map(|o| String::from_utf8_lossy(&o.stdout).contains("\"testsigning\":true"))
                .unwrap_or(false);
            let a = self.active.lock().unwrap();
            DisplayStatus {
                supported: true,
                installed: interface_path().is_some(),
                testsigning,
                active_peer: a.as_ref().map(|a| a.peer.clone()),
                width: a.as_ref().map(|a| a.width).unwrap_or(0),
                height: a.as_ref().map(|a| a.height).unwrap_or(0),
                setup_available: exe.is_some(),
            }
        }

        /// Lance l'outil d'installation (`install`, `uninstall`, `enable-testsigning`).
        /// L'outil demande lui-même l'élévation (fenêtre UAC de Windows).
        pub fn setup(&self, app: &AppHandle, arg: &str) -> Result<String, String> {
            let exe =
                setup_exe(app).ok_or("Outil d'installation du pilote absent de cette version.")?;
            let dir = exe.parent().unwrap().to_string_lossy().into_owned();
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg(arg);
            if arg == "install" {
                cmd.arg(&dir);
            }
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let code = cmd.status().map_err(|e| e.to_string())?.code().unwrap_or(1);
            match code {
                0 => Ok("ok".into()),
                12 => Ok("redemarrage".into()),
                3 => Err("Opération annulée dans la fenêtre d'autorisation Windows.".into()),
                10 => Err("Windows refuse les pilotes non signés par Microsoft. Activez d'abord le mode « signature de test » (bouton ci-dessous), puis redémarrez.".into()),
                11 => Err("Le démarrage sécurisé (Secure Boot) empêche le mode signature de test. Il faut le désactiver dans le BIOS/UEFI, ou attendre une version du pilote signée par Microsoft.".into()),
                2 => Err("Pilote absent.".into()),
                c => Err(format!("L'installation a échoué (code {c}). Consultez les journaux.")),
            }
        }

        /// Le téléphone est prêt et annonce la taille de son écran : branche le moniteur virtuel.
        pub fn on_phone_ready(
            &self,
            app: &AppHandle,
            peer: &str,
            w: u32,
            h: u32,
            hz: u32,
        ) -> Result<(), String> {
            self.stop_thread();
            let s = self.scale_pct.load(Ordering::SeqCst) as f32 / 100.0;
            let fit = |v: u32, max: u32| -> u32 { ((v as f32 * s) as u32).min(max) & !1 };
            let (mut w, mut h) = (fit(w, MAX_W as u32), fit(h, MAX_H as u32));
            // Respecte la limite du pilote en conservant les proportions.
            let k = (MAX_W as f32 / w as f32)
                .min(MAX_H as f32 / h as f32)
                .min(1.0);
            w = ((w as f32 * k) as u32).max(320) & !1;
            h = ((h as f32 * k) as u32).max(320) & !1;
            let mut req = Vec::with_capacity(12);
            req.extend_from_slice(&w.to_le_bytes());
            req.extend_from_slice(&h.to_le_bytes());
            req.extend_from_slice(&hz.clamp(30, 120).to_le_bytes());
            send_ioctl(IOCTL_ADD, &req)?;
            app.state::<AppState>()
                .node
                .lock()
                .unwrap()
                .set_streaming(true);

            let stop = Arc::new(AtomicBool::new(false));
            *self.active.lock().unwrap() = Some(Active {
                peer: peer.to_string(),
                stop: stop.clone(),
                width: w,
                height: h,
            });
            *self.rect.lock().unwrap() = None;
            let params = StreamParams {
                codec: Some("jpeg".into()),
                width: Some(w),
                height: Some(h),
                ..Default::default()
            };
            let _ = app.state::<AppState>().node.lock().unwrap().send(
                peer,
                Message::StreamStart {
                    kind: StreamKind::Display,
                    params,
                },
            );

            let (app2, peer2, force, quality, fps) = (
                app.clone(),
                peer.to_string(),
                self.force_frame.clone(),
                self.quality.clone(),
                self.fps.clone(),
            );
            std::thread::Builder::new()
                .name("nc-display".into())
                .spawn(move || {
                    // Le pilote crée la mémoire partagée quand Windows attribue la chaîne d'images.
                    let mut reader = None;
                    let started = Instant::now();
                    while reader.is_none() && !stop.load(Ordering::SeqCst) {
                        reader = FrameReader::open();
                        if reader.is_none() {
                            if started.elapsed() > Duration::from_secs(15) {
                                tracing::warn!("écran étendu : aucune image du pilote");
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    }
                    let Some(mut reader) = reader else { return };
                    let mut bgra = Vec::new();
                    let mut jpeg = Vec::with_capacity(512 * 1024);
                    let mut last_sent = Instant::now() - Duration::from_secs(10);
                    let t0 = Instant::now();
                    while !stop.load(Ordering::SeqCst) {
                        let interval =
                            Duration::from_millis(1000 / fps.load(Ordering::Relaxed).max(1) as u64);
                        // Image de rappel toutes les 2 s (téléphone qui rouvre l'écran) ou sur demande.
                        let forced = force.swap(false, Ordering::SeqCst)
                            || last_sent.elapsed() > Duration::from_secs(2);
                        let Some((w, h)) =
                            reader.next(&mut bgra, Duration::from_millis(100), forced)
                        else {
                            continue;
                        };
                        if last_sent.elapsed() < interval {
                            std::thread::sleep(interval - last_sent.elapsed());
                        }
                        if encode_jpeg(&bgra, w, h, quality.load(Ordering::Relaxed), &mut jpeg)
                            .is_err()
                        {
                            continue;
                        }
                        let ts = t0.elapsed().as_micros() as u64;
                        let st = app2.state::<AppState>();
                        let sent = st.node.lock().unwrap().send_stream(
                            &peer2,
                            StreamKind::Display,
                            nc_core::protocol::stream_flags::KEYFRAME,
                            ts,
                            &jpeg,
                        );
                        if sent {
                            last_sent = Instant::now();
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        fn stop_thread(&self) -> Option<Active> {
            let a = self.active.lock().unwrap().take();
            if let Some(a) = &a {
                a.stop.store(true, Ordering::SeqCst);
            }
            a
        }

        /// Débranche le moniteur virtuel (Windows replace les fenêtres sur l'écran principal).
        pub fn stop(&self, app: &AppHandle) {
            if let Some(a) = self.stop_thread() {
                let _ = send_ioctl(IOCTL_REMOVE, &[]);
                let st = app.state::<AppState>();
                let node = st.node.lock().unwrap();
                node.set_streaming(false);
                let _ = node.send(
                    &a.peer,
                    Message::StreamStop {
                        kind: StreamKind::Display,
                        reason: None,
                    },
                );
            }
        }

        pub fn stop_if_peer(&self, app: &AppHandle, peer: &str) {
            let is = self
                .active
                .lock()
                .unwrap()
                .as_ref()
                .map(|a| a.peer == peer)
                .unwrap_or(false);
            if is {
                self.stop(app);
            }
        }

        pub fn request_frame(&self) {
            self.force_frame.store(true, Ordering::SeqCst);
        }

        /// Toucher du téléphone → souris Windows sur le moniteur virtuel.
        pub fn pointer(&self, action: PointerAction, x: f32, y: f32) {
            let rect = {
                let mut r = self.rect.lock().unwrap();
                if r.is_none() {
                    *r = monitor_rect();
                }
                *r
            };
            let Some(mon) = rect else { return };
            let desk = unsafe {
                (
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN),
                )
            };
            let (ax, ay) = to_virtual_desk(x, y, mon, desk);
            match action {
                PointerAction::Down => send_mouse(MOUSEEVENTF_LEFTDOWN, ax, ay),
                PointerAction::Move => send_mouse(
                    windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS(0),
                    ax,
                    ay,
                ),
                PointerAction::Up | PointerAction::Cancel => {
                    send_mouse(MOUSEEVENTF_LEFTUP, ax, ay);
                    // La disposition des écrans peut avoir changé : recalcul au prochain toucher.
                    *self.rect.lock().unwrap() = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_roundtrip_size() {
        let (w, h) = (64u32, 48u32);
        let bgra: Vec<u8> = (0..w * h)
            .flat_map(|i| [(i % 255) as u8, 80, 160, 255])
            .collect();
        let mut out = Vec::new();
        encode_jpeg(&bgra, w, h, 70, &mut out).unwrap();
        assert_eq!(&out[..2], &[0xFF, 0xD8], "en-tête JPEG");
        assert!(out.len() < bgra.len());
    }

    #[test]
    fn pointer_mapping_on_secondary_monitor() {
        // Écran principal 1920×1080 en (0,0), écran du téléphone 1080×2400 à droite.
        let mon = (1920, 0, 1080, 2400);
        let desk = (0, 0, 3000, 2400);
        assert_eq!(
            to_virtual_desk(0.0, 0.0, mon, desk),
            (((1920.0 * 65535.0) / 2999.0f64).round() as i32, 0)
        );
        let (x, y) = to_virtual_desk(1.0, 1.0, mon, desk);
        assert_eq!((x, y), (65535, 65535));
        let (x, _) = to_virtual_desk(0.5, 0.5, mon, desk);
        assert!(x > 65535 * 2 / 3);
    }
}
