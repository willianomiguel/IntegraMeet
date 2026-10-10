//! Captura de áudio via WASAPI.
//!
//! - Microfone: dispositivo de captura padrão (ou escolhido).
//! - Áudio do computador: loopback do dispositivo de saída padrão (o que as
//!   outras pessoas da reunião falam chega por aqui).
//!
//! Formato de escrita: PCM 16 bits, 16 kHz, mono. O Windows converte no
//! autoconvert, então não há resampling manual. Esse é o formato que o
//! whisper.cpp consome diretamente.
//!
//! Timeline: o arquivo acompanha o relógio de parede. Quando o dispositivo fica
//! em silêncio, sem entregar buffers, ou quando a gravação está pausada, são
//! escritos zeros. Assim microfone e sistema continuam alinhados.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

pub const SAMPLE_RATE: u32 = 16_000;
const RECONNECT_WINDOW: Duration = Duration::from_secs(30);
const START_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Mic,
    System,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Mic => "microfone",
            Source::System => "sistema",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "estado", rename_all = "lowercase")]
pub enum StreamStatus {
    Iniciando,
    Ativo,
    Reconectando { mensagem: String },
    Falhou { mensagem: String },
    Parado,
}

/// Lista os microfones disponíveis.
pub fn list_microphones() -> Result<Vec<DeviceInfo>, String> {
    run_com(|| {
        let en = DeviceEnumerator::new().map_err(|_| sem_audio())?;
        let default_id = en
            .get_default_device(&Direction::Capture)
            .and_then(|d| d.get_id())
            .unwrap_or_default();
        let collection = en
            .get_device_collection(&Direction::Capture)
            .map_err(|_| sem_audio())?;
        let mut out = Vec::new();
        for dev in &collection {
            let Ok(dev) = dev else { continue };
            let id = dev.get_id().unwrap_or_default();
            let name = dev.get_friendlyname().unwrap_or_else(|_| "Microfone".into());
            out.push(DeviceInfo {
                is_default: id == default_id,
                id,
                name,
            });
        }
        Ok(out)
    })
}

/// Nome do dispositivo de saída padrão (usado para explicar o áudio do computador).
pub fn default_output_name() -> Option<String> {
    run_com(|| {
        let en = DeviceEnumerator::new().map_err(|_| sem_audio())?;
        let dev = en
            .get_default_device(&Direction::Render)
            .map_err(|_| sem_audio())?;
        dev.get_friendlyname().map_err(|_| sem_audio())
    })
    .ok()
}

fn sem_audio() -> String {
    "Não foi possível acessar o áudio do Windows.".into()
}

fn run_com<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _ = wasapi::initialize_mta().ok();
    f()
}

/// Nível de áudio (RMS 0..1) guardado como bits de f32 para leitura sem lock.
#[derive(Default)]
pub struct Level(AtomicU32);

impl Level {
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
}

pub struct CaptureStream {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    pub level: Arc<Level>,
    status: Arc<Mutex<StreamStatus>>,
    thread: Option<JoinHandle<()>>,
    pub source: Source,
    pub device_name: String,
}

pub struct StartParams {
    pub source: Source,
    pub device_id: Option<String>,
    pub pcm_path: PathBuf,
    pub clock_start: Instant,
}

impl CaptureStream {
    /// Inicia a captura e só retorna quando o stream está aberto. Se não abrir, retorna erro.
    pub fn start(params: StartParams) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let level = Arc::new(Level::default());
        let status = Arc::new(Mutex::new(StreamStatus::Iniciando));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<String, String>>();

        let ctx = Ctx {
            stop: Arc::clone(&stop),
            paused: Arc::clone(&paused),
            level: Arc::clone(&level),
            status: Arc::clone(&status),
            source: params.source,
            device_id: params.device_id.clone(),
            pcm_path: params.pcm_path.clone(),
            clock_start: params.clock_start,
        };

        let thread = std::thread::Builder::new()
            .name(format!("captura-{}", params.source.as_str()))
            .spawn(move || run(ctx, ready_tx))
            .map_err(|_| "Não foi possível iniciar a captura de áudio.".to_string())?;

        let device_name = match ready_rx.recv_timeout(START_TIMEOUT) {
            Ok(Ok(name)) => name,
            Ok(Err(msg)) => {
                let _ = thread.join();
                return Err(msg);
            }
            Err(_) => {
                stop.store(true, Ordering::Relaxed);
                return Err("O dispositivo de áudio não respondeu a tempo. Verifique a conexão.".into());
            }
        };

        Ok(Self {
            stop,
            paused,
            level,
            status,
            thread: Some(thread),
            source: params.source,
            device_name,
        })
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        if paused {
            self.level.set(0.0);
        }
    }

    pub fn status(&self) -> StreamStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(StreamStatus::Parado)
    }

    /// Para a captura e espera a thread escrever o último bloco.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Ok(mut s) = self.status.lock() {
            *s = StreamStatus::Parado;
        }
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Ctx {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    level: Arc<Level>,
    status: Arc<Mutex<StreamStatus>>,
    source: Source,
    device_id: Option<String>,
    pcm_path: PathBuf,
    clock_start: Instant,
}

fn set_status(status: &Mutex<StreamStatus>, s: StreamStatus) {
    if let Ok(mut g) = status.lock() {
        *g = s;
    }
}

fn run(ctx: Ctx, ready: std::sync::mpsc::Sender<Result<String, String>>) {
    let _ = wasapi::initialize_mta().ok();

    let file = match File::create(&ctx.pcm_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("não foi possível criar {}: {e}", ctx.pcm_path.display());
            let _ = ready.send(Err("Não foi possível criar o arquivo da gravação. Verifique o espaço em disco.".into()));
            return;
        }
    };
    let mut out = BufWriter::new(file);
    let mut written: u64 = 0;
    let mut first_open = true;
    let mut lost_since: Option<Instant> = None;

    while !ctx.stop.load(Ordering::Relaxed) {
        match open_and_capture(&ctx, &mut out, &mut written, &ready, first_open, &mut lost_since) {
            Ok(()) => break, // parada solicitada
            Err(msg) => {
                if first_open {
                    let _ = ready.send(Err(msg));
                    return;
                }
                // Dispositivo caiu durante a gravação: tenta reconectar por um tempo.
                let since = *lost_since.get_or_insert_with(Instant::now);
                if since.elapsed() > RECONNECT_WINDOW {
                    tracing::error!(source = ctx.source.as_str(), "captura não se recuperou: {msg}");
                    set_status(&ctx.status, StreamStatus::Falhou {
                        mensagem: format!("O {} foi desconectado e não voltou. Verifique o dispositivo.", ctx.source.as_str()),
                    });
                    break;
                }
                set_status(&ctx.status, StreamStatus::Reconectando { mensagem: msg });
                pad_to_clock(&mut out, &mut written, ctx.clock_start);
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        first_open = false;
    }

    pad_to_clock(&mut out, &mut written, ctx.clock_start);
    let _ = out.flush();
}

/// Abre o dispositivo e captura até parada ou erro. Erro = dispositivo perdido.
fn open_and_capture(
    ctx: &Ctx,
    out: &mut BufWriter<File>,
    written: &mut u64,
    ready: &std::sync::mpsc::Sender<Result<String, String>>,
    announce: bool,
    lost_since: &mut Option<Instant>,
) -> Result<(), String> {
    let en = DeviceEnumerator::new().map_err(|_| sem_audio())?;
    let device = match (&ctx.device_id, ctx.source) {
        (Some(id), _) => en.get_device(id).map_err(|_| "O microfone escolhido não está mais disponível.".to_string())?,
        (None, Source::Mic) => en
            .get_default_device(&Direction::Capture)
            .map_err(|_| "Nenhum microfone encontrado. Conecte um microfone e tente novamente.".to_string())?,
        // Loopback: dispositivo de saída (render) aberto em modo captura.
        (None, Source::System) => en
            .get_default_device(&Direction::Render)
            .map_err(|_| "Nenhuma saída de áudio encontrada.".to_string())?,
    };
    let device_name = device.get_friendlyname().unwrap_or_else(|_| ctx.source.as_str().into());

    let mut client = device.get_iaudioclient().map_err(|_| sem_audio())?;
    let format = WaveFormat::new(16, 16, &SampleType::Int, SAMPLE_RATE as usize, 1, None);
    let (_default, min_period) = client.get_device_period().map_err(|_| sem_audio())?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: min_period,
    };
    client
        .initialize_client(&format, &Direction::Capture, &mode)
        .map_err(|_| "Este dispositivo de áudio não aceita a configuração da gravação.".to_string())?;

    let event = client.set_get_eventhandle().map_err(|_| sem_audio())?;
    let capture = client.get_audiocaptureclient().map_err(|_| sem_audio())?;
    client.start_stream().map_err(|_| "Não foi possível iniciar a captura neste dispositivo.".to_string())?;

    if announce {
        let _ = ready.send(Ok(device_name.clone()));
    }
    // Reconectou: zera o temporizador de perda.
    *lost_since = None;
    set_status(&ctx.status, StreamStatus::Ativo);
    tracing::info!(source = ctx.source.as_str(), "captura iniciada");

    let mut queue: VecDeque<u8> = VecDeque::with_capacity(64 * 1024);
    let mut last_flush = Instant::now();

    loop {
        if ctx.stop.load(Ordering::Relaxed) {
            let _ = client.stop_stream();
            return Ok(());
        }

        capture
            .read_from_device_to_deque(&mut queue)
            .map_err(|_| "O dispositivo de áudio foi desconectado.".to_string())?;

        let mut samples: Vec<i16> = Vec::with_capacity(queue.len() / 2);
        while queue.len() >= 2 {
            let lo = queue.pop_front().unwrap_or(0);
            let hi = queue.pop_front().unwrap_or(0);
            samples.push(i16::from_le_bytes([lo, hi]));
        }

        if samples.is_empty() {
            // Sem dados: pode ser silêncio (loopback) ou dispositivo parado.
            pad_to_clock(out, written, ctx.clock_start);
        } else if ctx.paused.load(Ordering::Relaxed) {
            ctx.level.set(0.0);
            // Pausado: zeros no lugar do áudio, para o tempo continuar correto.
            for _ in 0..samples.len() {
                out.write_all(&0i16.to_le_bytes()).map_err(|_| disco_cheio())?;
            }
            *written += samples.len() as u64;
            pad_to_clock(out, written, ctx.clock_start);
        } else {
            ctx.level.set(rms(&samples));
            for s in &samples {
                out.write_all(&s.to_le_bytes()).map_err(|_| disco_cheio())?;
            }
            *written += samples.len() as u64;
            pad_to_clock(out, written, ctx.clock_start);
        }

        if last_flush.elapsed() >= Duration::from_secs(2) {
            out.flush().map_err(|_| disco_cheio())?;
            last_flush = Instant::now();
        }

        // Timeout não é erro: loopback em silêncio pode não entregar buffers.
        let _ = event.wait_for_event(200);
    }
}

/// Escreve zeros até acompanhar o relógio de parede (com tolerância de 100 ms).
fn pad_to_clock(out: &mut BufWriter<File>, written: &mut u64, clock_start: Instant) {
    let expected = (clock_start.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64;
    if expected > *written + (SAMPLE_RATE as u64 / 10) {
        let gap = (expected - *written).min(SAMPLE_RATE as u64 * 5);
        let zeros = vec![0u8; (gap * 2) as usize];
        if out.write_all(&zeros).is_ok() {
            *written += gap;
        }
    }
}

fn rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| {
        let v = *s as f64 / 32768.0;
        v * v
    }).sum();
    ((sum / samples.len() as f64).sqrt() as f32).min(1.0)
}

fn disco_cheio() -> String {
    "Sem espaço no disco. A gravação foi salva até este ponto.".into()
}
