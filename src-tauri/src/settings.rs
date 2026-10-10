//! Configurações persistidas em `%APPDATA%\LocalMeetingAI\config\settings.json`.
//! Escritas são atômicas (arquivo temporário + rename) para não corromper o arquivo.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::hardware::Profile;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub onboarding_completed: bool,
    pub onboarding_step: u32,
    pub profile: Option<Profile>,
    pub transcription_model: Option<String>,
    pub intelligence_model: Option<String>,
    pub recordings_dir: Option<PathBuf>,
    pub models_dir: Option<PathBuf>,
    pub microphone_id: Option<String>,
    pub capture_system_audio: bool,
}

const FILE_NAME: &str = "settings.json";

impl Settings {
    pub fn load(config_dir: &Path) -> Self {
        let path = config_dir.join(FILE_NAME);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                // Arquivo ilegível: guarda cópia para diagnóstico e começa limpo.
                tracing::error!("settings.json inválido: {e}");
                let _ = std::fs::rename(&path, config_dir.join("settings.invalido.json"));
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, config_dir: &Path) -> Result<(), String> {
        let path = config_dir.join(FILE_NAME);
        let tmp = config_dir.join("settings.json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| format!("Não foi possível salvar as configurações: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("Não foi possível salvar as configurações: {e}"))
    }
}
