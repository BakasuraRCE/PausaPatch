//! Icono del área de notificación (bandeja del sistema).
//!
//! Reactor solo ofrece `NotifyIcon` en el modo `Application`, que no permite
//! temporizadores ni trabajo en segundo plano desde el componente raíz. Como la
//! vigilancia periódica debe seguir viva con la ventana oculta, la bandeja se
//! implementa aquí con Win32: un hilo propio mantiene una ventana oculta y su
//! bucle de mensajes, y publica comandos en una cola que la interfaz consume.

use std::collections::VecDeque;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, IMAGE_ICON, LR_DEFAULTSIZE, LoadImageW, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    WM_APP, WM_COMMAND, WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
    WS_OVERLAPPED,
};

const CALLBACK_MESSAGE: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
const CLASS_NAME: &str = "PausaPatchTrayWindow";

const MENU_SHOW: u32 = 1;
const MENU_PAUSE: u32 = 2;
const MENU_RESUME: u32 = 3;
const MENU_EXIT: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Show,
    Pause,
    Resume,
    Exit,
}

#[derive(Clone, Debug)]
pub struct TrayState {
    pub show: String,
    pub pause: String,
    pub resume: String,
    pub exit: String,
    pub tooltip: String,
    /// `Some(true)` protegido, `Some(false)` activo, `None` desconocido.
    pub paused: Option<bool>,
}

static QUEUE: OnceLock<Mutex<VecDeque<Command>>> = OnceLock::new();
static STATE: OnceLock<Mutex<TrayState>> = OnceLock::new();
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
static ICON_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ICON_ON: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ICON_OFF: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static STARTED: AtomicBool = AtomicBool::new(false);

fn queue() -> &'static Mutex<VecDeque<Command>> {
    QUEUE.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn state() -> &'static Mutex<TrayState> {
    STATE.get_or_init(|| {
        Mutex::new(TrayState {
            show: "Show".to_string(),
            pause: "Pause".to_string(),
            resume: "Resume".to_string(),
            exit: "Exit".to_string(),
            tooltip: "PausaPatch".to_string(),
            paused: None,
        })
    })
}

fn lock_state() -> MutexGuard<'static, TrayState> {
    state().lock().unwrap_or_else(|error| error.into_inner())
}

/// Indica si el icono ya está creado en el área de notificación.
pub fn is_available() -> bool {
    TRAY_HWND.load(Ordering::Relaxed) != 0
}

pub fn drain() -> Vec<Command> {
    let mut pending = queue().lock().unwrap_or_else(|error| error.into_inner());
    pending.drain(..).collect()
}

pub fn set_state(next: TrayState) {
    let tooltip = next.tooltip.clone();
    let icon = state_icon(&next);
    *lock_state() = next;
    let hwnd = TRAY_HWND.load(Ordering::Relaxed) as HWND;
    if !hwnd.is_null() {
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: ICON_ID,
                uFlags: NIF_TIP | NIF_ICON,
                hIcon: icon,
                ..Default::default()
            };
            fill_tip(&mut data.szTip, &tooltip);
            Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }
}

/// Icono verde con la protección activa y ámbar si está desactivada.
fn state_icon(state: &TrayState) -> *mut c_void {
    let handle = if state.paused == Some(true) {
        ICON_ON.load(Ordering::Relaxed)
    } else {
        ICON_OFF.load(Ordering::Relaxed)
    };
    if handle.is_null() {
        ICON_HANDLE.load(Ordering::Relaxed)
    } else {
        handle
    }
}

/// Arranca el hilo de la bandeja. Las llamadas posteriores no hacen nada.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::Builder::new()
        .name("pausapatch-tray".to_string())
        .spawn(|| unsafe { run() })
        .ok();
}

unsafe fn run() {
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = wide(CLASS_NAME);
    let title = wide("PausaPatch");
    let window_class = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
    };
    unsafe { RegisterClassW(&window_class) };
    TASKBAR_CREATED.store(
        unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) },
        Ordering::Relaxed,
    );

    let hwnd = unsafe { CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    ) };
    if hwnd.is_null() {
        return;
    }
    TRAY_HWND.store(hwnd as isize, Ordering::Relaxed);

    // Iconos embebidos en el .exe: id 1 = protección activa, id 2 = inactiva.
    let on = unsafe { load_icon(instance, 1) };
    let off = unsafe { load_icon(instance, 2) };
    ICON_ON.store(on, Ordering::Relaxed);
    ICON_OFF.store(off, Ordering::Relaxed);

    let current = {
        let state = lock_state().clone();
        state_icon(&state)
    };
    ICON_HANDLE.store(current, Ordering::Relaxed);
    unsafe { add_icon(hwnd, current) };

    let mut message: MSG = unsafe { std::mem::zeroed() };
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe fn load_icon(instance: *mut c_void, id: u16) -> *mut c_void {
    let resource = std::ptr::without_provenance::<u16>(id as usize);
    unsafe { LoadImageW(instance, resource, IMAGE_ICON, 0, 0, LR_DEFAULTSIZE) }
}

unsafe fn add_icon(hwnd: HWND, icon: *mut c_void) {
    let tooltip = lock_state().tooltip.clone();
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: CALLBACK_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    fill_tip(&mut data.szTip, &tooltip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &data) };
}

unsafe fn show_menu(hwnd: HWND) {
    let labels = lock_state().clone();
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let pause_flag = if labels.paused == Some(true) {
        MF_STRING | MF_GRAYED
    } else {
        MF_STRING
    };
    let resume_flag = if labels.paused == Some(false) {
        MF_STRING | MF_GRAYED
    } else {
        MF_STRING
    };
    let show = wide(&labels.show);
    let pause = wide(&labels.pause);
    let resume = wide(&labels.resume);
    let exit = wide(&labels.exit);
    unsafe {
        AppendMenuW(menu, MF_STRING, MENU_SHOW as usize, show.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, pause_flag, MENU_PAUSE as usize, pause.as_ptr());
        AppendMenuW(menu, resume_flag, MENU_RESUME as usize, resume.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_EXIT as usize, exit.as_ptr());
    }

    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            point.x,
            point.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}

fn push(command: Command) {
    queue()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push_back(command);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar_created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if taskbar_created != 0 && message == taskbar_created {
        let icon = state_icon(&lock_state().clone());
        ICON_HANDLE.store(icon, Ordering::Relaxed);
        unsafe { add_icon(hwnd, icon) };
        return 0;
    }
    if message == CALLBACK_MESSAGE {
        match lparam as u32 {
            // Solo doble clic: en una secuencia de doble clic también llega
            // WM_LBUTTONUP y alternaría la ventana dos veces.
            WM_LBUTTONDBLCLK => push(Command::Toggle),
            WM_RBUTTONUP => unsafe { show_menu(hwnd) },
            _ => {}
        }
        return 0;
    }
    if message == WM_COMMAND {
        match (wparam & 0xFFFF) as u32 {
            MENU_SHOW => push(Command::Show),
            MENU_PAUSE => push(Command::Pause),
            MENU_RESUME => push(Command::Resume),
            MENU_EXIT => push(Command::Exit),
            _ => {}
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fill_tip(buffer: &mut [u16; 128], value: &str) {
    let encoded: Vec<u16> = value.encode_utf16().take(buffer.len() - 1).collect();
    for slot in buffer.iter_mut() {
        *slot = 0;
    }
    buffer[..encoded.len()].copy_from_slice(&encoded);
}
