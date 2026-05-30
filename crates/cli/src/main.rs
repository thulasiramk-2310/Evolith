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
                let store = EventStore::new(&db_path)?;
                let events = store.get_events()?;
                
                let engine = SessionEngine::new();
                let sessions = engine.rebuild_sessions(events)?;
                println!("Rebuilt {} sessions:", sessions.len());
                for (i, s) in sessions.iter().enumerate() {
                    println!("Session {}: {} ({} events)", i, s.id, s.events.len());
                }
            }
        },
        Commands::Replay { subcmd } => match subcmd {
            ReplayCommands::Session { id } => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let events = store.get_events()?;
                
                let engine = SessionEngine::new();
                let sessions = engine.rebuild_sessions(events)?;
                
                let target_session = sessions.iter().find(|s| s.id.to_string() == *id)
                    .or_else(|| {
                        if let Ok(idx) = id.parse::<usize>() {
                            sessions.get(idx)
                        } else {
                            None
                        }
                    });
                    
                match target_session {
                    Some(s) => {
                        let replay = ReplayEngine::new();
                        replay.replay_session(s)?;
                    }
                    None => println!("Session not found."),
                }
            }
        }
    }

    Ok(())
}
