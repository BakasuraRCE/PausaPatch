//! Registro de la aplicación.
//!
//! Un único archivo `pausapatch.log`, siempre en modo añadir. No rota ni se
//! limpia: conserva todo el histórico. Las operaciones de pausa/reactivación
//! se delimitan con cabeceras y cierre.

use std::path::Path;

use crate::paths;

/// Escribe una línea con el formato `fecha  NIVEL  mensaje`.
pub fn append(path: &Path, level: &str, message: impl AsRef<str>) {
    // Se crea el directorio del propio archivo: el proceso SYSTEM escribe en
    // la ruta del usuario, no en su propio %LOCALAPPDATA%.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{timestamp}  {level:<5}  {}", message.as_ref());
    }
}

pub fn log(message: impl AsRef<str>) {
    append(&paths::app_log_path(), "INFO", message);
}

pub fn error(message: impl AsRef<str>) {
    append(&paths::app_log_path(), "ERROR", message);
}

/// Devuelve las últimas `limit` líneas del registro.
pub fn read(limit: usize) -> Vec<String> {
    let Ok(data) = std::fs::read_to_string(paths::app_log_path()) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = data.lines().map(ToString::to_string).collect();
    if lines.len() > limit {
        lines = lines.split_off(lines.len() - limit);
    }
    lines
}
