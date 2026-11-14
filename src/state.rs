use std::path::PathBuf;
use std::time::Duration;

use windows_registry::LOCAL_MACHINE;

use crate::i18n::Lang;
use crate::win;

const SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services";
const POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overall {
    Paused,
    Active,
    Mixed,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemState {
    pub wuauserv: Option<u32>,
    pub uso: Option<u32>,
    pub uhssvc: Option<u32>,
    pub medic: Option<u32>,
    pub policy: Option<u32>,
    pub tasks_total: u32,
    pub tasks_disabled: u32,
    pub medic_dll_renamed: bool,
    pub wuaueng_dll_renamed: bool,
    pub checked_at: i64,
    pub overall: Overall,
}

impl SystemState {
    pub fn is_fully_paused(&self) -> bool {
        self.wuauserv == Some(4)
            && self.uso == Some(4)
            && self.medic == Some(4)
            && self.policy == Some(1)
            && self.medic_dll_renamed
            && self.wuaueng_dll_renamed
            && (self.tasks_total == 0 || self.tasks_disabled >= self.tasks_total)
    }
}

pub fn detect() -> Result<SystemState, String> {
    let wuauserv = read_start("wuauserv");
    if wuauserv.is_none() {
        return Err("Could not read the wuauserv service.".to_string());
    }
    let uso = read_start("UsoSvc");
    let uhssvc = read_start("uhssvc");
    let medic = read_start("WaaSMedicSvc");
    let policy = read_policy();
    let (tasks_total, tasks_disabled) = task_counts().unwrap_or((0, 0));
    let medic_dll_renamed = dll_renamed("WaaSMedicSvc");
    let wuaueng_dll_renamed = dll_renamed("wuaueng");

    let active = wuauserv != Some(4) || policy != Some(1);

    let mut state = SystemState {
        wuauserv,
        uso,
        uhssvc,
        medic,
        policy,
        tasks_total,
        tasks_disabled,
        medic_dll_renamed,
        wuaueng_dll_renamed,
        checked_at: chrono::Local::now().timestamp(),
        overall: Overall::Unknown,
    };
    state.overall = if state.is_fully_paused() {
        Overall::Paused
    } else if active {
        Overall::Active
    } else {
        Overall::Mixed
    };
    Ok(state)
}

fn read_start(name: &str) -> Option<u32> {
    LOCAL_MACHINE
        .open(format!(r"{SERVICE_KEY}\{name}"))
        .ok()
        .and_then(|key| key.get_u32("Start").ok())
}

fn read_policy() -> Option<u32> {
    LOCAL_MACHINE
        .open(POLICY_KEY)
        .ok()
        .and_then(|key| key.get_u32("NoAutoUpdate").ok())
}

fn system32() -> PathBuf {
    PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows")),
    )
    .join("System32")
}

fn dll_renamed(base: &str) -> bool {
    let dir = system32();
    let original = dir.join(format!("{base}.dll"));
    let backup = dir.join(format!("{base}_BAK.dll"));
    backup.exists() && !original.exists()
}

fn task_counts() -> Option<(u32, u32)> {
    match schtasks_task_counts() {
        Some(counts) => Some(counts),
        None => {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                crate::log::log(
                    "Task detection: schtasks returned no data; falling back to PowerShell.",
                );
            });
            powershell_task_counts()
        }
    }
}

/// Enumera las tareas con `schtasks /Query /FO CSV` (el mismo criterio que usa
/// la acción) y consulta el estado de cada una con `/XML`, cuyo `<Enabled>` no
/// depende del idioma del sistema.
fn schtasks_task_counts() -> Option<(u32, u32)> {
    let names = task_names()?;
    if names.is_empty() {
        return None;
    }
    let mut total = 0u32;
    let mut disabled = 0u32;
    for group in names.chunks(8) {
        let states: Vec<bool> = std::thread::scope(|scope| {
            let handles: Vec<_> = group
                .iter()
                .map(|name| scope.spawn(move || task_is_disabled(name)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or(false))
                .collect()
        });
        for state in states {
            total += 1;
            if state {
                disabled += 1;
            }
        }
    }
    Some((total, disabled))
}

/// Evita cifras transitorias: si la enumeración devuelve menos tareas que la
/// última vez (p. ej. mientras la acción las está cambiando), reintenta una vez.
fn task_names() -> Option<Vec<String>> {
    static LAST_TOTAL: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
    let mut names = query_task_names()?;
    let previous = *LAST_TOTAL.lock().unwrap_or_else(|error| error.into_inner());
    if names.len() < previous {
        std::thread::sleep(Duration::from_millis(300));
        if let Some(retry) = query_task_names()
            && retry.len() > names.len()
        {
            names = retry;
        }
    }
    *LAST_TOTAL.lock().unwrap_or_else(|error| error.into_inner()) = names.len();
    Some(names)
}

fn query_task_names() -> Option<Vec<String>> {
    let prefixes = [
        r"\Microsoft\Windows\InstallService",
        r"\Microsoft\Windows\UpdateOrchestrator",
        r"\Microsoft\Windows\UpdateAssistant",
        r"\Microsoft\Windows\WaaSMedic",
        r"\Microsoft\Windows\WindowsUpdate",
        r"\Microsoft\WindowsUpdate",
    ];
    let (ok, stdout, _) = win::run_hidden("schtasks.exe", &["/Query", "/FO", "CSV", "/NH"]).ok()?;
    if !ok {
        return None;
    }
    let mut names = Vec::new();
    for line in stdout.lines() {
        let Some(name) = first_csv_field(line) else {
            continue;
        };
        if prefixes
            .iter()
            .any(|prefix| name.to_lowercase().starts_with(&prefix.to_lowercase()))
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    Some(names)
}

fn task_is_disabled(name: &str) -> bool {
    let Ok((true, xml, _)) = win::run_hidden("schtasks.exe", &["/Query", "/TN", name, "/XML"])
    else {
        return false;
    };
    let settings = tag_text(&xml, "Settings").unwrap_or_default();
    tag_text(&settings, "Enabled").as_deref() == Some("false")
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

fn tag_text(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)? + start;
    Some(block[start..end].trim().to_string())
}

fn powershell_task_counts() -> Option<(u32, u32)> {
    let script = r#"
$prefixes = @(
  '\Microsoft\Windows\InstallService',
  '\Microsoft\Windows\UpdateOrchestrator',
  '\Microsoft\Windows\UpdateAssistant',
  '\Microsoft\Windows\WaaSMedic',
  '\Microsoft\Windows\WindowsUpdate',
  '\Microsoft\WindowsUpdate'
)
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {
  $full = $_.TaskPath + $_.TaskName
  $match = $false
  foreach ($prefix in $prefixes) {
    if ($full.StartsWith($prefix, 'OrdinalIgnoreCase')) { $match = $true; break }
  }
  $match
})
$disabled = @($tasks | Where-Object { $_.State -eq 'Disabled' }).Count
"$($tasks.Count)|$disabled"
"#;
    let (ok, stdout, _) = win::run_hidden(
        "powershell.exe",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )
    .ok()?;
    if !ok {
        return None;
    }
    let (total, disabled) = stdout.trim().split_once('|')?;
    Some((total.trim().parse().ok()?, disabled.trim().parse().ok()?))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Reactivated,
    Paused,
    Mixed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChangeDetail {
    Service {
        name: &'static str,
        from: Option<u32>,
        to: Option<u32>,
    },
    Policy {
        from: Option<u32>,
        to: Option<u32>,
    },
    Dll {
        name: &'static str,
        from_renamed: bool,
        to_renamed: bool,
    },
    Generic,
}

impl ChangeDetail {
    pub fn describe(&self, lang: Lang) -> String {
        match self {
            Self::Service { name, from, to } => format!(
                "{}   {} → {}",
                name,
                lang.service_state_label(*from),
                lang.service_state_label(*to)
            ),
            Self::Policy { from, to } => {
                let label = |value: &Option<u32>| {
                    value
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "ausente".to_string())
                };
                format!("NoAutoUpdate   {} → {}", label(from), label(to))
            }
            Self::Dll {
                name,
                from_renamed,
                to_renamed,
            } => format!(
                "{}   {} → {}",
                name,
                if *from_renamed { "_BAK" } else { "original" },
                if *to_renamed { "_BAK" } else { "original" }
            ),
            Self::Generic => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NoticeData {
    pub kind: NoticeKind,
    pub detail: ChangeDetail,
    pub at: i64,
}

impl NoticeData {
    pub fn detail_line(&self, lang: Lang) -> String {
        self.detail.describe(lang)
    }
}

pub fn change_detail(old: &SystemState, new: &SystemState) -> ChangeDetail {
    for (name, from, to) in [
        ("wuauserv", old.wuauserv, new.wuauserv),
        ("UsoSvc", old.uso, new.uso),
        ("WaaSMedicSvc", old.medic, new.medic),
        ("uhssvc", old.uhssvc, new.uhssvc),
    ] {
        if from != to {
            return ChangeDetail::Service { name, from, to };
        }
    }
    if old.policy != new.policy {
        return ChangeDetail::Policy {
            from: old.policy,
            to: new.policy,
        };
    }
    if old.medic_dll_renamed != new.medic_dll_renamed {
        return ChangeDetail::Dll {
            name: "WaaSMedicSvc.dll",
            from_renamed: old.medic_dll_renamed,
            to_renamed: new.medic_dll_renamed,
        };
    }
    if old.wuaueng_dll_renamed != new.wuaueng_dll_renamed {
        return ChangeDetail::Dll {
            name: "wuaueng.dll",
            from_renamed: old.wuaueng_dll_renamed,
            to_renamed: new.wuaueng_dll_renamed,
        };
    }
    ChangeDetail::Generic
}
