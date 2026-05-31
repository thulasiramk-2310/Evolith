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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: Uuid,
    pub project_id: String,
    pub title: String,
    pub status: String,
    pub evidence_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAssignment {
    pub id: Uuid,
    pub feature_id: Uuid,
    pub event_id: Uuid,
    pub confidence_score: f64,
    pub algorithm_version: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub superseded_by: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureRun {
    pub id: Uuid,
    pub status: String,
    pub algorithm_version: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub event_count: i64,
    pub feature_count: i64,
    pub anchor_coverage: f64,
    pub anchored_feature_count: i64,
    pub unanchored_feature_count: i64,
    pub avg_events_per_feature: f64,
    pub dominant_anchor_ratio: f64,
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

        if current_version < 3 {
            tx.execute_batch("
                CREATE TABLE IF NOT EXISTS features (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL,
                    evidence_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS feature_assignments (
                    id TEXT PRIMARY KEY,
                    feature_id TEXT NOT NULL,
                    event_id TEXT NOT NULL,
                    confidence_score REAL NOT NULL,
                    algorithm_version TEXT NOT NULL,
                    source TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    superseded_by TEXT NULL,
                    FOREIGN KEY(feature_id) REFERENCES features(id) ON DELETE CASCADE,
                    FOREIGN KEY(event_id) REFERENCES events(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS feature_runs (
                    id TEXT PRIMARY KEY,
                    status TEXT NOT NULL DEFAULT 'running',
                    algorithm_version TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    completed_at TEXT NULL,
                    event_count INTEGER NOT NULL,
                    feature_count INTEGER NOT NULL,
                    anchor_coverage REAL NOT NULL DEFAULT 0.0,
                    anchored_feature_count INTEGER NOT NULL DEFAULT 0,
                    unanchored_feature_count INTEGER NOT NULL DEFAULT 0,
                    avg_events_per_feature REAL NOT NULL DEFAULT 0.0,
                    dominant_anchor_ratio REAL NOT NULL DEFAULT 0.0
                );

                CREATE INDEX IF NOT EXISTS idx_features_project ON features(project_id);
                CREATE INDEX IF NOT EXISTS idx_feature_assignments_feature ON feature_assignments(feature_id);
                CREATE INDEX IF NOT EXISTS idx_feature_assignments_event ON feature_assignments(event_id);
                CREATE INDEX IF NOT EXISTS idx_feature_assignments_superseded ON feature_assignments(superseded_by);
                CREATE INDEX IF NOT EXISTS idx_feature_runs_algo ON feature_runs(algorithm_version);
            ")?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (3, ?)",
                params![Utc::now().to_rfc3339()],
            )?;
        }

        if current_version < 4 {
            let mut has_evidence = false;
            {
                let mut stmt = tx.prepare("PRAGMA table_info(features)")?;
                let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for c in cols {
                    if c? == "evidence_json" {
                        has_evidence = true;
                        break;
                    }
                }
            }
            if !has_evidence {
                tx.execute_batch("
                    ALTER TABLE features ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '{}';
                ")?;
            }
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (4, ?)",
                params![Utc::now().to_rfc3339()],
            )?;
        }

        if current_version < 5 {
            let mut existing_cols: Vec<String> = Vec::new();
            {
                let mut stmt = tx.prepare("PRAGMA table_info(feature_runs)")?;
                let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for c in cols {
                    existing_cols.push(c?);
                }
            }
            let additions = [
                ("anchor_coverage", "REAL NOT NULL DEFAULT 0.0"),
                ("anchored_feature_count", "INTEGER NOT NULL DEFAULT 0"),
                ("unanchored_feature_count", "INTEGER NOT NULL DEFAULT 0"),
                ("avg_events_per_feature", "REAL NOT NULL DEFAULT 0.0"),
                ("dominant_anchor_ratio", "REAL NOT NULL DEFAULT 0.0"),
            ];
            for (name, decl) in additions {
                if !existing_cols.iter().any(|c| c == name) {
                    tx.execute_batch(&format!("ALTER TABLE feature_runs ADD COLUMN {name} {decl};"))?;
                }
            }
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (5, ?)",
                params![Utc::now().to_rfc3339()],
            )?;
        }

        if current_version < 6 {
            let mut existing_cols: Vec<String> = Vec::new();
            {
                let mut stmt = tx.prepare("PRAGMA table_info(feature_runs)")?;
                let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
                for c in cols {
                    existing_cols.push(c?);
                }
            }
            if !existing_cols.iter().any(|c| c == "status") {
                tx.execute_batch("ALTER TABLE feature_runs ADD COLUMN status TEXT NOT NULL DEFAULT 'running';")?;
                tx.execute(
                    "UPDATE feature_runs
                     SET status = CASE
                         WHEN completed_at IS NULL THEN 'partial'
                         WHEN feature_count = 0 AND event_count > 0 THEN 'partial'
                         ELSE 'completed'
                     END",
                    [],
                )?;
            }
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (6, ?)",
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

    pub fn get_session_info(&self, session_id: &str) -> Result<Option<(DateTime<Utc>, DateTime<Utc>)>> {
        let mut stmt = self.conn.prepare("SELECT start_time, end_time FROM sessions WHERE id = ?1")?;
        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            let start: String = row.get(0)?;
            let end: String = row.get(1)?;
            let start_dt = DateTime::parse_from_rfc3339(&start).unwrap().with_timezone(&Utc);
            let end_dt = DateTime::parse_from_rfc3339(&end).unwrap().with_timezone(&Utc);
            Ok(Some((start_dt, end_dt)))
        } else {
            Ok(None)
        }
    }

    pub fn get_events_by_session(&self, session_id: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.event_type, e.project_id, e.payload, e.path, e.branch, e.metadata, e.collector_version, e.created_at
             FROM events e
             INNER JOIN session_assignments sa ON e.id = sa.event_id
             WHERE sa.session_id = ?1
             ORDER BY sa.timestamp ASC"
        )?;
        let event_iter = stmt.query_map(params![session_id], Self::map_row_to_event)?;
        
        let mut events = Vec::new();
        for e in event_iter {
            events.push(e?);
        }
        Ok(events)
    }

    pub fn has_git_commit_hash(&self, commit_hash: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT 1
             FROM events
             WHERE event_type = 'git.commit'
               AND json_extract(payload, '$.commit_hash') = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![commit_hash])?;
        Ok(rows.next()?.is_some())
    }

    pub fn event_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn clear_feature_projection(&self) -> Result<()> {
        self.conn.execute("DELETE FROM feature_assignments", [])?;
        self.conn.execute("DELETE FROM features", [])?;
        Ok(())
    }

    pub fn insert_feature(&self, feature: &Feature) -> Result<()> {
        self.conn.execute(
            "INSERT INTO features (id, project_id, title, status, evidence_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                feature.id.to_string(),
                feature.project_id,
                feature.title,
                feature.status,
                serde_json::to_string(&feature.evidence_json)?,
                feature.created_at.to_rfc3339(),
                feature.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn insert_feature_assignment(&self, assignment: &FeatureAssignment) -> Result<()> {
        self.conn.execute(
            "INSERT INTO feature_assignments
             (id, feature_id, event_id, confidence_score, algorithm_version, source, created_at, superseded_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                assignment.id.to_string(),
                assignment.feature_id.to_string(),
                assignment.event_id.to_string(),
                assignment.confidence_score,
                assignment.algorithm_version,
                assignment.source,
                assignment.created_at.to_rfc3339(),
                assignment.superseded_by.map(|v| v.to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn create_feature_run(&self, run_id: Uuid, algorithm_version: &str, event_count: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO feature_runs
             (id, status, algorithm_version, started_at, completed_at, event_count, feature_count,
              anchor_coverage, anchored_feature_count, unanchored_feature_count, avg_events_per_feature, dominant_anchor_ratio)
             VALUES (?1, 'running', ?2, ?3, NULL, ?4, 0, 0.0, 0, 0, 0.0, 0.0)",
            params![run_id.to_string(), algorithm_version, Utc::now().to_rfc3339(), event_count],
        )?;
        Ok(())
    }

    pub fn complete_feature_run(
        &self,
        run_id: Uuid,
        feature_count: i64,
        anchor_coverage: f64,
        anchored_feature_count: i64,
        unanchored_feature_count: i64,
        avg_events_per_feature: f64,
        dominant_anchor_ratio: f64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE feature_runs
             SET status = 'completed',
                 completed_at = ?1,
                 feature_count = ?2,
                 anchor_coverage = ?3,
                 anchored_feature_count = ?4,
                 unanchored_feature_count = ?5,
                 avg_events_per_feature = ?6,
                 dominant_anchor_ratio = ?7
             WHERE id = ?8",
            params![
                Utc::now().to_rfc3339(),
                feature_count,
                anchor_coverage,
                anchored_feature_count,
                unanchored_feature_count,
                avg_events_per_feature,
                dominant_anchor_ratio,
                run_id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn fail_feature_run(&self, run_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE feature_runs
             SET status = 'failed',
                 completed_at = ?1
             WHERE id = ?2",
            params![Utc::now().to_rfc3339(), run_id.to_string()],
        )?;
        Ok(())
    }

    pub fn list_feature_runs(&self) -> Result<Vec<FeatureRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, status, algorithm_version, started_at, completed_at, event_count, feature_count,
                    anchor_coverage, anchored_feature_count, unanchored_feature_count, avg_events_per_feature, dominant_anchor_ratio
             FROM feature_runs
             ORDER BY started_at DESC",
        )?;
        let iter = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let started_at: String = row.get(3)?;
            let completed_at: Option<String> = row.get(4)?;
            Ok(FeatureRun {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                status: row.get(1)?,
                algorithm_version: row.get(2)?,
                started_at: DateTime::parse_from_rfc3339(&started_at).map(|v| v.with_timezone(&Utc)).unwrap_or_default(),
                completed_at: completed_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|v| v.with_timezone(&Utc))),
                event_count: row.get(5)?,
                feature_count: row.get(6)?,
                anchor_coverage: row.get(7)?,
                anchored_feature_count: row.get(8)?,
                unanchored_feature_count: row.get(9)?,
                avg_events_per_feature: row.get(10)?,
                dominant_anchor_ratio: row.get(11)?,
            })
        })?;
        let mut out = Vec::new();
        for r in iter {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_feature_run_by_id(&self, run_id: &str) -> Result<Option<FeatureRun>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, status, algorithm_version, started_at, completed_at, event_count, feature_count,
                    anchor_coverage, anchored_feature_count, unanchored_feature_count, avg_events_per_feature, dominant_anchor_ratio
             FROM feature_runs
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![run_id])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let started_at: String = row.get(3)?;
            let completed_at: Option<String> = row.get(4)?;
            return Ok(Some(FeatureRun {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                status: row.get(1)?,
                algorithm_version: row.get(2)?,
                started_at: DateTime::parse_from_rfc3339(&started_at)
                    .map(|v| v.with_timezone(&Utc))
                    .unwrap_or_default(),
                completed_at: completed_at
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|v| v.with_timezone(&Utc))),
                event_count: row.get(5)?,
                feature_count: row.get(6)?,
                anchor_coverage: row.get(7)?,
                anchored_feature_count: row.get(8)?,
                unanchored_feature_count: row.get(9)?,
                avg_events_per_feature: row.get(10)?,
                dominant_anchor_ratio: row.get(11)?,
            }));
        }
        Ok(None)
    }

    pub fn list_features(&self) -> Result<Vec<Feature>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, status, evidence_json, created_at, updated_at
             FROM features
             ORDER BY created_at ASC",
        )?;
        let iter = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let evidence_json: String = row.get(4)?;
            let created_at: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            Ok(Feature {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                project_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                evidence_json: serde_json::from_str(&evidence_json).unwrap_or(serde_json::json!({})),
                created_at: DateTime::parse_from_rfc3339(&created_at).map(|v| v.with_timezone(&Utc)).unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_at).map(|v| v.with_timezone(&Utc)).unwrap_or_default(),
            })
        })?;
        let mut out = Vec::new();
        for f in iter {
            out.push(f?);
        }
        Ok(out)
    }

    pub fn get_feature_by_id(&self, feature_id: &str) -> Result<Option<Feature>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, status, evidence_json, created_at, updated_at
             FROM features
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![feature_id])?;
        if let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let evidence_json: String = row.get(4)?;
            let created_at: String = row.get(5)?;
            let updated_at: String = row.get(6)?;
            return Ok(Some(Feature {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                project_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                evidence_json: serde_json::from_str(&evidence_json).unwrap_or(serde_json::json!({})),
                created_at: DateTime::parse_from_rfc3339(&created_at).map(|v| v.with_timezone(&Utc)).unwrap_or_default(),
                updated_at: DateTime::parse_from_rfc3339(&updated_at).map(|v| v.with_timezone(&Utc)).unwrap_or_default(),
            }));
        }
        Ok(None)
    }

    pub fn get_feature_events(&self, feature_id: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.timestamp, e.event_type, e.project_id, e.payload, e.path, e.branch, e.metadata, e.collector_version, e.created_at
             FROM events e
             INNER JOIN feature_assignments fa ON e.id = fa.event_id
             WHERE fa.feature_id = ?1 AND fa.superseded_by IS NULL
             ORDER BY e.timestamp ASC",
        )?;
        let iter = stmt.query_map(params![feature_id], Self::map_row_to_event)?;
        let mut out = Vec::new();
        for e in iter {
            out.push(e?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("devreplay-{name}-{}.db", Uuid::new_v4()));
        p
    }

    #[test]
    fn migration_v3_creates_expected_tables() -> Result<()> {
        let db_path = temp_db_path("migration-v3");
        let store = EventStore::new(&db_path)?;

        let mut stmt = store.conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut tables: HashSet<String> = HashSet::new();
        for name in rows {
            tables.insert(name?);
        }

        for required in [
            "schema_migrations",
            "events",
            "sessions",
            "session_assignments",
            "features",
            "feature_assignments",
            "feature_runs",
        ] {
            assert!(tables.contains(required), "missing table {required}");
        }

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn feature_persistence_roundtrip_works() -> Result<()> {
        let db_path = temp_db_path("feature-roundtrip");
        let store = EventStore::new(&db_path)?;

        let event = Event {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: EventType::FileModified,
            project_id: "proj-1".to_string(),
            payload: serde_json::json!({"file":"src/auth.ts", "ticket_id":"AUTH-123"}),
            path: Some("src/auth.ts".to_string()),
            branch: Some("feature/auth-refactor".to_string()),
            metadata: None,
            collector_version: "test".to_string(),
            created_at: Utc::now(),
        };
        store.insert_event(&event)?;

        let feature_id = Uuid::new_v4();
        let now = Utc::now();
        store.insert_feature(&Feature {
            id: feature_id,
            project_id: "proj-1".to_string(),
            title: "Ticket AUTH-123".to_string(),
            status: "active".to_string(),
            evidence_json: serde_json::json!({"ticket_ids":["AUTH-123"]}),
            created_at: now,
            updated_at: now,
        })?;

        store.insert_feature_assignment(&FeatureAssignment {
            id: Uuid::new_v4(),
            feature_id,
            event_id: event.id,
            confidence_score: 0.95,
            algorithm_version: "v-test".to_string(),
            source: "ticket_anchor".to_string(),
            created_at: now,
            superseded_by: None,
        })?;

        let features = store.list_features()?;
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].title, "Ticket AUTH-123");

        let feature_events = store.get_feature_events(&feature_id.to_string())?;
        assert_eq!(feature_events.len(), 1);
        assert_eq!(feature_events[0].id, event.id);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }
}
