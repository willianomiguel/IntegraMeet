//! Diretórios do aplicativo. Nada é espalhado fora destas pastas.
//!
//! - config:   %APPDATA%\LocalMeetingAI\config
//! - banco:    %APPDATA%\LocalMeetingAI\data
//! - modelos:  %LOCALAPPDATA%\LocalMeetingAI\models
//! - cache:    %LOCALAPPDATA%\LocalMeetingAI\cache
//! - logs:     %LOCALAPPDATA%\LocalMeetingAI\logs
//! - gravações: Documentos\LocalMeetingAI\gravacoes (alterável nas configurações)

use std::fs;
use std::path::PathBuf;

const APP_DIR: &str = "LocalMeetingAI";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub data_db: PathBuf,
    pub models: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
    pub recordings: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, String> {
        let roaming = dirs::config_dir().ok_or("Pasta de dados do usuário não encontrada")?;
        let local = dirs::data_local_dir().ok_or("Pasta local do usuário não encontrada")?;
        let documents = dirs::document_dir().ok_or("Pasta Documentos não encontrada")?;

        let roaming_root = roaming.join(APP_DIR);
        let local_root = local.join(APP_DIR);

        Ok(Self {
            config: roaming_root.join("config"),
            data_db: roaming_root.join("data").join("meetings.db"),
            data: roaming_root.join("data"),
            models: local_root.join("models"),
            cache: local_root.join("cache"),
            logs: local_root.join("logs"),
            recordings: documents.join(APP_DIR).join("gravacoes"),
        })
    }

    /// Cria todas as pastas necessárias. Falha com mensagem clara se o disco não permitir.
    pub fn ensure_exist(&self) -> Result<(), String> {
        for dir in [
            &self.config,
            &self.data,
            &self.models,
            &self.cache,
            &self.logs,
            &self.recordings,
        ] {
            fs::create_dir_all(dir).map_err(|e| {
                format!("Não foi possível criar a pasta {}: {e}", dir.display())
            })?;
        }
        Ok(())
    }
}
