//! Sessão de gravação: une microfone e áudio do computador, mantém o relógio
//! comum e finaliza os arquivos.
//!
//! Durante a gravação cada fonte escreve `*.pcm` (PCM 16 bits, 16 kHz, mono).
//! Ao parar, o `.pcm` vira `.wav` com cabeçalho. Se o app fechar no meio,
//! o `.pcm` continua utilizável e é convertido na próxima abertura.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Local};

use crate::capture::{CaptureStream, Source, SAMPLE_RATE};

pub const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub struct Active {
    pub meeting_id: String,
    pub title: String,
    pub started_at: DateTime<Local>,
    pub clock: Instant,
    pub dir: PathBuf,
    pub streams: Vec<CaptureStream>,
    pub paused: bool,
}

pub fn pcm_path(dir: &Path, source: Source) -> PathBuf {
    dir.join(format!("{}.pcm", source.as_str()))
}

pub fn wav_path(dir: &Path, source: Source) -> PathBuf {
    dir.join(format!("{}.wav", source.as_str()))
}

/// Cabeçalho WAV PCM 16 bits mono 16 kHz para `data_len` bytes de dados.
pub fn wav_header(data_len: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    let byte_rate = SAMPLE_RATE * 2;
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    h[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes()); // alinhamento do bloco
    h[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits por amostra
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

/// Converte `.pcm` em `.wav` e remove o `.pcm` só depois de escrever o `.wav` por completo.
/// Devolve a duração em milissegundos.
pub fn finalize_wav(pcm: &Path, wav: &Path) -> io::Result<i64> {
    let len = std::fs::metadata(pcm)?.len();
    // Dados de PCM precisam ser pares (16 bits). Descarta um byte sobrando se existir.
    let data_len = (len / 2 * 2).min(u32::MAX as u64 - 36) as u32;

    let tmp = wav.with_extension("wav.tmp");
    {
        let mut src = BufReader::new(File::open(pcm)?);
        let mut dst = BufWriter::new(File::create(&tmp)?);
        dst.write_all(&wav_header(data_len))?;
        let mut limited = (&mut src).take(data_len as u64);
        io::copy(&mut limited, &mut dst)?;
        dst.flush()?;
    }
    std::fs::rename(&tmp, wav)?;
    let _ = std::fs::remove_file(pcm);
    Ok(duration_ms(data_len as u64))
}

pub fn duration_ms(data_bytes: u64) -> i64 {
    (data_bytes / 2 * 1000 / SAMPLE_RATE as u64) as i64
}

/// Verifica espaço livre antes de começar. Falha com mensagem para o usuário.
pub fn check_disk(dir: &Path) -> Result<(), String> {
    let free = fs2::available_space(dir).unwrap_or(u64::MAX);
    if free < MIN_FREE_BYTES {
        return Err(format!(
            "Pouco espaço livre no disco ({} MB). Libere pelo menos 2 GB antes de gravar.",
            free / 1024 / 1024
        ));
    }
    Ok(())
}

/// Recupera gravações que ficaram abertas (app fechado ou travado no meio).
/// Devolve os ids das reuniões recuperadas e a duração de cada uma.
pub fn recover_all(dir_root: &Path, interrupted: &[(String, String)]) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for (id, folder) in interrupted {
        let dir = if Path::new(folder).is_absolute() {
            PathBuf::from(folder)
        } else {
            dir_root.join(folder)
        };
        let mut best: i64 = 0;
        for source in [Source::Mic, Source::System] {
            let pcm = pcm_path(&dir, source);
            let wav = wav_path(&dir, source);
            if pcm.exists() {
                if let Ok(ms) = finalize_wav(&pcm, &wav) {
                    best = best.max(ms);
                }
            }
        }
        out.push((id.clone(), best));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_44_bytes_and_consistent() {
        let h = wav_header(32000);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(h[40..44].try_into().unwrap()), 32000);
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 36 + 32000);
    }

    #[test]
    fn one_second_is_one_second() {
        assert_eq!(duration_ms(32_000), 1000);
    }

    #[test]
    fn pcm_becomes_valid_wav() {
        let dir = std::env::temp_dir().join("lmai-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let pcm = dir.join("t.pcm");
        let wav = dir.join("t.wav");
        std::fs::write(&pcm, vec![1u8; 64_001]).unwrap(); // byte sobrando
        let ms = finalize_wav(&pcm, &wav).unwrap();
        assert_eq!(ms, duration_ms(64_000));
        assert_eq!(std::fs::metadata(&wav).unwrap().len(), 44 + 64_000);
        assert!(!pcm.exists());
        assert_eq!(ms, duration_ms(64_000));
    }
}
