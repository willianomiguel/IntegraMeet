//! Logs técnicos rotativos por dia. Nunca registrar conteúdo de reunião
//! (áudio, transcrição, resumo): apenas eventos, estados e erros.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Inicializa o logger. O guard precisa viver até o fim do processo,
/// senão as últimas linhas se perdem na saída.
pub fn init(logs_dir: &Path) -> Option<WorkerGuard> {
    let appender = tracing_appender::rolling::daily(logs_dir, "app.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(writer).with_ansi(false))
        .try_init()
        .ok()?;

    Some(guard)
}
