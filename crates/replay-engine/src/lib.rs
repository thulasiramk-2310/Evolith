use anyhow::Result;
use event_store::EventType;
use session_engine::Session;

#[derive(Default)]
pub struct ReplayEngine;

impl ReplayEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn replay_session(&self, session: &Session) -> Result<()> {
        println!("Replaying session {} ({} to {})", session.id, session.start_time, session.end_time);
        
        for event in &session.events {
            let time = event.timestamp.format("%H:%M");
            let ev_type = match &event.event_type {
                EventType::FileModified => "file.modified".to_string(),
                EventType::GitCommit => "git.commit".to_string(),
                EventType::TestRun => "test.run".to_string(),
                EventType::Other(s) => s.clone(),
            };
            
            let mut info = String::new();
            if ev_type == "file.modified" {
                if let Some(file) = event.payload.get("file").and_then(|v| v.as_str()) {
                    let path = std::path::Path::new(file);
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        info = name.to_string();
                    } else {
                        info = file.to_string();
                    }
                }
            }
            
            if info.is_empty() {
                println!("{} {}", time, ev_type);
            } else {
                println!("{} {} {}", time, ev_type, info);
            }
        }
        
        Ok(())
    }
}
