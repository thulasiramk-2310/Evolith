use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
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
    conn: Connection,
}

impl EventStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::init_db(&conn)?;
        Ok(Self { conn })
    }

    fn init_db(conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                event_type TEXT NOT NULL,
                project_id TEXT NOT NULL,
                payload TEXT NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn insert_event(&self, event: &Event) -> Result<()> {
        let ts = event.timestamp.to_rfc3339();
        let ev_type = serde_json::to_string(&event.event_type)?
            .trim_matches('\"')
            .to_string();
        let payload = serde_json::to_string(&event.payload)?;

        self.conn.execute(
            "INSERT INTO events (id, timestamp, event_type, project_id, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event.id.to_string(), ts, ev_type, event.project_id, payload,],
        )?;
        Ok(())
    }

    pub fn get_events(&self) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp, event_type, project_id, payload FROM events ORDER BY timestamp ASC")?;
        let event_iter = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let ts: String = row.get(1)?;
            let ev_type: String = row.get(2)?;
            let project_id: String = row.get(3)?;
            let payload: String = row.get(4)?;

            Ok((id, ts, ev_type, project_id, payload))
        })?;

        let mut events = Vec::new();
        for r in event_iter {
            let (id, ts, ev_type, project_id, payload) = r?;
            events.push(Event {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                timestamp: DateTime::parse_from_rfc3339(&ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_default(),
                event_type: serde_json::from_value(serde_json::Value::String(ev_type))
                    .unwrap_or(EventType::Other("unknown".to_string())),
                project_id,
                payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            });
        }
        Ok(events)
    }

    pub fn get_events_by_project(&self, project: &str) -> Result<Vec<Event>> {
        let mut events = self.get_events()?;
        events.retain(|e| e.project_id == project);
        Ok(events)
    }

    pub fn get_events_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Event>> {
        let mut events = self.get_events()?;
        events.retain(|e| e.timestamp >= start && e.timestamp <= end);
        Ok(events)
    }
}
