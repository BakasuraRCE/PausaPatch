//! Aplicación de los cambios de protección.
//!
//! Todo se ejecuta en el propio proceso elevado (manifiesto `requireAdministrator`):
//! servicios con la API SCM, política con el registro, DLL con el sistema de
//! archivos, tareas con `schtasks.exe` y limpieza con `std::fs`. No hay scripts
//! ni proceso auxiliar.

use std::ffi::c_void;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows_registry::LOCAL_MACHINE;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
    OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_ACTION, SC_ACTION_NONE,
    SC_ACTION_RESTART, SC_MANAGER_CONNECT, SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG,
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_CONTROL_STOP, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_FAILURE_ACTIONSW, SERVICE_NO_CHANGE, SERVICE_QUERY_STATUS,
    SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED,
};

use crate::log;
use crate::paths;
use crate::win;

const SERVICE_NAMES: [&str; 4] = ["wuauserv", "UsoSvc", "uhssvc", "WaaSMedicSvc"];
const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";
const SYSTEM32: &str = "System32";

/// Prefijos de tareas programadas que gestiona PausaPatch.
const TASK_PREFIXES: [&str; 6] = [
    r"\Microsoft\Windows\InstallService",
    r"\Microsoft\Windows\UpdateOrchestrator",
    r"\Microsoft\Windows\UpdateAssistant",
    r"\Microsoft\Windows\WaaSMedic",
    r"\Microsoft\Windows\WindowsUpdate",
    r"\Microsoft\WindowsUpdate",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pause,
    Resume,
}

pub const APPLY_ARGUMENT: &str = "--apply";
pub const RESULT_ARGUMENT: &str = "--result";

/// Nombre de la tarea temporal que ejecuta la operación como SYSTEM.
const SYSTEM_TASK: &str = "PausaPatch-Apply";
/// Marca que el proceso SYSTEM deja al terminar.
const DONE_MARKER: &str = "__done:";
/// Marca de tarea que no se pudo cambiar (para reintentarla como administrador).
const FAILED_MARKER: &str = "__failed:";

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            _ => None,
        }
    }
}

/// Lanza la operación como SYSTEM mediante una tarea programada temporal.
///
/// Las tareas de UpdateOrchestrator/WaaSMedic solo pueden modificarse desde
/// SYSTEM, así que el proceso elevado crea una tarea `/RU SYSTEM` que ejecuta
/// este mismo binario con `--apply`, espera a que termine y la elimina.
pub fn apply(action: Action) -> Result<(), String> {
    paths::ensure_dirs().map_err(|error| error.to_string())?;
    let result = paths::app_log_path();

    log::log(format!("=== {} start ===", action.as_str()));
    // Todo lo que escriba el proceso SYSTEM a partir de aquí pertenece a esta
    // ejecución; así no confundimos marcas de operaciones anteriores.
    let offset = file_len(&result);

    let exe = paths::exe_path();
    let action_line = format!(
        "\"{}\" {} {} {} \"{}\"",
        exe.display(),
        APPLY_ARGUMENT,
        action.as_str(),
        RESULT_ARGUMENT,
        result.display()
    );

    create_task(&action_line)?;
    log::log(format!("SYSTEM task created: {action_line}"));
    let outcome = run_and_wait(action, &result, offset);
    delete_task();
    retry_failed(action, &result, offset);
    match &outcome {
        Ok(()) => log::log(format!("=== {} end: ok ===", action.as_str())),
        Err(error) => log::error(format!("=== {} end: error: {error} ===", action.as_str())),
    }
    outcome
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Contenido añadido al registro desde `offset`.
fn read_since(path: &Path, offset: u64) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return String::new();
    }
    let mut text = String::new();
    let _ = file.read_to_string(&mut text);
    text
}

fn create_task(action_line: &str) -> Result<(), String> {
    let (ok, _, stderr) = win::run_hidden(
        "schtasks.exe",
        &[
            "/Create",
            "/TN",
            SYSTEM_TASK,
            "/TR",
            action_line,
            "/SC",
            "ONCE",
            "/ST",
            "00:00",
            "/RU",
            "SYSTEM",
            "/RL",
            "HIGHEST",
            "/F",
        ],
    )?;
    if ok {
        Ok(())
    } else {
        Err(format!("Could not create the SYSTEM task: {}", stderr.trim()))
    }
}

fn delete_task() {
    let _ = win::run_hidden("schtasks.exe", &["/Delete", "/TN", SYSTEM_TASK, "/F"]);
}

fn run_and_wait(action: Action, result: &Path, offset: u64) -> Result<(), String> {
    let (ok, _, stderr) = win::run_hidden("schtasks.exe", &["/Run", "/TN", SYSTEM_TASK])?;
    if !ok {
        return Err(format!("Could not start the SYSTEM task: {}", stderr.trim()));
    }
    let started = Instant::now();
    loop {
        if let Some(outcome) = read_outcome(result, offset) {
            return outcome;
        }
        let elapsed = started.elapsed();
        if elapsed > Duration::from_secs(300) {
            return Err(format!("Operation {} exceeded 300 s.", action.as_str()));
        }
        // Si la tarea ni siquiera llegó a escribir, algo la bloqueó.
        if elapsed > Duration::from_secs(15) && !result.exists() {
            return Err(
                "The SYSTEM task did not run (blocked by task policy?)".to_string(),
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// Reintenta como administrador las tareas que SYSTEM no pudo cambiar.
fn retry_failed(action: Action, result: &Path, offset: u64) {
    let text = read_since(result, offset);
    let failed: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_once(FAILED_MARKER).map(|(_, name)| name.trim()))
        .filter(|name| !name.is_empty())
        .collect();
    if failed.is_empty() {
        return;
    }
    let switch = if action == Action::Pause {
        "/DISABLE"
    } else {
        "/ENABLE"
    };
    let Ok(mut report) = Report::open(result) else {
        return;
    };
    report.line(format!(
        "retrying {} task(s) as administrator",
        failed.len()
    ));
    let mut recovered = 0usize;
    for name in &failed {
        match win::run_hidden("schtasks.exe", &["/Change", "/TN", name, switch]) {
            Ok((true, _, _)) => {
                recovered += 1;
                report.line(format!("  retry succeeded: {name}"));
            }
            Ok((false, _, stderr)) => {
                report.line(format!("  retry failed: {name}: {}", stderr.trim()))
            }
            Err(error) => report.line(format!("  retry failed: {name}: {error}")),
        }
    }
    report.line(format!(
        "retry: {recovered} of {} succeeded",
        failed.len()
    ));
}

/// Lee la marca final que deja el proceso SYSTEM.
fn read_outcome(result: &Path, offset: u64) -> Option<Result<(), String>> {
    let text = read_since(result, offset);
    let marker = text
        .lines()
        .rev()
        .find(|line| line.contains(DONE_MARKER))?;
    let value = marker.split_once(DONE_MARKER)?.1.trim();
    if value == "ok" {
        Some(Ok(()))
    } else {
        Some(Err(value.replace("error:", "").trim().to_string()))
    }
}

/// Ejecuta la operación dentro del proceso SYSTEM. Devuelve el código de salida.
pub fn run(action: Action, result: PathBuf) -> i32 {
    let mut report = match Report::create(&result) {
        Ok(report) => report,
        Err(_) => return 1,
    };
    report.line(format!("pausapatch --apply {} start", action.as_str()));
    let outcome = match action {
        Action::Pause => pause(&mut report),
        Action::Resume => resume(&mut report),
    };
    match outcome {
        Ok(()) => {
            report.line(format!("{DONE_MARKER} ok"));
            0
        }
        Err(error) => {
            report.line(format!("{DONE_MARKER} error: {error}"));
            1
        }
    }
}

struct Report {
    path: PathBuf,
}

impl Report {
    fn create(path: &Path) -> Result<Self, String> {
        Self::open(path)
    }

    fn open(path: &Path) -> Result<Self, String> {
        if !path.exists() && std::fs::File::create(path).is_err() {
            return Err("Could not open the operation log.".to_string());
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    fn line(&mut self, message: impl AsRef<str>) {
        log::append(&self.path, "OP", message);
    }
}

fn pause(report: &mut Report) -> Result<(), String> {
    let manager = ScManager::open()?;
    for name in SERVICE_NAMES {
        match set_start(manager.0, name, SERVICE_DISABLED, false) {
            Ok(true) => report.line(format!("service {name}: startup disabled")),
            Ok(false) => {
                report.line(format!("service {name}: not present on this machine"));
                continue;
            }
            Err(error) => report.line(format!("service {name}: warning: {error}")),
        }
        if let Err(error) = set_no_failure_actions(manager.0, name) {
            report.line(format!("  warning: {error}"));
        }
    }
    for name in SERVICE_NAMES {
        match stop_service(manager.0, name) {
            Ok(true) => report.line(format!("service {name}: stopped")),
            Ok(false) => {}
            Err(error) => report.line(format!("service {name}: {error}")),
        }
    }
    drop(manager);

    rename_dll(report, "WaaSMedicSvc", true)?;
    rename_dll(report, "wuaueng", true)?;

    report.line("policy NoAutoUpdate=1");
    match windows_registry::LOCAL_MACHINE.create(POLICY_KEY) {
        Ok(key) => {
            if let Err(error) = key.set_u32("NoAutoUpdate", 1) {
                report.line(format!("  warning: could not write ({error})"));
            }
        }
        Err(error) => report.line(format!("  warning: could not open the key ({error})")),
    }
    report.line(format!(
        "  NoAutoUpdate read back: {}",
        read_policy()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "absent".to_string())
    ));

    let distribution = paths::system_dir().join("SoftwareDistribution");
    report.line(format!("removing {}", distribution.display()));
    if let Err(error) = std::fs::remove_dir_all(&distribution)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        report.line(format!("  warning: {error}"));
    }

    set_tasks(report, false)
}

fn resume(report: &mut Report) -> Result<(), String> {
    let manager = ScManager::open()?;
    let starts: [(&str, u32, bool); 4] = [
        ("wuauserv", SERVICE_AUTO_START, false),
        ("UsoSvc", SERVICE_AUTO_START, false),
        ("uhssvc", SERVICE_AUTO_START, true),
        ("WaaSMedicSvc", SERVICE_DEMAND_START, false),
    ];
    for (name, start, delayed) in starts {
        match set_start(manager.0, name, start, delayed) {
            Ok(true) => report.line(format!("service {name}: startup restored")),
            Ok(false) => report.line(format!("service {name}: not present on this machine")),
            Err(error) => report.line(format!("service {name}: warning: {error}")),
        }
    }
    if let Err(error) = restore_failure_actions(manager.0, "WaaSMedicSvc") {
        report.line(format!("  warning: {error}"));
    }
    drop(manager);

    rename_dll(report, "WaaSMedicSvc", false)?;
    rename_dll(report, "wuaueng", false)?;

    report.line("removing NoAutoUpdate policy");
    // `open` abre solo en lectura; para borrar hace falta acceso de escritura.
    match LOCAL_MACHINE.options().read().write().open(POLICY_KEY) {
        Ok(key) => {
            if let Err(error) = key.remove_value("NoAutoUpdate") {
                // Las claves de directiva no siempre permiten borrar el valor
                // (acceso denegado incluso para SYSTEM); 0 es equivalente.
                report.line(format!(
                    "  could not delete ({error}); writing NoAutoUpdate=0"
                ));
                if let Err(error) = key.set_u32("NoAutoUpdate", 0) {
                    report.line(format!("  warning: could not write 0 either ({error})"));
                }
            }
        }
        Err(_) => report.line("  policy key does not exist"),
    }
    report.line(format!(
        "  NoAutoUpdate read back: {}",
        read_policy()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "absent".to_string())
    ));

    set_tasks(report, true)
}

fn read_policy() -> Option<u32> {
    LOCAL_MACHINE
        .open(POLICY_KEY)
        .ok()
        .and_then(|key| key.get_u32("NoAutoUpdate").ok())
}

// --- Servicios -------------------------------------------------------------

struct ScManager(windows_sys::Win32::System::Services::SC_HANDLE);

impl ScManager {
    fn open() -> Result<Self, String> {
        let handle = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
        if handle.is_null() {
            return Err(format!("could not open the service manager ({})", last_error()));
        }
        Ok(Self(handle))
    }
}

impl Drop for ScManager {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

/// `Ok(None)` significa que el servicio no existe en este equipo.
fn open_service(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
    access: u32,
) -> Result<Option<windows_sys::Win32::System::Services::SC_HANDLE>, String> {
    let name = wide(name);
    let handle = unsafe { OpenServiceW(manager, name.as_ptr(), access) };
    if handle.is_null() {
        let code = last_error();
        // 1060 = ERROR_SERVICE_DOES_NOT_EXIST (uhssvc no está en todas las builds).
        if code == 1060 {
            return Ok(None);
        }
        return Err(format!("OpenService failed ({code})"));
    }
    Ok(Some(handle))
}

/// `Ok(false)` significa que el servicio no existe en este equipo.
fn set_start(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
    start: u32,
    delayed: bool,
) -> Result<bool, String> {
    let Some(service) = open_service(manager, name, SERVICE_CHANGE_CONFIG)? else {
        return Ok(false);
    };
    let changed = unsafe {
        ChangeServiceConfigW(
            service,
            SERVICE_NO_CHANGE,
            start,
            SERVICE_NO_CHANGE,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if changed == 0 {
        let code = last_error();
        unsafe { CloseServiceHandle(service) };
        // El servicio puede tener una ACL que ni SYSTEM puede cambiar; el
        // script original escribía el valor Start directamente en el registro.
        return set_start_registry(name, start)
            .map(|()| true)
            .map_err(|error| format!("ChangeServiceConfig failed ({code}); registry: {error}"));
    }
    let delayed_info = SERVICE_DELAYED_AUTO_START_INFO {
        fDelayedAutostart: if delayed { 1 } else { 0 },
    };
    let result = unsafe {
        ChangeServiceConfig2W(
            service,
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            &delayed_info as *const _ as *const c_void,
        )
    };
    unsafe { CloseServiceHandle(service) };
    if result == 0 && delayed {
        return Err(format!("delayed start not applied ({})", last_error()));
    }
    Ok(true)
}

fn set_start_registry(name: &str, start: u32) -> Result<(), String> {
    let path = format!(r"SYSTEM\CurrentControlSet\Services\{name}");
    let key = windows_registry::LOCAL_MACHINE
        .create(&path)
        .map_err(|error| error.to_string())?;
    key.set_u32("Start", start).map_err(|error| error.to_string())
}

fn set_no_failure_actions(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
) -> Result<(), String> {
    apply_failure_actions(manager, name, 0, &[])
}

fn restore_failure_actions(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
) -> Result<(), String> {
    let actions = [
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 120_000,
        },
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 300_000,
        },
        SC_ACTION {
            Type: SC_ACTION_NONE,
            Delay: 0,
        },
    ];
    apply_failure_actions(manager, name, 900, &actions)
}

fn apply_failure_actions(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
    reset: u32,
    actions: &[SC_ACTION],
) -> Result<(), String> {
    let Some(service) = open_service(manager, name, SERVICE_CHANGE_CONFIG)? else {
        return Ok(());
    };
    let mut info = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: reset,
        lpRebootMsg: std::ptr::null_mut(),
        lpCommand: std::ptr::null_mut(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_ptr() as *mut SC_ACTION,
    };
    let result = unsafe {
        ChangeServiceConfig2W(
            service,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            &mut info as *mut _ as *const c_void,
        )
    };
    let code = if result == 0 { last_error() } else { 0 };
    unsafe { CloseServiceHandle(service) };
    if result == 0 {
        // Algunos servicios (WaaSMedicSvc) tienen una ACL que ni SYSTEM puede
        // modificar por API; el valor serializado del registro sí suele admitirlo.
        return set_failure_actions_registry(name, reset, actions)
            .map_err(|error| format!("ChangeServiceConfig2 failed ({code}); registry: {error}"));
    }
    Ok(())
}

fn failure_actions_bytes(reset: u32, actions: &[SC_ACTION]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16 + actions.len() * 8);
    bytes.extend_from_slice(&reset.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lpRebootMsg vacío
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lpCommand vacío
    bytes.extend_from_slice(&(actions.len() as u32).to_le_bytes());
    for action in actions {
        bytes.extend_from_slice(&(action.Type as u32).to_le_bytes());
        bytes.extend_from_slice(&action.Delay.to_le_bytes());
    }
    bytes
}

fn set_failure_actions_registry(
    name: &str,
    reset: u32,
    actions: &[SC_ACTION],
) -> Result<(), String> {
    let path = format!(r"SYSTEM\CurrentControlSet\Services\{name}");
    let key = windows_registry::LOCAL_MACHINE
        .create(&path)
        .map_err(|error| error.to_string())?;
    key.set_bytes(
        "FailureActions",
        windows_registry::Type::Bytes,
        &failure_actions_bytes(reset, actions),
    )
    .map_err(|error| error.to_string())
}

/// `Ok(false)` significa que el servicio no existe en este equipo.
fn stop_service(
    manager: windows_sys::Win32::System::Services::SC_HANDLE,
    name: &str,
) -> Result<bool, String> {
    let Some(service) = open_service(manager, name, SERVICE_STOP | SERVICE_QUERY_STATUS)? else {
        return Ok(false);
    };
    let mut status = SERVICE_STATUS::default();
    let controlled = unsafe { ControlService(service, SERVICE_CONTROL_STOP, &mut status) };
    if controlled == 0 {
        // 1062 = el servicio no está iniciado; 1060 = no existe.
        let code = last_error();
        unsafe { CloseServiceHandle(service) };
        if code == 1062 || code == 1060 {
            return Ok(true);
        }
        return Err(format!("could not stop ({code})"));
    }
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        let mut status = SERVICE_STATUS::default();
        if unsafe { QueryServiceStatus(service, &mut status) } != 0
            && status.dwCurrentState == SERVICE_STOPPED
        {
            unsafe { CloseServiceHandle(service) };
            return Ok(true);
        }
    }
    unsafe { CloseServiceHandle(service) };
    Err("did not stop within 10 s".to_string())
}

// --- DLLs ------------------------------------------------------------------

fn rename_dll(report: &mut Report, base: &str, to_backup: bool) -> Result<(), String> {
    let system32 = paths::system_dir().join(SYSTEM32);
    let original = system32.join(format!("{base}.dll"));
    let backup = system32.join(format!("{base}_BAK.dll"));

    if to_backup {
        if backup.exists() {
            report.line(format!("{base}: already renamed"));
            return Ok(());
        }
        if !original.exists() {
            report.line(format!("{base}: not found"));
            return Ok(());
        }
        take_ownership(&original, report);
        std::fs::rename(&original, &backup)
            .map_err(|error| format!("could not rename {base}: {error}"))?;
        lock_down(&backup, report);
        report.line(format!("{base}: renamed to _BAK"));
    } else {
        if !backup.exists() {
            report.line(format!("{base}: no backup found"));
            return Ok(());
        }
        if original.exists() {
            report.line(format!("{base}: original already exists"));
            return Ok(());
        }
        take_ownership(&backup, report);
        std::fs::rename(&backup, &original)
            .map_err(|error| format!("could not restore {base}: {error}"))?;
        lock_down(&original, report);
        report.line(format!("{base}: restored"));
    }
    Ok(())
}

fn take_ownership(path: &Path, report: &mut Report) {
    let _ = win::run_hidden("takeown.exe", &["/f", &path.to_string_lossy()]);
    let _ = win::run_hidden(
        "icacls.exe",
        &[&path.to_string_lossy(), "/grant", "*S-1-1-0:F"],
    );
    report.line(format!("  permissions adjusted: {}", path.display()));
}

fn lock_down(path: &Path, report: &mut Report) {
    let _ = win::run_hidden(
        "icacls.exe",
        &[
            &path.to_string_lossy(),
            "/setowner",
            r"NT SERVICE\TrustedInstaller",
        ],
    );
    let _ = win::run_hidden("icacls.exe", &[&path.to_string_lossy(), "/remove", "*S-1-1-0"]);
    report.line(format!("  ownership restored: {}", path.display()));
}

// --- Tareas programadas ----------------------------------------------------

fn set_tasks(report: &mut Report, enable: bool) -> Result<(), String> {
    let raw = task_names().unwrap_or_default();
    if raw.is_empty() {
        report.line("tasks: no matches found");
        return Ok(());
    }
    let mut names: Vec<String> = Vec::new();
    for name in &raw {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    report.line(format!(
        "tasks: {} rows returned by schtasks, {} unique",
        raw.len(),
        names.len()
    ));
    for name in &raw {
        let repeated = raw.iter().filter(|other| *other == name).count();
        if repeated > 1 {
            report.line(format!(
                "  · {name}  (repeated {repeated} times in the enumeration)"
            ));
        } else {
            report.line(format!("  · {name}"));
        }
    }
    let switch = if enable { "/ENABLE" } else { "/DISABLE" };
    let mut failed: Vec<&str> = Vec::new();
    for name in &names {
        match win::run_hidden("schtasks.exe", &["/Change", "/TN", name, switch]) {
            Ok((true, _, _)) => {}
            Ok((false, _, _)) => failed.push(name),
            Err(error) => {
                report.line(format!("  task {name}: {error}"));
                failed.push(name);
            }
        }
    }
    if failed.is_empty() {
        report.line(format!(
            "tasks: {} {}",
            names.len(),
            if enable { "enabled" } else { "disabled" }
        ));
    } else {
        report.line(format!(
            "tasks: {} {} · {} unchanged",
            names.len() - failed.len(),
            if enable { "enabled" } else { "disabled" },
            failed.len()
        ));
        let mut seen: Vec<&str> = Vec::new();
        for name in failed {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            report.line(format!("  could not change: {name}"));
            report.line(format!("{FAILED_MARKER} {name}"));
        }
    }
    Ok(())
}

fn task_names() -> Result<Vec<String>, String> {
    let (ok, stdout, stderr) = win::run_hidden("schtasks.exe", &["/Query", "/FO", "CSV", "/NH"])?;
    if !ok {
        return Err(format!("schtasks failed: {}", stderr.trim()));
    }
    let mut names = Vec::new();
    for line in stdout.lines() {
        let Some(name) = first_csv_field(line) else {
            continue;
        };
        if TASK_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
            names.push(name);
        }
    }
    Ok(names)
}

fn first_csv_field(line: &str) -> Option<String> {
    let mut chars = line.trim_start().strip_prefix('"')?.chars().peekable();
    let mut out = String::new();
    while let Some(character) = chars.next() {
        if character == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
                out.push('"');
            } else {
                break;
            }
        } else {
            out.push(character);
        }
    }
    Some(out)
}

// --- Utilidades ------------------------------------------------------------

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error() -> u32 {
    unsafe { GetLastError() }
}
