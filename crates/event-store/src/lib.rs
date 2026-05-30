use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub project_id: String,
    pub payload: serde_json::Value,
    pub path: Option<String>,
    pub branch: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub collector_version: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    #[serde(rename = "file.modified")]
    FileModified,
    #[serde(rename = "git.commit")]
    GitCommit,
    #[serde(rename = "test.run")]
    TestRun,
    #[serde(untagged)]
    Other(String),
}

pub struct EventStore {
    pub conn: Connection,
}

impl EventStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        Self::apply_migrations(&mut conn)?;
        Ok(Self { conn })
    }

    fn apply_migrations(conn: &mut Connection) -> Result<()> {
        let tx = conn.transaction()?;
        
        tx.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )",
            [],
        )?;

        let current_version: i64 = match tx.query_row(
            "SELECT MAX(version) FROM schema_migrations",
            [],
            |row| row.get(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => 0,
            Err(_) => 0,
        };

        if current_version < 1 {
            tx.execute_batch("
                CREATE TABLE IF NOT EXISTS events (
                    id TEXT PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    path TEXT,
                    branch TEXT,
                    metadata TEXT,
                    collector_version TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
                CREATE INDEX IF NOT EXISTS idx_events_project ON events(project_id);
                CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
            ")?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?)",
                params![Utc::now().to_rfc3339()],
            )?;
        }

        if current_version < 2 {
            tx.execute_batch("
                CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    start_time TEXT NOT NULL,
                    end_time TEXT NOT NULL,
                    event_count INTEGER NOT NULL
                );
                
                CREATE TABLE IF NOT EXISTS session_assignments (
                    session_id TEXT NOT NULL,
                    event_id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    PRIMARY KEY (session_id, event_id),
                    FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
                    FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
                );

                CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project_id);
                CREATE INDEX IF NOT EXISTS idx_session_assignments_session ON session_assignments(session_id);
                CREATE INDEX IF NOT EXISTS idx_session_assignments_event ON session_assignments(event_id);
            ")?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (2, ?)",
                params![Utc::now().to_rfc3339()],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let ts = event.timestamp.to_rfc3339();
        let ev_type = serde_json::to_string(&event.event_type)?.trim_matches('\"').to_string();
        let payload = serde_json::to_string(&event.payload)?;
        let metadata = event.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
        let created_at = event.created_at.to_rfc3339();

        self.conn.execute(
            "INSERT INTO events (id, timestamp, event_type, project_id, payload, path, branch, metadata, collector_version, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                event.id.to_string(),
                ts,
                ev_type,
                event.project_id,
                payload,
                event.path,
                event.branch,
                metadata,
                event.collector_version,
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn map_row_to_event(row: &Row) -> rusqlite::Result<Event> {
        let id: String = row.get(0)?;
        let ts: String = row.get(1)?;
        let ev_type: String = row.get(2)?;
        let project_id: String = row.get(3)?;
        let payload: String = row.get(4)?;
        let path: Option<String> = row.get(5)?;
        let branch: Option<String> = row.get(6)?;
        let metadata: Option<String> = row.get(7)?;
        let collector_version: String = row.get(8)?;
        let created_at: String = row.get(9)?;

        Ok(Event {
            id: Uuid::parse_str(&id).unwrap_or_default(),
            timestamp: DateTime::parse_from_rfc3339(&ts).map(|dt| dt.with_timezone(&Utc)).unwrap_or_default(),
            event_type: serde_json::from_value(serde_json::Value::String(ev_type)).unwrap_or(EventType::Other("unknown".to_string())),
            project_id,
            payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            path,
            branch,
            metadata: metadata.and_then(|m| serde_json::from_str(&m).ok()),
            collector_version,
            created_at: DateTime::parse_from_rfc3339(&created_at).map(|dt| dt.with_timezone(&Utc)).unwrap_or_default(),
        })
    }

    pub fn get_events(&self) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, event_type, project_id, payload, path, branch, metadata, collector_version, created_at
             FROM events ORDER BY timestamp ASC"
        )?;
        let event_iter = stmt.query_map([], Self::map_row_to_event)?;
        
        let mut events = Vec::new();
        for e in event_iter {
            events.push(e?);
        }
        Ok(events)
    }

    pub fn get_events_by_project(&self, project: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, event_type, project_id, payload, path, branch, metadata, collector_version, created_at
             FROM events WHERE project_id = ?1 ORDER BY timestamp ASC"
        )?;
        let event_iter = stmt.query_map(params![project], Self::map_row_to_event)?;
        
        let mut events = Vec::new();
        for e in event_iter {
            events.push(e?);
        }
        Ok(events)
    }

    pub fn get_events_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, event_type, project_id, payload, path, branch, metadata, collector_version, created_at
             FROM events WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp ASC"
        )?;
        let event_iter = stmt.query_map(params![start.to_rfc3339(), end.to_rfc3339()], Self::map_row_to_event)?;
        
        let mut events = Vec::new();
        for e in event_iter {
            events.push(e?);
        }
        Ok(events)
    }
}
