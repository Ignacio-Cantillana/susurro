use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::now_ms;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS entries (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL CHECK (kind IN ('dictation', 'note')),
    text        TEXT    NOT NULL,
    raw_text    TEXT    NOT NULL,
    app         TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS entries_kind_created ON entries(kind, created_at DESC);

CREATE TABLE IF NOT EXISTS meetings (
    id          INTEGER PRIMARY KEY,
    title       TEXT    NOT NULL,
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER,
    audio_path  TEXT
);

CREATE TABLE IF NOT EXISTS segments (
    id          INTEGER PRIMARY KEY,
    meeting_id  INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    speaker     TEXT    NOT NULL CHECK (speaker IN ('me', 'others')),
    start_ms    INTEGER NOT NULL,
    end_ms      INTEGER NOT NULL,
    text        TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS segments_meeting ON segments(meeting_id, start_ms);

CREATE TABLE IF NOT EXISTS dictionary (
    id          INTEGER PRIMARY KEY,
    term        TEXT    NOT NULL UNIQUE,
    sounds_like TEXT    NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS snippets (
    id          INTEGER PRIMARY KEY,
    trigger     TEXT    NOT NULL UNIQUE,
    expansion   TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
);

-- Índice de texto completo común para dictados, notas y segmentos de reunión.
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
    text,
    source UNINDEXED,
    ref_id UNINDEXED,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
    INSERT INTO search_fts(text, source, ref_id) VALUES (new.text, new.kind, new.id);
END;
CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
    DELETE FROM search_fts WHERE source = old.kind AND ref_id = old.id;
END;
CREATE TRIGGER IF NOT EXISTS segments_ai AFTER INSERT ON segments BEGIN
    INSERT INTO search_fts(text, source, ref_id) VALUES (new.text, 'segment', new.id);
END;
CREATE TRIGGER IF NOT EXISTS segments_ad AFTER DELETE ON segments BEGIN
    DELETE FROM search_fts WHERE source = 'segment' AND ref_id = old.id;
END;
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Dictation,
    Note,
}

impl EntryKind {
    fn as_str(self) -> &'static str {
        match self {
            EntryKind::Dictation => "dictation",
            EntryKind::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub kind: EntryKind,
    pub text: String,
    pub raw_text: String,
    pub app: Option<String>,
    pub duration_ms: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEntry {
    pub kind: EntryKind,
    pub text: String,
    pub raw_text: String,
    pub app: Option<String>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub audio_path: Option<String>,
    /// Número de segmentos transcritos.
    pub segment_count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Me,
    Others,
}

impl Speaker {
    pub fn as_str(self) -> &'static str {
        match self {
            Speaker::Me => "me",
            Speaker::Others => "others",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Speaker::Me => "Yo",
            Speaker::Others => "Otros",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "me" { Speaker::Me } else { Speaker::Others }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub speaker: Speaker,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictTerm {
    pub id: i64,
    pub term: String,
    /// Formas en que Whisper suele oírlo, separadas por comas («cubernetis, kuberneti»).
    pub sounds_like: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// `dictation`, `note` o `segment`.
    pub source: String,
    /// Id de la entrada o de la reunión (en los segmentos).
    pub id: i64,
    pub meeting_title: Option<String>,
    pub snippet: String,
    pub created_at: i64,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Abre (o crea) la base de datos en modo lectura/escritura y aplica el esquema.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creando {}", dir.display()))?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// Abre la base de datos en solo lectura (lo que usa el servidor MCP).
    pub fn open_readonly(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("abriendo {} (¿has abierto Susurro alguna vez?)", path.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(Self { conn })
    }

    // ---------- Dictados y notas ----------

    pub fn insert_entry(&self, e: &NewEntry) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entries (kind, text, raw_text, app, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![e.kind.as_str(), e.text, e.raw_text, e.app, e.duration_ms, now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_entries(&self, kind: EntryKind, limit: i64, offset: i64) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, raw_text, app, duration_ms, created_at FROM entries
             WHERE kind = ?1 ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(params![kind.as_str(), limit, offset], |r| {
            Ok(Entry {
                id: r.get(0)?,
                kind,
                text: r.get(1)?,
                raw_text: r.get(2)?,
                app: r.get(3)?,
                duration_ms: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn update_entry_text(&self, id: i64, text: &str) -> Result<()> {
        // El trigger solo cubre insert/delete: actualizamos el índice a mano.
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE entries SET text = ?1 WHERE id = ?2", params![text, id])?;
        tx.execute(
            "UPDATE search_fts SET text = ?1 WHERE source IN ('dictation', 'note') AND ref_id = ?2",
            params![text, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Borra las entradas de un tipo más antiguas que `before_ms`. Devuelve cuántas.
    pub fn purge_entries(&self, kind: EntryKind, before_ms: i64) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM entries WHERE kind = ?1 AND created_at < ?2", params![kind.as_str(), before_ms])?)
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- Reuniones ----------

    pub fn create_meeting(&self, title: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO meetings (title, started_at) VALUES (?1, ?2)",
            params![title, now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_segments(&self, meeting_id: i64, segments: &[Segment]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO segments (meeting_id, speaker, start_ms, end_ms, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for s in segments {
                stmt.execute(params![meeting_id, s.speaker.as_str(), s.start_ms, s.end_ms, s.text])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn finish_meeting(&self, id: i64, audio_path: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE meetings SET ended_at = ?1, audio_path = ?2 WHERE id = ?3",
            params![now_ms(), audio_path, id],
        )?;
        Ok(())
    }

    pub fn rename_meeting(&self, id: i64, title: &str) -> Result<()> {
        self.conn.execute("UPDATE meetings SET title = ?1 WHERE id = ?2", params![title, id])?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        // Borrado explícito para que salten los triggers del índice FTS.
        tx.execute("DELETE FROM segments WHERE meeting_id = ?1", params![id])?;
        tx.execute("DELETE FROM meetings WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_meetings(&self, limit: i64, offset: i64) -> Result<Vec<Meeting>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.title, m.started_at, m.ended_at, m.audio_path,
                    (SELECT COUNT(*) FROM segments s WHERE s.meeting_id = m.id)
             FROM meetings m ORDER BY m.started_at DESC, m.id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], meeting_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get_meeting(&self, id: i64) -> Result<Option<(Meeting, Vec<Segment>)>> {
        let meeting = self
            .conn
            .query_row(
                "SELECT m.id, m.title, m.started_at, m.ended_at, m.audio_path,
                        (SELECT COUNT(*) FROM segments s WHERE s.meeting_id = m.id)
                 FROM meetings m WHERE m.id = ?1",
                params![id],
                meeting_from_row,
            )
            .optional()?;
        let Some(meeting) = meeting else { return Ok(None) };
        let mut stmt = self.conn.prepare(
            "SELECT speaker, start_ms, end_ms, text FROM segments
             WHERE meeting_id = ?1 ORDER BY start_ms, id",
        )?;
        let segments = stmt
            .query_map(params![id], |r| {
                Ok(Segment {
                    speaker: Speaker::parse(&r.get::<_, String>(0)?),
                    start_ms: r.get(1)?,
                    end_ms: r.get(2)?,
                    text: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(Some((meeting, segments)))
    }

    // ---------- Búsqueda ----------

    /// Búsqueda de texto completo en dictados, notas y reuniones. Cada palabra de la
    /// consulta se busca como prefijo, así que no hace falta saber la sintaxis de FTS5.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<SearchHit>> {
        let Some(fts_query) = to_fts_query(query) else { return Ok(vec![]) };
        let mut stmt = self.conn.prepare(
            "SELECT f.source,
                    CASE WHEN f.source = 'segment' THEN s.meeting_id ELSE f.ref_id END,
                    m.title,
                    snippet(search_fts, 0, '«', '»', '…', 16),
                    COALESCE(e.created_at, m.started_at + s.start_ms, 0)
             FROM search_fts f
             LEFT JOIN entries e  ON f.source IN ('dictation', 'note') AND e.id = f.ref_id
             LEFT JOIN segments s ON f.source = 'segment' AND s.id = f.ref_id
             LEFT JOIN meetings m ON m.id = s.meeting_id
             WHERE search_fts MATCH ?1
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit], |r| {
            Ok(SearchHit {
                source: r.get(0)?,
                id: r.get(1)?,
                meeting_title: r.get(2)?,
                snippet: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // ---------- Diccionario ----------

    pub fn list_dictionary(&self) -> Result<Vec<DictTerm>> {
        let mut stmt = self.conn.prepare("SELECT id, term, sounds_like FROM dictionary ORDER BY term COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| Ok(DictTerm { id: r.get(0)?, term: r.get(1)?, sounds_like: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn upsert_dictionary(&self, term: &str, sounds_like: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO dictionary (term, sounds_like) VALUES (?1, ?2)
             ON CONFLICT(term) DO UPDATE SET sounds_like = excluded.sounds_like",
            params![term.trim(), sounds_like.trim()],
        )?;
        Ok(())
    }

    pub fn delete_dictionary(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM dictionary WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- Snippets ----------

    pub fn list_snippets(&self) -> Result<Vec<Snippet>> {
        let mut stmt = self.conn.prepare("SELECT id, trigger, expansion FROM snippets ORDER BY trigger COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| Ok(Snippet { id: r.get(0)?, trigger: r.get(1)?, expansion: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn upsert_snippet(&self, trigger: &str, expansion: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO snippets (trigger, expansion) VALUES (?1, ?2)
             ON CONFLICT(trigger) DO UPDATE SET expansion = excluded.expansion",
            params![trigger.trim(), expansion],
        )?;
        Ok(())
    }

    pub fn delete_snippet(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---------- Ajustes ----------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn meeting_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: r.get(0)?,
        title: r.get(1)?,
        started_at: r.get(2)?,
        ended_at: r.get(3)?,
        audio_path: r.get(4)?,
        segment_count: r.get(5)?,
    })
}

/// Convierte texto libre en una consulta FTS5 segura: `hola mun` → `"hola"* "mun"*`.
fn to_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EntryKind, text: &str) -> NewEntry {
        NewEntry { kind, text: text.into(), raw_text: text.into(), app: Some("Notes".into()), duration_ms: 1000 }
    }

    #[test]
    fn entries_roundtrip_and_delete() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_entry(&entry(EntryKind::Dictation, "hola mundo")).unwrap();
        db.insert_entry(&entry(EntryKind::Note, "una nota")).unwrap();
        let dictations = db.list_entries(EntryKind::Dictation, 10, 0).unwrap();
        assert_eq!(dictations.len(), 1);
        assert_eq!(dictations[0].text, "hola mundo");
        db.delete_entry(id).unwrap();
        assert!(db.list_entries(EntryKind::Dictation, 10, 0).unwrap().is_empty());
        assert!(db.search("hola", 10).unwrap().is_empty(), "el índice debe borrarse con la entrada");
    }

    #[test]
    fn search_finds_entries_and_meeting_segments_ignoring_accents() {
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&entry(EntryKind::Dictation, "Revisa la migración de la base de datos")).unwrap();
        let m = db.create_meeting("Daily").unwrap();
        db.add_segments(
            m,
            &[Segment { speaker: Speaker::Others, start_ms: 0, end_ms: 2000, text: "La migracion va el lunes".into() }],
        )
        .unwrap();

        let hits = db.search("migra", 10).unwrap();
        assert_eq!(hits.len(), 2);
        let seg = hits.iter().find(|h| h.source == "segment").unwrap();
        assert_eq!(seg.id, m);
        assert_eq!(seg.meeting_title.as_deref(), Some("Daily"));
    }

    #[test]
    fn search_tolerates_fts_syntax() {
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&entry(EntryKind::Note, "precio: 10 euros")).unwrap();
        assert_eq!(db.search("\"precio\" ( *:", 10).unwrap().len(), 1);
        assert!(db.search("  ", 10).unwrap().is_empty());
    }

    #[test]
    fn update_entry_text_reindexes() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert_entry(&entry(EntryKind::Note, "texto viejo")).unwrap();
        db.update_entry_text(id, "texto nuevo").unwrap();
        assert!(db.search("viejo", 10).unwrap().is_empty());
        assert_eq!(db.search("nuevo", 10).unwrap().len(), 1);
    }

    #[test]
    fn meetings_with_segments_and_cascade_delete() {
        let db = Db::open_in_memory().unwrap();
        let m = db.create_meeting("Kickoff").unwrap();
        db.add_segments(
            m,
            &[
                Segment { speaker: Speaker::Others, start_ms: 3000, end_ms: 4000, text: "segundo".into() },
                Segment { speaker: Speaker::Me, start_ms: 0, end_ms: 2000, text: "primero".into() },
            ],
        )
        .unwrap();
        db.finish_meeting(m, None).unwrap();
        let (meeting, segs) = db.get_meeting(m).unwrap().unwrap();
        assert_eq!(meeting.segment_count, 2);
        assert!(meeting.ended_at.is_some());
        assert_eq!(segs[0].text, "primero");
        db.delete_meeting(m).unwrap();
        assert!(db.get_meeting(m).unwrap().is_none());
        assert!(db.search("segundo", 10).unwrap().is_empty());
    }

    #[test]
    fn dictionary_snippets_settings_upsert() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_dictionary("Kubernetes", "cubernetis").unwrap();
        db.upsert_dictionary("Kubernetes", "cubernetis, kuberneti").unwrap();
        assert_eq!(db.list_dictionary().unwrap()[0].sounds_like, "cubernetis, kuberneti");
        db.upsert_snippet("mi firma", "Un saludo,\nAlex").unwrap();
        assert_eq!(db.list_snippets().unwrap().len(), 1);
        db.set_setting("language", "es").unwrap();
        db.set_setting("language", "auto").unwrap();
        assert_eq!(db.get_setting("language").unwrap().as_deref(), Some("auto"));
        assert_eq!(db.get_setting("nope").unwrap(), None);
    }
}
