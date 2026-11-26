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

/// Fixe `<monitors><count>N</count>` dans le fichier de réglages de Virtual Display Driver.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn set_xml_count(xml: &str, count: u32) -> String {
    let Some(m) = xml.find("<monitors>") else {
        return xml.to_string();
    };
    let rest = &xml[m..];
    let (Some(a), Some(b)) = (rest.find("<count>"), rest.find("</count>")) else {
        return xml.to_string();
    };
    format!("{}{}<count>{count}{}", &xml[..m], &rest[..a], &rest[b..])
}

/// Ajoute une résolution à `<resolutions>` si elle n'y est pas déjà.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn ensure_xml_resolution(xml: &str, w: u32, h: u32, hz: u32) -> String {
    let needle = format!("<width>{w}</width>\n            <height>{h}</height>");
    let compact = format!("<width>{w}</width><height>{h}</height>");
    if xml.contains(&needle) || xml.replace(char::is_whitespace, "").contains(&compact) {
        return xml.to_string();
    }
    let Some(i) = xml.find("</resolutions>") else {
        return xml.to_string();
    };
    let entry = format!(
        "    <resolution>\n            <width>{w}</width>\n            <height>{h}</height>\n            <refresh_rate>{hz}</refresh_rate>\n        </resolution>\n    "
    );
    format!("{}{entry}{}", &xml[..i], &xml[i..])
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
    /// Pilote libre et signé « Virtual Display Driver » détecté (alternative sans signature de test).
    pub vdd_installed: bool,
    /// Pilote utilisé : « nothing-connect », « vdd » ou absent.
    pub backend: Option<String>,
    /// Mesures en direct de la session : format, images par seconde envoyées, débit.
    pub codec: Option<String>,
    pub fps: u32,
    pub kbps: u32,
    /// Fréquence de l'écran virtuel (celle du téléphone).
    pub refresh_hz: u32,
}

/// Compteurs de la session en cours (mis à jour chaque seconde par le fil de capture).
#[derive(Default)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct LiveStats {
    pub fps: std::sync::atomic::AtomicU32,
    pub kbps: std::sync::atomic::AtomicU32,
    pub h264: std::sync::atomic::AtomicBool,
    pub hz: std::sync::atomic::AtomicU32,
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
            _h264: bool,
        ) -> Result<(), String> {
            Err("L'écran étendu n'est disponible que sous Windows.".into())
        }
        pub fn stop(&self, _app: &AppHandle) {}
        pub fn stop_if_peer(&self, _app: &AppHandle, _peer: &str) {}
        pub fn pointer(
            &self,
            _action: nc_core::protocol::PointerAction,
            _id: u32,
            _x: f32,
            _y: f32,
        ) {
        }
        pub fn input(&self, _action: &nc_core::protocol::DisplayAction) {}
        pub fn set_touch(&self, _touch: bool) {}
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

    /// Nom GDI (\\.\DISPLAYn) de l'écran virtuel actif, d'après le nom de son adaptateur.
    fn display_device_name(adapter: &str) -> Option<[u16; 32]> {
        unsafe {
            let mut i = 0;
            loop {
                let mut dd = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                if !EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0).as_bool() {
                    return None;
                }
                let desc = String::from_utf16_lossy(&dd.DeviceString);
                // DISPLAY_DEVICE_ATTACHED_TO_DESKTOP = 0x1
                if desc.contains(adapter) && dd.StateFlags.0 & 0x1 != 0 {
                    return Some(dd.DeviceName);
                }
                i += 1;
            }
        }
    }

    /// Rectangle (gauche, haut, largeur, hauteur) du moniteur virtuel sur le bureau.
    fn monitor_rect(adapter: &str) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let name = display_device_name(adapter)?;
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

    // ------------------------------------------------------------------ pilote « Virtual Display Driver »
    //
    // Pilote IddCx libre (MIT, github.com/VirtualDrivers/Virtual-Display-Driver), signé et
    // installable sans mode test. Il est piloté par son tube nommé et son fichier de réglages ;
    // l'image de l'écran est capturée avec l'API Desktop Duplication de Windows.
    mod vdd {
        use std::io::Write;
        use std::path::PathBuf;

        pub const ADAPTER: &str = "Virtual Display Driver";
        const PIPE: &str = r"\\.\pipe\MTTVirtualDisplayPipe";

        /// ERROR_PIPE_BUSY : le tube existe mais sert déjà un autre client.
        const PIPE_BUSY: i32 = 231;

        fn open() -> std::io::Result<std::fs::File> {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(PIPE)
        }

        pub fn present() -> bool {
            match open() {
                Ok(_) => true,
                Err(e) => e.raw_os_error() == Some(PIPE_BUSY),
            }
        }

        pub fn command(cmd: &str) -> Result<(), String> {
            let mut tries = 0;
            let mut p = loop {
                match open() {
                    Ok(f) => break f,
                    Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && tries < 20 => {
                        tries += 1;
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        return Err(format!("pilote Virtual Display Driver injoignable : {e}"))
                    }
                }
            };
            let bytes: Vec<u8> = cmd
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|c| c.to_le_bytes())
                .collect();
            p.write_all(&bytes).map_err(|e| e.to_string())
        }

        fn settings_path() -> PathBuf {
            use windows::core::PCWSTR;
            use windows::Win32::System::Registry::{
                RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
            };
            let key: Vec<u16> = "SOFTWARE\\MikeTheTech\\VirtualDisplayDriver\0"
                .encode_utf16()
                .collect();
            let val: Vec<u16> = "VDDPATH\0".encode_utf16().collect();
            let mut buf = vec![0u16; 520];
            let mut size = (buf.len() * 2) as u32;
            let ok = unsafe {
                RegGetValueW(
                    HKEY_LOCAL_MACHINE,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(val.as_ptr()),
                    RRF_RT_REG_SZ,
                    None,
                    Some(buf.as_mut_ptr() as *mut _),
                    Some(&mut size),
                )
                .is_ok()
            };
            let dir = if ok {
                let end = buf.iter().position(|c| *c == 0).unwrap_or(0);
                PathBuf::from(String::from_utf16_lossy(&buf[..end]))
            } else {
                PathBuf::from(r"C:\VirtualDisplayDriver")
            };
            dir.join("vdd_settings.xml")
        }

        /// Ajoute la résolution du téléphone à la liste des modes et fixe le nombre d'écrans.
        pub fn configure(w: u32, h: u32, hz: u32, count: u32) -> Result<(), String> {
            let path = settings_path();
            let mut xml = std::fs::read_to_string(&path)
                .map_err(|e| format!("réglages du pilote illisibles ({}) : {e}", path.display()))?;
            xml = super::super::set_xml_count(&xml, count);
            xml = super::super::ensure_xml_resolution(&xml, w, h, hz);
            std::fs::write(&path, xml).map_err(|e| {
                format!(
                    "réglages du pilote non modifiables ({}) : {e}",
                    path.display()
                )
            })
        }
    }

    /// Capture d'un écran par l'API Desktop Duplication (DXGI).
    struct Duplicator {
        dup: windows::Win32::Graphics::Dxgi::IDXGIOutputDuplication,
        device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
        ctx: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
        staging: Option<(
            windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
            u32,
            u32,
        )>,
        has_frame: bool,
    }

    unsafe impl Send for Duplicator {}

    impl Duplicator {
        fn open(device_name: &[u16; 32]) -> Option<Self> {
            use windows::core::Interface;
            use windows::Win32::Foundation::HMODULE;
            use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
            use windows::Win32::Graphics::Direct3D11::{
                D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
            };
            use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1};
            unsafe {
                let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
                let mut ai = 0;
                while let Ok(adapter) = factory.EnumAdapters1(ai) {
                    let mut oi = 0;
                    while let Ok(output) = adapter.EnumOutputs(oi) {
                        if output
                            .GetDesc()
                            .map(|d| &d.DeviceName == device_name)
                            .unwrap_or(false)
                        {
                            let mut device = None;
                            let mut ctx = None;
                            D3D11CreateDevice(
                                &adapter,
                                D3D_DRIVER_TYPE_UNKNOWN,
                                HMODULE::default(),
                                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                                None,
                                D3D11_SDK_VERSION,
                                Some(&mut device),
                                None,
                                Some(&mut ctx),
                            )
                            .ok()?;
                            let (device, ctx) = (device?, ctx?);
                            let out1: IDXGIOutput1 = output.cast().ok()?;
                            let dup = out1.DuplicateOutput(&device).ok()?;
                            return Some(Self {
                                dup,
                                device,
                                ctx,
                                staging: None,
                                has_frame: false,
                            });
                        }
                        oi += 1;
                    }
                    ai += 1;
                }
                None
            }
        }

        /// `None` : pas de nouvelle image (ou accès perdu : l'appelant rouvre la capture).
        fn next(
            &mut self,
            buf: &mut Vec<u8>,
            timeout: Duration,
            force: bool,
        ) -> Result<Option<(u32, u32)>, ()> {
            use windows::core::Interface;
            use windows::Win32::Graphics::Direct3D11::{
                ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            };
            use windows::Win32::Graphics::Dxgi::{
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
            };
            unsafe {
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut res = None;
                match self
                    .dup
                    .AcquireNextFrame(timeout.as_millis() as u32, &mut info, &mut res)
                {
                    Ok(()) => {}
                    Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                        if !(force && self.has_frame) {
                            return Ok(None);
                        }
                        // Pas de changement : on renvoie la dernière image copiée.
                        return Ok(self
                            .staging
                            .as_ref()
                            .map(|s| (s.1, s.2))
                            .filter(|_| !buf.is_empty()));
                    }
                    Err(_) => return Err(()),
                }
                let changed =
                    info.LastPresentTime != 0 || info.AccumulatedFrames > 0 || !self.has_frame;
                let result = (|| -> Option<(u32, u32)> {
                    if !changed && !force {
                        return None;
                    }
                    let tex: ID3D11Texture2D = res.as_ref()?.cast().ok()?;
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    tex.GetDesc(&mut desc);
                    if self
                        .staging
                        .as_ref()
                        .map(|s| (s.1, s.2) != (desc.Width, desc.Height))
                        .unwrap_or(true)
                    {
                        let mut sd = desc;
                        sd.Usage = D3D11_USAGE_STAGING;
                        sd.BindFlags = 0;
                        sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                        sd.MiscFlags = 0;
                        sd.MipLevels = 1;
                        sd.ArraySize = 1;
                        let mut st = None;
                        self.device.CreateTexture2D(&sd, None, Some(&mut st)).ok()?;
                        self.staging = Some((st?, desc.Width, desc.Height));
                    }
                    let (st, w, h) = self.staging.clone()?;
                    self.ctx.CopyResource(&st, &tex);
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    self.ctx
                        .Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                        .ok()?;
                    let (wu, hu, pitch) = (w as usize, h as usize, mapped.RowPitch as usize);
                    buf.resize(wu * hu * 4, 0);
                    let src = mapped.pData as *const u8;
                    for y in 0..hu {
                        std::ptr::copy_nonoverlapping(
                            src.add(y * pitch),
                            buf.as_mut_ptr().add(y * wu * 4),
                            wu * 4,
                        );
                    }
                    self.ctx.Unmap(&st, 0);
                    self.has_frame = true;
                    Some((w, h))
                })();
                let _ = self.dup.ReleaseFrame();
                Ok(result)
            }
        }
    }

    enum Source {
        Shared(FrameReader),
        Dup(Duplicator, [u16; 32]),
    }

    impl Source {
        fn next(
            &mut self,
            buf: &mut Vec<u8>,
            timeout: Duration,
            force: bool,
        ) -> Option<(u32, u32)> {
            match self {
                Source::Shared(r) => r.next(buf, timeout, force),
                Source::Dup(d, name) => match d.next(buf, timeout, force) {
                    Ok(v) => v,
                    Err(()) => {
                        // Accès perdu (changement de mode, verrouillage) : on rouvre la capture.
                        std::thread::sleep(Duration::from_millis(200));
                        if let Some(n) = Duplicator::open(name) {
                            *d = n;
                        }
                        None
                    }
                },
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Backend {
        Own,
        Vdd,
    }

    struct Active {
        peer: String,
        stop: Arc<AtomicBool>,
        width: u32,
        height: u32,
        backend: Backend,
    }

    pub struct Display {
        active: Mutex<Option<Active>>,
        force_frame: Arc<AtomicBool>,
        stats: Arc<super::LiveStats>,
        /// Toucher du téléphone : écran tactile Windows (vrai) ou souris (faux).
        touch: AtomicBool,
        /// Mode souris : seul le premier doigt pilote le curseur.
        mouse_finger: Mutex<Option<u32>>,
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
                stats: Arc::new(super::LiveStats::default()),
                touch: AtomicBool::new(true),
                mouse_finger: Mutex::new(None),
                scale_pct: AtomicU32::new(75),
                quality: Arc::new(AtomicU8::new(70)),
                fps: Arc::new(AtomicU32::new(30)),
                rect: Mutex::new(None),
            }
        }
    }

    fn remove_monitor(b: Backend) {
        match b {
            Backend::Own => {
                let _ = send_ioctl(IOCTL_REMOVE, &[]);
            }
            Backend::Vdd => {
                let _ = vdd::command("SETDISPLAYCOUNT 0");
            }
        }
    }

    /// Règle la définition et la fréquence d'un écran (sans droits administrateur).
    /// Définition actuelle d'un écran.
    fn current_mode(name: &[u16; 32]) -> Option<(u32, u32, u32)> {
        use windows::Win32::Graphics::Gdi::{
            EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS,
        };
        let mut dm = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        unsafe { EnumDisplaySettingsW(PCWSTR(name.as_ptr()), ENUM_CURRENT_SETTINGS, &mut dm) }
            .as_bool()
            .then_some((dm.dmPelsWidth, dm.dmPelsHeight, dm.dmDisplayFrequency))
    }

    /// Impose au moniteur virtuel le format exact du téléphone (sinon l'image n'occupe pas
    /// tout l'écran du téléphone) **et sa fréquence** : Windows choisit sinon souvent 60 Hz,
    /// ce qui plafonne l'écran étendu à 60 images/s. Réessaie le temps que Windows publie le mode.
    /// Renvoie la fréquence réellement appliquée.
    fn ensure_mode(name: &[u16; 32], w: u32, h: u32, hz: u32) -> Option<u32> {
        // 119 Hz annoncé pour 119,88 : tolérance d'un hertz.
        let ok = |m: Option<(u32, u32, u32)>| matches!(m, Some((mw, mh, mhz)) if mw == w && mh == h && mhz.abs_diff(hz) <= 1);
        for _ in 0..10 {
            if ok(current_mode(name)) {
                return Some(hz);
            }
            set_mode(name, w, h, hz);
            std::thread::sleep(Duration::from_millis(300));
        }
        let got = current_mode(name);
        tracing::warn!("écran virtuel en {got:?} au lieu de {w}×{h} à {hz} Hz");
        got.map(|m| m.2)
    }

    fn set_mode(name: &[u16; 32], w: u32, h: u32, hz: u32) {
        use windows::Win32::Graphics::Gdi::{
            ChangeDisplaySettingsExW, CDS_UPDATEREGISTRY, DEVMODEW, DM_DISPLAYFREQUENCY,
            DM_PELSHEIGHT, DM_PELSWIDTH,
        };
        let dm = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            dmPelsWidth: w,
            dmPelsHeight: h,
            dmDisplayFrequency: hz,
            dmFields: DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY,
            ..Default::default()
        };
        let r = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(name.as_ptr()),
                Some(&dm),
                None,
                CDS_UPDATEREGISTRY,
                None,
            )
        };
        tracing::info!("mode {w}×{h}@{hz} demandé pour l'écran virtuel : {r:?}");
    }

    /// Installe le pilote libre et signé « Virtual Display Driver » avec winget (Windows demande
    /// l'autorisation). Aucune signature de test n'est nécessaire.
    fn install_vdd() -> Result<String, String> {
        let mut cmd = std::process::Command::new("winget");
        cmd.args([
            "install",
            "--id",
            "VirtualDrivers.Virtual-Display-Driver",
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]);
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd.output().map_err(|_| "winget est introuvable : installez « Programme d'installation d'application » depuis le Microsoft Store, ou installez Virtual Display Driver depuis github.com/VirtualDrivers/Virtual-Display-Driver.".to_string())?;
        // Le pilote crée son tube quelques secondes après l'installation.
        for _ in 0..20 {
            if vdd::present() {
                // Aucun écran tant que NOTHING CONNECT ne le demande.
                let _ = vdd::command("SETDISPLAYCOUNT 0");
                return Ok("ok".into());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if out.status.success() {
            Ok("redemarrage".into())
        } else {
            Err(format!(
                "winget n'a pas pu installer le pilote : {}",
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .last()
                    .unwrap_or("")
                    .trim()
            ))
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
            // 0 = fréquence maximale annoncée par le téléphone.
            self.fps.store(
                if fps == 0 { 0 } else { fps.clamp(5, 240) },
                Ordering::SeqCst,
            );
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
            let own = interface_path().is_some();
            let vdd_installed = vdd::present();
            DisplayStatus {
                supported: true,
                installed: own || vdd_installed,
                testsigning,
                active_peer: a.as_ref().map(|a| a.peer.clone()),
                width: a.as_ref().map(|a| a.width).unwrap_or(0),
                height: a.as_ref().map(|a| a.height).unwrap_or(0),
                setup_available: exe.is_some(),
                vdd_installed,
                backend: if own {
                    Some("nothing-connect".into())
                } else if vdd_installed {
                    Some("vdd".into())
                } else {
                    None
                },
                codec: a.as_ref().map(|_| {
                    if self.stats.h264.load(Ordering::Relaxed) {
                        "h264"
                    } else {
                        "jpeg"
                    }
                    .into()
                }),
                fps: a
                    .as_ref()
                    .map(|_| self.stats.fps.load(Ordering::Relaxed))
                    .unwrap_or(0),
                kbps: a
                    .as_ref()
                    .map(|_| self.stats.kbps.load(Ordering::Relaxed))
                    .unwrap_or(0),
                refresh_hz: a
                    .as_ref()
                    .map(|_| self.stats.hz.load(Ordering::Relaxed))
                    .unwrap_or(0),
            }
        }

        /// Lance l'outil d'installation (`install`, `uninstall`, `enable-testsigning`).
        /// L'outil demande lui-même l'élévation (fenêtre UAC de Windows).
        pub fn setup(&self, app: &AppHandle, arg: &str) -> Result<String, String> {
            if arg == "install-vdd" {
                return install_vdd();
            }
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
            h264: bool,
        ) -> Result<(), String> {
            let backend = if interface_path().is_some() {
                Backend::Own
            } else if vdd::present() {
                Backend::Vdd
            } else {
                return Err("Aucun pilote d'écran installé sur ce PC (page Écran étendu).".into());
            };
            // Nouvelle annonce du téléphone (écran rouvert) : on relance la capture ; l'écran
            // précédent n'est retiré que s'il venait de l'autre pilote.
            if let Some(prev) = self.stop_thread() {
                if prev.backend != backend {
                    remove_monitor(prev.backend);
                }
            }
            let s = self.scale_pct.load(Ordering::SeqCst) as f32 / 100.0;
            let fit = |v: u32, max: u32| -> u32 { ((v as f32 * s) as u32).min(max) & !1 };
            let (mut w, mut h) = (fit(w, MAX_W as u32), fit(h, MAX_H as u32));
            // Respecte la limite du pilote en conservant les proportions.
            let k = (MAX_W as f32 / w as f32)
                .min(MAX_H as f32 / h as f32)
                .min(1.0);
            w = ((w as f32 * k) as u32).max(320) & !1;
            h = ((h as f32 * k) as u32).max(320) & !1;
            let hz = hz.clamp(30, 240);
            match backend {
                Backend::Own => {
                    let mut req = Vec::with_capacity(12);
                    req.extend_from_slice(&w.to_le_bytes());
                    req.extend_from_slice(&h.to_le_bytes());
                    req.extend_from_slice(&hz.to_le_bytes());
                    send_ioctl(IOCTL_ADD, &req)?;
                }
                Backend::Vdd => {
                    vdd::configure(w, h, hz, 1)?;
                    // Recharge le pilote avec un écran ; Windows l'ajoute en mode « Étendre ».
                    vdd::command("SETDISPLAYCOUNT 1")?;
                }
            }
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
                backend,
            });
            *self.rect.lock().unwrap() = None;
            // H.264 si le téléphone sait le décoder (version 0.3+), sinon JPEG image par image.
            let params = StreamParams {
                codec: Some(if h264 { "h264" } else { "jpeg" }.into()),
                width: Some(w),
                height: Some(h),
                refresh_hz: Some(hz),
                ..Default::default()
            };
            let _ = app.state::<AppState>().node.lock().unwrap().send(
                peer,
                Message::StreamStart {
                    kind: StreamKind::Display,
                    params,
                },
            );

            let (app2, peer2, force, quality, fps, stats) = (
                app.clone(),
                peer.to_string(),
                self.force_frame.clone(),
                self.quality.clone(),
                self.fps.clone(),
                self.stats.clone(),
            );
            stats.h264.store(h264, Ordering::Relaxed);
            stats.hz.store(hz, Ordering::Relaxed);
            stats.fps.store(0, Ordering::Relaxed);
            stats.kbps.store(0, Ordering::Relaxed);
            std::thread::Builder::new()
                .name("nc-display".into())
                .spawn(move || {
                    // Notre pilote crée la mémoire partagée quand Windows attribue la chaîne d'images ;
                    // avec VDD, on attend que l'écran apparaisse puis on règle sa définition.
                    let mut reader: Option<Source> = None;
                    let started = Instant::now();
                    let mut mode_set = false;
                    while reader.is_none() && !stop.load(Ordering::SeqCst) {
                        // Dès que l'écran existe : format exact du téléphone, quel que soit le pilote.
                        let adapter = match backend {
                            Backend::Own => ADAPTER_NAME,
                            Backend::Vdd => vdd::ADAPTER,
                        };
                        let name = display_device_name(adapter);
                        if let (Some(n), false) = (name.as_ref(), mode_set) {
                            if let Some(real) = ensure_mode(n, w, h, hz) {
                                stats.hz.store(real, Ordering::Relaxed);
                            }
                            mode_set = true;
                        }
                        reader = match backend {
                            Backend::Own => FrameReader::open().map(Source::Shared),
                            Backend::Vdd => {
                                name.and_then(|n| Duplicator::open(&n).map(|d| Source::Dup(d, n)))
                            }
                        };
                        if reader.is_none() {
                            if started.elapsed() > Duration::from_secs(20) {
                                tracing::warn!("écran étendu : aucune image du pilote");
                                return;
                            }
                            std::thread::sleep(Duration::from_millis(250));
                        }
                    }
                    let Some(mut reader) = reader else { return };
                    if !mode_set {
                        let adapter = if backend == Backend::Own {
                            ADAPTER_NAME
                        } else {
                            vdd::ADAPTER
                        };
                        if let Some(n) = display_device_name(adapter) {
                            if let Some(real) = ensure_mode(&n, w, h, hz) {
                                stats.hz.store(real, Ordering::Relaxed);
                            }
                        }
                    }
                    let mut bgra = Vec::new();
                    let mut jpeg = Vec::with_capacity(512 * 1024);
                    let mut enc: Option<crate::h264enc::H264> = None;
                    let mut enc_q = 0u8;
                    let mut last_sent = Instant::now() - Duration::from_secs(10);
                    let t0 = Instant::now();
                    // Mesures : images et octets envoyés pendant la dernière seconde.
                    let counters = std::cell::Cell::new((0u32, 0u64, Instant::now()));
                    let send = |flags: u8, data: &[u8]| -> bool {
                        let ts = t0.elapsed().as_micros() as u64;
                        let st = app2.state::<AppState>();
                        let ok = st.node.lock().unwrap().send_stream(
                            &peer2,
                            StreamKind::Display,
                            flags,
                            ts,
                            data,
                        );
                        if ok {
                            let (mut n, mut b, since) = counters.get();
                            b += data.len() as u64;
                            if flags & crate::h264enc::CONFIG == 0 {
                                n += 1;
                            }
                            let el = since.elapsed();
                            if el >= Duration::from_secs(1) {
                                stats.fps.store(
                                    (n as f64 / el.as_secs_f64()).round() as u32,
                                    Ordering::Relaxed,
                                );
                                stats.kbps.store(
                                    (b as f64 * 8.0 / 1000.0 / el.as_secs_f64()) as u32,
                                    Ordering::Relaxed,
                                );
                                counters.set((0, 0, Instant::now()));
                            } else {
                                counters.set((n, b, since));
                            }
                        }
                        ok
                    };
                    while !stop.load(Ordering::SeqCst) {
                        // Écran immobile : rien n'est envoyé, les mesures retombent à zéro.
                        if counters.get().2.elapsed() > Duration::from_millis(1500) {
                            stats.fps.store(0, Ordering::Relaxed);
                            stats.kbps.store(0, Ordering::Relaxed);
                            counters.set((0, 0, Instant::now()));
                        }
                        // 0 = fréquence maximale du téléphone.
                        let f = match fps.load(Ordering::Relaxed) {
                            0 => hz,
                            f => f,
                        };
                        let interval = Duration::from_micros(1_000_000 / f.max(1) as u64);
                        // JPEG : image de rappel toutes les 2 s. H.264 : image clé seulement sur demande.
                        let asked = force.swap(false, Ordering::SeqCst);
                        let forced =
                            asked || (!h264 && last_sent.elapsed() > Duration::from_secs(2));
                        let Some((w, h)) =
                            reader.next(&mut bgra, Duration::from_millis(100), forced)
                        else {
                            continue;
                        };
                        if last_sent.elapsed() < interval {
                            std::thread::sleep(interval - last_sent.elapsed());
                        }
                        let q = quality.load(Ordering::Relaxed);
                        if !h264 {
                            if encode_jpeg(&bgra, w, h, q, &mut jpeg).is_ok()
                                && send(nc_core::protocol::stream_flags::KEYFRAME, &jpeg)
                            {
                                last_sent = Instant::now();
                            }
                            continue;
                        }
                        // (Re)création de l'encodeur si la définition, la qualité ou la fréquence change.
                        let size = crate::h264enc::H264::even(w, h);
                        if enc.as_ref().map(|e| e.size() != size).unwrap_or(true) || enc_q != q {
                            let rate = crate::h264enc::bitrate_for(size.0, size.1, f, q);
                            match crate::h264enc::H264::new(size.0, size.1, f, rate) {
                                Ok(e) => {
                                    enc = Some(e);
                                    enc_q = q;
                                    tracing::info!(
                                        "écran étendu : H.264 {}×{} à {f} i/s, {} kbit/s",
                                        size.0,
                                        size.1,
                                        rate / 1000
                                    );
                                    let st = app2.state::<AppState>();
                                    let _ = st.node.lock().unwrap().send(
                                        &peer2,
                                        Message::StreamStart {
                                            kind: StreamKind::Display,
                                            params: StreamParams {
                                                codec: Some("h264".into()),
                                                width: Some(size.0),
                                                height: Some(size.1),
                                                refresh_hz: Some(hz),
                                                ..Default::default()
                                            },
                                        },
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!("{e}");
                                    std::thread::sleep(Duration::from_millis(500));
                                    continue;
                                }
                            }
                        }
                        let Some(e) = enc.as_mut() else { continue };
                        if asked {
                            e.force_keyframe();
                        }
                        match e.encode_bgra(&bgra, w) {
                            Ok(packets) => {
                                let mut ok = true;
                                for (flags, data) in &packets {
                                    if !send(*flags, data) {
                                        ok = false;
                                        break;
                                    }
                                }
                                if ok {
                                    last_sent = Instant::now();
                                } else {
                                    // Image perdue (réseau saturé) : la suivante repart d'une image clé.
                                    e.force_keyframe();
                                }
                            }
                            Err(err) => tracing::warn!("{err}"),
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
                crate::wininput::release_all();
                remove_monitor(a.backend);
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

        pub fn set_touch(&self, touch: bool) {
            if !touch {
                crate::wininput::release_all();
            }
            self.touch.store(touch, Ordering::SeqCst);
        }

        /// Geste, touche ou texte venant du téléphone.
        pub fn input(&self, action: &nc_core::protocol::DisplayAction) {
            if self.active.lock().unwrap().is_none() {
                return;
            }
            match action {
                nc_core::protocol::DisplayAction::Key { key } => crate::wininput::key(*key),
                nc_core::protocol::DisplayAction::Text { text } => crate::wininput::text(text),
            }
        }

        /// Toucher du téléphone → écran tactile Windows (gestes natifs) ou souris.
        pub fn pointer(&self, action: PointerAction, id: u32, x: f32, y: f32) {
            let adapter = match self.active.lock().unwrap().as_ref().map(|a| a.backend) {
                Some(Backend::Own) => ADAPTER_NAME,
                Some(Backend::Vdd) => vdd::ADAPTER,
                None => return,
            };
            let rect = {
                let mut r = self.rect.lock().unwrap();
                // Recalculé à chaque appui : la définition ou la disposition a pu changer.
                if r.is_none() || action == PointerAction::Down {
                    *r = monitor_rect(adapter);
                }
                *r
            };
            let Some(mon) = rect else { return };
            if self.touch.load(Ordering::SeqCst) {
                // Pixels physiques du moniteur virtuel (application consciente du DPI).
                let px = mon.0 + (x.clamp(0.0, 1.0) * (mon.2 - 1).max(0) as f32).round() as i32;
                let py = mon.1 + (y.clamp(0.0, 1.0) * (mon.3 - 1).max(0) as f32).round() as i32;
                if crate::wininput::touch(action, id, px, py) {
                    return;
                }
                // Injection tactile refusée par Windows : souris.
                self.touch.store(false, Ordering::SeqCst);
            }
            {
                let mut f = self.mouse_finger.lock().unwrap();
                match action {
                    PointerAction::Down if f.is_none() => *f = Some(id),
                    _ if *f != Some(id) => return,
                    PointerAction::Up | PointerAction::Cancel => *f = None,
                    _ => {}
                }
            }
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
    fn vdd_settings_edit() {
        let xml = "<vdd_settings>\n    <monitors>\n        <count>1</count>\n    </monitors>\n    <resolutions>\n        <resolution>\n            <width>1920</width>\n            <height>1080</height>\n            <refresh_rate>60</refresh_rate>\n        </resolution>\n    </resolutions>\n</vdd_settings>";
        let x = set_xml_count(xml, 0);
        assert!(x.contains("<count>0</count>"));
        let x = ensure_xml_resolution(&x, 2400, 1080, 60);
        assert!(x.contains("<width>2400</width>"));
        assert_eq!(x.matches("<resolution>").count(), 2);
        // Idempotent : pas de doublon.
        assert_eq!(ensure_xml_resolution(&x, 2400, 1080, 60), x);
        assert_eq!(ensure_xml_resolution(&x, 1920, 1080, 60), x);
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
