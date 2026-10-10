//! Supervisor do `llama-server` (IA local). Sobe sob demanda, espera ficar
//! pronto, reinicia com backoff se cair e é encerrado junto com o app.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Mutex;

use super::job::JobObject;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const MAX_RESTARTS: u32 = 3;
const READY_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "estado", rename_all = "lowercase")]
pub enum ServerState {
    Parado,
    Iniciando,
    Pronto,
    Reiniciando,
    Falhou { mensagem: String },
}

struct Inner {
    child: Option<Child>,
    job: Option<JobObject>,
    port: u16,
    model: Option<PathBuf>,
    wanted: bool,
    restarts: u32,
    state: ServerState,
}

pub struct LlamaServer {
    exe: PathBuf,
    inner: Mutex<Inner>,
    http: reqwest::Client,
}

impl LlamaServer {
    pub fn new(runtime_dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            exe: runtime_dir.join("llama-server.exe"),
            inner: Mutex::new(Inner {
                child: None,
                job: None,
                port: 0,
                model: None,
                wanted: false,
                restarts: 0,
                state: ServerState::Parado,
            }),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("cliente HTTP local"),
        })
    }

    pub async fn state(&self) -> ServerState {
        self.inner.lock().await.state.clone()
    }

    /// Garante que o servidor está pronto com o modelo indicado e devolve a porta.
    pub async fn ensure_running(self: &Arc<Self>, model: &Path) -> Result<u16, String> {
        if !self.exe.exists() {
            return Err("O mecanismo de IA não foi encontrado. Reinstale o aplicativo.".into());
        }
        {
            let mut inner = self.inner.lock().await;
            inner.wanted = true;
            inner.model = Some(model.to_path_buf());
            if inner.state == ServerState::Pronto && self.is_alive(&mut inner) {
                return Ok(inner.port);
            }
        }
        self.spawn_and_wait().await?;
        self.start_monitor();
        Ok(self.inner.lock().await.port)
    }

    /// Encerra o servidor sem reiniciar. Chamado ao fechar o app.
    pub async fn stop(&self) {
        let mut inner = self.inner.lock().await;
        inner.wanted = false;
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        inner.job = None;
        inner.state = ServerState::Parado;
        tracing::info!("servidor de IA encerrado");
    }

    pub fn base_url(port: u16) -> String {
        format!("http://127.0.0.1:{port}")
    }

    async fn spawn_and_wait(self: &Arc<Self>) -> Result<(), String> {
        let (port, model) = {
            let mut inner = self.inner.lock().await;
            inner.state = ServerState::Iniciando;
            let model = inner.model.clone().ok_or("Nenhum modelo de IA selecionado.")?;
            (free_port()?, model)
        };

        let mut cmd = Command::new(&self.exe);
        cmd.arg("--model")
            .arg(&model)
            .args(["--host", "127.0.0.1"])
            .args(["--port", &port.to_string()])
            .args(["--ctx-size", "8192"])
            .arg("--no-webui")
            .args(["--parallel", "1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let child = cmd.spawn().map_err(|e| {
            tracing::error!("falha ao iniciar llama-server: {e}");
            "Não foi possível iniciar a inteligência local.".to_string()
        })?;

        let job = JobObject::new()?;
        job.assign(&child)?;

        {
            let mut inner = self.inner.lock().await;
            inner.child = Some(child);
            inner.job = Some(job);
            inner.port = port;
        }

        self.wait_ready(port).await?;

        let mut inner = self.inner.lock().await;
        inner.state = ServerState::Pronto;
        tracing::info!(port, "servidor de IA pronto");
        Ok(())
    }

    async fn wait_ready(&self, port: u16) -> Result<(), String> {
        let url = format!("{}/health", Self::base_url(port));
        let started = std::time::Instant::now();
        loop {
            if started.elapsed() > READY_TIMEOUT {
                return Err("A inteligência local demorou demais para carregar. Tente novamente.".into());
            }
            // O processo pode ter morrido (modelo corrompido, falta de memória).
            {
                let mut inner = self.inner.lock().await;
                if let Some(child) = inner.child.as_mut() {
                    if let Ok(Some(status)) = child.try_wait() {
                        tracing::error!("llama-server encerrou durante a carga: {status}");
                        return Err(
                            "Não foi possível carregar a inteligência local. O arquivo do modelo pode estar corrompido; baixe-o novamente nas configurações."
                                .into(),
                        );
                    }
                }
            }
            if let Ok(resp) = self.http.get(&url).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    fn is_alive(&self, inner: &mut Inner) -> bool {
        match inner.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Acompanha o processo: se cair enquanto deveria estar rodando, reinicia com limite.
    fn start_monitor(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let mut inner = this.inner.lock().await;
                if !inner.wanted {
                    break;
                }
                let alive = this.is_alive(&mut inner);
                if alive {
                    if inner.state == ServerState::Reiniciando {
                        inner.state = ServerState::Pronto;
                    }
                    continue;
                }
                if inner.restarts >= MAX_RESTARTS {
                    inner.state = ServerState::Falhou {
                        mensagem: "A inteligência local parou repetidamente. Reabra o aplicativo.".into(),
                    };
                    inner.wanted = false;
                    tracing::error!("servidor de IA atingiu o limite de reinícios");
                    break;
                }
                inner.restarts += 1;
                inner.state = ServerState::Reiniciando;
                inner.child = None;
                inner.job = None;
                drop(inner);

                tracing::warn!("servidor de IA caiu; reiniciando ({})", this.inner.lock().await.restarts);
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Err(e) = this.spawn_and_wait().await {
                    let mut inner = this.inner.lock().await;
                    inner.state = ServerState::Falhou { mensagem: e };
                }
            }
        });
    }
}

fn free_port() -> Result<u16, String> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|_| "Não foi possível reservar uma porta local.".to_string())
}
