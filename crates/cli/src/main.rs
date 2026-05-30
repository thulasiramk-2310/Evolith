use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use event_store::{Event, EventStore, EventType};
use session_engine::SessionEngine;
use replay_engine::ReplayEngine;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Import { file: PathBuf },
    Events { #[command(subcommand)] subcmd: EventCommands },
    Sessions { #[command(subcommand)] subcmd: SessionCommands },
    Replay {
        #[command(subcommand)] subcmd: ReplayCommands,
    },
}

#[derive(Subcommand)]
enum EventCommands {
    List,
}

#[derive(Subcommand)]
enum SessionCommands {
    Rebuild,
}

#[derive(Subcommand)]
enum ReplayCommands {
    Session { id: String },
}

fn get_db_path() -> PathBuf {
    Path::new(".devreplay").join("events.db")
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir_all(".devreplay")?;
            let db_path = get_db_path();
            let _store = EventStore::new(&db_path)?;
            println!("Initialized DevReplay in .devreplay");
        }
        Commands::Import { file } => {
            let db_path = get_db_path();
            let store = EventStore::new(&db_path)?;
            let content = fs::read_to_string(file).context("Failed to read file")?;
            let events: Vec<Event> = serde_json::from_str(&content).context("Failed to parse JSON")?;
            for event in events {
                store.insert_event(&event)?;
            }
            println!("Imported events successfully.");
        }
        Commands::Events { subcmd } => match subcmd {
            EventCommands::List => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let events = store.get_events()?;
                for e in events {
                    let file_info = if let Some(f) = e.payload.get("file").and_then(|v| v.as_str()) {
                        f.to_string()
                    } else {
                        String::new()
                    };
                    
                    let ev_type = match &e.event_type {
                        EventType::FileModified => "file.modified",
                        EventType::GitCommit => "git.commit",
                        EventType::TestRun => "test.run",
                        EventType::Other(s) => s.as_str(),
                    };
                    
                    if file_info.is_empty() {
                        println!("{} {}", e.timestamp.format("%Y-%m-%d %H:%M"), ev_type);
                    } else {
                        println!("{} {} {}", e.timestamp.format("%Y-%m-%d %H:%M"), ev_type, file_info);
                    }
                }
            }
        },
        Commands::Sessions { subcmd } => match subcmd {
            SessionCommands::Rebuild => {
                let db_path = get_db_path();
                let mut store = EventStore::new(&db_path)?;
                let events = store.get_events()?;
                
                let engine = SessionEngine::new();
                let sessions = engine.rebuild_sessions(events)?;
                println!("Rebuilt {} sessions:", sessions.len());
                
                let tx = store.conn.transaction()?;
                
                for (i, s) in sessions.iter().enumerate() {
                    let project_id = s.events.first().map(|e| e.project_id.as_str()).unwrap_or("unknown");
                    
                    tx.execute(
                        "INSERT OR REPLACE INTO sessions (id, project_id, start_time, end_time, event_count) 
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![
                            s.id.to_string(),
                            project_id,
                            s.start_time.to_rfc3339(),
                            s.end_time.to_rfc3339(),
                            s.events.len() as i64,
                        ],
                    )?;

                    for event in &s.events {
                        tx.execute(
                            "INSERT OR REPLACE INTO session_assignments (session_id, event_id, project_id, timestamp) 
                             VALUES (?1, ?2, ?3, ?4)",
                            rusqlite::params![
                                s.id.to_string(),
                                event.id.to_string(),
                                project_id,
                                event.timestamp.to_rfc3339(),
                            ],
                        )?;
                    }

                    println!("Session {}: {} ({} events) -> Persisted to DB", i, s.id, s.events.len());
                }
                
                tx.commit()?;
            }
        },
        Commands::Replay { subcmd } => match subcmd {
            ReplayCommands::Session { id } => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                
                let mut actual_id = id.clone();
                
                if let Ok(idx) = id.parse::<i64>() {
                    let mut stmt = store.conn.prepare("SELECT id FROM sessions ORDER BY start_time ASC LIMIT 1 OFFSET ?1")?;
                    let mut rows = stmt.query(rusqlite::params![idx])?;
                    if let Some(row) = rows.next()? {
                        let row_id: String = row.get(0)?;
                        actual_id = row_id;
                    }
                }
                
                if let Some((start_time, end_time)) = store.get_session_info(&actual_id)? {
                    let events = store.get_events_by_session(&actual_id)?;
                    let s = session_engine::Session {
                        id: uuid::Uuid::parse_str(&actual_id).unwrap_or_default(),
                        start_time,
                        end_time,
                        events,
                    };
                    
                    let replay = ReplayEngine::new();
                    replay.replay_session(&s)?;
                } else {
                    println!("Session not found.");
                }
            }
        }
    }

    Ok(())
}
