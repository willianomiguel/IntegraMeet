//! Transcrição com whisper.cpp (`whisper-cli`). Roda uma fonte por vez, em
//! processo filho controlado pelo app (vinculado ao Job Object).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::runtime::job::JobObject;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Um trecho transcrito, já com o tempo absoluto da reunião.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSegment {
    pub source: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f64>,
}

#[derive(Deserialize)]
struct WhisperJson {
    transcription: Vec<WhisperItem>,
}

#[derive(Deserialize)]
struct WhisperItem {
    offsets: Offsets,
    text: String,
    #[serde(default)]
    tokens: Vec<Token>,
}

#[derive(Deserialize)]
struct Offsets {
    from: i64,
    to: i64,
}

#[derive(Deserialize)]
struct Token {
    text: String,
    #[serde(default)]
    p: f64,
}

/// Converte o JSON do whisper-cli em segmentos. `offset_ms` é o início da fonte na reunião.
pub fn parse_output(json: &str, source: &str, offset_ms: i64) -> Result<Vec<RawSegment>, String> {
    let parsed: WhisperJson =
        serde_json::from_str(json).map_err(|_| "A transcrição veio em formato inesperado.".to_string())?;

    let segs = parsed
        .transcription
        .into_iter()
        .filter_map(|item| {
            let text = item.text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            // Tokens especiais do whisper ([_BEG_], [_TT_123]) não contam para a confiança.
            let probs: Vec<f64> = item
                .tokens
                .iter()
                .filter(|t| !t.text.trim().is_empty() && !t.text.trim_start().starts_with("[_"))
                .map(|t| t.p)
                .collect();
            let confidence = if probs.is_empty() {
                None
            } else {
                Some(probs.iter().sum::<f64>() / probs.len() as f64)
            };
            Some(RawSegment {
                source: source.to_string(),
                start_ms: offset_ms + item.offsets.from,
                end_ms: offset_ms + item.offsets.to,
                text,
                confidence,
            })
        })
        .collect();
    Ok(segs)
}

pub struct WhisperRun<'a> {
    pub exe: PathBuf,
    pub model: &'a Path,
    pub wav: &'a Path,
    pub out_prefix: &'a Path,
    pub threads: usize,
}

/// Executa whisper-cli e espera o fim. `cancel` encerra o processo imediatamente.
/// `on_progress` recebe percentuais de 0 a 100.
pub fn run_whisper(
    run: &WhisperRun,
    cancel: &AtomicBool,
    job: &JobObject,
    on_progress: impl Fn(u8),
) -> Result<PathBuf, String> {
    let mut cmd = Command::new(&run.exe);
    cmd.arg("-m")
        .arg(run.model)
        .arg("-f")
        .arg(run.wav)
        .args(["-l", "pt"])
        .args(["-t", &run.threads.to_string()])
        .arg("-oj")
        .arg("-ojf")
        .arg("-pp")
        .arg("-of")
        .arg(run.out_prefix)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| {
        tracing::error!("falha ao iniciar whisper-cli: {e}");
        "Não foi possível iniciar a transcrição.".to_string()
    })?;
    job.assign(&child)?;

    // O progresso sai no stderr ("whisper_print_progress_callback: progress = 42%").
    let stderr = child.stderr.take();
    let reader_done = Arc::new(AtomicBool::new(false));
    let reader_flag = Arc::clone(&reader_done);
    let (tx, rx) = std::sync::mpsc::channel::<u8>();
    let reader = std::thread::spawn(move || {
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if let Some(p) = parse_progress(&line) {
                    let _ = tx.send(p);
                }
            }
        }
        reader_flag.store(true, Ordering::Relaxed);
    });

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err("Transcrição cancelada.".into());
        }
        while let Ok(p) = rx.try_recv() {
            on_progress(p);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = reader.join();
                while let Ok(p) = rx.try_recv() {
                    on_progress(p);
                }
                if !status.success() {
                    tracing::error!("whisper-cli terminou com código {status}");
                    return Err("A transcrição falhou. Tente novamente; se persistir, troque o modelo de transcrição.".into());
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(300)),
            Err(_) => return Err("Falha ao acompanhar a transcrição.".into()),
        }
    }

    let json_path = with_json_ext(run.out_prefix);
    if !json_path.exists() {
        return Err("A transcrição terminou sem resultado. Verifique se o áudio tem fala.".into());
    }
    Ok(json_path)
}

fn with_json_ext(prefix: &Path) -> PathBuf {
    let mut s = prefix.as_os_str().to_os_string();
    s.push(".json");
    PathBuf::from(s)
}

fn parse_progress(line: &str) -> Option<u8> {
    let idx = line.find("progress =")?;
    let rest = line[idx + "progress =".len()..].trim();
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    num.parse::<u8>().ok().map(|p| p.min(100))
}

/// Formata milissegundos como "MM:SS" ou "HH:MM:SS" (para prompts e exibição).
pub fn fmt_time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_whisper_json_with_offset_and_confidence() {
        let json = r#"{
          "transcription": [
            {"offsets":{"from":0,"to":2000},"text":" Bom dia pessoal.",
             "tokens":[{"text":"[_BEG_]","p":0.99},{"text":" Bom","p":0.9},{"text":" dia","p":0.7}]},
            {"offsets":{"from":2000,"to":3000},"text":"   ","tokens":[]}
          ]
        }"#;
        let segs = parse_output(json, "microfone", 5000).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 5000);
        assert_eq!(segs[0].end_ms, 7000);
        assert_eq!(segs[0].text, "Bom dia pessoal.");
        let c = segs[0].confidence.unwrap();
        assert!((c - 0.8).abs() < 1e-9);
    }

    #[test]
    fn invalid_json_is_friendly_error() {
        let err = parse_output("{ruim", "sistema", 0).unwrap_err();
        assert!(!err.contains("serde"));
    }

    #[test]
    fn reads_progress_lines() {
        assert_eq!(parse_progress("whisper_print_progress_callback: progress =  42%"), Some(42));
        assert_eq!(parse_progress("qualquer outra linha"), None);
    }

    #[test]
    fn formats_time() {
        assert_eq!(fmt_time(65_000), "01:05");
        assert_eq!(fmt_time(3_725_000), "01:02:05");
    }
}
