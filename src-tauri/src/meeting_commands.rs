//! Comandos de reuniões, pesquisa, configurações e diagnóstico.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::db::meetings::{self, MeetingDetail, MeetingSummary, SearchHit};
use crate::hardware;
use crate::models::catalog;
use crate::settings::Settings;
use crate::state::AppState;
use crate::worker;

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn db_err(e: rusqlite::Error) -> String {
    tracing::error!("erro no banco: {e}");
    "Não foi possível acessar o banco de dados.".into()
}

#[tauri::command]
pub async fn meetings_list(state: State<'_, AppState>) -> Result<Vec<MeetingSummary>, String> {
    let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    meetings::list(&db, 200).map_err(db_err)
}

#[tauri::command]
pub async fn meeting_detail(state: State<'_, AppState>, id: String) -> Result<MeetingDetail, String> {
    let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    meetings::detail(&db, &id)
        .map_err(db_err)?
        .ok_or_else(|| "Esta reunião não existe mais.".to_string())
}

#[tauri::command]
pub async fn meeting_rename(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("O título não pode ficar vazio.".into());
    }
    let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    meetings::rename(&db, &id, title).map_err(db_err)
}

/// Apaga a reunião e seus arquivos. A UI deve pedir confirmação antes.
#[tauri::command]
pub async fn meeting_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let active_id = state.active.lock().await.as_ref().map(|a| a.meeting_id.clone());
    if active_id.as_deref() == Some(id.as_str()) {
        return Err("Esta reunião está sendo gravada. Finalize a gravação antes de excluir.".into());
    }
    let folder = {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        let detail = meetings::detail(&db, &id).map_err(db_err)?.ok_or("Esta reunião não existe mais.")?;
        meetings::cancel_jobs(&db, &id, &now_iso()).map_err(db_err)?;
        meetings::delete(&db, &id).map_err(db_err)?;
        detail.folder
    };
    let path = PathBuf::from(folder);
    if path.exists() {
        std::fs::remove_dir_all(&path).map_err(|_| "A reunião foi removida do histórico, mas não foi possível apagar os arquivos. Apague a pasta manualmente.".to_string())?;
    }
    Ok(())
}

/// Reenfileira a transcrição (usado em "tentar novamente").
#[tauri::command]
pub async fn meeting_transcribe(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        meetings::enqueue_job(&db, "transcrever", &id, &now_iso()).map_err(db_err)?;
    }
    state.worker.wake.notify_one();
    Ok(())
}

#[tauri::command]
pub async fn meeting_analyze(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        let segs = meetings::segments(&db, &id).map_err(db_err)?;
        if segs.is_empty() {
            return Err("Esta reunião ainda não tem transcrição para analisar.".into());
        }
        meetings::enqueue_job(&db, "analisar", &id, &now_iso()).map_err(db_err)?;
    }
    state.worker.wake.notify_one();
    Ok(())
}

/// Cancela a tarefa em andamento (quando possível) e as que estão na fila.
#[tauri::command]
pub async fn meeting_cancel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        meetings::cancel_jobs(&db, &id, &now_iso()).map_err(db_err)?;
    }
    worker::cancel_flag(&state.worker, &id).store(true, Ordering::Relaxed);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingItem {
    meeting_id: String,
    title: String,
    kind: String,
    status: String,
    progress: Option<u8>,
}

/// Tarefas pendentes ou em execução, para o painel "processamento em andamento".
#[tauri::command]
pub async fn processing_status(state: State<'_, AppState>) -> Result<Vec<ProcessingItem>, String> {
    let rows: Vec<(String, String, String, String)> = {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        let mut stmt = db
            .prepare(
                "SELECT j.meeting_id, m.title, j.kind, j.status FROM jobs j JOIN meetings m ON m.id = j.meeting_id
                 WHERE j.status IN ('pendente','executando') ORDER BY j.id",
            )
            .map_err(db_err)?;
        let found: rusqlite::Result<Vec<_>> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .and_then(|it| it.collect());
        found.map_err(db_err)?
    };
    let progress = state.worker.progress.lock().map(|p| p.clone()).unwrap_or_default();
    Ok(rows
        .into_iter()
        .map(|(meeting_id, title, kind, status)| ProcessingItem {
            progress: progress.get(&meeting_id).copied(),
            meeting_id,
            title,
            kind,
            status,
        })
        .collect())
}

#[tauri::command]
pub async fn search_meetings(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, String> {
    let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    meetings::search(&db, &query, 50).map_err(db_err)
}

// ---------- Configurações ----------

#[tauri::command]
pub async fn set_microphone(state: State<'_, AppState>, mic_id: Option<String>) -> Result<Settings, String> {
    let mut s = state.settings.lock().await;
    s.microphone_id = mic_id;
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

#[tauri::command]
pub async fn set_capture_system(state: State<'_, AppState>, enabled: bool) -> Result<Settings, String> {
    let mut s = state.settings.lock().await;
    s.capture_system_audio = enabled;
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

#[tauri::command]
pub async fn set_transcription_model(state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let spec = catalog::find(&id).ok_or("Modelo desconhecido.")?;
    if spec.kind != catalog::ModelKind::Transcription {
        return Err("Este não é um modelo de transcrição.".into());
    }
    let mut s = state.settings.lock().await;
    s.transcription_model = Some(id);
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

#[tauri::command]
pub async fn set_intelligence_model(state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let spec = catalog::find(&id).ok_or("Modelo desconhecido.")?;
    if spec.kind != catalog::ModelKind::Intelligence {
        return Err("Este não é um modelo de inteligência.".into());
    }
    let mut s = state.settings.lock().await;
    s.intelligence_model = Some(id);
    s.save(&state.paths.config)?;
    // A IA carregada usa o modelo antigo; ela será reiniciada na próxima análise.
    drop(s);
    state.llama.stop().await;
    Ok(state.settings.lock().await.clone())
}

/// Troca a pasta de gravações. Os arquivos existentes não são movidos.
#[tauri::command]
pub async fn set_recordings_dir(app: AppHandle, state: State<'_, AppState>, path: Option<String>) -> Result<Settings, String> {
    let target = match path {
        Some(p) => {
            let dir = PathBuf::from(p);
            ensure_writable(&dir)?;
            Some(dir)
        }
        None => None,
    };
    if state.active.lock().await.is_some() {
        return Err("Finalize a gravação antes de trocar a pasta.".into());
    }
    let mut s = state.settings.lock().await;
    s.recordings_dir = target;
    s.save(&state.paths.config)?;
    let root = state.meetings_root(&s);
    crate::set_asset_scope(&app, &root);
    Ok(s.clone())
}

/// Troca a pasta de modelos. Não move modelos existentes; eles são baixados de novo na nova pasta.
#[tauri::command]
pub async fn set_models_dir(state: State<'_, AppState>, path: Option<String>) -> Result<Settings, String> {
    let target = match path {
        Some(p) => {
            let dir = PathBuf::from(p);
            ensure_writable(&dir)?;
            Some(dir)
        }
        None => None,
    };
    if !state.downloads.lock().await.is_empty() {
        return Err("Aguarde o download de modelos terminar antes de trocar a pasta.".into());
    }
    let mut s = state.settings.lock().await;
    s.models_dir = target;
    s.save(&state.paths.config)?;
    Ok(s.clone())
}

fn ensure_writable(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|_| "Não foi possível criar esta pasta. Escolha outro local.".to_string())?;
    let probe = dir.join(".lmai-teste");
    std::fs::write(&probe, b"ok").map_err(|_| "Sem permissão para salvar nesta pasta. Escolha outro local.".to_string())?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePaths {
    recordings: String,
    models: String,
    database: String,
    logs: String,
    config: String,
}

#[tauri::command]
pub async fn storage_paths(state: State<'_, AppState>) -> Result<StoragePaths, String> {
    let s = state.settings.lock().await.clone();
    Ok(StoragePaths {
        recordings: state.meetings_root(&s).display().to_string(),
        models: s.models_dir.clone().unwrap_or_else(|| state.paths.models.clone()).display().to_string(),
        database: state.paths.data_db.display().to_string(),
        logs: state.paths.logs.display().to_string(),
        config: state.paths.config.display().to_string(),
    })
}

/// Abre uma pasta do app no Explorer.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("Este caminho não existe mais.".into());
    }
    let mut cmd = std::process::Command::new("explorer");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
        if p.is_file() {
            cmd.arg(format!("/select,{}", p.display()));
        } else {
            cmd.arg(p.display().to_string());
        }
    }
    cmd.spawn().map(|_| ()).map_err(|_| "Não foi possível abrir a pasta.".to_string())
}

/// Gera um relatório de diagnóstico (sem conteúdo de reuniões) em Documentos\LocalMeetingAI.
#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    let llama_dir = state.llama_dir();
    let data_dir = state.paths.data.clone();
    let hw = tauri::async_runtime::spawn_blocking(move || hardware::detect(&llama_dir, &data_dir))
        .await
        .map_err(|_| "Falha ao coletar dados do computador.".to_string())?;
    let settings = state.settings.lock().await.clone();

    let (total, interrupted, failed) = {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        let count = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap_or(-1) };
        (
            count("SELECT count(*) FROM meetings"),
            count("SELECT count(*) FROM meetings WHERE status = 'gravacao_interrompida'"),
            count("SELECT count(*) FROM meetings WHERE status = 'erro'"),
        )
    };

    let log_tail = latest_log_tail(&state.paths.logs, 300);
    let mut out = String::new();
    out.push_str("IntegraMeet — diagnóstico\n");
    out.push_str(&format!("Gerado em: {}\n", now_iso()));
    out.push_str(&format!("Versão: {}\n\n", env!("CARGO_PKG_VERSION")));
    out.push_str("[Computador]\n");
    out.push_str(&format!("Windows: {} (build {})\n", hw.os_caption, hw.os_build));
    out.push_str(&format!("Processador: {} ({} threads)\n", hw.cpu_name, hw.cpu_threads));
    out.push_str(&format!("Memória: {:.1} GB\n", hw.ram_gb));
    for g in &hw.gpus {
        out.push_str(&format!("GPU: {} ({} MB de VRAM)\n", g.name, g.vram_mb));
    }
    out.push_str(&format!("Espaço livre na pasta de dados: {:.1} GB\n\n", hw.disk_free_gb));
    out.push_str("[Configurações]\n");
    out.push_str(&format!("Perfil: {:?}\n", settings.profile));
    out.push_str(&format!("Modelo de transcrição: {:?}\n", settings.transcription_model));
    out.push_str(&format!("Modelo de inteligência: {:?}\n", settings.intelligence_model));
    out.push_str(&format!("Áudio do computador ativado: {}\n\n", settings.capture_system_audio));
    out.push_str("[Reuniões]\n");
    out.push_str(&format!("Total: {total}\nGravações interrompidas: {interrupted}\nCom erro: {failed}\n\n"));
    out.push_str("[Últimas linhas do log técnico]\n");
    out.push_str(&log_tail);

    let dir = dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalMeetingAI");
    std::fs::create_dir_all(&dir).map_err(|_| "Não foi possível salvar o diagnóstico.".to_string())?;
    let file = dir.join(format!("diagnostico-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&file, out).map_err(|_| "Não foi possível salvar o diagnóstico.".to_string())?;
    Ok(file.display().to_string())
}

fn latest_log_tail(logs: &Path, lines: usize) -> String {
    let Ok(entries) = std::fs::read_dir(logs) else { return String::new() };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    let Some(last) = files.pop() else { return String::new() };
    let text = std::fs::read_to_string(last).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
