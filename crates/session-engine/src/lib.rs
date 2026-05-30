use anyhow::Result;
use chrono::{DateTime, Utc};
use event_store::Event;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub events: Vec<Event>,
}

#[derive(Default)]
pub struct SessionEngine;

impl SessionEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn rebuild_sessions(&self, mut events: Vec<Event>) -> Result<Vec<Session>> {
        events.sort_by_key(|e| e.timestamp);
        
        let mut sessions: Vec<Session> = Vec::new();
        let mut current_session: Option<Session> = None;
        
        for event in events {
            if let Some(mut session) = current_session.take() {
                let gap = event.timestamp.signed_duration_since(session.end_time);
                if gap.num_minutes() > 30 {
                    sessions.push(session);
                    current_session = Some(Session {
                        id: Uuid::new_v4(),
                        start_time: event.timestamp,
                        end_time: event.timestamp,
                        events: vec![event],
                    });
                } else {
                    session.end_time = event.timestamp;
                    session.events.push(event);
                    current_session = Some(session);
                }
            } else {
                current_session = Some(Session {
                    id: Uuid::new_v4(),
                    start_time: event.timestamp,
                    end_time: event.timestamp,
                    events: vec![event],
                });
            }
        }
        
        if let Some(session) = current_session {
            sessions.push(session);
        }
        
        Ok(sessions)
    }
}
