use std::ffi::c_void;
use std::path::Path;
use std::mem::size_of;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows_registry::CURRENT_USER;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::core::BOOL;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, RDW_ALLCHILDREN,
    RDW_INVALIDATE, RedrawWindow,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken, TerminateProcess};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, DefWindowProcW, FindWindowW, GWL_WNDPROC, GetCursorPos, GetWindowLongPtrW,
    EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    PostQuitMessage, SW_HIDE, SW_RESTORE, SW_SHOWNOACTIVATE, SPI_GETWORKAREA,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW,
    WM_CLOSE, WM_NCDESTROY, WM_SYSCOMMAND, SC_MAXIMIZE, WNDPROC,
};

use crate::paths;

const GWL_STYLE: i32 = -16;
const GWLP_HWNDPARENT: i32 = -8;
const GWL_EXSTYLE: i32 = -20;

static ORIGINAL_WND_PROC: AtomicIsize = AtomicIsize::new(0);
static MODAL_HWND: AtomicIsize = AtomicIsize::new(0);
/// Procedimiento original por ventana subclasada (registro y notificación
/// pueden coexistir, así que no basta un único hueco).
static DIALOG_PROCS: std::sync::Mutex<Vec<(isize, isize)>> = std::sync::Mutex::new(Vec::new());
static ALLOW_CLOSE: AtomicBool = AtomicBool::new(false);
static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);

const WS_CAPTION: u32 = 0x00c0_0000;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
const WS_BORDER: u32 = 0x0080_0000;
const WS_DLGFRAME: u32 = 0x0040_0000;
const WS_POPUP: u32 = 0x8000_0000;

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;

const HWND_TOPMOST: HWND = -1isize as HWND;

const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Área de trabajo del monitor donde está el cursor, que es el que el usuario
/// está usando (no necesariamente el primario).
pub fn active_work_area() -> Bounds {
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return work_area();
        }
        Bounds {
            x: info.rcWork.left,
            y: info.rcWork.top,
            width: info.rcWork.right - info.rcWork.left,
            height: info.rcWork.bottom - info.rcWork.top,
        }
    }
}

/// Posición centrada para una ventana del tamaño indicado (en DIPs).
pub fn centered_position(width_dip: f64, height_dip: f64) -> (i32, i32) {
    let scale = dpi_scale();
    let width = (width_dip * scale).round() as i32;
    let height = (height_dip * scale).round() as i32;
    let area = active_work_area();
    (
        area.x + (area.width - width).max(0) / 2,
        area.y + (area.height - height).max(0) / 2,
    )
}

/// Oculta la ventana en cuanto recibe `WM_CLOSE`, antes de que WinUI libere
/// su contenido. Evita el fotograma negro durante el desmontaje.
pub fn hide_on_close(hwnd: HWND) {
    unsafe {
        let mut procs = DIALOG_PROCS.lock().unwrap_or_else(|error| error.into_inner());
        if procs.iter().any(|(window, _)| *window == hwnd as isize) {
            return;
        }
        let previous = GetWindowLongPtrW(hwnd, GWL_WNDPROC);
        procs.push((hwnd as isize, previous));
        drop(procs);
        SetWindowLongPtrW(hwnd, GWL_WNDPROC, dialog_window_proc as *const () as usize as isize);
    }
}

fn original_dialog_proc(hwnd: HWND) -> isize {
    DIALOG_PROCS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(window, _)| *window == hwnd as isize)
        .map(|(_, proc)| *proc)
        .unwrap_or(0)
}

fn forget_dialog_proc(hwnd: HWND) {
    DIALOG_PROCS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .retain(|(window, _)| *window != hwnd as isize);
}

unsafe extern "system" fn dialog_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLOSE {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
    let previous: WNDPROC = unsafe { std::mem::transmute(original_dialog_proc(hwnd)) };
    let result = match previous {
        Some(_) => unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) },
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    };
    if message == WM_NCDESTROY {
        forget_dialog_proc(hwnd);
    }
    result
}

/// Alterna la ventana principal: si está visible la oculta, y si está oculta
/// la restaura. Si hay un diálogo modal abierto (el registro), lo cierra.
pub fn toggle_window(hwnd: HWND) {
    let modal = MODAL_HWND.load(Ordering::SeqCst) as HWND;
    unsafe {
        if IsWindowVisible(hwnd) != 0 {
            // Se oculta la principal y después se cierra el diálogo modal:
            // al restaurar desde la bandeja solo aparecerá la principal.
            ShowWindow(hwnd, SW_HIDE);
            if !modal.is_null() {
                PostMessageW(modal, WM_CLOSE, 0, 0);
            }
        } else {
            restore_window(hwnd);
        }
    }
}

/// Centra una ventana ya creada en el monitor activo.
pub fn center_on_active_monitor(hwnd: HWND) {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return;
        }
        let area = active_work_area();
        let width = (rect.right - rect.left).max(1);
        let height = (rect.bottom - rect.top).max(1);
        let x = area.x + (area.width - width).max(0) / 2;
        let y = area.y + (area.height - height).max(0) / 2;
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn work_area() -> Bounds {
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut c_void, 0);
        Bounds {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        }
    }
}

pub fn dpi_scale() -> f64 {
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    dpi as f64 / 96.0
}

pub fn popup_bounds(width_dip: f64, height_dip: f64, margin_dip: f64) -> Bounds {
    let scale = dpi_scale();
    let width = (width_dip * scale).round() as i32;
    let height = (height_dip * scale).round() as i32;
    let margin = (margin_dip * scale).round() as i32;
    let area = work_area();
    Bounds {
        x: area.x + area.width - width - margin,
        y: area.y + area.height - height - margin,
        width,
        height,
    }
}

pub fn configure_popup(hwnd: HWND, bounds: Bounds) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let style = (style
            & !(WS_CAPTION
                | WS_THICKFRAME
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_BORDER
                | WS_DLGFRAME))
            | WS_POPUP;
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let ex_style = (ex_style | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style as isize);

        let dark = 1i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const c_void,
            size_of::<u32>() as u32,
        );

        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

pub fn hide_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn restore_window(hwnd: HWND) {
    unsafe {
        // Windows recorta las posiciones totalmente fuera de pantalla, así que
        // al volver de la bandeja se recoloca centrada en el monitor activo.
        center_on_active_monitor(hwnd);
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

/// Hace que el botón de cerrar oculte la ventana en la bandeja en lugar de
/// terminar la aplicación, para que la vigilancia siga activa.
/// Convierte `dialog` en una ventana modal de `owner_title`: la centra sobre
/// ella, la deshabilita mientras esté abierta y quita minimizar/maximizar.
pub fn make_modal(dialog: HWND, owner_title: &str) {
    MODAL_HWND.store(dialog as isize, Ordering::SeqCst);
    let title: Vec<u16> = owner_title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let owner = FindWindowW(std::ptr::null(), title.as_ptr());
        if !owner.is_null() {
            SetWindowLongPtrW(dialog, GWLP_HWNDPARENT, owner as isize);

            let mut owner_rect: RECT = std::mem::zeroed();
            let mut rect: RECT = std::mem::zeroed();
            if GetWindowRect(owner, &mut owner_rect) != 0
                && GetWindowRect(dialog, &mut rect) != 0
            {
                let width = (rect.right - rect.left).max(1);
                let height = (rect.bottom - rect.top).max(1);
                let x = owner_rect.left + ((owner_rect.right - owner_rect.left) - width) / 2;
                let y = owner_rect.top + ((owner_rect.bottom - owner_rect.top) - height) / 2;
                SetWindowPos(
                    dialog,
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            EnableWindow(owner, 0);
        }

        let style = GetWindowLongPtrW(dialog, GWL_STYLE) as u32;
        let style = style & !(WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME);
        SetWindowLongPtrW(dialog, GWL_STYLE, style as isize);
        SetWindowPos(
            dialog,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Termina la aplicación: cierra el bucle de mensajes y garantiza que el
/// proceso no quede vivo aunque queden ventanas secundarias abiertas.
pub fn quit_application(owner_title: &str) {
    release_modal(owner_title);
    unsafe {
        // Ocultar antes de desmontar evita que se vea una ventana en negro
        // mientras Reactor destruye los componentes.
        EnumWindows(Some(hide_own_window), 0);
        PostQuitMessage(0);
    }
    // Red de seguridad: si el bucle no atendiera WM_QUIT, el proceso muere igual.
    // Se usa TerminateProcess y no `std::process::exit`: este último dispara los
    // atexit y el DLL_PROCESS_DETACH, y si WinUI aún está desmontando provoca un
    // fail-fast (0xc0000602 en Microsoft.UI.Input.dll).
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(3));
        unsafe {
            TerminateProcess(GetCurrentProcess(), 0);
        }
    });
}

unsafe extern "system" fn hide_own_window(hwnd: HWND, _param: LPARAM) -> BOOL {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() && IsWindowVisible(hwnd) != 0 {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
    1
}

/// Reactiva la ventana que quedó deshabilitada por `make_modal`.
pub fn release_modal(owner_title: &str) {
    MODAL_HWND.store(0, Ordering::SeqCst);
    let title: Vec<u16> = owner_title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let owner = FindWindowW(std::ptr::null(), title.as_ptr());
        if !owner.is_null() {
            EnableWindow(owner, 1);
            // Una ventana deshabilitada puede quedar sin pintar; se fuerza el
            // repintado completo antes de devolverle el foco.
            if IsWindowVisible(owner) != 0 {
                // Sin RDW_UPDATENOW: forzar el repintado aquí bloquea el
                // hilo de UI mientras la ventana que se cierra sigue visible,
                // y se ve un rectángulo negro.
                RedrawWindow(
                    owner,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    RDW_INVALIDATE | RDW_ALLCHILDREN,
                );
                SetForegroundWindow(owner);
            }
        }
    }
}

/// Ajusta la ventana principal: quita el botón de maximizar (y la capacidad
/// de hacerlo) y, si hay bandeja, hace que cerrar la oculte.
pub fn configure_main_window(hwnd: HWND, tray_available: bool) {
    TRAY_AVAILABLE.store(tray_available, Ordering::SeqCst);

    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let without_maximize = style & !WS_MAXIMIZEBOX;
        if without_maximize != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, without_maximize as isize);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }

        if ORIGINAL_WND_PROC.load(Ordering::SeqCst) == 0 {
            let previous = GetWindowLongPtrW(hwnd, GWL_WNDPROC);
            ORIGINAL_WND_PROC.store(previous, Ordering::SeqCst);
            SetWindowLongPtrW(hwnd, GWL_WNDPROC, tray_window_proc as *const () as usize as isize);
        }
    }
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Maximizar no está disponible: ni botón, ni doble clic en la barra.
    if message == WM_SYSCOMMAND && (wparam & 0xFFF0) as u32 == SC_MAXIMIZE {
        return 0;
    }
    if message == WM_CLOSE
        && !ALLOW_CLOSE.load(Ordering::SeqCst)
        && TRAY_AVAILABLE.load(Ordering::SeqCst)
    {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
        return 0;
    }
    let previous: WNDPROC = unsafe { std::mem::transmute(ORIGINAL_WND_PROC.load(Ordering::SeqCst)) };
    match previous {
        Some(_) => unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) },
        None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

const AUTOSTART_TASK: &str = "PausaPatch";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Una entrada en `Run` no puede arrancar un proceso que requiere UAC:
/// Windows la descarta al iniciar sesión. Por eso el arranque automático se
/// hace con una tarea programada con privilegios más altos.
pub fn is_autostart_enabled() -> bool {
    let task = run_hidden("schtasks.exe", &["/Query", "/TN", AUTOSTART_TASK])
        .map(|(ok, _, _)| ok)
        .unwrap_or(false);
    if task {
        return true;
    }
    // Compatibilidad con versiones que usaban el registro.
    CURRENT_USER
        .open(RUN_KEY)
        .and_then(|key| key.get_string("PausaPatch"))
        .is_ok()
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    // Elimina siempre la entrada antigua del registro; coexistir con la tarea
    // provocaría dos arranques.
    if let Ok(key) = CURRENT_USER.create(RUN_KEY) {
        let _ = key.remove_value("PausaPatch");
    }
    if enabled {
        create_autostart_task()
    } else {
        let _ = run_hidden("schtasks.exe", &["/Delete", "/TN", AUTOSTART_TASK, "/F"]);
        Ok(())
    }
}

fn create_autostart_task() -> Result<(), String> {
    let sid = current_user_sid().ok_or("Could not obtain the user SID.")?;
    let exe = escape_xml(&paths::exe_path().display().to_string());
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>PausaPatch</Description>
    <URI>\PausaPatch</URI>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{sid}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{sid}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--minimized</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    );

    // `schtasks /XML` espera un archivo Unicode.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let path = paths::app_dir().join("autostart.xml");
    std::fs::create_dir_all(paths::app_dir()).map_err(|error| error.to_string())?;
    std::fs::write(&path, &bytes).map_err(|error| error.to_string())?;

    let result = run_hidden(
        "schtasks.exe",
        &[
            "/Create",
            "/TN",
            AUTOSTART_TASK,
            "/XML",
            &path.to_string_lossy(),
            "/F",
        ],
    );
    let _ = std::fs::remove_file(&path);
    match result? {
        (true, _, _) => Ok(()),
        (false, _, stderr) => Err(format!(
            "Could not create the startup task: {}",
            stderr.trim()
        )),
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn current_user_sid() -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{TOKEN_USER, TokenUser};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut size = 0u32;
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
        if size == 0 {
            CloseHandle(token);
            return None;
        }
        let mut buffer: Vec<u64> = vec![0; (size as usize).div_ceil(8)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr() as *mut c_void,
            size,
            &mut size,
        );
        CloseHandle(token);
        if ok == 0 {
            return None;
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut raw = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut raw) == 0 {
            return None;
        }
        let length = (0usize..)
            .take_while(|index| *raw.add(*index) != 0)
            .count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(raw, length));
        LocalFree(raw as *mut c_void);
        Some(sid)
    }
}

/// Abre el Explorador con el archivo seleccionado.
pub fn reveal_in_explorer(path: &Path) {
    let argument = format!("/select,{}", path.display());
    let mut command = Command::new("explorer.exe");
    command.arg(argument);
    let _ = command.spawn();
}

/// Copia texto Unicode al portapapeles.
pub fn copy_text(text: &str) -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("Could not open the clipboard.".to_string());
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return Err("Could not empty the clipboard.".to_string());
        }

        let data: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = data.len() * std::mem::size_of::<u16>();
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if handle.is_null() {
            CloseClipboard();
            return Err("Could not allocate clipboard memory.".to_string());
        }
        let target = GlobalLock(handle) as *mut u16;
        if target.is_null() {
            GlobalFree(handle);
            CloseClipboard();
            return Err("Could not lock clipboard memory.".to_string());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), target, data.len());
        GlobalUnlock(handle);
        if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
            GlobalFree(handle);
            CloseClipboard();
            return Err("Could not write to the clipboard.".to_string());
        }
        CloseClipboard();
    }
    Ok(())
}

pub fn run_hidden(program: &str, args: &[&str]) -> Result<(bool, String, String), String> {
    let mut command = Command::new(program);
    command.args(args);
    no_window(&mut command);
    let output = command
        .output()
        .map_err(|error| format!("Could not run {program}: {error}"))?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

pub fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}
