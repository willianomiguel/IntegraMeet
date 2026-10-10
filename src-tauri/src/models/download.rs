//! Download de modelos com retomada, verificação SHA-256, progresso e
//! mensagens compreensíveis. Arquivos são baixados em `.part` e só viram o
//! nome final depois de conferidos.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::catalog::{ModelFile, ModelSpec};

const MAX_ATTEMPTS: u32 = 3;
const SPACE_MARGIN: u64 = 1024 * 1024 * 1024; // 1 GB de folga

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub file_name: String,
    pub file_index: usize,
    pub file_count: usize,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: u64,
    pub state: &'static str, // "baixando" | "verificando" | "concluido"
}

/// Pasta de um modelo: `<models>/<id>`.
pub fn model_dir(models_root: &Path, spec: &ModelSpec) -> PathBuf {
    models_root.join(spec.id)
}

pub fn file_path(models_root: &Path, spec: &ModelSpec, file: &ModelFile) -> PathBuf {
    model_dir(models_root, spec).join(file.name)
}

/// Verdadeiro quando todos os arquivos existem com o tamanho esperado.
/// A integridade completa (hash) já foi conferida no download.
pub fn is_installed(models_root: &Path, spec: &ModelSpec) -> bool {
    spec.files.iter().all(|f| {
        std::fs::metadata(file_path(models_root, spec, f))
            .map(|m| m.len() == f.size)
            .unwrap_or(false)
    })
}

/// Baixa todos os arquivos de um modelo. `on_progress` é chamado com frequência.
/// `cancel` interrompe com segurança: o `.part` fica para retomar depois.
pub async fn download_model(
    client: &reqwest::Client,
    models_root: &Path,
    spec: &'static ModelSpec,
    cancel: Arc<AtomicBool>,
    on_progress: impl Fn(DownloadProgress) + Send + Sync,
) -> Result<(), String> {
    let dir = model_dir(models_root, spec);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Não foi possível criar a pasta do modelo: {e}"))?;

    let missing: u64 = spec
        .files
        .iter()
        .map(|f| {
            let done = std::fs::metadata(file_path(models_root, spec, f))
                .map(|m| m.len() == f.size)
                .unwrap_or(false);
            if done { 0 } else { f.size }
        })
        .sum();
    let free = fs2::available_space(models_root).unwrap_or(u64::MAX);
    if missing + SPACE_MARGIN > free {
        return Err(format!(
            "Espaço insuficiente no disco. Este modelo precisa de cerca de {} GB livres.",
            (missing + SPACE_MARGIN) / 1_073_741_824
        ));
    }

    for (index, file) in spec.files.iter().enumerate() {
        let final_path = file_path(models_root, spec, file);
        if std::fs::metadata(&final_path).map(|m| m.len() == file.size).unwrap_or(false) {
            continue;
        }

        let mut attempt = 0;
        loop {
            attempt += 1;
            match download_file(client, spec, index, file, &final_path, &cancel, &on_progress).await {
                Ok(()) => break,
                Err(DownloadError::Cancelled) => return Err("Download pausado. Você pode continuar depois.".into()),
                Err(DownloadError::Fatal(msg)) => return Err(msg),
                Err(DownloadError::Retryable(msg)) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(msg);
                    }
                    tracing::warn!(model = spec.id, attempt, "falha temporária no download: {msg}");
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                }
            }
        }
    }
    Ok(())
}

enum DownloadError {
    Cancelled,
    Retryable(String),
    Fatal(String),
}

async fn download_file(
    client: &reqwest::Client,
    spec: &ModelSpec,
    index: usize,
    file: &ModelFile,
    final_path: &Path,
    cancel: &AtomicBool,
    on_progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<(), DownloadError> {
    let part_path = final_path.with_extension(format!(
        "{}.part",
        final_path.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));

    let mut resume_from = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
    if resume_from > file.size {
        let _ = tokio::fs::remove_file(&part_path).await;
        resume_from = 0;
    }

    let mut req = client.get(file.url);
    if resume_from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let resp = req
        .send()
        .await
        .map_err(|e| DownloadError::Retryable(network_message(&e)))?;

    let status = resp.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && resume_from == file.size {
        // O .part já estava completo; só falta verificar.
        return finish_file(spec, index, file, &part_path, final_path, resume_from, on_progress).await;
    }
    if !status.is_success() {
        return Err(DownloadError::Fatal(format!(
            "O servidor de modelos respondeu com erro ({}). Tente novamente mais tarde.",
            status.as_u16()
        )));
    }

    // Se o servidor ignorou o Range, recomeça do zero.
    let append = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !append && resume_from > 0 && status.is_success() {
        resume_from = 0;
    }

    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&part_path)
        .await
        .map_err(|e| DownloadError::Fatal(disk_message(&e)))?;

    let mut downloaded = resume_from;
    let mut stream = resp.bytes_stream();
    let started = Instant::now();
    let mut last_emit = Instant::now();
    let mut session_bytes: u64 = 0;

    on_progress(progress(spec, index, file, downloaded, "baixando", 0));

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            let _ = out.flush().await;
            return Err(DownloadError::Cancelled);
        }
        let chunk = chunk.map_err(|e| DownloadError::Retryable(network_message(&e)))?;
        out.write_all(&chunk)
            .await
            .map_err(|e| DownloadError::Fatal(disk_message(&e)))?;
        downloaded += chunk.len() as u64;
        session_bytes += chunk.len() as u64;

        if last_emit.elapsed() >= Duration::from_millis(250) {
            let secs = started.elapsed().as_secs_f64().max(0.001);
            on_progress(progress(spec, index, file, downloaded, "baixando", (session_bytes as f64 / secs) as u64));
            last_emit = Instant::now();
        }
    }
    out.flush()
        .await
        .map_err(|e| DownloadError::Fatal(disk_message(&e)))?;
    drop(out);

    if downloaded != file.size {
        // Conexão caiu no meio: mantém o .part para retomar na próxima tentativa.
        return Err(DownloadError::Retryable(
            "A conexão caiu durante o download. Tentando continuar de onde parou.".into(),
        ));
    }

    finish_file(spec, index, file, &part_path, final_path, downloaded, on_progress).await
}

/// Confere o hash do `.part` e só então promove para o nome final.
async fn finish_file(
    spec: &ModelSpec,
    index: usize,
    file: &ModelFile,
    part_path: &Path,
    final_path: &Path,
    downloaded: u64,
    on_progress: &(impl Fn(DownloadProgress) + Send + Sync),
) -> Result<(), DownloadError> {
    on_progress(progress(spec, index, file, downloaded, "verificando", 0));
    let part_for_hash = part_path.to_path_buf();
    let hash = tokio::task::spawn_blocking(move || sha256_file(&part_for_hash))
        .await
        .map_err(|e| DownloadError::Fatal(format!("Falha interna na verificação: {e}")))?
        .map_err(|e| DownloadError::Fatal(disk_message(&e)))?;

    if hash != file.sha256 {
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(DownloadError::Retryable(
            "O arquivo baixado veio incompleto ou corrompido. Vamos baixar novamente.".into(),
        ));
    }

    tokio::fs::rename(part_path, final_path)
        .await
        .map_err(|e| DownloadError::Fatal(disk_message(&e)))?;
    on_progress(progress(spec, index, file, downloaded, "concluido", 0));
    Ok(())
}

fn progress(
    spec: &ModelSpec,
    index: usize,
    file: &ModelFile,
    downloaded: u64,
    state: &'static str,
    speed_bps: u64,
) -> DownloadProgress {
    DownloadProgress {
        model_id: spec.id.to_string(),
        file_name: file.name.to_string(),
        file_index: index + 1,
        file_count: spec.files.len(),
        downloaded,
        total: file.size,
        speed_bps,
        state,
    }
}

/// SHA-256 em streaming (arquivos de vários GB não cabem na memória).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn network_message(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "A conexão demorou demais. Verifique sua internet e tente novamente.".into()
    } else {
        "Sem conexão com a internet. Verifique a rede e tente novamente.".into()
    }
}

fn disk_message(e: &std::io::Error) -> String {
    match e.raw_os_error() {
        // ERROR_DISK_FULL (112) e ERROR_HANDLE_DISK_FULL (39)
        Some(112) | Some(39) => "Sem espaço no disco. Libere espaço ou escolha outra pasta.".into(),
        _ => format!("Não foi possível gravar o arquivo no disco: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_value() {
        let dir = std::env::temp_dir().join("lmai-sha-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
