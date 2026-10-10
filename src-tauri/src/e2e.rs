//! Teste de ponta a ponta com os binários e modelos reais. Não roda por padrão
//! (baixa ~2,6 GB). Para executar:
//!   cargo test pipeline_real -- --ignored --nocapture
//! Variáveis opcionais: LMAI_E2E_WAV (WAV 16 kHz mono) e LMAI_E2E_MODELS (pasta de modelos).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::analysis;
use crate::db::meetings::Segment;
use crate::models::{catalog, download};
use crate::runtime::job::JobObject;
use crate::runtime::llama::LlamaServer;
use crate::transcription::{self, WhisperRun};

fn runtime_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("runtime")
}

fn models_root() -> PathBuf {
    std::env::var("LMAI_E2E_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("lmai-e2e-models"))
}

fn wav_path() -> PathBuf {
    std::env::var("LMAI_E2E_WAV")
        .map(PathBuf::from)
        .expect("defina LMAI_E2E_WAV com o caminho de um WAV 16 kHz mono")
}

#[test]
#[ignore = "baixa modelos (~2,6 GB) e executa whisper e llama reais"]
fn pipeline_real_transcription_and_analysis() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let client = reqwest::Client::new();
    let root = models_root();
    std::fs::create_dir_all(&root).unwrap();

    // 1) Baixa os modelos com o mesmo código usado pelo app.
    for id in ["whisper-small", "qwen2.5-3b"] {
        let spec = catalog::find(id).unwrap();
        if download::is_installed(&root, spec) {
            println!("já instalado: {id}");
            continue;
        }
        println!("baixando {id} ({} MB)…", spec.total_size() / 1024 / 1024);
        rt.block_on(download::download_model(
            &client,
            &root,
            spec,
            Arc::new(AtomicBool::new(false)),
            |p| {
                if p.state == "concluido" {
                    println!("  concluído: {}", p.file_name);
                }
            },
        ))
        .expect("download do modelo");
        assert!(download::is_installed(&root, spec));
    }

    // 2) Transcrição com whisper-cli.
    let whisper_spec = catalog::find("whisper-small").unwrap();
    let whisper_model = download::file_path(&root, whisper_spec, &whisper_spec.files[0]);
    let wav = wav_path();
    let prefix = std::env::temp_dir().join("lmai-e2e-transcricao");
    let job = JobObject::new().unwrap();
    let cancel = AtomicBool::new(false);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let run = WhisperRun {
        exe: runtime_dir().join("whisper").join("whisper-cli.exe"),
        model: &whisper_model,
        wav: &wav,
        out_prefix: &prefix,
        threads,
    };
    let json_path = transcription::run_whisper(&run, &cancel, &job, |p| {
        if p % 25 == 0 {
            println!("  progresso da transcrição: {p}%");
        }
    })
    .expect("whisper-cli");
    let json = std::fs::read_to_string(json_path).unwrap();
    let raw = transcription::parse_output(&json, "microfone", 0).expect("parse");
    assert!(!raw.is_empty(), "nenhum segmento transcrito");
    println!("\n--- TRANSCRIÇÃO ({} segmentos) ---", raw.len());
    for s in &raw {
        println!(
            "[{} - {}] ({:.2}) {}",
            transcription::fmt_time(s.start_ms),
            transcription::fmt_time(s.end_ms),
            s.confidence.unwrap_or(0.0),
            s.text
        );
    }

    let segments: Vec<Segment> = raw
        .iter()
        .enumerate()
        .map(|(i, s)| Segment {
            id: i as i64 + 1,
            source: s.source.clone(),
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text.clone(),
            confidence: s.confidence,
        })
        .collect();

    // 3) Análise com llama-server (Vulkan) e validação anti-invenção.
    let llm_spec = catalog::find("qwen2.5-3b").unwrap();
    let llm_model = download::file_path(&root, llm_spec, &llm_spec.files[0]);
    let server = LlamaServer::new(&runtime_dir().join("llama"));
    println!("\nsubindo llama-server…");
    let port = rt.block_on(server.ensure_running(&llm_model)).expect("llama-server");
    let base = LlamaServer::base_url(port);

    let mut parts = Vec::new();
    for chunk in analysis::build_chunks(&segments) {
        let user = analysis::user_prompt(&chunk);
        let content = rt
            .block_on(analysis::chat(&client, &base, analysis::system_prompt(), &user))
            .expect("chat");
        println!("\n--- RESPOSTA BRUTA ---\n{content}");
        match analysis::validate(&content, &segments) {
            Ok(a) => parts.push(a),
            Err(e) => println!("resposta descartada: {e}"),
        }
    }
    rt.block_on(server.stop());

    let merged = analysis::merge(parts);
    println!("\n--- ANÁLISE VALIDADA ---");
    println!("Resumo: {}", merged.summary);
    println!("Assuntos: {:?}", merged.topics);
    for i in &merged.items {
        println!(
            "- [{}] {} | responsável: {:?} | prazo: {:?} | segmentos: {:?}",
            i.kind, i.text, i.owner, i.due, i.segment_ids
        );
    }
    assert!(!merged.items.is_empty(), "a análise não encontrou nenhum item com evidência");
}
