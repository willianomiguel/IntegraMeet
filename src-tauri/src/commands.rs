//! Comandos chamados pelo frontend. Erros sempre em português e sem detalhes técnicos.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::hardware::{self, HardwareInfo, Profile};
use crate::models::catalog::{self, ModelKind, ModelSpec};
use crate::models::download::{self, DownloadProgress};
use crate::runtime::llama::ServerState;
use crate::settings::Settings;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    name: String,
    version: String,
    build_date: String,
    data_dir: String,
    platform: String,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: "IntegraMeet".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        build_date: option_env!("LMAI_BUILD_DATE").unwrap_or("desenvolvimento").into(),
        data_dir: state.paths.config.parent().unwrap_or(&state.paths.config).display().to_string(),
        platform: std::env::consts::OS.into(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareReport {
    info: HardwareInfo,
    recommended: Profile,
}

#[tauri::command]
pub async fn hardware_report(state: State<'_, AppState>) -> Result<HardwareReport, String> {
    let llama_dir = state.llama_dir();
    let data_dir = state.paths.data.clone();
    let info = tauri::async_runtime::spawn_blocking(move || hardware::detect(&llama_dir, &data_dir))
        .await
        .map_err(|_| "Não foi possível verificar o computador.".to_string())?;
    let recommended = hardware::recommend(&info);
    Ok(HardwareReport { info, recommended })
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn save_onboarding_step(state: State<'_, AppState>, step: u32) -> Result<(), String> {
    let mut s = state.settings.lock().await;
    s.onboarding_step = step;
    s.save(&state.paths.config)
}

/// Define o perfil e os modelos recomendados para ele.
#[tauri::command]
pub async fn set_profile(state: State<'_, AppState>, profile: Profile) -> Result<Settings, String> {
    let (transcription, intelligence) = catalog::for_profile(profile);
    let mut s = state.settings.lock().await;
    s.profile = Some(profile);
    s.transcription_model = Some(transcription.to_string());
    s.intelligence_model = Some(intelligence.to_string());
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

#[tauri::command]
pub async fn complete_onboarding(state: State<'_, AppState>) -> Result<Settings, String> {
    let mut s = state.settings.lock().await;
    s.onboarding_completed = true;
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    id: String,
    kind: ModelKind,
    title: &'static str,
    purpose: &'static str,
    requirement: &'static str,
    size_bytes: u64,
    installed: bool,
    downloading: bool,
    /// Ativo quando é o modelo escolhido para o seu tipo.
    active: bool,
}

#[tauri::command]
pub async fn model_catalog(state: State<'_, AppState>) -> Result<Vec<ModelView>, String> {
    let root = state.models_root().await;
    let settings = state.settings.lock().await.clone();
    let downloading = state.downloads.lock().await.keys().cloned().collect::<Vec<_>>();

    Ok(catalog::ALL
        .iter()
        .map(|m| ModelView {
            id: m.id.into(),
            kind: m.kind,
            title: m.title,
            purpose: m.purpose,
            requirement: m.requirement,
            size_bytes: m.total_size(),
            installed: download::is_installed(&root, m),
            downloading: downloading.iter().any(|d| d == m.id),
            active: match m.kind {
                ModelKind::Transcription => settings.transcription_model.as_deref() == Some(m.id),
                ModelKind::Intelligence => settings.intelligence_model.as_deref() == Some(m.id),
            },
        })
        .collect())
}

/// Inicia o download em segundo plano. O progresso chega pelo evento `model-progress`.
#[tauri::command]
pub async fn download_model(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let spec: &'static ModelSpec = catalog::find(&id).ok_or("Modelo desconhecido.")?;
    let root = state.models_root().await;
    if download::is_installed(&root, spec) {
        return Ok(());
    }

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut map = state.downloads.lock().await;
        if map.contains_key(&id) {
            return Err("Este modelo já está sendo baixado.".into());
        }
        map.insert(id.clone(), Arc::clone(&cancel));
    }

    let client = state.http.clone();
    let app_for_task = app.clone();
    let id_for_task = id.clone();
    tauri::async_runtime::spawn(async move {
        let emit_app = app_for_task.clone();
        let result = download::download_model(&client, &root, spec, cancel, move |p: DownloadProgress| {
            let _ = emit_app.emit("model-progress", p);
        })
        .await;

        let app_state = app_for_task.state::<AppState>();
        app_state.downloads.lock().await.remove(&id_for_task);

        match result {
            Ok(()) => {
                tracing::info!(model = id_for_task.as_str(), "modelo instalado");
                let _ = app_for_task.emit("model-finished", &id_for_task);
            }
            Err(message) => {
                tracing::warn!(model = id_for_task.as_str(), "download interrompido: {message}");
                let _ = app_for_task.emit("model-failed", serde_json::json!({ "id": id_for_task, "message": message }));
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_model_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Some(flag) = state.downloads.lock().await.get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_model(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let spec = catalog::find(&id).ok_or("Modelo desconhecido.")?;
    let root = state.models_root().await;
    let dir = download::model_dir(&root, spec);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|_| "Não foi possível remover o modelo. Feche outros programas que possam usá-lo e tente de novo.".to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn llama_status(state: State<'_, AppState>) -> Result<ServerState, String> {
    Ok(state.llama.state().await)
}

/// Sobe a inteligência local com o modelo ativo e devolve a porta. Usado pelo teste de onboarding e pela análise.
#[tauri::command]
pub async fn llama_start(state: State<'_, AppState>) -> Result<u16, String> {
    let model_id = state
        .settings
        .lock()
        .await
        .intelligence_model
        .clone()
        .ok_or("Escolha um modelo de inteligência nas configurações.")?;
    let spec = catalog::find(&model_id).ok_or("Modelo desconhecido.")?;
    let models_root = state.models_root().await;
    if !download::is_installed(&models_root, spec) {
        return Err("O modelo de inteligência ainda não foi baixado.".into());
    }
    let first = download::file_path(&models_root, spec, &spec.files[0]);
    state.llama.ensure_running(&first).await
}
