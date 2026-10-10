//! Consultas de reuniões, segmentos, análises, fila de jobs e pesquisa.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Gravando,
    GravacaoInterrompida,
    Pronta,
    Transcrevendo,
    Transcrita,
    Analisando,
    Analisada,
    Erro,
}

impl MeetingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gravando => "gravando",
            Self::GravacaoInterrompida => "gravacao_interrompida",
            Self::Pronta => "pronta",
            Self::Transcrevendo => "transcrevendo",
            Self::Transcrita => "transcrita",
            Self::Analisando => "analisando",
            Self::Analisada => "analisada",
            Self::Erro => "erro",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "gravando" => Self::Gravando,
            "gravacao_interrompida" => Self::GravacaoInterrompida,
            "pronta" => Self::Pronta,
            "transcrevendo" => Self::Transcrevendo,
            "transcrita" => Self::Transcrita,
            "analisando" => Self::Analisando,
            "analisada" => Self::Analisada,
            _ => Self::Erro,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub duration_ms: i64,
    pub status: MeetingStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingRow {
    pub source: String,
    pub device_name: String,
    pub file_path: String,
    pub offset_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: i64,
    pub source: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisItem {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub owner: Option<String>,
    pub due: Option<String>,
    pub segment_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub created_at: String,
    pub model: String,
    pub summary: String,
    pub topics: Vec<String>,
    pub items: Vec<AnalysisItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: i64,
    pub status: MeetingStatus,
    pub error: Option<String>,
    pub folder: String,
    pub recordings: Vec<RecordingRow>,
    pub segments: Vec<Segment>,
    pub analysis: Option<Analysis>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub started_at: String,
    /// Trecho com o termo destacado entre << >>.
    pub snippet: String,
    pub segment_id: Option<i64>,
    pub start_ms: Option<i64>,
}

pub struct NewMeeting<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub started_at: &'a str,
    pub status: MeetingStatus,
    pub folder: &'a str,
}

pub fn insert_meeting(conn: &Connection, m: &NewMeeting) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meetings (id, title, started_at, status, folder) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![m.id, m.title, m.started_at, m.status.as_str(), m.folder],
    )?;
    Ok(())
}

pub fn insert_recording(
    conn: &Connection,
    meeting_id: &str,
    source: &str,
    device_name: &str,
    file_path: &str,
    offset_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO recordings (meeting_id, source, device_name, file_path, offset_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![meeting_id, source, device_name, file_path, offset_ms],
    )?;
    Ok(())
}

pub fn set_status(conn: &Connection, id: &str, status: &MeetingStatus, error: Option<&str>) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meetings SET status = ?2, error = ?3 WHERE id = ?1",
        params![id, status.as_str(), error],
    )?;
    Ok(())
}

pub fn finish_recording(conn: &Connection, id: &str, ended_at: &str, duration_ms: i64, status: &MeetingStatus) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE meetings SET ended_at = ?2, duration_ms = ?3, status = ?4 WHERE id = ?1",
        params![id, ended_at, duration_ms, status.as_str()],
    )?;
    Ok(())
}

pub fn rename(conn: &Connection, id: &str, title: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE meetings SET title = ?2 WHERE id = ?1", params![id, title])?;
    Ok(())
}

pub fn list(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<MeetingSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, started_at, duration_ms, status FROM meetings ORDER BY started_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(MeetingSummary {
            id: r.get(0)?,
            title: r.get(1)?,
            started_at: r.get(2)?,
            duration_ms: r.get(3)?,
            status: MeetingStatus::parse(&r.get::<_, String>(4)?),
        })
    })?;
    rows.collect()
}

pub fn detail(conn: &Connection, id: &str) -> rusqlite::Result<Option<MeetingDetail>> {
    let base = conn
        .query_row(
            "SELECT id, title, started_at, ended_at, duration_ms, status, error, folder FROM meetings WHERE id = ?1",
            params![id],
            |r| {
                Ok(MeetingDetail {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    started_at: r.get(2)?,
                    ended_at: r.get(3)?,
                    duration_ms: r.get(4)?,
                    status: MeetingStatus::parse(&r.get::<_, String>(5)?),
                    error: r.get(6)?,
                    folder: r.get(7)?,
                    recordings: vec![],
                    segments: vec![],
                    analysis: None,
                })
            },
        )
        .optional()?;

    let Some(mut m) = base else { return Ok(None) };

    let mut stmt = conn.prepare(
        "SELECT source, device_name, file_path, offset_ms FROM recordings WHERE meeting_id = ?1 ORDER BY id",
    )?;
    m.recordings = stmt
        .query_map(params![id], |r| {
            Ok(RecordingRow {
                source: r.get(0)?,
                device_name: r.get(1)?,
                file_path: r.get(2)?,
                offset_ms: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    m.segments = segments(conn, id)?;
    m.analysis = analysis(conn, id)?;
    Ok(Some(m))
}

pub fn segments(conn: &Connection, meeting_id: &str) -> rusqlite::Result<Vec<Segment>> {
    let mut stmt = conn.prepare(
        "SELECT id, source, start_ms, end_ms, text, confidence FROM segments WHERE meeting_id = ?1 ORDER BY start_ms, id",
    )?;
    let rows = stmt.query_map(params![meeting_id], |r| {
        Ok(Segment {
            id: r.get(0)?,
            source: r.get(1)?,
            start_ms: r.get(2)?,
            end_ms: r.get(3)?,
            text: r.get(4)?,
            confidence: r.get(5)?,
        })
    })?;
    rows.collect()
}

/// Substitui todos os segmentos da reunião (reprocessamento não duplica).
pub fn replace_segments(conn: &mut Connection, meeting_id: &str, segs: &[(String, i64, i64, String, Option<f64>)]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM segments_fts WHERE meeting_id = ?1", params![meeting_id])?;
    tx.execute("DELETE FROM segments WHERE meeting_id = ?1", params![meeting_id])?;
    for (source, start, end, text, conf) in segs {
        tx.execute(
            "INSERT INTO segments (meeting_id, source, start_ms, end_ms, text, confidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![meeting_id, source, start, end, text, conf],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO segments_fts (text, meeting_id, segment_id) VALUES (?1, ?2, ?3)",
            params![text, meeting_id, id],
        )?;
    }
    tx.commit()
}

pub fn save_analysis(
    conn: &mut Connection,
    meeting_id: &str,
    created_at: &str,
    model: &str,
    summary: &str,
    topics: &[String],
    items: &[(String, String, Option<String>, Option<String>, Vec<i64>)],
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM analysis_items WHERE meeting_id = ?1", params![meeting_id])?;
    tx.execute("DELETE FROM analyses WHERE meeting_id = ?1", params![meeting_id])?;
    tx.execute(
        "INSERT INTO analyses (meeting_id, created_at, model, summary, topics) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![meeting_id, created_at, model, summary, serde_json::to_string(topics).unwrap_or_else(|_| "[]".into())],
    )?;
    for (kind, text, owner, due, seg_ids) in items {
        tx.execute(
            "INSERT INTO analysis_items (meeting_id, kind, text, owner, due, segment_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![meeting_id, kind, text, owner, due, serde_json::to_string(seg_ids).unwrap_or_else(|_| "[]".into())],
        )?;
    }
    tx.commit()
}

pub fn analysis(conn: &Connection, meeting_id: &str) -> rusqlite::Result<Option<Analysis>> {
    let head = conn
        .query_row(
            "SELECT created_at, model, summary, topics FROM analyses WHERE meeting_id = ?1",
            params![meeting_id],
            |r| {
                let topics: String = r.get(3)?;
                Ok(Analysis {
                    created_at: r.get(0)?,
                    model: r.get(1)?,
                    summary: r.get(2)?,
                    topics: serde_json::from_str(&topics).unwrap_or_default(),
                    items: vec![],
                })
            },
        )
        .optional()?;
    let Some(mut a) = head else { return Ok(None) };

    let mut stmt = conn.prepare(
        "SELECT id, kind, text, owner, due, segment_ids FROM analysis_items WHERE meeting_id = ?1 ORDER BY id",
    )?;
    a.items = stmt
        .query_map(params![meeting_id], |r| {
            let ids: String = r.get(5)?;
            Ok(AnalysisItem {
                id: r.get(0)?,
                kind: r.get(1)?,
                text: r.get(2)?,
                owner: r.get(3)?,
                due: r.get(4)?,
                segment_ids: serde_json::from_str(&ids).unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(a))
}

pub fn enqueue_job(conn: &Connection, kind: &str, meeting_id: &str, now: &str) -> rusqlite::Result<()> {
    // Evita duplicar um job que ainda não terminou.
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE kind = ?1 AND meeting_id = ?2 AND status IN ('pendente','executando'))",
        params![kind, meeting_id],
        |r| r.get(0),
    )?;
    if exists {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO jobs (kind, meeting_id, status, created_at, updated_at) VALUES (?1, ?2, 'pendente', ?3, ?3)",
        params![kind, meeting_id, now],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub meeting_id: String,
    pub attempts: i64,
}

pub fn next_job(conn: &Connection) -> rusqlite::Result<Option<Job>> {
    conn.query_row(
        "SELECT id, kind, meeting_id, attempts FROM jobs WHERE status = 'pendente' ORDER BY id LIMIT 1",
        [],
        |r| {
            Ok(Job {
                id: r.get(0)?,
                kind: r.get(1)?,
                meeting_id: r.get(2)?,
                attempts: r.get(3)?,
            })
        },
    )
    .optional()
}

pub fn set_job(conn: &Connection, id: i64, status: &str, error: Option<&str>, now: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE jobs SET status = ?2, error = ?3, updated_at = ?4, attempts = attempts + CASE WHEN ?2 = 'executando' THEN 1 ELSE 0 END WHERE id = ?1",
        params![id, status, error, now],
    )?;
    Ok(())
}

/// Ao iniciar o app, jobs que estavam em execução voltam para pendente (recuperação de falha).
pub fn requeue_interrupted_jobs(conn: &Connection, now: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE jobs SET status = 'pendente', updated_at = ?1 WHERE status = 'executando'",
        params![now],
    )
}

pub fn cancel_jobs(conn: &Connection, meeting_id: &str, now: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE jobs SET status = 'cancelado', updated_at = ?2 WHERE meeting_id = ?1 AND status = 'pendente'",
        params![meeting_id, now],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM segments_fts WHERE meeting_id = ?1", params![id])?;
    conn.execute("DELETE FROM meetings WHERE id = ?1", params![id])
}

/// Pesquisa no texto das transcrições e nos títulos. Consulta sem caracteres especiais do FTS.
pub fn search(conn: &Connection, query: &str, limit: i64) -> rusqlite::Result<Vec<SearchHit>> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let fts_query = terms.join(" AND ");

    let mut hits = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT f.meeting_id, m.title, m.started_at, snippet(segments_fts, 0, '<<', '>>', '…', 12), f.segment_id, s.start_ms
         FROM segments_fts f
         JOIN meetings m ON m.id = f.meeting_id
         JOIN segments s ON s.id = f.segment_id
         WHERE segments_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![fts_query, limit], |r| {
        Ok(SearchHit {
            meeting_id: r.get(0)?,
            meeting_title: r.get(1)?,
            started_at: r.get(2)?,
            snippet: r.get(3)?,
            segment_id: r.get(4)?,
            start_ms: r.get(5)?,
        })
    })?;
    for h in rows {
        hits.push(h?);
    }

    // Títulos que contêm o termo aparecem mesmo sem texto transcrito.
    let like = format!("%{}%", query.trim());
    let mut t_stmt = conn.prepare(
        "SELECT id, title, started_at FROM meetings WHERE title LIKE ?1 ORDER BY started_at DESC LIMIT ?2",
    )?;
    let titles = t_stmt.query_map(params![like, limit], |r| {
        Ok(SearchHit {
            meeting_id: r.get(0)?,
            meeting_title: r.get(1)?,
            started_at: r.get(2)?,
            snippet: String::new(),
            segment_id: None,
            start_ms: None,
        })
    })?;
    for h in titles {
        let h = h?;
        if !hits.iter().any(|x| x.meeting_id == h.meeting_id) {
            hits.push(h);
        }
    }
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MIGRATIONS;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        for sql in MIGRATIONS {
            conn.execute_batch(sql).unwrap();
        }
        conn
    }

    #[test]
    fn search_returns_segment_with_snippet() {
        let mut conn = db();
        insert_meeting(&conn, &NewMeeting { id: "m1", title: "Planejamento", started_at: "2026-01-01T10:00:00", status: MeetingStatus::Transcrita, folder: "x" }).unwrap();
        replace_segments(&mut conn, "m1", &[("microfone".into(), 1000, 3000, "Vamos fechar o orçamento hoje".into(), Some(0.9))]).unwrap();
        let hits = search(&conn, "orçamento", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].start_ms, Some(1000));
        assert!(hits[0].snippet.contains("<<orçamento>>"));
    }

    #[test]
    fn replacing_segments_does_not_duplicate_search_index() {
        let mut conn = db();
        insert_meeting(&conn, &NewMeeting { id: "m1", title: "T", started_at: "2026-01-01T10:00:00", status: MeetingStatus::Pronta, folder: "x" }).unwrap();
        let segs = [("microfone".to_string(), 0, 1000, "alpha beta".to_string(), None)];
        replace_segments(&mut conn, "m1", &segs).unwrap();
        replace_segments(&mut conn, "m1", &segs).unwrap();
        assert_eq!(search(&conn, "alpha", 10).unwrap().len(), 1);
    }

    #[test]
    fn job_queue_avoids_duplicates_and_requeues_interrupted() {
        let conn = db();
        insert_meeting(&conn, &NewMeeting { id: "m1", title: "T", started_at: "2026-01-01T10:00:00", status: MeetingStatus::Pronta, folder: "x" }).unwrap();
        enqueue_job(&conn, "transcrever", "m1", "2026-01-01T10:01:00").unwrap();
        enqueue_job(&conn, "transcrever", "m1", "2026-01-01T10:01:01").unwrap();
        let job = next_job(&conn).unwrap().expect("job pendente");
        set_job(&conn, job.id, "executando", None, "t").unwrap();
        assert_eq!(requeue_interrupted_jobs(&conn, "t").unwrap(), 1);
        assert!(next_job(&conn).unwrap().is_some());
    }
}
