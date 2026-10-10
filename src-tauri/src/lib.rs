mod analysis;
mod capture;
mod commands;
mod db;
#[cfg(test)]
mod e2e;
mod hardware;
mod logs;
mod meeting_commands;
mod models;
mod paths;
mod recording;
mod recording_commands;
mod runtime;
mod settings;
mod state;
mod transcription;
mod worker;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Manager, RunEvent};
use tokio::sync::Mutex;

use paths::AppPaths;
use runtime::llama::LlamaServer;
use settings::Settings;
use state::AppState;
use worker::{WorkerCtx, WorkerShared};

// Guard do logger mantido vivo pelo processo inteiro.
static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Libera o webview para tocar arquivos de uma pasta (áudio das reuniões).
pub fn set_asset_scope(app: &AppHandle, dir: &Path) {
    let _ = app.asset_protocol_scope().allow_directory(dir, true);
}

/// Recupera o que ficou aberto quando o app fechou no meio:
/// gravações em andamento viram "interrompidas" com o áudio até o ponto em que foram salvas.
fn recover_on_start(conn: &rusqlite::Connection) {
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    if let Ok(n) = db::meetings::requeue_interrupted_jobs(conn, &now) {
        if n > 0 {
            tracing::info!(jobs = n, "tarefas interrompidas voltaram para a fila");
        }
    }

    let interrupted: Vec<(String, String)> = conn
        .prepare("SELECT id, folder FROM meetings WHERE status = 'gravando'")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();

    for (id, duration) in recording::recover_all(Path::new(""), &interrupted) {
        let msg = "O aplicativo foi fechado durante a gravação. O áudio foi recuperado até o último momento salvo.";
        let _ = db::meetings::finish_recording(conn, &id, &now, duration, &db::meetings::MeetingStatus::GravacaoInterrompida);
        let _ = db::meetings::set_status(conn, &id, &db::meetings::MeetingStatus::GravacaoInterrompida, Some(msg));
        tracing::warn!("gravação recuperada após encerramento inesperado");
    }
}

pub fn run() {
    let app_paths = match AppPaths::resolve().and_then(|p| {
        p.ensure_exist()?;
        Ok(p)
    }) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Erro ao preparar pastas do aplicativo: {e}");
            std::process::exit(1);
        }
    };

    if let Some(guard) = logs::init(&app_paths.logs) {
        let _ = LOG_GUARD.set(guard);
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Aplicativo iniciado");

    let conn = match db::open(&app_paths.data_db) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("banco indisponível: {e}");
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    recover_on_start(&conn);

    let settings = Settings::load(&app_paths.config);
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .expect("cliente HTTP");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            // Em produção os binários ficam em resources/runtime; em desenvolvimento,
            // o Tauri aponta resource_dir para a pasta do projeto.
            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|e| format!("pasta de recursos indisponível: {e}"))?;
            let runtime_dir = resource_dir.join("resources").join("runtime");

            let db = Arc::new(StdMutex::new(conn));
            let settings = Arc::new(Mutex::new(settings.clone()));
            let llama = LlamaServer::new(&runtime_dir.join("llama"));
            let worker_shared = Arc::new(WorkerShared::default());

            set_asset_scope(app.handle(), &app_paths.cache);
            set_asset_scope(app.handle(), app_paths.config.parent().unwrap_or(&app_paths.config));
            if let Some(dir) = settings.try_lock().ok().and_then(|s| s.recordings_dir.clone()) {
                set_asset_scope(app.handle(), &dir);
            }
            set_asset_scope(app.handle(), &app_paths.recordings);

            tauri::async_runtime::spawn(worker::run(WorkerCtx {
                db: Arc::clone(&db),
                llama: Arc::clone(&llama),
                http: http.clone(),
                whisper_dir: runtime_dir.join("whisper"),
                default_models: app_paths.models.clone(),
                settings: Arc::clone(&settings),
                shared: Arc::clone(&worker_shared),
            }));

            app.manage(AppState {
                paths: app_paths.clone(),
                runtime_dir,
                settings,
                llama,
                downloads: Mutex::new(HashMap::<String, Arc<AtomicBool>>::new()),
                http: http.clone(),
                db,
                active: Mutex::new(None),
                preview: Mutex::new(None),
                worker: worker_shared,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::hardware_report,
            commands::get_settings,
            commands::save_onboarding_step,
            commands::set_profile,
            commands::complete_onboarding,
            commands::model_catalog,
            commands::download_model,
            commands::cancel_model_download,
            commands::delete_model,
            commands::llama_status,
            commands::llama_start,
            recording_commands::audio_devices,
            recording_commands::audio_test_start,
            recording_commands::audio_test_read,
            recording_commands::audio_test_stop,
            recording_commands::audio_test_transcribe,
            recording_commands::recording_start,
            recording_commands::recording_status,
            recording_commands::recording_pause,
            recording_commands::recording_stop,
            meeting_commands::meetings_list,
            meeting_commands::meeting_detail,
            meeting_commands::meeting_rename,
            meeting_commands::meeting_delete,
            meeting_commands::meeting_transcribe,
            meeting_commands::meeting_analyze,
            meeting_commands::meeting_cancel,
            meeting_commands::processing_status,
            meeting_commands::search_meetings,
            meeting_commands::set_microphone,
            meeting_commands::set_capture_system,
            meeting_commands::set_transcription_model,
            meeting_commands::set_intelligence_model,
            meeting_commands::set_recordings_dir,
            meeting_commands::set_models_dir,
            meeting_commands::storage_paths,
            meeting_commands::reveal_path,
            meeting_commands::export_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao montar o aplicativo");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            // Encerramento limpo: para gravação em andamento e processos filhos.
            let state: tauri::State<AppState> = app_handle.state();
            if let Ok(mut active) = state.active.try_lock() {
                if let Some(mut act) = active.take() {
                    for s in &mut act.streams {
                        s.stop();
                    }
                    tracing::warn!("app fechado durante a gravação; áudio salvo até agora");
                }
            }
            let llama = state.llama.clone();
            tauri::async_runtime::block_on(llama.stop());
        }
    });
}
