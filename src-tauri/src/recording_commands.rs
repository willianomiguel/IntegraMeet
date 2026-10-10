//! Comandos de gravação e de teste de áudio (usados pelo onboarding e pela tela de gravação).

use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::capture::{self, CaptureStream, DeviceInfo, Source, StartParams, StreamStatus};
use crate::db::meetings::{self, MeetingStatus, NewMeeting};
use crate::recording::{self, Active};
use crate::state::AppState;

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn map_db(e: rusqlite::Error) -> String {
    tracing::error!("erro no banco: {e}");
    "Não foi possível salvar a reunião no banco de dados.".into()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevices {
    microphones: Vec<DeviceInfo>,
    output_name: Option<String>,
}

#[tauri::command]
pub async fn audio_devices() -> Result<AudioDevices, String> {
    tauri::async_runtime::spawn_blocking(|| {
        Ok(AudioDevices {
            microphones: capture::list_microphones()?,
            output_name: capture::default_output_name(),
        })
    })
    .await
    .map_err(|_| "Não foi possível listar os dispositivos de áudio.".to_string())?
}

// ---------- Teste de áudio (onboarding) ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReading {
    level: f32,
    status: StreamStatus,
    device_name: String,
    elapsed_ms: i64,
}

fn test_paths(state: &AppState, source: Source) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = state.paths.cache.clone();
    (
        dir.join(format!("teste-{}.pcm", source.as_str())),
        dir.join(format!("teste-{}.wav", source.as_str())),
    )
}

/// Começa a ouvir um dispositivo para mostrar o nível. `source` = "microfone" ou "sistema".
#[tauri::command]
pub async fn audio_test_start(state: State<'_, AppState>, source: String, mic_id: Option<String>) -> Result<(), String> {
    let src = match source.as_str() {
        "microfone" => Source::Mic,
        "sistema" => Source::System,
        _ => return Err("Fonte de áudio inválida.".into()),
    };
    let mut slot = state.preview.lock().await;
    if slot.is_some() {
        return Err("Já existe um teste de áudio em andamento.".into());
    }
    if state.active.lock().await.is_some() {
        return Err("Pare a gravação antes de testar o áudio.".into());
    }
    let (pcm, _) = test_paths(&state, src);
    std::fs::create_dir_all(&state.paths.cache).map_err(|_| "Não foi possível criar a pasta de teste.".to_string())?;
    let stream = tauri::async_runtime::spawn_blocking(move || {
        CaptureStream::start(StartParams {
            source: src,
            device_id: mic_id,
            pcm_path: pcm,
            clock_start: Instant::now(),
        })
    })
    .await
    .map_err(|_| "Falha interna ao iniciar o teste.".to_string())??;
    *slot = Some((stream, Instant::now()));
    Ok(())
}

#[tauri::command]
pub async fn audio_test_read(state: State<'_, AppState>) -> Result<Option<TestReading>, String> {
    let slot = state.preview.lock().await;
    Ok(slot.as_ref().map(|(s, started)| TestReading {
        level: s.level.get(),
        status: s.status(),
        device_name: s.device_name.clone(),
        elapsed_ms: started.elapsed().as_millis() as i64,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    path: String,
    duration_ms: i64,
    peak: f32,
}

/// Para o teste e grava um WAV de amostra que pode ser reproduzido.
#[tauri::command]
pub async fn audio_test_stop(state: State<'_, AppState>, source: String) -> Result<TestResult, String> {
    let src = match source.as_str() {
        "microfone" => Source::Mic,
        "sistema" => Source::System,
        _ => return Err("Fonte de áudio inválida.".into()),
    };
    let (mut stream, _) = state.preview.lock().await.take().ok_or("Nenhum teste em andamento.")?;
    let (pcm, wav) = test_paths(&state, src);
    let peak_level = stream.level.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        stream.stop();
        let ms = recording::finalize_wav(&pcm, &wav).map_err(|_| "Não foi possível salvar o teste de áudio.".to_string())?;
        Ok::<_, String>(TestResult {
            path: wav.display().to_string(),
            duration_ms: ms,
            peak: peak_level.get(),
        })
    })
    .await
    .map_err(|_| "Falha interna ao salvar o teste.".to_string())??;
    Ok(result)
}

/// Transcreve o trecho de teste com o modelo de transcrição, para o onboarding confirmar o funcionamento.
#[tauri::command]
pub async fn audio_test_transcribe(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let settings = state.settings.lock().await.clone();
    let spec_id = settings.transcription_model.clone().ok_or("Escolha um modelo de transcrição.")?;
    let spec = crate::models::catalog::find(&spec_id).ok_or("Modelo desconhecido.")?;
    let models_root = state.models_root().await;
    if !crate::models::download::is_installed(&models_root, spec) {
        return Err("Baixe o modelo de transcrição antes do teste.".into());
    }
    let model = crate::models::download::file_path(&models_root, spec, &spec.files[0]);
    let exe = state.whisper_dir().join("whisper-cli.exe");
    let prefix = state.paths.cache.join("teste-transcricao");
    let wav = std::path::PathBuf::from(path);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);

    tauri::async_runtime::spawn_blocking(move || {
        let job = crate::runtime::job::JobObject::new()?;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let run = crate::transcription::WhisperRun {
            exe,
            model: &model,
            wav: &wav,
            out_prefix: &prefix,
            threads,
        };
        let json = crate::transcription::run_whisper(&run, &cancel, &job, |_| {})?;
        let text = std::fs::read_to_string(json).map_err(|_| "Não foi possível ler o resultado do teste.".to_string())?;
        let segs = crate::transcription::parse_output(&text, "teste", 0)?;
        Ok(segs.into_iter().map(|s| s.text).collect::<Vec<_>>().join(" "))
    })
    .await
    .map_err(|_| "Falha interna no teste de transcrição.".to_string())?
}

// ---------- Gravação ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    source: &'static str,
    device_name: String,
    status: StreamStatus,
    level: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStatus {
    meeting_id: String,
    title: String,
    started_at: String,
    elapsed_ms: i64,
    paused: bool,
    sources: Vec<SourceStatus>,
    /// Problema que o usuário precisa ver agora (captura falhou ou está reconectando).
    problem: Option<String>,
}

#[tauri::command]
pub async fn recording_start(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    mic_id: Option<String>,
    include_system: bool,
) -> Result<String, String> {
    let mut slot = state.active.lock().await;
    if slot.is_some() {
        return Err("Já existe uma gravação em andamento.".into());
    }
    if state.preview.lock().await.is_some() {
        return Err("Feche o teste de áudio antes de gravar.".into());
    }

    let settings = state.settings.lock().await.clone();
    let root = state.meetings_root(&settings);
    std::fs::create_dir_all(&root).map_err(|_| "Não foi possível acessar a pasta de gravações.".to_string())?;
    recording::check_disk(&root)?;

    let id = uuid::Uuid::new_v4().to_string();
    let started = chrono::Local::now();
    let dir = root.join(format!("{}_{}", started.format("%Y-%m-%d_%H%M"), &id[..8]));
    std::fs::create_dir_all(&dir).map_err(|_| "Não foi possível criar a pasta da reunião.".to_string())?;

    let title = {
        let t = title.trim();
        if t.is_empty() {
            format!("Reunião de {}", started.format("%d/%m/%Y %H:%M"))
        } else {
            t.to_string()
        }
    };

    let clock = Instant::now();
    let mut sources: Vec<Source> = vec![Source::Mic];
    if include_system {
        sources.push(Source::System);
    }

    let dir_c = dir.clone();
    let mic_c = mic_id.clone();
    let streams = tauri::async_runtime::spawn_blocking(move || {
        let mut streams = Vec::new();
        for src in sources {
            let params = StartParams {
                source: src,
                device_id: if src == Source::Mic { mic_c.clone() } else { None },
                pcm_path: recording::pcm_path(&dir_c, src),
                clock_start: clock,
            };
            match CaptureStream::start(params) {
                Ok(s) => streams.push(s),
                Err(msg) => {
                    // Não grava com fonte faltando: para o que já abriu e avisa o usuário.
                    for mut s in streams {
                        s.stop();
                    }
                    return Err(msg);
                }
            }
        }
        Ok(streams)
    })
    .await
    .map_err(|_| "Falha interna ao iniciar a gravação.".to_string())??;

    {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        let folder = dir.display().to_string();
        meetings::insert_meeting(
            &db,
            &NewMeeting {
                id: &id,
                title: &title,
                started_at: &started.format("%Y-%m-%dT%H:%M:%S").to_string(),
                status: MeetingStatus::Gravando,
                folder: &folder,
            },
        )
        .map_err(map_db)?;
        for s in &streams {
            meetings::insert_recording(
                &db,
                &id,
                s.source.as_str(),
                &s.device_name,
                &recording::wav_path(&dir, s.source).display().to_string(),
                0,
            )
            .map_err(map_db)?;
        }
    }

    crate::set_asset_scope(&app, &root);
    *slot = Some(Active {
        meeting_id: id.clone(),
        title,
        started_at: started,
        clock,
        dir,
        streams,
        paused: false,
    });
    tracing::info!("gravação iniciada");
    Ok(id)
}

#[tauri::command]
pub async fn recording_status(state: State<'_, AppState>) -> Result<Option<RecordingStatus>, String> {
    let slot = state.active.lock().await;
    let Some(act) = slot.as_ref() else { return Ok(None) };

    let mut problem = None;
    let sources = act
        .streams
        .iter()
        .map(|s| {
            let status = s.status();
            match &status {
                StreamStatus::Falhou { mensagem } => problem = Some(mensagem.clone()),
                StreamStatus::Reconectando { mensagem } => {
                    problem.get_or_insert_with(|| format!("{} Tentando reconectar…", mensagem));
                }
                _ => {}
            }
            SourceStatus {
                source: s.source.as_str(),
                device_name: s.device_name.clone(),
                status,
                level: s.level.get(),
            }
        })
        .collect();

    Ok(Some(RecordingStatus {
        meeting_id: act.meeting_id.clone(),
        title: act.title.clone(),
        started_at: act.started_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        elapsed_ms: act.clock.elapsed().as_millis() as i64,
        paused: act.paused,
        sources,
        problem,
    }))
}

#[tauri::command]
pub async fn recording_pause(state: State<'_, AppState>, paused: bool) -> Result<(), String> {
    let mut slot = state.active.lock().await;
    let act = slot.as_mut().ok_or("Nenhuma gravação em andamento.")?;
    for s in &act.streams {
        s.set_paused(paused);
    }
    act.paused = paused;
    Ok(())
}

/// Encerra a gravação, gera os WAVs e enfileira a transcrição.
#[tauri::command]
pub async fn recording_stop(state: State<'_, AppState>) -> Result<String, String> {
    let act = state.active.lock().await.take().ok_or("Nenhuma gravação em andamento.")?;
    let meeting_id = act.meeting_id.clone();
    let elapsed_ms = act.clock.elapsed().as_millis() as i64;
    let dir = act.dir.clone();

    let issues = tauri::async_runtime::spawn_blocking(move || {
        let mut streams = act.streams;
        let mut issues: Vec<String> = Vec::new();
        for s in &mut streams {
            if let StreamStatus::Falhou { mensagem } = s.status() {
                issues.push(mensagem);
            }
            s.stop();
        }
        for s in &streams {
            let pcm = recording::pcm_path(&dir, s.source);
            let wav = recording::wav_path(&dir, s.source);
            if pcm.exists() {
                recording::finalize_wav(&pcm, &wav).map_err(|_| "Não foi possível salvar o áudio. Verifique o espaço em disco.".to_string())?;
            }
        }
        Ok::<_, String>(issues)
    })
    .await
    .map_err(|_| "Falha interna ao finalizar a gravação.".to_string())??;

    let error = if issues.is_empty() { None } else { Some(issues.join(" ")) };
    let status = MeetingStatus::Pronta;
    {
        let db = state.db.lock().map_err(|_| "Banco indisponível.".to_string())?;
        meetings::finish_recording(&db, &meeting_id, &now_iso(), elapsed_ms, &status).map_err(map_db)?;
        if let Some(e) = &error {
            meetings::set_status(&db, &meeting_id, &status, Some(e)).map_err(map_db)?;
        }
        meetings::enqueue_job(&db, "transcrever", &meeting_id, &now_iso()).map_err(map_db)?;
    }
    state.worker.wake.notify_one();
    tracing::info!(duration_ms = elapsed_ms, "gravação finalizada");
    Ok(meeting_id)
}
