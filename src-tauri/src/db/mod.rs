//! Banco SQLite local com migrações versionadas (PRAGMA user_version).
//! Migrações são somente para frente e nunca apagam reuniões. Antes de migrar
//! uma base existente, um backup é feito ao lado do arquivo.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

pub mod meetings;

/// Migrações em ordem. Cada entrada leva o banco da versão i para i+1.
const MIGRATIONS: &[&str] = &[
    // v1: esquema inicial
    r#"
    CREATE TABLE meetings (
        id          TEXT PRIMARY KEY,
        title       TEXT NOT NULL,
        started_at  TEXT NOT NULL,
        ended_at    TEXT,
        duration_ms INTEGER NOT NULL DEFAULT 0,
        status      TEXT NOT NULL,
        folder      TEXT NOT NULL,
        error       TEXT
    );

    CREATE TABLE recordings (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        source      TEXT NOT NULL,
        device_name TEXT NOT NULL,
        file_path   TEXT NOT NULL,
        offset_ms   INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE segments (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        source      TEXT NOT NULL,
        start_ms    INTEGER NOT NULL,
        end_ms      INTEGER NOT NULL,
        text        TEXT NOT NULL,
        confidence  REAL
    );
    CREATE INDEX segments_meeting ON segments(meeting_id, start_ms);

    -- Índice de pesquisa do texto das transcrições (FTS5).
    CREATE VIRTUAL TABLE segments_fts USING fts5(
        text,
        meeting_id UNINDEXED,
        segment_id UNINDEXED,
        tokenize = 'unicode61 remove_diacritics 2'
    );

    CREATE TABLE analyses (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        created_at  TEXT NOT NULL,
        model       TEXT NOT NULL,
        summary     TEXT NOT NULL,
        topics      TEXT NOT NULL DEFAULT '[]'
    );

    CREATE TABLE analysis_items (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id   TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        kind         TEXT NOT NULL,
        text         TEXT NOT NULL,
        owner        TEXT,
        due          TEXT,
        segment_ids  TEXT NOT NULL DEFAULT '[]'
    );
    CREATE INDEX analysis_items_meeting ON analysis_items(meeting_id, kind);

    CREATE TABLE jobs (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        kind        TEXT NOT NULL,
        meeting_id  TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
        status      TEXT NOT NULL,
        attempts    INTEGER NOT NULL DEFAULT 0,
        error       TEXT,
        created_at  TEXT NOT NULL,
        updated_at  TEXT NOT NULL
    );
    "#,
];

pub fn open(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| format!("Não foi possível abrir o banco de dados: {e}"))?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Falha ao configurar o banco: {e}"))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("Falha ao configurar o banco: {e}"))?;
    migrate(&conn, path)?;
    Ok(conn)
}

fn current_version(conn: &Connection) -> Result<usize, String> {
    conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
        .map(|v| v as usize)
        .map_err(|e| format!("Falha ao ler a versão do banco: {e}"))
}

fn migrate(conn: &Connection, path: &Path) -> Result<(), String> {
    let from = current_version(conn)?;
    let target = MIGRATIONS.len();
    if from == target {
        return Ok(());
    }
    if from > target {
        return Err("Esta base foi criada por uma versão mais nova do aplicativo. Atualize o programa.".into());
    }

    // Backup antes de alterar uma base que já tem dados.
    if from > 0 {
        let backup: PathBuf = path.with_extension(format!("db.bak-v{from}"));
        if !backup.exists() {
            std::fs::copy(path, &backup).map_err(|e| format!("Não foi possível criar a cópia de segurança: {e}"))?;
            tracing::info!("cópia de segurança criada antes da migração");
        }
    }

    for (i, sql) in MIGRATIONS.iter().enumerate().skip(from) {
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("Falha na migração: {e}"))?;
        tx.execute_batch(sql)
            .map_err(|e| format!("Falha na migração v{}: {e}", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)
            .map_err(|e| format!("Falha na migração v{}: {e}", i + 1))?;
        tx.commit().map_err(|e| format!("Falha na migração v{}: {e}", i + 1))?;
        tracing::info!(version = i + 1, "migração aplicada");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_on_fresh_database() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        for sql in MIGRATIONS {
            conn.execute_batch(sql).unwrap();
        }
        let tables: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type IN ('table')", [], |r| r.get(0))
            .unwrap();
        assert!(tables >= 6);
    }

    #[test]
    fn fts_finds_portuguese_text_without_accents() {
        let conn = Connection::open_in_memory().unwrap();
        for sql in MIGRATIONS {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute(
            "INSERT INTO segments_fts(text, meeting_id, segment_id) VALUES ('Decisão sobre o orçamento', 'm1', 1)",
            [],
        )
        .unwrap();
        let hits: i64 = conn
            .query_row("SELECT count(*) FROM segments_fts WHERE segments_fts MATCH 'decisao'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hits, 1);
    }
}
