//! Estado compartilhado entre comandos Tauri e o worker.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::capture::CaptureStream;
use crate::paths::AppPaths;
use crate::recording::Active;
use crate::runtime::llama::LlamaServer;
use crate::settings::Settings;
use crate::worker::WorkerShared;

pub struct AppState {
    pub paths: AppPaths,
    pub runtime_dir: PathBuf,
    pub settings: Arc<Mutex<Settings>>,
    pub llama: Arc<LlamaServer>,
    /// Downloads em andamento: id do modelo -> flag de cancelamento.
    pub downloads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub http: reqwest::Client,
    pub db: Arc<StdMutex<Connection>>,
    pub active: Mutex<Option<Active>>,
    /// Teste de áudio do onboarding: captura aberta e instante de início.
    pub preview: Mutex<Option<(CaptureStream, Instant)>>,
    pub worker: Arc<WorkerShared>,
}

impl AppState {
    pub fn llama_dir(&self) -> PathBuf {
        self.runtime_dir.join("llama")
    }

    pub fn whisper_dir(&self) -> PathBuf {
        self.runtime_dir.join("whisper")
    }

    /// Pasta de modelos: a escolhida pelo usuário ou a padrão.
    pub async fn models_root(&self) -> PathBuf {
        self.settings
            .lock()
            .await
            .models_dir
            .clone()
            .unwrap_or_else(|| self.paths.models.clone())
    }

    pub fn meetings_root(&self, settings: &Settings) -> PathBuf {
        settings
            .recordings_dir
            .clone()
            .unwrap_or_else(|| self.paths.recordings.clone())
    }
}
