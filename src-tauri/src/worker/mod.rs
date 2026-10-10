//! Worker de processamento: consome a fila `jobs` uma tarefa por vez.
//! Transcrição e análise nunca rodam juntas, para não disputar memória e VRAM.
//! A IA local é desligada depois de um tempo ocioso.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tokio::sync::{Mutex, Notify};

use crate::analysis;
use crate::db::meetings::{self, Job, MeetingStatus};
use crate::models::catalog::{self, ModelKind};
use crate::models::download;
use crate::runtime::job::JobObject;
use crate::runtime::llama::LlamaServer;
use crate::settings::Settings;
use crate::transcription::{self, WhisperRun};

const IDLE_STOP: Duration = Duration::from_secs(5 * 60);

pub struct WorkerCtx {
    pub db: Arc<StdMutex<Connection>>,
    pub llama: Arc<LlamaServer>,
    pub http: reqwest::Client,
    pub whisper_dir: PathBuf,
    pub default_models: PathBuf,
    pub settings: Arc<Mutex<Settings>>,
    pub shared: Arc<WorkerShared>,
}

/// Estado compartilhado com os comandos (cancelamento e progresso).
#[derive(Default)]
pub struct WorkerShared {
    pub cancel: StdMutex<HashMap<String, Arc<AtomicBool>>>,
    pub progress: StdMutex<HashMap<String, u8>>,
    pub wake: Notify,
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn with_db<T>(db: &StdMutex<Connection>, f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> Result<T, String> {
    let mut guard = db.lock().map_err(|_| "Banco indisponível.".to_string())?;
    f(&mut guard).map_err(|e| {
        tracing::error!("erro no banco: {e}");
        "Não foi possível acessar o banco de dados.".to_string()
    })
}

/// Loop principal. Roda enquanto o app estiver aberto.
pub async fn run(ctx: WorkerCtx) {
    let mut last_llm_use = Instant::now();
    loop {
        let next = match with_db(&ctx.db, |c| meetings::next_job(c)) {
            Ok(j) => j,
            Err(_) => None,
        };
        match next {
            Some(job) => {
                process(&ctx, job).await;
                last_llm_use = Instant::now();
            }
            None => {
                if last_llm_use.elapsed() > IDLE_STOP && ctx.llama.state().await != crate::runtime::llama::ServerState::Parado {
                    tracing::info!("IA local ociosa; encerrando para liberar memória");
                    ctx.llama.stop().await;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                    _ = ctx.shared.wake.notified() => {}
                }
            }
        }
    }
}

/// Flag de cancelamento da reunião. Criada na primeira vez e reaproveitada.
pub fn cancel_flag(shared: &WorkerShared, meeting_id: &str) -> Arc<AtomicBool> {
    let mut map = shared.cancel.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(meeting_id.to_string())
        .or_insert_with(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

async fn process(ctx: &WorkerCtx, job: Job) {
    // Começa a tarefa sem cancelamento pendente de uma execução anterior.
    cancel_flag(&ctx.shared, &job.meeting_id).store(false, Ordering::Relaxed);
    let _ = with_db(&ctx.db, |c| meetings::set_job(c, job.id, "executando", None, &now()));
    let result = match job.kind.as_str() {
        "transcrever" => transcribe(ctx, &job.meeting_id).await,
        "analisar" => analyze(ctx, &job.meeting_id).await,
        other => Err(format!("tarefa desconhecida: {other}")),
    };

    match result {
        Ok(()) => {
            let _ = with_db(&ctx.db, |c| meetings::set_job(c, job.id, "concluido", None, &now()));
        }
        Err(msg) if msg == CANCELLED => {
            let _ = with_db(&ctx.db, |c| meetings::set_job(c, job.id, "cancelado", None, &now()));
            let _ = with_db(&ctx.db, |c| meetings::set_status(c, &job.meeting_id, &MeetingStatus::Pronta, None));
        }
        Err(msg) => {
            tracing::warn!(job = job.kind.as_str(), attempts = job.attempts, "tarefa falhou: {msg}");
            let _ = with_db(&ctx.db, |c| meetings::set_job(c, job.id, "falhou", Some(&msg), &now()));
            let _ = with_db(&ctx.db, |c| meetings::set_status(c, &job.meeting_id, &MeetingStatus::Erro, Some(&msg)));
        }
    }
    ctx.shared.progress.lock().map(|mut p| p.remove(&job.meeting_id)).ok();
    ctx.shared.cancel.lock().map(|mut c| c.remove(&job.meeting_id)).ok();
}

const CANCELLED: &str = "__cancelado__";

async fn transcribe(ctx: &WorkerCtx, meeting_id: &str) -> Result<(), String> {
    let detail = with_db(&ctx.db, |c| meetings::detail(c, meeting_id))?.ok_or("Reunião não encontrada.")?;
    let _ = with_db(&ctx.db, |c| meetings::set_status(c, meeting_id, &MeetingStatus::Transcrevendo, None));

    let settings = ctx.settings.lock().await.clone();
    let spec_id = settings.transcription_model.clone().ok_or("Escolha um modelo de transcrição nas configurações.")?;
    let spec = catalog::find(&spec_id).ok_or("Modelo de transcrição desconhecido.")?;
    let models_root = settings.models_dir.clone().unwrap_or_else(|| ctx.default_models.clone());
    if !download::is_installed(&models_root, spec) {
        return Err("O modelo de transcrição ainda não foi baixado. Baixe-o em Configurações > Modelos locais.".into());
    }
    let model_path = download::file_path(&models_root, spec, &spec.files[0]);
    let exe = ctx.whisper_dir.join("whisper-cli.exe");
    if !exe.exists() {
        return Err("O mecanismo de transcrição não foi encontrado. Reinstale o aplicativo.".into());
    }

    let cancel = cancel_flag(&ctx.shared, meeting_id);

    let total = detail.recordings.len().max(1);
    let mut all = Vec::new();
    for (i, rec) in detail.recordings.iter().enumerate() {
        let wav = PathBuf::from(&rec.file_path);
        if !wav.exists() {
            continue;
        }
        let dir = wav.parent().map(Path::to_path_buf).unwrap_or_default();
        let prefix = dir.join(format!("{}_transcricao", rec.source));
        let _ = std::fs::remove_file(prefix.with_extension("json"));

        let exe_c = exe.clone();
        let model_c = model_path.clone();
        let wav_c = wav.clone();
        let prefix_c = prefix.clone();
        let cancel_c = Arc::clone(&cancel);
        let shared = Arc::clone(&ctx.shared);
        let meeting = meeting_id.to_string();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);

        let json_path = tokio::task::spawn_blocking(move || {
            let job = JobObject::new()?;
            let run = WhisperRun {
                exe: exe_c,
                model: &model_c,
                wav: &wav_c,
                out_prefix: &prefix_c,
                threads,
            };
            transcription::run_whisper(&run, &cancel_c, &job, |p| {
                let overall = ((i as u32 * 100) + p as u32) / total as u32;
                if let Ok(mut map) = shared.progress.lock() {
                    map.insert(meeting.clone(), overall.min(100) as u8);
                }
            })
        })
        .await
        .map_err(|_| "Falha interna na transcrição.".to_string())?;

        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        let json_path = json_path?;
        let text = std::fs::read_to_string(&json_path)
            .map_err(|_| "Não foi possível ler o resultado da transcrição.".to_string())?;
        let segs = transcription::parse_output(&text, &rec.source, rec.offset_ms)?;
        all.extend(segs);
    }

    all.sort_by_key(|s| s.start_ms);
    let rows: Vec<(String, i64, i64, String, Option<f64>)> = all
        .into_iter()
        .map(|s| (s.source, s.start_ms, s.end_ms, s.text, s.confidence))
        .collect();

    with_db(&ctx.db, |c| meetings::replace_segments(c, meeting_id, &rows))?;
    with_db(&ctx.db, |c| meetings::set_status(c, meeting_id, &MeetingStatus::Transcrita, None))?;
    tracing::info!(segments = rows.len(), "transcrição concluída");

    // Análise automática quando a IA estiver instalada.
    let intel_ok = settings
        .intelligence_model
        .as_deref()
        .and_then(catalog::find)
        .map(|s| s.kind == ModelKind::Intelligence && download::is_installed(&models_root, s))
        .unwrap_or(false);
    if intel_ok {
        with_db(&ctx.db, |c| meetings::enqueue_job(c, "analisar", meeting_id, &now()))?;
        ctx.shared.wake.notify_one();
    }
    Ok(())
}

async fn analyze(ctx: &WorkerCtx, meeting_id: &str) -> Result<(), String> {
    let settings = ctx.settings.lock().await.clone();
    let spec_id = settings.intelligence_model.clone().ok_or("Escolha um modelo de inteligência nas configurações.")?;
    let spec = catalog::find(&spec_id).ok_or("Modelo de inteligência desconhecido.")?;
    let models_root = settings.models_dir.clone().unwrap_or_else(|| ctx.default_models.clone());
    if !download::is_installed(&models_root, spec) {
        return Err("O modelo de inteligência ainda não foi baixado. Baixe-o em Configurações > Modelos locais.".into());
    }

    let segments = with_db(&ctx.db, |c| meetings::segments(c, meeting_id))?;
    if segments.is_empty() {
        return Err("Esta reunião ainda não tem transcrição para analisar.".into());
    }
    let _ = with_db(&ctx.db, |c| meetings::set_status(c, meeting_id, &MeetingStatus::Analisando, None));

    let model_path = download::file_path(&models_root, spec, &spec.files[0]);
    let port = ctx.llama.ensure_running(&model_path).await?;
    let base = LlamaServer::base_url(port);

    let chunks = analysis::build_chunks(&segments);
    let mut parts = Vec::new();
    let mut failures = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        if cancel_flag(&ctx.shared, meeting_id).load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        ctx.shared
            .progress
            .lock()
            .map(|mut p| p.insert(meeting_id.to_string(), ((i * 100) / chunks.len()) as u8))
            .ok();

        // Até duas tentativas por bloco: a segunda já reforça o formato.
        let mut done = None;
        for attempt in 0..2 {
            let user = if attempt == 0 {
                analysis::user_prompt(chunk)
            } else {
                format!("{}\n\nResponda somente com o objeto JSON, sem texto antes ou depois.", analysis::user_prompt(chunk))
            };
            let Ok(raw) = analysis::chat(&ctx.http, &base, analysis::system_prompt(), &user).await else {
                continue;
            };
            if let Ok(parsed) = analysis::validate(&raw, &segments) {
                done = Some(parsed);
                break;
            }
        }
        match done {
            Some(a) => parts.push(a),
            None => failures += 1,
        }
    }

    if parts.is_empty() {
        return Err("A análise não pôde ser concluída. Tente novamente; a transcrição continua disponível.".into());
    }
    if failures > 0 {
        tracing::warn!(failures, "alguns blocos não foram analisados");
    }

    let merged = analysis::merge(parts);
    let items: Vec<(String, String, Option<String>, Option<String>, Vec<i64>)> = merged
        .items
        .into_iter()
        .map(|i| (i.kind, i.text, i.owner, i.due, i.segment_ids))
        .collect();

    with_db(&ctx.db, |c| {
        meetings::save_analysis(c, meeting_id, &now(), spec.id, &merged.summary, &merged.topics, &items)
    })?;
    with_db(&ctx.db, |c| meetings::set_status(c, meeting_id, &MeetingStatus::Analisada, None))?;
    Ok(())
}

