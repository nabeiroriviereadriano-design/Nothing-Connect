//! Saisie injectée dans Windows depuis l'écran étendu : toucher multipoint (Windows le traite
//! comme un vrai écran tactile : défilement, zoom, appui long, gestes), touches, texte et
//! clavier tactile de Windows.
#![cfg(windows)]

use nc_core::protocol::{DisplayKey, PointerAction};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK,
    VK_BROWSER_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_LWIN, VK_MENU,
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::Input::Pointer::{
    InitializeTouchInjection, InjectTouchInput, POINTER_FLAGS, POINTER_FLAG_CANCELED,
    POINTER_FLAG_DOWN, POINTER_FLAG_INCONTACT, POINTER_FLAG_INRANGE, POINTER_FLAG_UP,
    POINTER_FLAG_UPDATE, POINTER_TOUCH_INFO, TOUCH_FEEDBACK_DEFAULT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PT_TOUCH, TOUCH_FLAG_NONE, TOUCH_MASK_CONTACTAREA, TOUCH_MASK_ORIENTATION, TOUCH_MASK_PRESSURE,
};

const MAX_CONTACTS: usize = 10;

struct Contact {
    phone_id: u32,
    slot: u32,
    x: i32,
    y: i32,
}

#[derive(Default)]
struct Touch {
    ready: Option<bool>,
    contacts: Vec<Contact>,
    last: Option<Instant>,
}

fn state() -> &'static Mutex<Touch> {
    static S: OnceLock<Mutex<Touch>> = OnceLock::new();
    S.get_or_init(|| {
        // Windows annule un contact qui n'est pas mis à jour régulièrement : un doigt
        // immobile (appui long) est réinjecté toutes les 100 ms.
        std::thread::Builder::new()
            .name("nc-touch".into())
            .spawn(|| loop {
                std::thread::sleep(Duration::from_millis(100));
                let mut t = state().lock().unwrap();
                if !t.contacts.is_empty()
                    && t.last
                        .map(|l| l.elapsed() > Duration::from_millis(90))
                        .unwrap_or(true)
                {
                    let frame: Vec<_> = t.contacts.iter().map(|c| info(c, update())).collect();
                    inject(&mut t, &frame);
                }
            })
            .ok();
        Mutex::new(Touch::default())
    })
}

fn update() -> POINTER_FLAGS {
    POINTER_FLAG_UPDATE | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT
}

fn info(c: &Contact, flags: POINTER_FLAGS) -> POINTER_TOUCH_INFO {
    let mut t = POINTER_TOUCH_INFO::default();
    t.pointerInfo.pointerType = PT_TOUCH;
    t.pointerInfo.pointerId = c.slot;
    t.pointerInfo.ptPixelLocation = POINT { x: c.x, y: c.y };
    t.pointerInfo.pointerFlags = flags;
    t.touchFlags = TOUCH_FLAG_NONE;
    t.touchMask = TOUCH_MASK_CONTACTAREA | TOUCH_MASK_ORIENTATION | TOUCH_MASK_PRESSURE;
    t.rcContact = RECT {
        left: c.x - 2,
        top: c.y - 2,
        right: c.x + 2,
        bottom: c.y + 2,
    };
    t.orientation = 90;
    t.pressure = 32000;
    t
}

fn inject(t: &mut Touch, frame: &[POINTER_TOUCH_INFO]) {
    t.last = Some(Instant::now());
    if let Err(e) = unsafe { InjectTouchInput(frame) } {
        tracing::debug!("toucher refusé par Windows : {e}");
    }
}

/// Toucher multipoint injecté. `false` si Windows refuse l'injection (repli souris).
pub fn touch(action: PointerAction, phone_id: u32, x: i32, y: i32) -> bool {
    let mut t = state().lock().unwrap();
    if t.ready.is_none() {
        let ok = unsafe { InitializeTouchInjection(MAX_CONTACTS as u32, TOUCH_FEEDBACK_DEFAULT) }
            .is_ok();
        if !ok {
            tracing::warn!("injection tactile indisponible : retour au mode souris");
        }
        t.ready = Some(ok);
    }
    if t.ready != Some(true) {
        return false;
    }
    match action {
        PointerAction::Down => {
            if t.contacts.len() >= MAX_CONTACTS || t.contacts.iter().any(|c| c.phone_id == phone_id)
            {
                return true;
            }
            let slot = (0..MAX_CONTACTS as u32)
                .find(|s| t.contacts.iter().all(|c| c.slot != *s))
                .unwrap_or(0);
            t.contacts.push(Contact {
                phone_id,
                slot,
                x,
                y,
            });
            let frame: Vec<_> = t
                .contacts
                .iter()
                .map(|c| {
                    if c.phone_id == phone_id {
                        info(
                            c,
                            POINTER_FLAG_DOWN | POINTER_FLAG_INRANGE | POINTER_FLAG_INCONTACT,
                        )
                    } else {
                        info(c, update())
                    }
                })
                .collect();
            inject(&mut t, &frame);
        }
        PointerAction::Move => {
            let Some(c) = t.contacts.iter_mut().find(|c| c.phone_id == phone_id) else {
                return true;
            };
            if (c.x, c.y) == (x, y) {
                return true;
            }
            c.x = x;
            c.y = y;
            let frame: Vec<_> = t.contacts.iter().map(|c| info(c, update())).collect();
            inject(&mut t, &frame);
        }
        PointerAction::Up => {
            let Some(i) = t.contacts.iter().position(|c| c.phone_id == phone_id) else {
                return true;
            };
            let mut lifted = t.contacts.remove(i);
            lifted.x = x;
            lifted.y = y;
            let mut frame: Vec<_> = t.contacts.iter().map(|c| info(c, update())).collect();
            frame.push(info(&lifted, POINTER_FLAG_UP));
            inject(&mut t, &frame);
        }
        PointerAction::Cancel => cancel_locked(&mut t),
    }
    true
}

fn cancel_locked(t: &mut Touch) {
    if t.contacts.is_empty() {
        return;
    }
    let frame: Vec<_> = t
        .contacts
        .iter()
        .map(|c| info(c, POINTER_FLAG_UP | POINTER_FLAG_CANCELED))
        .collect();
    t.contacts.clear();
    inject(t, &frame);
}

/// Lève tous les doigts (fin de session, téléphone déconnecté).
pub fn release_all() {
    if let Some(m) = state().lock().ok().as_deref_mut() {
        cancel_locked(m);
    }
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let extended = matches!(
        vk,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_DELETE | VK_BROWSER_BACK | VK_LWIN
    );
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Combinaison : touches enfoncées dans l'ordre, relâchées dans l'ordre inverse.
fn combo(keys: &[VIRTUAL_KEY]) {
    let mut v: Vec<INPUT> = keys.iter().map(|k| key_input(*k, false)).collect();
    v.extend(keys.iter().rev().map(|k| key_input(*k, true)));
    unsafe {
        SendInput(&v, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn key(k: DisplayKey) {
    match k {
        DisplayKey::Back => combo(&[VK_BROWSER_BACK]),
        DisplayKey::TaskView => combo(&[VK_LWIN, VK_TAB]),
        DisplayKey::ShowDesktop => combo(&[VK_LWIN, VIRTUAL_KEY(b'D' as u16)]),
        DisplayKey::NextApp => combo(&[VK_MENU, VK_TAB]),
        DisplayKey::PrevApp => combo(&[VK_MENU, VK_SHIFT, VK_TAB]),
        DisplayKey::NextDesktop => combo(&[VK_LWIN, VK_CONTROL, VK_RIGHT]),
        DisplayKey::PrevDesktop => combo(&[VK_LWIN, VK_CONTROL, VK_LEFT]),
        DisplayKey::TouchKeyboard => {
            // Appel COM qui peut attendre le démarrage du clavier : hors du fil réseau.
            std::thread::spawn(toggle_touch_keyboard);
        }
        DisplayKey::Backspace => combo(&[VK_BACK]),
        DisplayKey::Delete => combo(&[VK_DELETE]),
        DisplayKey::Enter => combo(&[VK_RETURN]),
        DisplayKey::Tab => combo(&[VK_TAB]),
        DisplayKey::Escape => combo(&[VK_ESCAPE]),
        DisplayKey::Left => combo(&[VK_LEFT]),
        DisplayKey::Right => combo(&[VK_RIGHT]),
        DisplayKey::Up => combo(&[VK_UP]),
        DisplayKey::Down => combo(&[VK_DOWN]),
        DisplayKey::Unknown => {}
    }
}

/// Texte tapé sur le clavier du téléphone → fenêtre active du PC (caractères Unicode).
pub fn text(s: &str) {
    let mut v = Vec::new();
    for ch in s.chars() {
        if ch == '\n' {
            v.push(key_input(VK_RETURN, false));
            v.push(key_input(VK_RETURN, true));
            continue;
        }
        let mut buf = [0u16; 2];
        for unit in ch.encode_utf16(&mut buf) {
            for up in [false, true] {
                v.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(0),
                            wScan: *unit,
                            dwFlags: if up {
                                KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                            } else {
                                KEYEVENTF_UNICODE
                            },
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                });
            }
        }
    }
    if !v.is_empty() {
        unsafe {
            SendInput(&v, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

/// Affiche ou masque le clavier tactile de Windows (interface ITipInvocation de TabTip).
fn toggle_touch_keyboard() {
    use windows::core::{IUnknown, Interface, GUID, HRESULT};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;
    const CLSID_UIHOSTNOLAUNCH: GUID = GUID::from_u128(0x4ce576fa_83dc_4f88_951c_9d0782b4e376);
    const IID_ITIPINVOCATION: GUID = GUID::from_u128(0x37c994e7_432b_4834_a2f7_dce1f13b834b);
    type Toggle = unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> HRESULT;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        for attempt in 0..2 {
            let created: windows::core::Result<IUnknown> = CoCreateInstance(
                &CLSID_UIHOSTNOLAUNCH,
                None::<&IUnknown>,
                CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER,
            );
            if let Ok(unk) = created {
                let mut p: *mut std::ffi::c_void = std::ptr::null_mut();
                if unk.query(&IID_ITIPINVOCATION, &mut p).is_ok() && !p.is_null() {
                    // Table virtuelle : QueryInterface, AddRef, Release, Toggle.
                    let vtbl = *(p as *const *const usize);
                    let toggle: Toggle = std::mem::transmute(*vtbl.add(3));
                    let _ = toggle(p, GetDesktopWindow());
                    let release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32 =
                        std::mem::transmute(*vtbl.add(2));
                    release(p);
                    return;
                }
            }
            if attempt == 0 {
                // Clavier tactile pas encore lancé : on démarre son processus puis on réessaie.
                let tabtip = std::path::Path::new(
                    &std::env::var("CommonProgramFiles")
                        .unwrap_or_else(|_| r"C:\Program Files\Common Files".into()),
                )
                .join(r"microsoft shared\ink\TabTip.exe");
                let _ = std::process::Command::new(tabtip).spawn();
                std::thread::sleep(Duration::from_millis(700));
            }
        }
        tracing::warn!("clavier tactile de Windows indisponible");
    }
}
