use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use event_store::{Event, EventStore, EventType, Feature, FeatureAssignment};
use regex::Regex;
use serde::Serialize;
use session_engine::SessionEngine;
use replay_engine::ReplayEngine;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Collect { #[command(subcommand)] subcmd: CollectCommands },
    Import { file: PathBuf },
    Runs { #[command(subcommand)] subcmd: RunCommands },
    Events { #[command(subcommand)] subcmd: EventCommands },
    Sessions { #[command(subcommand)] subcmd: SessionCommands },
    Features { #[command(subcommand)] subcmd: FeatureCommands },
    Replay {
        #[command(subcommand)] subcmd: ReplayCommands,
    },
}

#[derive(Subcommand)]
enum EventCommands {
    List,
}

#[derive(Subcommand)]
enum CollectCommands {
    Git,
}

#[derive(Subcommand)]
enum RunCommands {
    List,
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    Rebuild,
}

#[derive(Subcommand)]
enum ReplayCommands {
    Session { id: String },
}

#[derive(Subcommand)]
enum FeatureCommands {
    Rebuild,
    List,
    Show { id: String },
    Explain {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

const FEATURE_ALGO_VERSION: &str = "v0.1-anchor-hybrid";

fn get_db_path() -> PathBuf {
    let preferred = Path::new(".evolith");
    let legacy = Path::new(".devreplay");
    if preferred.exists() || !legacy.exists() {
        return preferred.join("events.db");
    }
    legacy.join("events.db")
}

fn main() -> Result<()> {
    if let Some(invoked_as) = env::args().next() {
        let invoked_name = Path::new(&invoked_as)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if invoked_name == "devreplay" {
            eprintln!("Warning: `devreplay` is deprecated; use `evolith`.");
        }
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => {
            fs::create_dir_all(".evolith")?;
            let db_path = get_db_path();
            let _store = EventStore::new(&db_path)?;
            println!("Initialized Evolith in {}", db_path.parent().and_then(|p| p.to_str()).unwrap_or(".evolith"));
        }
        Commands::Collect { subcmd } => match subcmd {
            CollectCommands::Git => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let imported = collect_git_events(&store)?;
                println!("Collected {} git commit events.", imported);
            }
        },
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
        Commands::Runs { subcmd } => match subcmd {
            RunCommands::List => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let runs = store.list_feature_runs()?;
                if runs.is_empty() {
                    println!("No feature runs found.");
                } else {
                    for run in runs {
                        println!(
                            "{} {} status={} features={} anchor_coverage={:.1}% anchored={} unanchored={} avg_events_per_feature={:.2} dominant_anchor_ratio={:.2}",
                            run.started_at.format("%Y-%m-%d %H:%M:%S"),
                            run.algorithm_version,
                            run.status,
                            run.feature_count,
                            run.anchor_coverage * 100.0,
                            run.anchored_feature_count,
                            run.unanchored_feature_count,
                            run.avg_events_per_feature,
                            run.dominant_anchor_ratio
                        );
                    }
                }
            }
            RunCommands::Show { id, json } => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let run = if let Some(found) = store.get_feature_run_by_id(id)? {
                    Some(found)
                } else if let Ok(idx) = id.parse::<usize>() {
                    let runs = store.list_feature_runs()?;
                    runs.get(idx).cloned()
                } else {
                    None
                };

                if let Some(run) = run {
                    let run_health = classify_run_health(&run);
                    let observations = build_run_observations(&run);
                    if *json {
                        let report = RunExplainReport::from_run(&run, run_health, observations);
                        println!("{}", serde_json::to_string_pretty(&report)?);
                        return Ok(());
                    }
                    println!("Run {}", run.id);
                    println!();
                    println!("Algorithm:");
                    println!("{}", run.algorithm_version);
                    println!();
                    println!("Status:");
                    println!("{}", run.status);
                    println!();
                    println!("Window:");
                    println!(
                        "{} -> {}",
                        run.started_at.format("%Y-%m-%d %H:%M:%S"),
                        run.completed_at
                            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "in-progress".to_string())
                    );
                    println!();
                    println!("Metrics:");
                    println!("Events: {}", run.event_count);
                    println!("Features: {}", run.feature_count);
                    println!("Anchor Coverage: {:.1}%", run.anchor_coverage * 100.0);
                    println!("Anchored Features: {}", run.anchored_feature_count);
                    println!("Unanchored Features: {}", run.unanchored_feature_count);
                    println!("Avg Events/Feature: {:.2}", run.avg_events_per_feature);
                    println!("Dominant Anchor Ratio: {:.2}", run.dominant_anchor_ratio);
                    println!();
                    println!("Health:");
                    println!("{}", run_health);
                    println!();
                    println!("Observations:");
                    for line in observations {
                        println!("- {}", line);
                    }
                } else {
                    if *json {
                        println!("{}", serde_json::json!({"error":"feature run not found"}));
                    } else {
                        println!("Feature run not found.");
                    }
                }
            }
        },
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
        },
        Commands::Features { subcmd } => match subcmd {
            FeatureCommands::Rebuild => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let events = store.get_events()?;
                let run_id = Uuid::new_v4();
                store.create_feature_run(run_id, FEATURE_ALGO_VERSION, events.len() as i64)?;
                let rebuild_result: Result<i64> = (|| {
                    store.clear_feature_projection()?;
                    let grouped = build_feature_candidates(events);

                    let mut feature_count = 0_i64;
                    let mut anchored_feature_count = 0_i64;
                    let mut unanchored_feature_count = 0_i64;
                    let mut anchored_event_count = 0_i64;
                    let mut dominant_anchor_total = 0.0_f64;
                    let mut total_events_in_features = 0_i64;
                    for (_key, group) in grouped {
                        if group.is_empty() {
                            continue;
                        }
                        feature_count += 1;
                        total_events_in_features += group.len() as i64;
                        let now = Utc::now();
                        let project_id = group
                            .first()
                            .map(|e| e.project_id.clone())
                            .unwrap_or_else(|| "unknown".to_string());
                        let title = derive_feature_title(&group);
                        let source = derive_feature_source(&group);
                        let evidence = derive_feature_evidence(&group);
                        let is_anchored = source != "episode";
                        if is_anchored {
                            anchored_feature_count += 1;
                        } else {
                            unanchored_feature_count += 1;
                        }
                        if let Some(anchor_events) = evidence.get("anchor_event_count").and_then(|v| v.as_i64()) {
                            anchored_event_count += anchor_events;
                        }
                        if let Some(ratio) = evidence.get("dominant_anchor_ratio").and_then(|v| v.as_f64()) {
                            dominant_anchor_total += ratio;
                        }

                        let feature_id = Uuid::new_v4();
                        store.insert_feature(&Feature {
                            id: feature_id,
                            project_id,
                            title,
                            status: "active".to_string(),
                            evidence_json: evidence,
                            created_at: now,
                            updated_at: now,
                        })?;

                        for event in group {
                            store.insert_feature_assignment(&FeatureAssignment {
                                id: Uuid::new_v4(),
                                feature_id,
                                event_id: event.id,
                                confidence_score: if source == "episode" { 0.60 } else { 0.95 },
                                algorithm_version: FEATURE_ALGO_VERSION.to_string(),
                                source: source.clone(),
                                created_at: now,
                                superseded_by: None,
                            })?;
                        }
                    }

                    let total_event_count = store.event_count()?;
                    let anchor_coverage = if total_event_count > 0 {
                        anchored_event_count as f64 / total_event_count as f64
                    } else {
                        0.0
                    };
                    let avg_events_per_feature = if feature_count > 0 {
                        total_events_in_features as f64 / feature_count as f64
                    } else {
                        0.0
                    };
                    let dominant_anchor_ratio = if feature_count > 0 {
                        dominant_anchor_total / feature_count as f64
                    } else {
                        0.0
                    };

                    store.complete_feature_run(
                        run_id,
                        feature_count,
                        anchor_coverage,
                        anchored_feature_count,
                        unanchored_feature_count,
                        avg_events_per_feature,
                        dominant_anchor_ratio,
                    )?;
                    Ok(feature_count)
                })();

                match rebuild_result {
                    Ok(feature_count) => {
                        println!("Rebuilt {} features using {}.", feature_count, FEATURE_ALGO_VERSION);
                    }
                    Err(err) => {
                        let _ = store.fail_feature_run(run_id);
                        return Err(err);
                    }
                }
            }
            FeatureCommands::List => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                let features = store.list_features()?;
                if features.is_empty() {
                    println!("No features found. Run `evolith features rebuild` first.");
                } else {
                    for (i, feature) in features.iter().enumerate() {
                        println!(
                            "{} {} {} [{}]",
                            i,
                            feature.id,
                            feature.title,
                            feature.status
                        );
                    }
                }
            }
            FeatureCommands::Show { id } => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                if let Some(feature) = store.get_feature_by_id(id)? {
                    let events = store.get_feature_events(id)?;
                    println!("Feature {}", feature.id);
                    println!("Title: {}", feature.title);
                    println!("Project: {}", feature.project_id);
                    println!("Status: {}", feature.status);
                    println!("Events: {}", events.len());
                    let anchor_confidence = estimate_anchor_confidence(&feature);
                    let evidence_strength = estimate_evidence_strength(&feature, &events);
                    let feature_health = classify_feature_health(&feature, anchor_confidence, &evidence_strength);
                    println!("Anchor Confidence: {:.2}", anchor_confidence);
                    println!(
                        "Evidence Strength: {} ({:.2})",
                        evidence_strength.label, evidence_strength.score
                    );
                    println!("Feature Health: {}", feature_health);
                    println!("Evidence: {}", feature.evidence_json);
                    if let (Some(first), Some(last)) = (events.first(), events.last()) {
                        println!(
                            "Window: {} -> {}",
                            first.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            last.timestamp.format("%Y-%m-%d %H:%M:%S")
                        );
                    }
                } else {
                    println!("Feature not found.");
                }
            }
            FeatureCommands::Explain { id, json } => {
                let db_path = get_db_path();
                let store = EventStore::new(&db_path)?;
                if let Some(feature) = store.get_feature_by_id(id)? {
                    let events = store.get_feature_events(id)?;
                    let anchor_confidence = estimate_anchor_confidence(&feature);
                    let evidence_strength = estimate_evidence_strength(&feature, &events);
                    let feature_health = classify_feature_health(&feature, anchor_confidence, &evidence_strength);
                    if *json {
                        let report = build_feature_explain_report(
                            &feature,
                            &events,
                            anchor_confidence,
                            &evidence_strength,
                            feature_health,
                        );
                        println!("{}", serde_json::to_string_pretty(&report)?);
                        return Ok(());
                    }
                    let explanation = build_feature_explanation(
                        &feature,
                        &events,
                        anchor_confidence,
                        &evidence_strength,
                        feature_health,
                    );
                    println!("{explanation}");
                } else {
                    if *json {
                        println!("{}", serde_json::json!({"error":"feature not found"}));
                    } else {
                        println!("Feature not found.");
                    }
                }
            }
        },
    }

    Ok(())
}

fn extract_anchor(event: &Event) -> Option<(String, String)> {
    if let Some(ticket) = event.payload.get("ticket_id").and_then(|v| v.as_str()) {
        if !ticket.is_empty() {
            return Some(("ticket".to_string(), format!("ticket:{ticket}")));
        }
    }
    if let Some(pr) = event.payload.get("pr_id").and_then(|v| v.as_str()) {
        if !pr.is_empty() {
            return Some(("pr".to_string(), format!("pr:{pr}")));
        }
    }
    if let Some(branch) = &event.branch {
        if let Some(normalized) = normalize_branch_anchor(branch) {
            return Some(("branch".to_string(), format!("branch:{normalized}")));
        }
    }
    if let Some(pattern) = extract_commit_pattern_anchor(event) {
        return Some(("pattern".to_string(), format!("pattern:{pattern}")));
    }
    None
}

fn normalize_branch_anchor(branch: &str) -> Option<String> {
    let mut b = branch.trim().to_lowercase();
    if b.is_empty() {
        return None;
    }
    if b.starts_with("origin/") {
        b = b.trim_start_matches("origin/").to_string();
    }
    if b == "main" || b == "master" || b == "develop" || b == "dev" {
        return None;
    }
    for prefix in [
        "feature/",
        "feat/",
        "bugfix/",
        "hotfix/",
        "chore/",
        "task/",
        "fix/",
        "refactor/",
    ] {
        if b.starts_with(prefix) {
            b = b.trim_start_matches(prefix).to_string();
            break;
        }
    }
    let tokens: Vec<&str> = b
        .split(['/', '-', '_'])
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }
    for strong in [
        "auth", "authentication", "login", "jwt", "session", "payment", "billing", "invoice",
        "cache", "search", "backend", "frontend", "api", "db", "database", "interview",
    ] {
        if tokens.iter().any(|t| *t == strong) {
            return Some(strong.to_string());
        }
    }
    Some(tokens[0].to_string())
}

fn extract_commit_pattern_anchor(event: &Event) -> Option<String> {
    let msg = event
        .payload
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if msg.is_empty() {
        return None;
    }
    for pattern in [
        "auth", "authentication", "login", "jwt", "session", "payment", "billing", "invoice",
        "cache", "search", "frontend", "backend", "api", "database", "db", "interview",
        "refactor", "migration", "deploy", "ci", "test",
    ] {
        if msg.contains(pattern) {
            return Some(pattern.to_string());
        }
    }
    None
}

fn build_feature_candidates(events: Vec<Event>) -> HashMap<String, Vec<Event>> {
    let mut sorted = events;
    sorted.sort_by_key(|e| e.timestamp);

    let mut grouped: HashMap<String, Vec<Event>> = HashMap::new();
    let mut episode_counter: usize = 0;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut last_branch: Option<String> = None;
    let mut current_episode_key = String::new();

    for event in sorted {
        let key = if let Some((_src, anchor)) = extract_anchor(&event) {
            anchor
        } else {
            let is_new_episode = match (&last_ts, &last_branch) {
                (Some(ts), Some(branch)) => {
                    let gap = event.timestamp.signed_duration_since(*ts).num_minutes();
                    gap > 30 || (event.branch.clone().unwrap_or_default() != *branch)
                }
                _ => true,
            };
            if is_new_episode {
                episode_counter += 1;
                current_episode_key = format!("episode:{episode_counter}");
            }
            current_episode_key.clone()
        };

        last_ts = Some(event.timestamp);
        last_branch = Some(event.branch.clone().unwrap_or_else(|| "main".to_string()));
        grouped.entry(key).or_default().push(event);
    }
    grouped
}

fn derive_feature_title(events: &[Event]) -> String {
    if let Some(event) = events.first() {
        if let Some(ticket) = event.payload.get("ticket_id").and_then(|v| v.as_str()) {
            return format!("Ticket {}", ticket);
        }
        if let Some(pr) = event.payload.get("pr_id").and_then(|v| v.as_str()) {
            return format!("PR {}", pr);
        }
        if let Some(branch) = &event.branch {
            if let Some(normalized) = normalize_branch_anchor(branch) {
                return format!("Branch {}", normalized);
            }
        }
        if let Some(pattern) = extract_commit_pattern_anchor(event) {
            return format!("Pattern {}", pattern);
        }
    }
    "Episode Candidate".to_string()
}

fn derive_feature_source(events: &[Event]) -> String {
    if let Some(event) = events.first() {
        if event.payload.get("ticket_id").and_then(|v| v.as_str()).is_some() {
            return "ticket_anchor".to_string();
        }
        if event.payload.get("pr_id").and_then(|v| v.as_str()).is_some() {
            return "pr_anchor".to_string();
        }
        if event.branch.as_deref().is_some_and(|b| b != "main") {
            return "branch_anchor".to_string();
        }
        if extract_commit_pattern_anchor(event).is_some() {
            return "pattern_anchor".to_string();
        }
    }
    "episode".to_string()
}

fn derive_feature_evidence(events: &[Event]) -> serde_json::Value {
    let mut ticket_ids: Vec<String> = Vec::new();
    let mut pr_ids: Vec<String> = Vec::new();
    let mut branches: Vec<String> = Vec::new();
    let mut commit_count = 0_i64;
    let mut episode_count = 1_i64;
    let mut anchor_event_count = 0_i64;
    let mut anchor_key_counts: HashMap<String, i64> = HashMap::new();
    let mut last_ts: Option<DateTime<Utc>> = None;

    for event in events {
        if let Some((_, anchor_key)) = extract_anchor(event) {
            anchor_event_count += 1;
            *anchor_key_counts.entry(anchor_key).or_insert(0) += 1;
        }
        if let Some(ticket) = event.payload.get("ticket_id").and_then(|v| v.as_str()) {
            if !ticket_ids.iter().any(|t| t == ticket) {
                ticket_ids.push(ticket.to_string());
            }
        }
        if let Some(pr) = event.payload.get("pr_id").and_then(|v| v.as_str()) {
            if !pr_ids.iter().any(|p| p == pr) {
                pr_ids.push(pr.to_string());
            }
        }
        if let Some(branch) = &event.branch {
            if !branches.iter().any(|b| b == branch) {
                branches.push(branch.clone());
            }
        }
        if event.event_type == EventType::GitCommit {
            commit_count += 1;
        }
        if let Some(prev) = last_ts {
            if event.timestamp.signed_duration_since(prev).num_minutes() > 30 {
                episode_count += 1;
            }
        }
        last_ts = Some(event.timestamp);
    }

    let dominant_anchor_ratio = if events.is_empty() {
        0.0
    } else {
        let dominant = anchor_key_counts.values().copied().max().unwrap_or(0);
        dominant as f64 / events.len() as f64
    };

    serde_json::json!({
        "ticket_ids": ticket_ids,
        "pr_ids": pr_ids,
        "branches": branches,
        "commit_count": commit_count,
        "episode_count": episode_count,
        "anchor_event_count": anchor_event_count,
        "dominant_anchor_ratio": dominant_anchor_ratio
    })
}

fn estimate_anchor_confidence(feature: &Feature) -> f64 {
    let tickets = feature
        .evidence_json
        .get("ticket_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let prs = feature
        .evidence_json
        .get("pr_ids")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let branches = feature
        .evidence_json
        .get("branches")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    if tickets > 0 || prs > 0 {
        0.95
    } else if branches > 0 {
        0.85
    } else {
        0.60
    }
}

struct EvidenceStrength {
    score: f64,
    label: &'static str,
}

#[derive(Serialize)]
struct EvidenceSummary {
    event_count: usize,
    commit_count: i64,
    episode_count: i64,
    ticket_ids: Vec<String>,
    pr_ids: Vec<String>,
    branches: Vec<String>,
}

#[derive(Serialize)]
struct FeatureExplainReport {
    id: String,
    title: String,
    project_id: String,
    status: String,
    feature_health: String,
    anchor_confidence: f64,
    evidence_strength_label: String,
    evidence_strength_score: f64,
    dominant_anchor_ratio: f64,
    evidence: EvidenceSummary,
    why_grouped: String,
    caveat: String,
}

#[derive(Serialize)]
struct RunExplainReport {
    id: String,
    algorithm_version: String,
    status: String,
    started_at: String,
    completed_at: Option<String>,
    metrics: RunMetricsSummary,
    health: String,
    observations: Vec<String>,
}

#[derive(Serialize)]
struct RunMetricsSummary {
    event_count: i64,
    feature_count: i64,
    anchor_coverage: f64,
    anchored_feature_count: i64,
    unanchored_feature_count: i64,
    avg_events_per_feature: f64,
    dominant_anchor_ratio: f64,
}

impl RunExplainReport {
    fn from_run(run: &event_store::FeatureRun, health: &str, observations: Vec<String>) -> Self {
        Self {
            id: run.id.to_string(),
            algorithm_version: run.algorithm_version.clone(),
            status: run.status.clone(),
            started_at: run.started_at.to_rfc3339(),
            completed_at: run.completed_at.map(|t| t.to_rfc3339()),
            metrics: RunMetricsSummary {
                event_count: run.event_count,
                feature_count: run.feature_count,
                anchor_coverage: run.anchor_coverage,
                anchored_feature_count: run.anchored_feature_count,
                unanchored_feature_count: run.unanchored_feature_count,
                avg_events_per_feature: run.avg_events_per_feature,
                dominant_anchor_ratio: run.dominant_anchor_ratio,
            },
            health: health.to_string(),
            observations,
        }
    }
}

fn estimate_evidence_strength(feature: &Feature, events: &[Event]) -> EvidenceStrength {
    let commit_count = feature
        .evidence_json
        .get("commit_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as f64;
    let episode_count = feature
        .evidence_json
        .get("episode_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as f64;
    let event_count = events.len() as f64;

    // Deterministic saturation: scales quickly for small samples, then tapers.
    let commit_signal = (commit_count / 10.0).min(1.0);
    let event_signal = (event_count / 25.0).min(1.0);
    let episode_signal = (episode_count / 4.0).min(1.0);
    let score = (0.45 * commit_signal) + (0.40 * event_signal) + (0.15 * episode_signal);

    let label = if score >= 0.75 {
        "Strong"
    } else if score >= 0.40 {
        "Moderate"
    } else {
        "Weak"
    };
    EvidenceStrength { score, label }
}

fn build_feature_explanation(
    feature: &Feature,
    events: &[Event],
    anchor_confidence: f64,
    evidence_strength: &EvidenceStrength,
    feature_health: &str,
) -> String {
    let commit_count = feature
        .evidence_json
        .get("commit_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let episode_count = feature
        .evidence_json
        .get("episode_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let dominant_anchor_ratio = feature
        .evidence_json
        .get("dominant_anchor_ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let ticket_ids = feature
        .evidence_json
        .get("ticket_ids")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let pr_ids = feature
        .evidence_json
        .get("pr_ids")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let branches = feature
        .evidence_json
        .get("branches")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let mut evidence_lines: Vec<String> = Vec::new();
    evidence_lines.push(format!("- {} events", events.len()));
    evidence_lines.push(format!("- {} commits", commit_count));
    evidence_lines.push(format!("- {} episodes", episode_count));
    if !ticket_ids.is_empty() {
        evidence_lines.push(format!("- Ticket anchors: {}", ticket_ids.join(", ")));
    }
    if !pr_ids.is_empty() {
        evidence_lines.push(format!("- PR anchors: {}", pr_ids.join(", ")));
    }
    if !branches.is_empty() {
        evidence_lines.push(format!("- Branches: {}", branches.join(", ")));
    }

    let rationale = if dominant_anchor_ratio >= 0.75 {
        format!(
            "Strong anchor agreement: {:.0}% of events align to a dominant anchor.",
            dominant_anchor_ratio * 100.0
        )
    } else if dominant_anchor_ratio >= 0.50 {
        format!(
            "Moderate anchor agreement: {:.0}% of events align to a dominant anchor.",
            dominant_anchor_ratio * 100.0
        )
    } else {
        format!(
            "Low anchor agreement: {:.0}% of events align to a dominant anchor; grouping relies more on fallback heuristics.",
            dominant_anchor_ratio * 100.0
        )
    };

    format!(
        "Feature: {}\n\nFeature Health: {}\nAnchor Confidence: {:.2}\nEvidence Strength: {} ({:.2})\n\nEvidence:\n{}\n\nWhy grouped:\n{}\n{}",
        feature.title,
        feature_health,
        anchor_confidence,
        evidence_strength.label,
        evidence_strength.score,
        evidence_lines.join("\n"),
        rationale,
        if evidence_strength.label == "Weak" {
            "Limited evidence available."
        } else {
            "Evidence volume supports this grouping."
        }
    )
}

fn build_feature_explain_report(
    feature: &Feature,
    events: &[Event],
    anchor_confidence: f64,
    evidence_strength: &EvidenceStrength,
    feature_health: &str,
) -> FeatureExplainReport {
    let commit_count = feature
        .evidence_json
        .get("commit_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let episode_count = feature
        .evidence_json
        .get("episode_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let dominant_anchor_ratio = feature
        .evidence_json
        .get("dominant_anchor_ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let ticket_ids = feature
        .evidence_json
        .get("ticket_ids")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let pr_ids = feature
        .evidence_json
        .get("pr_ids")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    let branches = feature
        .evidence_json
        .get("branches")
        .and_then(|v| v.as_array())
        .map(|v| {
            v.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let rationale = if dominant_anchor_ratio >= 0.75 {
        format!(
            "Strong anchor agreement: {:.0}% of events align to a dominant anchor.",
            dominant_anchor_ratio * 100.0
        )
    } else if dominant_anchor_ratio >= 0.50 {
        format!(
            "Moderate anchor agreement: {:.0}% of events align to a dominant anchor.",
            dominant_anchor_ratio * 100.0
        )
    } else {
        format!(
            "Low anchor agreement: {:.0}% of events align to a dominant anchor; grouping relies more on fallback heuristics.",
            dominant_anchor_ratio * 100.0
        )
    };

    FeatureExplainReport {
        id: feature.id.to_string(),
        title: feature.title.clone(),
        project_id: feature.project_id.clone(),
        status: feature.status.clone(),
        feature_health: feature_health.to_string(),
        anchor_confidence,
        evidence_strength_label: evidence_strength.label.to_string(),
        evidence_strength_score: evidence_strength.score,
        dominant_anchor_ratio,
        evidence: EvidenceSummary {
            event_count: events.len(),
            commit_count,
            episode_count,
            ticket_ids,
            pr_ids,
            branches,
        },
        why_grouped: rationale,
        caveat: if evidence_strength.label == "Weak" {
            "Limited evidence available.".to_string()
        } else {
            "Evidence volume supports this grouping.".to_string()
        },
    }
}

fn classify_feature_health(feature: &Feature, anchor_confidence: f64, evidence_strength: &EvidenceStrength) -> &'static str {
    let title_lower = feature.title.to_lowercase();
    let is_episode_candidate = title_lower.contains("episode candidate");

    if is_episode_candidate && evidence_strength.label == "Weak" {
        return "Weak Evidence";
    }
    if evidence_strength.label == "Weak" {
        return "Weak Evidence";
    }
    if anchor_confidence < 0.50 && evidence_strength.score >= 0.40 {
        return "Anchor Conflict";
    }
    if anchor_confidence < 0.65 || evidence_strength.label == "Moderate" {
        return "Fragmented";
    }
    "Healthy"
}

fn classify_run_health(run: &event_store::FeatureRun) -> &'static str {
    if run.status != "completed" {
        return "Needs Attention";
    }
    if run.anchored_feature_count > run.unanchored_feature_count
        && run.anchor_coverage >= 0.30
        && run.dominant_anchor_ratio >= 0.60
    {
        return "Good";
    }
    if run.anchor_coverage >= 0.20 && run.dominant_anchor_ratio >= 0.50 {
        return "Moderate";
    }
    "Needs Attention"
}

fn build_run_observations(run: &event_store::FeatureRun) -> Vec<String> {
    let mut notes = Vec::new();
    if run.anchored_feature_count > run.unanchored_feature_count {
        notes.push("Anchored features exceed unanchored features.".to_string());
    } else {
        notes.push("Unanchored features are still significant.".to_string());
    }
    if run.dominant_anchor_ratio >= 0.60 {
        notes.push("Dominant anchor ratio is above 0.60.".to_string());
    } else {
        notes.push("Dominant anchor ratio is below 0.60; identity is weaker.".to_string());
    }
    if run.anchor_coverage >= 0.30 {
        notes.push("Anchor coverage is above 30%.".to_string());
    } else {
        notes.push("Anchor coverage is below 30%; extraction enrichment recommended.".to_string());
    }
    if run.avg_events_per_feature < 2.0 {
        notes.push("High fragmentation risk: average events per feature is below 2.0.".to_string());
    } else {
        notes.push("Feature consolidation is reasonable based on average events per feature.".to_string());
    }
    notes
}

fn collect_git_events(store: &EventStore) -> Result<usize> {
    let output = Command::new("git")
        .args([
            "log",
            "--all",
            "--date=iso-strict",
            "--pretty=format:%H%x1f%aI%x1f%an%x1f%D%x1f%s%x1e",
        ])
        .output()
        .context("Failed to execute git log")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("git log returned non-zero exit status"));
    }

    let raw = String::from_utf8(output.stdout).context("git log output was not UTF-8")?;
    collect_git_events_from_log(store, &raw)
}

fn collect_git_events_from_log(store: &EventStore, raw: &str) -> Result<usize> {
    let ticket_re = Regex::new(r"\b([A-Z]{2,}-\d+)\b").context("Invalid ticket regex")?;
    let pr_re = Regex::new(r"#(\d+)").context("Invalid PR regex")?;

    let mut imported = 0_usize;
    for rec in raw.split('\x1e') {
        if rec.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = rec.split('\x1f').collect();
        if parts.len() < 5 {
            continue;
        }
        let commit_hash = parts[0].trim();
        let ts = parts[1].trim();
        let author = parts[2].trim();
        let refs = parts[3].trim();
        let message = parts[4].trim();

        let timestamp = DateTime::parse_from_rfc3339(ts)
            .map(|v| v.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let branch = infer_branch_from_refs(refs);
        let ticket_id = ticket_re
            .captures(message)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
        let pr_id = pr_re
            .captures(message)
            .and_then(|c| c.get(1).map(|m| format!("PR-{}", m.as_str())));

        let payload = serde_json::json!({
            "commit_hash": commit_hash,
            "author": author,
            "message": message,
            "ticket_id": ticket_id,
            "pr_id": pr_id
        });

        if store.has_git_commit_hash(commit_hash)? {
            continue;
        }

        let event = Event {
            id: Uuid::new_v4(),
            timestamp,
            event_type: EventType::GitCommit,
            project_id: "devreplay".to_string(),
            payload,
            path: None,
            branch,
            metadata: None,
            collector_version: "git-collector-v0.1".to_string(),
            created_at: Utc::now(),
        };
        store.insert_event(&event)?;
        imported += 1;
    }
    Ok(imported)
}

fn infer_branch_from_refs(refs: &str) -> Option<String> {
    for token in refs.split(',').map(|s| s.trim()) {
        if token.is_empty() {
            continue;
        }
        if let Some(rest) = token.strip_prefix("HEAD -> ") {
            return Some(rest.to_string());
        }
        if token.starts_with("tag:") || token == "HEAD" || token.contains("origin/HEAD") {
            continue;
        }
        if token.contains('/') {
            return Some(token.trim_start_matches("origin/").to_string());
        }
        return Some(token.to_string());
    }
    Some("main".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    fn make_event(
        minutes_offset: i64,
        file: &str,
        branch: &str,
        ticket: Option<&str>,
        pr: Option<&str>,
    ) -> Event {
        let mut payload = serde_json::json!({ "file": file });
        if let Some(t) = ticket {
            payload["ticket_id"] = serde_json::Value::String(t.to_string());
        }
        if let Some(p) = pr {
            payload["pr_id"] = serde_json::Value::String(p.to_string());
        }
        Event {
            id: Uuid::new_v4(),
            timestamp: Utc::now() + TimeDelta::minutes(minutes_offset),
            event_type: EventType::FileModified,
            project_id: "proj-1".to_string(),
            payload,
            path: Some(file.to_string()),
            branch: Some(branch.to_string()),
            metadata: None,
            collector_version: "test".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn feature_rebuild_flow_groups_and_persists_features() -> Result<()> {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("devreplay-cli-flow-{}.db", Uuid::new_v4()));
        let store = EventStore::new(&db_path)?;

        let events = vec![
            make_event(0, "src/auth.ts", "main", Some("AUTH-123"), None),
            make_event(1, "src/jwt.ts", "main", Some("AUTH-123"), None),
            make_event(2, "src/auth.ts", "main", Some("AUTH-456"), None),
            make_event(3, "src/jwt.ts", "main", Some("AUTH-456"), None),
        ];
        for e in &events {
            store.insert_event(e)?;
        }

        let grouped = build_feature_candidates(store.get_events()?);
        assert_eq!(grouped.len(), 2, "expected two feature groups from two anchors");

        store.clear_feature_projection()?;
        for (_key, group) in grouped {
            let now = Utc::now();
            let feature_id = Uuid::new_v4();
            store.insert_feature(&Feature {
                id: feature_id,
                project_id: "proj-1".to_string(),
                title: derive_feature_title(&group),
                status: "active".to_string(),
                evidence_json: derive_feature_evidence(&group),
                created_at: now,
                updated_at: now,
            })?;
            for event in group {
                store.insert_feature_assignment(&FeatureAssignment {
                    id: Uuid::new_v4(),
                    feature_id,
                    event_id: event.id,
                    confidence_score: 0.95,
                    algorithm_version: FEATURE_ALGO_VERSION.to_string(),
                    source: derive_feature_source(std::slice::from_ref(&event)),
                    created_at: now,
                    superseded_by: None,
                })?;
            }
        }

        let features = store.list_features()?;
        assert_eq!(features.len(), 2);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn anchor_dominance_keeps_identical_files_separate_by_ticket() {
        let events = vec![
            make_event(0, "src/auth.ts", "main", Some("AUTH-123"), None),
            make_event(1, "src/jwt.ts", "main", Some("AUTH-123"), None),
            make_event(2, "src/session.ts", "main", Some("AUTH-123"), None),
            make_event(3, "src/auth.ts", "main", Some("AUTH-456"), None),
            make_event(4, "src/jwt.ts", "main", Some("AUTH-456"), None),
            make_event(5, "src/session.ts", "main", Some("AUTH-456"), None),
        ];

        let grouped = build_feature_candidates(events);
        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key("ticket:AUTH-123"));
        assert!(grouped.contains_key("ticket:AUTH-456"));
    }

    #[test]
    fn collect_git_is_idempotent() -> Result<()> {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("devreplay-collect-idempotent-{}.db", Uuid::new_v4()));
        let store = EventStore::new(&db_path)?;

        let sample_log = concat!(
            "abc123\x1f2026-05-31T10:00:00Z\x1fAlice\x1fHEAD -> feature/auth-refresh\x1fAUTH-123: migrate jwt flow\x1e",
            "def456\x1f2026-05-31T11:00:00Z\x1fBob\x1forigin/feature/auth-refresh\x1f#88 refine token exchange\x1e"
        );

        let first = collect_git_events_from_log(&store, sample_log)?;
        let first_count = store.event_count()?;
        let second = collect_git_events_from_log(&store, sample_log)?;
        let second_count = store.event_count()?;

        assert_eq!(first, 2);
        assert_eq!(second, 0);
        assert_eq!(first_count, second_count);
        assert_eq!(first_count, 2);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn repeated_git_collection_does_not_change_counts() -> Result<()> {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("devreplay-repeat-collect-{}.db", Uuid::new_v4()));
        let store = EventStore::new(&db_path)?;
        let sample_log = concat!(
            "a1\x1f2026-05-31T10:00:00Z\x1fAlice\x1fHEAD -> feature/a\x1fAUTH-1 init\x1e",
            "b2\x1f2026-05-31T10:05:00Z\x1fAlice\x1forigin/feature/a\x1fAUTH-1 tweak #12\x1e",
            "c3\x1f2026-05-31T10:10:00Z\x1fBob\x1fmain\x1fminor cleanup\x1e"
        );
        collect_git_events_from_log(&store, sample_log)?;
        let count1 = store.event_count()?;
        collect_git_events_from_log(&store, sample_log)?;
        let count2 = store.event_count()?;
        assert_eq!(count1, count2);
        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn run_metrics_are_populated() -> Result<()> {
        let mut db_path = std::env::temp_dir();
        db_path.push(format!("devreplay-run-metrics-{}.db", Uuid::new_v4()));
        let store = EventStore::new(&db_path)?;

        let events = vec![
            make_event(0, "src/auth.ts", "main", Some("AUTH-123"), None),
            make_event(1, "src/jwt.ts", "main", Some("AUTH-123"), None),
            make_event(2, "src/pay.ts", "feature/payments", None, None),
            make_event(40, "src/pay.ts", "feature/payments", None, None),
        ];
        for e in events {
            store.insert_event(&e)?;
        }

        let run_id = Uuid::new_v4();
        store.create_feature_run(run_id, FEATURE_ALGO_VERSION, store.event_count()?)?;
        store.clear_feature_projection()?;
        let grouped = build_feature_candidates(store.get_events()?);

        let mut feature_count = 0_i64;
        let mut anchored_feature_count = 0_i64;
        let mut unanchored_feature_count = 0_i64;
        let mut anchored_event_count = 0_i64;
        let mut dominant_anchor_total = 0.0_f64;
        let mut total_events_in_features = 0_i64;

        for (_k, group) in grouped {
            feature_count += 1;
            total_events_in_features += group.len() as i64;
            let source = derive_feature_source(&group);
            if source == "episode" {
                unanchored_feature_count += 1;
            } else {
                anchored_feature_count += 1;
            }
            let evidence = derive_feature_evidence(&group);
            anchored_event_count += evidence
                .get("anchor_event_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            dominant_anchor_total += evidence
                .get("dominant_anchor_ratio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
        }
        let total_event_count = store.event_count()?;
        let anchor_coverage = anchored_event_count as f64 / total_event_count as f64;
        let avg_events_per_feature = total_events_in_features as f64 / feature_count as f64;
        let dominant_anchor_ratio = dominant_anchor_total / feature_count as f64;
        store.complete_feature_run(
            run_id,
            feature_count,
            anchor_coverage,
            anchored_feature_count,
            unanchored_feature_count,
            avg_events_per_feature,
            dominant_anchor_ratio,
        )?;

        let runs = store.list_feature_runs()?;
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert!(run.completed_at.is_some());
        assert!(run.feature_count > 0);
        assert!(run.avg_events_per_feature > 0.0);
        assert!(run.anchor_coverage > 0.0);
        assert!(run.anchored_feature_count >= 1);
        assert!(run.unanchored_feature_count >= 0);
        assert!(run.dominant_anchor_ratio >= 0.0);

        let _ = std::fs::remove_file(db_path);
        Ok(())
    }

    #[test]
    fn feature_explain_includes_anchor_confidence_evidence_strength_and_reasoning() {
        let feature = Feature {
            id: Uuid::new_v4(),
            project_id: "proj-1".to_string(),
            title: "Pattern interview".to_string(),
            status: "active".to_string(),
            evidence_json: serde_json::json!({
                "commit_count": 8,
                "episode_count": 2,
                "ticket_ids": [],
                "pr_ids": ["PR-12"],
                "branches": ["feature/interview"],
                "dominant_anchor_ratio": 0.75
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let events = vec![
            make_event(0, "src/interview.ts", "feature/interview", None, Some("PR-12")),
            make_event(1, "src/interview.ts", "feature/interview", None, Some("PR-12")),
        ];
        let evidence_strength = estimate_evidence_strength(&feature, &events);
        let health = classify_feature_health(&feature, 0.82, &evidence_strength);
        let explanation = build_feature_explanation(&feature, &events, 0.82, &evidence_strength, health);
        assert!(explanation.contains("Feature: Pattern interview"));
        assert!(explanation.contains("Feature Health:"));
        assert!(explanation.contains("Anchor Confidence: 0.82"));
        assert!(explanation.contains("Evidence Strength:"));
        assert!(explanation.contains("PR anchors: PR-12"));
        assert!(explanation.contains("feature/interview"));
        assert!(explanation.contains("Strong anchor agreement") || explanation.contains("Moderate anchor agreement"));
    }

    #[test]
    fn health_marks_weak_single_event_features_as_weak_evidence() {
        let feature = Feature {
            id: Uuid::new_v4(),
            project_id: "proj-1".to_string(),
            title: "Branch aiml".to_string(),
            status: "active".to_string(),
            evidence_json: serde_json::json!({
                "commit_count": 1,
                "episode_count": 1,
                "ticket_ids": [],
                "pr_ids": [],
                "branches": ["feature/aiml-engine"],
                "dominant_anchor_ratio": 1.0
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let events = vec![make_event(0, "src/aiml.ts", "feature/aiml-engine", None, None)];
        let evidence_strength = estimate_evidence_strength(&feature, &events);
        let health = classify_feature_health(&feature, 0.85, &evidence_strength);
        assert_eq!(health, "Weak Evidence");
    }
}
