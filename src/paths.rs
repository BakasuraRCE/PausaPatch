use std::path::PathBuf;

pub const APP_NAME: &str = "PausaPatch";

pub fn local_app_data() -> PathBuf {
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(value);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile).join("AppData").join("Local");
    }
    std::env::temp_dir()
}

pub fn app_dir() -> PathBuf {
    local_app_data().join(APP_NAME)
}

pub fn system_dir() -> PathBuf {
    PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows")),
    )
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

pub fn app_log_path() -> PathBuf {
    app_dir().join("pausapatch.log")
}

pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(app_dir())
}

pub fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("pausapatch.exe"))
}
