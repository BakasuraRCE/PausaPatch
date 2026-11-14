use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Option<String>,
    pub last_change: Option<i64>,
    pub last_overall: Option<String>,
    pub poll_seconds: Option<u64>,
}

impl Settings {
    pub fn load() -> Self {
        let Ok(data) = std::fs::read_to_string(paths::settings_path()) else {
            return Self::default();
        };
        serde_json::from_str(&data).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        paths::ensure_dirs()?;
        let data = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string());
        std::fs::write(paths::settings_path(), data)
    }
}
