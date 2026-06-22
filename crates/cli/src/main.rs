use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use event_store::{Event, EventStore, EventType, Feature, FeatureAssignment, FeatureRunMetrics};
use regex::Regex;
use serde::Serialize;
use session_engine::SessionEngine;
use replay_engine::ReplayEngine;
use std::collections::HashMap;
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
    /// Assess repository health and get actionable recommendations
    Doctor {
        #[arg(long)]
        json: bool,
        /// Show a focused security-only report (no feature run required)
        #[arg(long)]
        security: bool,
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

// Doctor command thresholds — keep in one place for easy tuning (RFC-006)
const DOCTOR_RULESET_VERSION: &str = "v0.1";
const DOCTOR_ANCHOR_COV_GOOD: f64 = 0.40;
const DOCTOR_ANCHOR_COV_LOW: f64 = 0.20;
const DOCTOR_ANCHOR_COV_WARN: f64 = 0.30;
const DOCTOR_DOM_RATIO_GOOD: f64 = 0.70;
const DOCTOR_DOM_RATIO_POOR: f64 = 0.50;
const DOCTOR_DOM_RATIO_WARN: f64 = 0.60;
const DOCTOR_FRAG_THRESHOLD: f64 = 2.0;
const DOCTOR_OVER_MERGE_THRESHOLD: f64 = 25.0;
const DOCTOR_WEAK_RATE_POOR: f64 = 0.40;
const DOCTOR_WEAK_RATE_WARN: f64 = 0.30;

// ─── Repository Kind Detection ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RepoKind {
    Personal,
    Team,
    OpenSource,
    AiMl,
    Production,
    CliTool,
    Library,
    Hackathon,
    Unknown,
}

impl RepoKind {
    fn label(&self) -> &'static str {
        match self {
            RepoKind::Personal    => "Personal Project",
            RepoKind::Team        => "Team Project",
            RepoKind::OpenSource  => "Open Source Project",
            RepoKind::AiMl        => "AI / ML Project",
            RepoKind::Production  => "Production Project",
            RepoKind::CliTool     => "CLI Tool",
            RepoKind::Library     => "Library",
            RepoKind::Hackathon   => "Hackathon / Student Project",
            RepoKind::Unknown     => "Project",
        }
    }

    fn opening_line(&self, readable: i64, total: i64) -> String {
        let pct = if total > 0 { readable * 100 / total } else { 0 };
        match self {
            RepoKind::Personal => format!(
                "This is a personal project. Evolith can follow {pct}% of your history."
            ),
            RepoKind::Hackathon => format!(
                "This looks like a hackathon or student project — quick, sparse commits are normal.\n  Evolith can trace {pct}% of your project's story so far."
            ),
            RepoKind::AiMl => format!(
                "This is an AI / ML project. Evolith can trace {pct}% of your experiments and iterations."
            ),
            RepoKind::Production => format!(
                "This is a production project. Evolith can follow {pct}% of your team's history."
            ),
            RepoKind::OpenSource => format!(
                "This is an open source project. Evolith can read {pct}% of the public contribution history."
            ),
            RepoKind::CliTool => format!(
                "This is a CLI tool. Evolith can trace {pct}% of your development arc."
            ),
            RepoKind::Library => format!(
                "This is a library. Evolith can follow {pct}% of the API evolution history."
            ),
            RepoKind::Team => format!(
                "This is a team project. Evolith can follow {pct}% of your shared history."
            ),
            RepoKind::Unknown => format!(
                "Evolith can follow {pct}% of this project's history."
            ),
        }
    }

    fn context_note(&self) -> Option<&'static str> {
        match self {
            RepoKind::Personal  => Some("Personal projects often have informal commits — that's completely fine. These notes help Evolith follow your story, not grade your style."),
            RepoKind::Hackathon => Some("Hackathon commits are fast and scrappy by design. Evolith won't ask you to slow down — just a few small hints help it read your progress."),
            RepoKind::AiMl      => Some("ML projects often have lots of exploratory commits. Naming your experiments in commit messages helps Evolith trace your research arc."),
            _                   => None,
        }
    }

    fn action_header(&self) -> &'static str {
        match self {
            RepoKind::Personal | RepoKind::Hackathon => "What Would Help",
            _ => "What To Do",
        }
    }

    fn filter_recommendation(&self, id: &str) -> bool {
        match self {
            RepoKind::Personal | RepoKind::Hackathon => {
                // Skip over-merge advice for tiny personal projects
                id != "over_merge_risk"
            }
            _ => true,
        }
    }
}

fn count_git_contributors() -> usize {
    let out = Command::new("git").args(["log", "--format=%ae"]).output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let unique: std::collections::HashSet<&str> =
                text.lines().filter(|l| !l.is_empty()).collect();
            unique.len().max(1)
        }
        _ => 1,
    }
}

fn detect_repo_kind(files: &[String], contributors: usize, commit_count: i64) -> RepoKind {
    let lower: Vec<String> = files.iter().map(|f| f.to_lowercase()).collect();
    let has = |pat: &str| lower.iter().any(|f| f.contains(pat));

    let has_notebooks   = has(".ipynb");
    let has_docker      = has("dockerfile") || has("docker-compose");
    let has_ci          = has(".github/workflows/") || has(".gitlab-ci") || has(".circleci/");
    let has_contributing = has("contributing.md");
    let has_cargo       = has("cargo.toml");
    let has_lib_rs      = has("src/lib.rs");
    let has_main_rs     = has("src/main.rs");
    let has_lib_only    = has_lib_rs && !has_main_rs;
    let has_ml          = has_notebooks || (has("requirements.txt") && (has("train") || has("model") || has("notebook")));

    if has_ml || has_notebooks {
        return RepoKind::AiMl;
    }
    if contributors >= 10 || has_contributing {
        return RepoKind::OpenSource;
    }
    if (has_docker || has_ci) && contributors >= 2 {
        return RepoKind::Production;
    }
    if has_lib_only {
        return RepoKind::Library;
    }
    if has_cargo && !has_docker && contributors <= 2 {
        return RepoKind::CliTool;
    }
    if contributors == 1 && commit_count < 25 {
        return RepoKind::Hackathon;
    }
    if contributors == 1 {
        return RepoKind::Personal;
    }
    if contributors >= 2 {
        return RepoKind::Team;
    }
    RepoKind::Unknown
}

fn get_db_path() -> PathBuf {
    let legacy = Path::new(".devreplay");
    if legacy.exists() && !Path::new(".evolith").exists() {
        if let Err(e) = fs::rename(legacy, ".evolith") {
            eprintln!("Warning: could not migrate .devreplay to .evolith: {e}");
        } else {
            println!("Migrated legacy DevReplay data to Evolith.");
        }
    }
    Path::new(".evolith").join("events.db")
}

fn main() -> Result<()> {
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
                    println!("No analysis runs yet. Run `evolith features rebuild` to analyze your project.");
                } else {
                    for run in runs {
                        let health = classify_run_health(&run);
                        println!(
                            "  {}  {} feature{}  ·  {} identified  ·  {} unknown  ·  {}",
                            run.started_at.format("%Y-%m-%d %H:%M"),
                            run.feature_count,
                            if run.feature_count == 1 { "" } else { "s" },
                            run.anchored_feature_count,
                            run.unanchored_feature_count,
                            health,
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
                    println!("Analysis  ·  {}", run.started_at.format("%Y-%m-%d %H:%M"));
                    println!();
                    println!("Repository Health:  {}", run_health);
                    println!();
                    for obs in &observations {
                        println!("  ✓  {}", obs);
                    }
                    println!();
                    println!("{}", "─".repeat(48));
                    println!();
                    println!("Details");
                    println!();
                    println!("  Events collected:      {}", run.event_count);
                    println!("  Features found:        {}", run.feature_count);
                    println!("  Identity Coverage:     {:.1}%", run.anchor_coverage * 100.0);
                    println!("  Identified features:   {}", run.anchored_feature_count);
                    println!("  Unknown features:      {}", run.unanchored_feature_count);
                    println!("  Avg commits / feature: {:.2}", run.avg_events_per_feature);
                    println!("  Feature Consistency:   {:.2}", run.dominant_anchor_ratio);
                } else if *json {
                    println!("{}", serde_json::json!({"error":"analysis run not found"}));
                } else {
                    println!("Analysis run not found.");
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

                    store.complete_feature_run(run_id, FeatureRunMetrics {
                        feature_count,
                        anchor_coverage,
                        anchored_feature_count,
                        unanchored_feature_count,
                        avg_events_per_feature,
                        dominant_anchor_ratio,
                    })?;
                    Ok(feature_count)
                })();

                match rebuild_result {
                    Ok(feature_count) => {
                        println!(
                            "Done. Found {} feature{} in your project history.",
                            feature_count,
                            if feature_count == 1 { "" } else { "s" }
                        );
                        println!("Run `evolith features list` to see them.");
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
                    println!("No features found yet. Run `evolith features rebuild` to analyze your project history.");
                } else {
                    println!("Your project has {} identified feature{}.\n", features.len(), if features.len() == 1 { "" } else { "s" });
                    for (i, feature) in features.iter().enumerate() {
                        println!(
                            "  {:>2}.  {}  [{}]",
                            i + 1,
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
                    let clarity_label = if anchor_confidence >= 0.75 { "Strong" } else if anchor_confidence >= 0.50 { "Moderate" } else { "Low" };
                    println!("{}", feature.title);
                    println!("{}", "─".repeat(50));
                    println!();
                    println!("  Status:            {}", feature.status);
                    println!("  Feature Health:    {}", feature_health);
                    println!("  Identity Clarity:  {}", clarity_label);
                    println!("  History Strength:  {}", evidence_strength.label);
                    println!("  Events:            {}", events.len());
                    if let (Some(first), Some(last)) = (events.first(), events.last()) {
                        println!(
                            "  Work period:       {} → {}",
                            first.timestamp.format("%Y-%m-%d"),
                            last.timestamp.format("%Y-%m-%d")
                        );
                    }
                } else {
                    println!("Feature not found. Try `evolith features list` to see all features.");
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
                } else if *json {
                    println!("{}", serde_json::json!({"error":"feature not found"}));
                } else {
                    println!("Feature not found. Try `evolith features list` to see all features.");
                }
            }
        },
        Commands::Doctor { json, security } => {
            if *security {
                // Security-only mode — no feature run required
                let sec = scan_security();
                if *json {
                    println!("{}", serde_json::to_string_pretty(&sec)?);
                } else {
                    print!("{}", render_security_text(&sec));
                }
                return Ok(());
            }
            let db_path = get_db_path();
            let store = EventStore::new(&db_path)?;
            let run = match store.get_latest_completed_run()? {
                Some(r) => r,
                None => {
                    if *json {
                        println!("{}", serde_json::json!({"error": "no completed feature run found; run `evolith features rebuild` first"}));
                    } else {
                        println!("No analysis complete yet.");
                        println!("Run `evolith features rebuild` first to analyze your project.");
                    }
                    return Ok(());
                }
            };
            let features = store.list_features()?;
            let security_report = scan_security();
            let tracked = list_tracked_files();
            let contributors = count_git_contributors();
            let repo_kind = detect_repo_kind(&tracked, contributors, run.event_count);
            let report = build_doctor_report(&run, &features, security_report, repo_kind.clone());
            if *json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", render_doctor_text(&report, &repo_kind));
            }
        }
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
        if tokens.contains(&strong) {
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

#[derive(Serialize)]
struct DoctorRecommendation {
    id: String,
    priority: String,
    message: String,
    expected_impact: String,
}

#[derive(Serialize)]
struct DoctorMetrics {
    anchor_coverage: f64,
    dominant_anchor_ratio: f64,
    feature_count: i64,
    anchored_feature_count: i64,
    unanchored_feature_count: i64,
    avg_events_per_feature: f64,
    weak_evidence_feature_count: i64,
    weak_evidence_rate: f64,
}

#[derive(Serialize)]
struct DoctorScoring {
    overall_score: f64,
    band: String,
}

#[derive(Serialize, Clone)]
struct SecurityFinding {
    severity: String,
    kind: String,
    message: String,
    location: Option<String>,
}

#[derive(Serialize)]
struct SecurityReport {
    risk_level: String,
    findings: Vec<SecurityFinding>,
    recommendations: Vec<String>,
    scanned_file_count: usize,
}

impl SecurityReport {
    #[cfg(test)]
    fn empty() -> Self {
        Self { risk_level: "None".to_string(), findings: vec![], recommendations: vec![], scanned_file_count: 0 }
    }
}

#[derive(Serialize)]
struct DoctorReport {
    run_id: String,
    ruleset_version: String,
    status: String,
    repository_health: String,
    repo_kind: String,
    security: SecurityReport,
    metrics: DoctorMetrics,
    issues: Vec<String>,
    recommendations: Vec<DoctorRecommendation>,
    scoring: DoctorScoring,
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

    // What happened?
    let mut what_parts: Vec<String> = Vec::new();
    what_parts.push(format!(
        "{} commit{}",
        commit_count,
        if commit_count == 1 { "" } else { "s" }
    ));
    what_parts.push(format!(
        "{} file change{}",
        events.len(),
        if events.len() == 1 { "" } else { "s" }
    ));
    if !ticket_ids.is_empty() {
        what_parts.push(format!("linked to {}", ticket_ids.join(", ")));
    }
    if !branches.is_empty() {
        what_parts.push(format!("on {}", branches[0]));
    }
    let what_happened = what_parts.join("  ·  ");

    // Why it matters
    let why_matters = match feature_health {
        "Well Defined" => format!(
            "This feature is clearly identified — {pct:.0}% of its commits point to the same \
             feature. Evolith can reliably trace this back through your history.",
            pct = dominant_anchor_ratio * 100.0
        ),
        "Limited History" => {
            "This feature has very few commits. Its history is too thin to reconstruct \
             reliably — not enough signal to be confident about what belongs here."
                .to_string()
        }
        "Mixed Signals" => {
            "Commits in this feature point in different directions. It may contain \
             unrelated changes bundled together, making it hard to understand as a unit."
                .to_string()
        }
        "Scattered Commits" => format!(
            "Only {pct:.0}% of commits agree on what this feature is. The history exists, \
             but it's fragmented — future readers (or your future self) will find it confusing.",
            pct = dominant_anchor_ratio * 100.0
        ),
        _ => "Feature identity could not be reliably determined.".to_string(),
    };

    // What to do
    let what_to_do = match feature_health {
        "Well Defined" => {
            "Nothing — this feature is in good shape. Keep referencing \
             tickets and PRs in your commits to maintain this clarity."
        }
        "Limited History" => {
            "Add more commits that reference this feature's ticket or branch. \
             Even a small follow-up commit improves your project memory."
        }
        "Mixed Signals" => {
            "Review which commits actually belong here. Use a consistent ticket ID \
             or branch name to keep related changes together."
        }
        "Scattered Commits" => {
            "Use a stable ticket reference (e.g. AUTH-123) across all commits \
             in this feature so Evolith can group them with confidence."
        }
        _ => "Review this feature manually to understand what it represents.",
    };

    // Expected result
    let expected = match feature_health {
        "Well Defined" => "Your project memory for this feature is reliable.",
        "Limited History" => "More commits will make this feature clearly identifiable.",
        "Mixed Signals" => "A consistent reference will make this feature readable at a glance.",
        "Scattered Commits" => "Stable commit references will make this feature Well Defined.",
        _ => "Feature history will become easier to read.",
    };

    // Identity links
    let mut links: Vec<String> = Vec::new();
    if !ticket_ids.is_empty() {
        links.push(ticket_ids.join(", "));
    }
    if !pr_ids.is_empty() {
        links.push(format!("PR {}", pr_ids.join(", ")));
    }
    if !branches.is_empty() {
        links.push(branches.join(", "));
    }
    let linked_to = if links.is_empty() {
        "none detected".to_string()
    } else {
        links.join("  ·  ")
    };

    let clarity_label = if anchor_confidence >= 0.75 {
        "Strong"
    } else if anchor_confidence >= 0.50 {
        "Moderate"
    } else {
        "Low"
    };

    let sep = "─".repeat(50);
    format!(
        "{title}\n{sep}\n\nWhat happened?\n  {what}\n\nWhy it matters\n  {why}\n\nWhat to do\n  {todo}\n\nExpected result\n  {expected}\n\n{sep}\nIdentity Clarity:   {clarity}\nHistory Strength:   {strength}\nCommits:            {commits}\nWork sessions:      {episodes}\nLinked to:          {linked}\n",
        title = feature.title,
        sep = sep,
        what = what_happened,
        why = why_matters,
        todo = what_to_do,
        expected = expected,
        clarity = clarity_label,
        strength = evidence_strength.label,
        commits = commit_count,
        episodes = episode_count,
        linked = linked_to
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
        return "Needs More Context";
    }
    if evidence_strength.label == "Weak" {
        return "Needs More Context";
    }
    if anchor_confidence < 0.50 && evidence_strength.score >= 0.40 {
        return "Mixed Signals";
    }
    if anchor_confidence < 0.65 || evidence_strength.label == "Moderate" {
        return "Not Fully Readable";
    }
    "Well Defined"
}

fn classify_run_health(run: &event_store::FeatureRun) -> &'static str {
    if run.status != "completed" {
        return "Thin";
    }
    if run.anchored_feature_count > run.unanchored_feature_count
        && run.anchor_coverage >= 0.30
        && run.dominant_anchor_ratio >= 0.60
    {
        return "Clear";
    }
    if run.anchor_coverage >= 0.20 && run.dominant_anchor_ratio >= 0.50 {
        return "Forming";
    }
    "Thin"
}

fn build_run_observations(run: &event_store::FeatureRun) -> Vec<String> {
    let mut notes = Vec::new();
    if run.anchored_feature_count > run.unanchored_feature_count {
        notes.push("Most features are clearly identified.".to_string());
    } else {
        notes.push("More than half your features are unidentified — your history is hard to read.".to_string());
    }
    if run.dominant_anchor_ratio >= 0.60 {
        notes.push("Commits are consistent within each feature — easy to trace what changed and why.".to_string());
    } else {
        notes.push("Commits are mixed across features — your project story is harder to follow.".to_string());
    }
    if run.anchor_coverage >= 0.30 {
        notes.push("Enough commits reference tickets or PRs to reconstruct your project story.".to_string());
    } else {
        notes.push("Most commits lack ticket or PR references — your project story is incomplete.".to_string());
    }
    if run.avg_events_per_feature < 2.0 {
        notes.push("Most features have very few commits — consider grouping related work together.".to_string());
    } else {
        notes.push("Features have a reasonable number of commits — project history looks coherent.".to_string());
    }
    notes
}

fn count_weak_features(features: &[Feature]) -> i64 {
    features
        .iter()
        .filter(|f| {
            let commits = f
                .evidence_json
                .get("commit_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let episodes = f
                .evidence_json
                .get("episode_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            commits + episodes <= 2
        })
        .count() as i64
}

fn score_anchor_coverage(cov: f64) -> f64 {
    // Piecewise: Poor(<0.20)→0–0.30, Moderate(0.20–0.40)→0.30–0.70, Good(≥0.40)→0.70–1.0
    if cov >= DOCTOR_ANCHOR_COV_GOOD {
        0.70 + 0.30 * (cov - DOCTOR_ANCHOR_COV_GOOD) / (1.0 - DOCTOR_ANCHOR_COV_GOOD)
    } else if cov >= DOCTOR_ANCHOR_COV_LOW {
        0.30 + 0.40 * (cov - DOCTOR_ANCHOR_COV_LOW) / (DOCTOR_ANCHOR_COV_GOOD - DOCTOR_ANCHOR_COV_LOW)
    } else {
        0.30 * (cov / DOCTOR_ANCHOR_COV_LOW)
    }
}

fn score_dominant_ratio(ratio: f64) -> f64 {
    // Piecewise: Poor(<0.50)→0–0.30, Moderate(0.50–0.70)→0.30–0.70, Good(≥0.70)→0.70–1.0
    if ratio >= DOCTOR_DOM_RATIO_GOOD {
        0.70 + 0.30 * (ratio - DOCTOR_DOM_RATIO_GOOD) / (1.0 - DOCTOR_DOM_RATIO_GOOD)
    } else if ratio >= DOCTOR_DOM_RATIO_POOR {
        0.30 + 0.40 * (ratio - DOCTOR_DOM_RATIO_POOR) / (DOCTOR_DOM_RATIO_GOOD - DOCTOR_DOM_RATIO_POOR)
    } else {
        0.30 * (ratio / DOCTOR_DOM_RATIO_POOR)
    }
}

fn score_fragmentation(avg: f64) -> f64 {
    if (DOCTOR_FRAG_THRESHOLD..=DOCTOR_OVER_MERGE_THRESHOLD).contains(&avg) {
        1.0
    } else if avg < DOCTOR_FRAG_THRESHOLD {
        (avg / DOCTOR_FRAG_THRESHOLD).max(0.0)
    } else {
        (1.0 - (avg - DOCTOR_OVER_MERGE_THRESHOLD) / DOCTOR_OVER_MERGE_THRESHOLD).max(0.0)
    }
}

fn score_weak_evidence(rate: f64) -> f64 {
    ((DOCTOR_WEAK_RATE_POOR - rate) / DOCTOR_WEAK_RATE_POOR).max(0.0)
}

fn compute_doctor_score(run: &event_store::FeatureRun, weak_rate: f64) -> f64 {
    0.35 * score_anchor_coverage(run.anchor_coverage)
        + 0.30 * score_dominant_ratio(run.dominant_anchor_ratio)
        + 0.20 * score_fragmentation(run.avg_events_per_feature)
        + 0.15 * score_weak_evidence(weak_rate)
}

fn classify_health_band(score: f64) -> &'static str {
    if score >= 0.75 {
        "Clear"
    } else if score >= 0.50 {
        "Forming"
    } else {
        "Thin"
    }
}

fn build_doctor_recommendations(
    run: &event_store::FeatureRun,
    weak_rate: f64,
    repo_kind: &RepoKind,
) -> Vec<DoctorRecommendation> {
    let mut recs: Vec<DoctorRecommendation> = Vec::new();

    // Choose wording based on who's reading
    let (commit_anchor_msg, pr_ref_msg, branch_msg, ticket_msg, consistency_msg) =
        match repo_kind {
            RepoKind::Personal | RepoKind::Hackathon => (
                "Add a short note in your commit messages about what you were working on (e.g. \"auth - adding login flow\").",
                "If you open a PR, mention what it covers in the title.",
                "Name your branches after the thing you're building (e.g. \"add-login\", \"fix-crash\").",
                "A quick feature tag in your commits (e.g. \"login:\", \"ui:\") helps Evolith follow your story.",
                "Try to keep one thing per commit — it makes your history much easier to read.",
            ),
            RepoKind::AiMl => (
                "Name your experiments in commit messages (e.g. \"exp/bert-v2: add dropout layer\").",
                "Reference your PR or experiment branch when merging results.",
                "Use branch names that reflect the experiment (e.g. \"exp/gpt-finetune\").",
                "Tag your commits with an experiment ID so Evolith can group related runs.",
                "Separate data prep, training, and evaluation into distinct commits.",
            ),
            _ => (
                "Include ticket IDs in commit messages (e.g. AUTH-123, PROJ-42).",
                "Reference PR numbers in merge and commit messages (e.g. #123).",
                "Use descriptive branch names tied to features or tickets.",
                "Include ticket references in commit messages consistently.",
                "Avoid mixing unrelated changes on the same branch or commit.",
            ),
        };

    // Rule A: Low identity coverage
    if run.anchor_coverage < DOCTOR_ANCHOR_COV_LOW {
        recs.push(DoctorRecommendation {
            id: "low_anchor_coverage_commit_anchors".to_string(),
            priority: "high".to_string(),
            message: commit_anchor_msg.to_string(),
            expected_impact: "Identity Coverage +15–25%".to_string(),
        });
        recs.push(DoctorRecommendation {
            id: "low_anchor_coverage_pr_refs".to_string(),
            priority: "high".to_string(),
            message: pr_ref_msg.to_string(),
            expected_impact: "Identity Coverage +15–25%".to_string(),
        });
    } else if run.anchor_coverage < DOCTOR_ANCHOR_COV_WARN {
        recs.push(DoctorRecommendation {
            id: "moderate_anchor_coverage_branch_naming".to_string(),
            priority: "medium".to_string(),
            message: branch_msg.to_string(),
            expected_impact: "Identity Coverage +8–15%".to_string(),
        });
        recs.push(DoctorRecommendation {
            id: "moderate_anchor_coverage_ticket_refs".to_string(),
            priority: "medium".to_string(),
            message: ticket_msg.to_string(),
            expected_impact: "Identity Coverage +8–15%".to_string(),
        });
    }

    // Rule B: Low feature consistency
    if run.dominant_anchor_ratio < DOCTOR_DOM_RATIO_POOR {
        recs.push(DoctorRecommendation {
            id: "low_identity_consistency".to_string(),
            priority: "high".to_string(),
            message: consistency_msg.to_string(),
            expected_impact: "Feature Consistency +0.10–0.25".to_string(),
        });
    } else if run.dominant_anchor_ratio < DOCTOR_DOM_RATIO_WARN {
        recs.push(DoctorRecommendation {
            id: "moderate_identity_consistency".to_string(),
            priority: "medium".to_string(),
            message: "Use stable references across commits in the same feature.".to_string(),
            expected_impact: "Feature Consistency +0.05–0.15".to_string(),
        });
    }

    // Rule C: Many thin features
    if weak_rate > DOCTOR_WEAK_RATE_WARN {
        recs.push(DoctorRecommendation {
            id: "high_weak_evidence".to_string(),
            priority: "medium".to_string(),
            message: "Group related file changes into one commit rather than many tiny ones.".to_string(),
            expected_impact: "Fewer features that need more context".to_string(),
        });
    }

    // Rule D: Fragmentation
    if run.avg_events_per_feature < DOCTOR_FRAG_THRESHOLD {
        recs.push(DoctorRecommendation {
            id: "fragmentation".to_string(),
            priority: "medium".to_string(),
            message: "Group related changes into a single commit and mention what you were working on.".to_string(),
            expected_impact: "Larger, more readable features".to_string(),
        });
    }

    // Rule E: Over-merge (skip for personal/hackathon — irrelevant at that scale)
    if run.avg_events_per_feature > DOCTOR_OVER_MERGE_THRESHOLD {
        recs.push(DoctorRecommendation {
            id: "over_merge_risk".to_string(),
            priority: "low".to_string(),
            message: "Split broad branches by feature intent; use a distinct reference per feature.".to_string(),
            expected_impact: "Clearer feature boundaries".to_string(),
        });
    }

    // Filter out recommendations that don't apply to this project type
    recs.retain(|r| repo_kind.filter_recommendation(&r.id));
    recs
}

fn build_doctor_issues(run: &event_store::FeatureRun, weak_count: i64) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    let uncovered_pct = (1.0 - run.anchor_coverage) * 100.0;
    if uncovered_pct > 50.0 {
        issues.push(format!("{:.0}% of your commits have no ticket, PR, or branch reference.", uncovered_pct));
    }
    if run.unanchored_feature_count > 0 {
        issues.push(format!(
            "{} of {} features have unknown or ambiguous history.",
            run.unanchored_feature_count, run.feature_count
        ));
    }
    if weak_count > 0 {
        issues.push(format!(
            "{} feature{} have very few commits — too thin to reconstruct reliably.",
            weak_count,
            if weak_count == 1 { " has" } else { "s" }
        ));
    }
    if run.avg_events_per_feature < DOCTOR_FRAG_THRESHOLD {
        issues.push("Most features have fewer than 2 commits — your history is fragmented.".to_string());
    }
    if run.avg_events_per_feature > DOCTOR_OVER_MERGE_THRESHOLD {
        issues.push("Some features contain too many unrelated commits — they may need to be split.".to_string());
    }
    issues
}

fn build_doctor_report(
    run: &event_store::FeatureRun,
    features: &[Feature],
    security: SecurityReport,
    repo_kind: RepoKind,
) -> DoctorReport {
    let weak_count = count_weak_features(features);
    let weak_rate = if run.feature_count > 0 {
        weak_count as f64 / run.feature_count as f64
    } else {
        0.0
    };
    let score = compute_doctor_score(run, weak_rate);
    let band = classify_health_band(score);
    let recommendations = build_doctor_recommendations(run, weak_rate, &repo_kind);
    let issues = build_doctor_issues(run, weak_count);
    let repo_kind_label = repo_kind.label().to_string();

    DoctorReport {
        run_id: run.id.to_string(),
        ruleset_version: DOCTOR_RULESET_VERSION.to_string(),
        status: run.status.clone(),
        repository_health: band.to_string(),
        repo_kind: repo_kind_label,
        security,
        metrics: DoctorMetrics {
            anchor_coverage: run.anchor_coverage,
            dominant_anchor_ratio: run.dominant_anchor_ratio,
            feature_count: run.feature_count,
            anchored_feature_count: run.anchored_feature_count,
            unanchored_feature_count: run.unanchored_feature_count,
            avg_events_per_feature: run.avg_events_per_feature,
            weak_evidence_feature_count: weak_count,
            weak_evidence_rate: weak_rate,
        },
        issues,
        recommendations,
        scoring: DoctorScoring {
            overall_score: (score * 1000.0).round() / 1000.0,
            band: band.to_string(),
        },
    }
}

fn render_doctor_text(report: &DoctorReport, repo_kind: &RepoKind) -> String {
    let mut out = String::new();
    let sep = "─".repeat(48);
    let security_leads = matches!(report.security.risk_level.as_str(), "Critical" | "High");
    let identified = report.metrics.anchored_feature_count;
    let unknown    = report.metrics.unanchored_feature_count;
    let total      = report.metrics.feature_count;

    // ── Security Alert — lead when credentials actually found ─────────────
    if security_leads {
        out.push_str("Security Alert\n");
        out.push_str(&format!("{sep}\n\n"));
        for f in &report.security.findings {
            out.push_str(&format!("  {}  {}\n", severity_label(&f.severity), f.message));
        }
        out.push('\n');
        let status = match report.security.risk_level.as_str() {
            "Critical" => "Secrets Exposed — act before pushing to a shared repository.",
            _ => "Credentials detected — review and remove before sharing.",
        };
        out.push_str(&format!("  {status}\n\n"));
        out.push_str(&format!("{sep}\n\n"));
    }

    // ── Project Story header ──────────────────────────────────────────────
    out.push_str(&format!("Project Story  ·  {}\n", repo_kind.label()));
    out.push_str(&format!("{sep}\n\n"));
    out.push_str(&format!("  {}\n\n", repo_kind.opening_line(identified, total)));

    if identified > 0 {
        out.push_str(&format!(
            "  ✓  {} feature{} readable\n",
            identified,
            if identified == 1 { " is" } else { "s are" }
        ));
    }
    if unknown > 0 {
        out.push_str(&format!(
            "  ⚠  {} feature{} need more context\n",
            unknown,
            if unknown == 1 { " needs" } else { "s need" }
        ));
    } else {
        out.push_str("  ✓  All features are readable\n");
    }
    out.push('\n');
    out.push_str(&format!("  Project Story:  {}\n\n", report.repository_health));
    out.push_str(&format!("{sep}\n\n"));

    // ── Context note (repo-kind-specific gentle framing) ─────────────────
    if let Some(note) = repo_kind.context_note() {
        out.push_str(&format!("  {note}\n\n"));
        out.push_str(&format!("{sep}\n\n"));
    }

    // ── What Evolith Sees (only when there are issues) ───────────────────
    if !report.issues.is_empty() {
        out.push_str("What Evolith Sees\n\n");
        for issue in &report.issues {
            out.push_str(&format!("  ⚠  {}\n", issue));
        }
        out.push('\n');
        out.push_str(&format!("{sep}\n\n"));
    }

    // ── Security Notes — always shown when no alert ───────────────────────
    if !security_leads {
        out.push_str("Security Notes\n\n");
        if report.security.findings.is_empty() {
            out.push_str("  ✓  No credentials or secrets detected.\n");
        } else {
            for f in &report.security.findings {
                out.push_str(&format!("  {}  {}\n", severity_label(&f.severity), f.message));
            }
        }
        out.push('\n');
        out.push_str(&format!("{sep}\n\n"));
    }

    // ── What Would Help / What To Do ─────────────────────────────────────
    let all_recs_count = report.security.recommendations.len() + report.recommendations.len();
    if all_recs_count > 0 {
        out.push_str(&format!("{}\n\n", repo_kind.action_header()));
        let mut idx = 1usize;
        for rec in &report.security.recommendations {
            out.push_str(&format!("  {idx}. {}\n", rec));
            idx += 1;
        }
        for rec in &report.recommendations {
            out.push_str(&format!("  {idx}. {}\n", rec.message));
            idx += 1;
        }
        out.push('\n');

        let has_anchor_rule    = report.recommendations.iter().any(|r| r.id.contains("anchor_coverage"));
        let has_frag_rule      = report.recommendations.iter().any(|r| r.id == "fragmentation");
        let has_identity_rule  = report.recommendations.iter().any(|r| r.id.contains("identity_consistency"));
        let has_security_recs  = !report.security.recommendations.is_empty();

        if has_security_recs || has_anchor_rule || has_identity_rule || has_frag_rule {
            out.push_str("Expected Result\n\n");
            let msg = if has_security_recs && has_anchor_rule {
                "  Reduced credential risk and more of your project's story becomes readable."
            } else if has_security_recs {
                "  Reduced risk of credential exposure and a safer repository."
            } else if has_anchor_rule && has_identity_rule {
                "  Evolith will be able to read more of your history, and features will be easier to trace."
            } else if has_anchor_rule {
                "  More features become readable and your project story becomes more complete."
            } else if has_frag_rule {
                "  Larger, more readable features with clearer boundaries."
            } else {
                "  More consistent features and a cleaner project story."
            };
            out.push_str(&format!("{msg}\n\n"));
        }
        out.push_str(&format!("{sep}\n\n"));
    } else {
        out.push_str("Evolith can already read this project well. No changes needed.\n\n");
        out.push_str(&format!("{sep}\n\n"));
    }

    // ── Details ──────────────────────────────────────────────────────────
    out.push_str("Details\n");
    out.push_str(&format!("{sep}\n"));
    out.push_str(&format!(
        "  Identity Coverage:      {:.1}%\n",
        report.metrics.anchor_coverage * 100.0
    ));
    out.push_str(&format!(
        "  Feature Consistency:    {:.2}\n",
        report.metrics.dominant_anchor_ratio
    ));
    out.push_str(&format!(
        "  Readable Features:      {}\n",
        report.metrics.anchored_feature_count
    ));
    out.push_str(&format!(
        "  Need More Context:      {}\n",
        report.metrics.unanchored_feature_count
    ));
    out.push_str(&format!(
        "  Avg Commits / Feature:  {:.2}\n",
        report.metrics.avg_events_per_feature
    ));

    let has_anchor_rule   = report.recommendations.iter().any(|r| r.id.contains("anchor_coverage"));
    let has_identity_rule = report.recommendations.iter().any(|r| r.id.contains("identity_consistency"));
    if has_anchor_rule {
        let range = if report.metrics.anchor_coverage < DOCTOR_ANCHOR_COV_LOW { "15–25%" } else { "8–15%" };
        out.push_str(&format!("  Expected Coverage Gain:         {range}\n"));
    }
    if has_identity_rule {
        let range = if report.metrics.dominant_anchor_ratio < DOCTOR_DOM_RATIO_POOR { "0.10–0.25" } else { "0.05–0.15" };
        out.push_str(&format!("  Expected Consistency Gain:      {range}\n"));
    }

    out
}

// ─── RFC-007: Security & Safety ────────────────────────────────────────────

fn list_tracked_files() -> Vec<String> {
    let out = Command::new("git").args(["ls-files"]).output();
    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect()
        }
        _ => Vec::new(),
    }
}

fn should_scan_content(path: &str) -> bool {
    let lower = path.to_lowercase();
    // Skip vendor/build dirs
    if lower.contains("node_modules/") || lower.contains("/.git/")
        || lower.contains("/dist/") || lower.contains("/build/")
        || lower.contains("/target/") || lower.contains("/venv/")
        || lower.contains("/__pycache__/") || lower.contains("/.cargo/")
    {
        return false;
    }
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    // No extension (e.g. .env, Makefile, Dockerfile) or known text extension
    ext.is_empty()
        || matches!(
            ext.as_str(),
            "env" | "js" | "ts" | "jsx" | "tsx" | "py" | "rb" | "go" | "rs" | "sh"
                | "bash" | "zsh" | "json" | "yaml" | "yml" | "toml" | "ini" | "conf"
                | "config" | "xml" | "php" | "java" | "cs" | "cpp" | "c" | "h" | "tf"
                | "tfvars" | "properties" | "cfg" | "txt" | "md" | "lock"
        )
}

fn scan_tracked_file_names(files: &[String]) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    for file in files {
        let name = std::path::Path::new(file)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file);
        let name_lc = name.to_lowercase();

        // .env files
        // .env.example / .env.sample / .env.template are intentional templates — downgrade to medium.
        // All other .env.* variants (.env.production, .env.local, .env.staging, etc.) are HIGH.
        if name_lc == ".env" || name_lc.starts_with(".env.") {
            let is_template = matches!(
                name_lc.as_str(),
                ".env.example" | ".env.sample" | ".env.template" | ".env.schema"
            );
            let (severity, message) = if is_template {
                (
                    "medium",
                    format!(
                        "{} is tracked by git — verify it contains only placeholder values, not real credentials.",
                        file
                    ),
                )
            } else {
                (
                    "high",
                    format!("{} is tracked by git — secrets may be exposed.", file),
                )
            };
            findings.push(SecurityFinding {
                severity: severity.to_string(),
                kind: "tracked_env_file".to_string(),
                message,
                location: Some(file.clone()),
            });
        }

        // Private key files by name
        // Skip files inside vendor/dependency directories — they're not project secrets.
        let path_lc = file.replace('\\', "/").to_lowercase();
        let in_vendor = path_lc.contains("/.venv/")
            || path_lc.contains("/site-packages/")
            || path_lc.contains("/node_modules/")
            || path_lc.contains("/.tox/")
            || path_lc.contains("/vendor/")
            || path_lc.contains("/target/");
        // Known CA bundle filenames are not private keys.
        let is_ca_bundle = matches!(name_lc.as_str(), "cacert.pem" | "ca-bundle.crt" | "ca-bundle.pem" | "ca-certificates.crt");
        let is_key_file = !in_vendor && !is_ca_bundle && (
            matches!(
                name_lc.as_str(),
                "id_rsa" | "id_ed25519" | "id_ecdsa" | "id_dsa" | "id_ecdsa_sk" | "id_ed25519_sk"
            ) || (name_lc.ends_with(".pem") && !name_lc.contains("public"))
                || (name_lc.ends_with(".key") && !name_lc.contains("public"))
                || name_lc.ends_with(".p12")
                || name_lc.ends_with(".pfx")
        );
        if is_key_file {
            findings.push(SecurityFinding {
                severity: "critical".to_string(),
                kind: "tracked_key_file".to_string(),
                message: format!("Private key file tracked by git: {}", file),
                location: Some(file.clone()),
            });
        }
    }
    findings
}

pub(crate) fn check_content_patterns(content: &str, file: &str) -> Vec<SecurityFinding> {
    let patterns: &[(&str, &str, &str)] = &[
        ("private_key_content", "critical", r"-----BEGIN (?:RSA |OPENSSH |EC |DSA |PGP )?PRIVATE KEY"),
        ("openai_key",          "high",     r"sk-[A-Za-z0-9_-]{20,}"),
        ("github_token",        "high",     r"gh[pors]_[A-Za-z0-9]{36,}"),
        ("aws_key",             "high",     r"AKIA[0-9A-Z]{16}"),
        ("google_key",          "high",     r"AIza[0-9A-Za-z\-_]{35}"),
        ("slack_token",         "high",     r"xox[baprs]-[0-9A-Za-z\-_]{10,}"),
        ("db_credentials",      "high",     r"(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis)://[^:@\s/]{1,64}:[^@\s/]{1,64}@"),
    ];
    let mut findings = Vec::new();
    for (kind, severity, pattern) in patterns {
        let Ok(re) = Regex::new(pattern) else { continue };
        if re.is_match(content) {
            let msg = match *kind {
                "private_key_content" => format!("Private key content detected in {}", file),
                "openai_key"          => format!("OpenAI API key pattern detected in {}", file),
                "github_token"        => format!("GitHub token pattern detected in {}", file),
                "aws_key"             => format!("AWS access key pattern detected in {}", file),
                "google_key"          => format!("Google API key pattern detected in {}", file),
                "slack_token"         => format!("Slack token pattern detected in {}", file),
                "db_credentials"      => format!("Database URL with credentials detected in {}", file),
                _                     => format!("Secret pattern detected in {}", file),
            };
            findings.push(SecurityFinding {
                severity: severity.to_string(),
                kind: kind.to_string(),
                message: msg,
                location: Some(file.to_string()),
            });
        }
    }
    findings
}

fn scan_tracked_file_contents(files: &[String]) -> (Vec<SecurityFinding>, usize) {
    const MAX_FILE_BYTES: u64 = 100 * 1024;
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    for file in files {
        if !should_scan_content(file) {
            continue;
        }
        let path = std::path::Path::new(file);
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else { continue };
        scanned += 1;
        findings.extend(check_content_patterns(&content, file));
    }
    (findings, scanned)
}

fn check_gitignore_completeness(tracked: &[String]) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    let gitignore_path = std::path::Path::new(".gitignore");

    if !gitignore_path.exists() {
        findings.push(SecurityFinding {
            severity: "medium".to_string(),
            kind: "no_gitignore".to_string(),
            message: "No .gitignore found — sensitive files may be accidentally committed.".to_string(),
            location: None,
        });
        return findings;
    }

    let gitignore = std::fs::read_to_string(gitignore_path).unwrap_or_default();
    let covers_env = gitignore.lines().any(|l| {
        let l = l.trim();
        l == ".env" || l == ".env*" || l == "*.env" || l.starts_with(".env.")
    });
    if !covers_env {
        findings.push(SecurityFinding {
            severity: "medium".to_string(),
            kind: "missing_env_in_gitignore".to_string(),
            message: ".env files are not covered by .gitignore — future secrets could be committed.".to_string(),
            location: Some(".gitignore".to_string()),
        });
    }

    // Detect commonly-excluded dirs being tracked
    let common_dirs = ["node_modules/", "venv/", "dist/", "__pycache__/", ".venv/"];
    for dir in common_dirs {
        if tracked.iter().any(|f| f.starts_with(dir) || f.contains(&format!("/{}", dir))) {
            findings.push(SecurityFinding {
                severity: "medium".to_string(),
                kind: "common_files_tracked".to_string(),
                message: format!("{} is tracked — consider adding it to .gitignore.", dir.trim_end_matches('/')),
                location: None,
            });
        }
    }

    findings
}

fn compute_risk_level(findings: &[SecurityFinding]) -> &'static str {
    if findings.iter().any(|f| f.severity == "critical") {
        return "Critical";
    }
    if findings.iter().any(|f| f.severity == "high") {
        return "High";
    }
    if findings.iter().any(|f| f.severity == "medium") {
        return "Medium";
    }
    "None"
}

fn build_security_recommendations(findings: &[SecurityFinding]) -> Vec<String> {
    let mut recs: Vec<String> = Vec::new();
    let has_high_or_critical = findings.iter().any(|f| f.severity == "critical" || f.severity == "high");

    for f in findings {
        let rec = match f.kind.as_str() {
            "tracked_env_file" => {
                let loc = f.location.as_deref().unwrap_or(".env");
                if f.severity == "medium" {
                    // Template files (.env.example etc.) are intentionally committed — verify, don't remove.
                    format!("Verify that {} contains only placeholder values, not real credentials.", loc)
                } else {
                    format!("Remove {} from git tracking: `git rm --cached {}` then add it to .gitignore.", loc, loc)
                }
            }
            "tracked_key_file" => {
                let loc = f.location.as_deref().unwrap_or("the key file");
                format!("Revoke and rotate the private key at {} immediately — it may already be exposed.", loc)
            }
            "private_key_content" => {
                let loc = f.location.as_deref().unwrap_or("the file");
                format!("Rotate the private key material found in {} and remove it from git history.", loc)
            }
            "openai_key" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Rotate the OpenAI API key detected in {}.", loc)
            }
            "github_token" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Revoke and regenerate the GitHub token found in {}.", loc)
            }
            "aws_key" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Rotate the AWS access key in {} immediately via the AWS Console.", loc)
            }
            "google_key" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Rotate the Google API key found in {}.", loc)
            }
            "slack_token" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Revoke the Slack token found in {}.", loc)
            }
            "db_credentials" => {
                let loc = f.location.as_deref().unwrap_or("a tracked file");
                format!("Move the database credentials in {} to environment variables.", loc)
            }
            "no_gitignore" => "Create a .gitignore and add .env* and other sensitive patterns.".to_string(),
            "missing_env_in_gitignore" => "Add .env* to .gitignore to prevent future credential exposure.".to_string(),
            "common_files_tracked" => format!(
                "Remove {} from git tracking.",
                f.message.split_whitespace().next().unwrap_or("the directory")
            ),
            _ => continue,
        };
        if !recs.contains(&rec) {
            recs.push(rec);
        }
    }

    if has_high_or_critical {
        recs.push("Use `git filter-repo` or BFG Repo Cleaner to purge secrets from git history.".to_string());
    }
    recs
}

fn scan_security() -> SecurityReport {
    let tracked = list_tracked_files();
    let mut findings: Vec<SecurityFinding> = Vec::new();
    findings.extend(scan_tracked_file_names(&tracked));
    let (content_findings, scanned_count) = scan_tracked_file_contents(&tracked);
    findings.extend(content_findings);
    findings.extend(check_gitignore_completeness(&tracked));
    let risk_level = compute_risk_level(&findings).to_string();
    let recommendations = build_security_recommendations(&findings);
    SecurityReport { risk_level, findings, recommendations, scanned_file_count: scanned_count }
}

fn severity_label(severity: &str) -> &'static str {
    match severity {
        "critical" => "🚨 Secrets Exposed",
        "high"     => "⚠  Needs Attention",
        "medium"   => "ℹ  Worth a Look   ",
        _          => "   Note           ",
    }
}

fn render_security_text(report: &SecurityReport) -> String {
    let mut out = String::new();
    let sep = "─".repeat(48);

    out.push_str("Security Notes\n");
    out.push_str(&format!("{sep}\n\n"));

    if report.findings.is_empty() {
        out.push_str("  ✓  No credentials or secrets detected.\n\n");
    } else {
        out.push_str("Findings\n\n");
        for f in &report.findings {
            out.push_str(&format!("  {}  {}\n", severity_label(&f.severity), f.message));
        }
        out.push('\n');
    }

    let overall = match report.risk_level.as_str() {
        "None"     => "Clean",
        "Critical" => "Secrets Exposed",
        "High"     => "Needs Attention",
        "Medium"   => "Worth a Look",
        other      => other,
    };
    out.push_str(&format!("Overall: {overall}\n\n"));
    out.push_str(&format!("{sep}\n\n"));

    // What We Checked checklist
    let count_kind = |kind: &str| report.findings.iter().filter(|f| f.kind == kind).count();
    let has_kind = |kind: &str| count_kind(kind) > 0;
    let check = |found: bool| if found { "⚠ " } else { "✓ " };

    out.push_str("What We Checked\n\n");
    let env_count = count_kind("tracked_env_file");
    out.push_str(&format!(
        "  {} .env files tracked in git  : {}\n",
        check(env_count > 0),
        if env_count == 0 { "none".to_string() } else { env_count.to_string() }
    ));
    let key_count = count_kind("tracked_key_file") + count_kind("private_key_content");
    out.push_str(&format!(
        "  {} Private keys in history    : {}\n",
        check(key_count > 0),
        if key_count == 0 { "none".to_string() } else { key_count.to_string() }
    ));
    let openai_count = count_kind("openai_key");
    out.push_str(&format!(
        "  {} OpenAI keys                : {}\n",
        check(has_kind("openai_key")),
        if openai_count == 0 { "none".to_string() } else { openai_count.to_string() }
    ));
    let github_count = count_kind("github_token");
    out.push_str(&format!(
        "  {} GitHub tokens              : {}\n",
        check(has_kind("github_token")),
        if github_count == 0 { "none".to_string() } else { github_count.to_string() }
    ));
    let aws_count = count_kind("aws_key");
    out.push_str(&format!(
        "  {} AWS keys                   : {}\n",
        check(has_kind("aws_key")),
        if aws_count == 0 { "none".to_string() } else { aws_count.to_string() }
    ));
    let google_count = count_kind("google_key");
    out.push_str(&format!(
        "  {} Google API keys            : {}\n",
        check(has_kind("google_key")),
        if google_count == 0 { "none".to_string() } else { google_count.to_string() }
    ));
    let slack_count = count_kind("slack_token");
    out.push_str(&format!(
        "  {} Slack tokens               : {}\n",
        check(has_kind("slack_token")),
        if slack_count == 0 { "none".to_string() } else { slack_count.to_string() }
    ));
    let db_count = count_kind("db_credentials");
    out.push_str(&format!(
        "  {} Database credentials       : {}\n",
        check(has_kind("db_credentials")),
        if db_count == 0 { "none".to_string() } else { db_count.to_string() }
    ));
    let gitignore_ok = !has_kind("no_gitignore") && !has_kind("missing_env_in_gitignore");
    out.push_str(&format!(
        "  {} .gitignore covers .env     : {}\n",
        check(!gitignore_ok),
        if gitignore_ok { "yes" } else { "no" }
    ));
    out.push('\n');

    if !report.recommendations.is_empty() {
        out.push_str(&format!("{sep}\n\n"));
        out.push_str("What To Do\n\n");
        for (i, rec) in report.recommendations.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", i + 1, rec));
        }
        out.push('\n');
    }

    out.push_str(&format!("Scanned {} files.\n", report.scanned_file_count));
    out
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
            project_id: "evolith".to_string(),
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
        db_path.push(format!("evolith-cli-flow-{}.db", Uuid::new_v4()));
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
        db_path.push(format!("evolith-collect-idempotent-{}.db", Uuid::new_v4()));
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
        db_path.push(format!("evolith-repeat-collect-{}.db", Uuid::new_v4()));
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
        db_path.push(format!("evolith-run-metrics-{}.db", Uuid::new_v4()));
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
        store.complete_feature_run(run_id, FeatureRunMetrics {
            feature_count,
            anchor_coverage,
            anchored_feature_count,
            unanchored_feature_count,
            avg_events_per_feature,
            dominant_anchor_ratio,
        })?;

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
        assert!(explanation.contains("Pattern interview"), "explanation must include feature title");
        assert!(explanation.contains("What happened?"), "explanation must include What happened section");
        assert!(explanation.contains("Why it matters"), "explanation must include Why it matters section");
        assert!(explanation.contains("What to do"), "explanation must include What to do section");
        assert!(explanation.contains("Identity Clarity:"), "explanation must include Identity Clarity");
        assert!(explanation.contains("PR-12") || explanation.contains("feature/interview"), "explanation must reference linked identities");
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
        assert_eq!(health, "Needs More Context");
    }

    // --- evolith doctor tests ---

    fn make_run(
        anchor_coverage: f64,
        dominant_anchor_ratio: f64,
        feature_count: i64,
        anchored: i64,
        unanchored: i64,
        avg_events: f64,
    ) -> event_store::FeatureRun {
        event_store::FeatureRun {
            id: Uuid::new_v4(),
            status: "completed".to_string(),
            algorithm_version: FEATURE_ALGO_VERSION.to_string(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            event_count: (feature_count as f64 * avg_events) as i64,
            feature_count,
            anchor_coverage,
            anchored_feature_count: anchored,
            unanchored_feature_count: unanchored,
            avg_events_per_feature: avg_events,
            dominant_anchor_ratio,
        }
    }

    fn make_feature_with_commits(commit_count: i64) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            project_id: "proj-1".to_string(),
            title: "Pattern auth".to_string(),
            status: "active".to_string(),
            evidence_json: serde_json::json!({
                "commit_count": commit_count,
                "episode_count": 1,
                "ticket_ids": [],
                "pr_ids": [],
                "branches": [],
                "dominant_anchor_ratio": 0.8
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn doctor_good_health_for_bravely_like_run() {
        // Bravely M2.2-like: high coverage, strong identity, healthy avg
        let run = make_run(0.75, 0.85, 9, 8, 1, 8.5);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(8)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        assert_eq!(report.repository_health, "Clear");
        assert!(report.scoring.overall_score >= 0.75);
        assert!(report.recommendations.is_empty());
    }

    #[test]
    fn doctor_moderate_for_trx_ai_like_run() {
        // TRX-AI-like: low anchor coverage, moderate identity
        let run = make_run(0.228, 0.56, 9, 5, 4, 6.33);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(4)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        assert!(
            report.repository_health == "Forming" || report.repository_health == "Thin",
            "expected Forming or Thin, got {}",
            report.repository_health
        );
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn doctor_fragmentation_rule_triggers_when_avg_below_threshold() {
        let run = make_run(0.50, 0.70, 10, 8, 2, 1.5); // avg_events < 2.0
        let features: Vec<Feature> = (0..10).map(|_| make_feature_with_commits(1)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let has_frag = report.recommendations.iter().any(|r| r.id == "fragmentation");
        assert!(has_frag, "fragmentation rule should trigger when avg_events_per_feature < 2.0");
    }

    #[test]
    fn doctor_no_fragmentation_when_avg_is_healthy() {
        let run = make_run(0.60, 0.75, 8, 7, 1, 10.0);
        let features: Vec<Feature> = (0..8).map(|_| make_feature_with_commits(8)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let has_frag = report.recommendations.iter().any(|r| r.id == "fragmentation");
        assert!(!has_frag, "fragmentation rule should not trigger for healthy avg_events_per_feature");
    }

    #[test]
    fn doctor_json_report_has_required_fields() {
        let run = make_run(0.228, 0.56, 9, 5, 4, 6.33);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(4)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("run_id").is_some());
        assert!(json.get("repository_health").is_some());
        assert!(json.get("metrics").is_some());
        assert!(json.get("recommendations").is_some());
        assert!(json.get("issues").is_some());
        assert!(json.get("scoring").is_some());
        let metrics = json.get("metrics").unwrap();
        assert!(metrics.get("anchor_coverage").is_some());
        assert!(metrics.get("dominant_anchor_ratio").is_some());
        assert!(metrics.get("weak_evidence_rate").is_some());
    }

    #[test]
    fn doctor_text_output_leads_with_repository_summary() {
        let run = make_run(0.228, 0.56, 9, 5, 4, 6.33);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(4)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let text = render_doctor_text(&report, &RepoKind::Unknown);
        assert!(text.starts_with("Project Story"), "text output must start with Project Story");
        let health_pos = text.find("Project Story:").unwrap_or(usize::MAX);
        let summary_pos = text.find("Project Story  ·").unwrap();
        assert!(summary_pos < health_pos, "header must appear before health score line");
        assert!(!text.contains("anchor_coverage"), "raw metric keys must not appear in text output");
        assert!(!text.contains("dominant_anchor_ratio"), "raw metric keys must not appear in text output");
    }

    #[test]
    fn doctor_text_output_contains_details_section() {
        let run = make_run(0.50, 0.70, 9, 7, 2, 6.0);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(6)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let text = render_doctor_text(&report, &RepoKind::Unknown);
        assert!(text.contains("Details\n"), "text output must contain Details section");
        assert!(text.contains("Identity Coverage:"), "Details must include Identity Coverage");
        assert!(text.contains("Feature Consistency:"), "Details must include Feature Consistency");
    }

    #[test]
    fn doctor_over_merge_rule_triggers() {
        let run = make_run(0.80, 0.85, 3, 3, 0, 30.0); // avg_events > 25.0
        let features: Vec<Feature> = (0..3).map(|_| make_feature_with_commits(30)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let has_over_merge = report.recommendations.iter().any(|r| r.id == "over_merge_risk");
        assert!(has_over_merge, "over-merge rule should trigger when avg_events_per_feature > 25.0");
    }

    #[test]
    fn doctor_recommendations_are_deterministic() {
        let run = make_run(0.228, 0.56, 9, 5, 4, 6.33);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(4)).collect();
        let report1 = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let report2 = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let ids1: Vec<&str> = report1.recommendations.iter().map(|r| r.id.as_str()).collect();
        let ids2: Vec<&str> = report2.recommendations.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids1, ids2, "recommendations must be deterministic across repeated calls");
    }

    // --- RFC-007: Security & Safety tests ---

    #[test]
    fn security_detects_env_file_by_name() {
        let files = vec![
            ".env".to_string(),
            "src/app.js".to_string(),
            ".env.production".to_string(),
            ".env.local".to_string(),
            ".env.example".to_string(),  // template — should be medium, not high
        ];
        let findings = scan_tracked_file_names(&files);
        let env: Vec<_> = findings.iter().filter(|f| f.kind == "tracked_env_file").collect();
        assert_eq!(env.len(), 4, "should detect all four .env variants");
        let high: Vec<_> = env.iter().filter(|f| f.severity == "high").collect();
        let medium: Vec<_> = env.iter().filter(|f| f.severity == "medium").collect();
        assert_eq!(high.len(), 3, ".env, .env.production, .env.local should be high");
        assert_eq!(medium.len(), 1, ".env.example should be medium");
    }

    #[test]
    fn security_detects_private_key_file_by_name() {
        let files = vec![
            "id_rsa".to_string(),
            "server.pem".to_string(),
            "deploy.key".to_string(),
            "public.key".to_string(),   // has "public" in name — should NOT flag
            "src/index.ts".to_string(), // normal file — should NOT flag
        ];
        let findings = scan_tracked_file_names(&files);
        let keys: Vec<_> = findings.iter().filter(|f| f.kind == "tracked_key_file").collect();
        assert!(keys.iter().any(|f| f.location.as_deref() == Some("id_rsa")));
        assert!(keys.iter().any(|f| f.location.as_deref() == Some("server.pem")));
        assert!(!keys.iter().any(|f| f.location.as_deref() == Some("public.key")), "public key should not be flagged");
        assert!(keys.iter().all(|f| f.severity == "critical"));
    }

    #[test]
    fn security_content_detects_aws_key_pattern() {
        let content = "export AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let findings = check_content_patterns(content, "config.sh");
        let aws: Vec<_> = findings.iter().filter(|f| f.kind == "aws_key").collect();
        assert!(!aws.is_empty(), "AWS key pattern should be detected");
        assert_eq!(aws[0].severity, "high");
    }

    #[test]
    fn security_content_detects_private_key_header() {
        let content = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
        let findings = check_content_patterns(content, "keys/server.pem");
        let pk: Vec<_> = findings.iter().filter(|f| f.kind == "private_key_content").collect();
        assert!(!pk.is_empty(), "private key header should be detected");
        assert_eq!(pk[0].severity, "critical");
    }

    #[test]
    fn security_content_detects_openssh_private_key() {
        let content = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk...\n-----END OPENSSH PRIVATE KEY-----";
        let findings = check_content_patterns(content, "id_ed25519");
        assert!(findings.iter().any(|f| f.kind == "private_key_content"));
    }

    #[test]
    fn security_content_detects_db_url_with_credentials() {
        let content = r#"DATABASE_URL=postgres://admin:s3cr3t@db.example.com:5432/mydb"#;
        let findings = check_content_patterns(content, ".env");
        assert!(findings.iter().any(|f| f.kind == "db_credentials"), "DB URL with credentials should be detected");
    }

    #[test]
    fn security_content_no_false_positive_for_clean_db_url() {
        let content = "DATABASE_URL=postgres://localhost:5432/mydb"; // no password
        let findings = check_content_patterns(content, "config.py");
        assert!(!findings.iter().any(|f| f.kind == "db_credentials"), "DB URL without credentials should not trigger");
    }

    #[test]
    fn security_risk_level_critical_for_private_key() {
        let findings = vec![SecurityFinding {
            severity: "critical".to_string(),
            kind: "tracked_key_file".to_string(),
            message: "Private key".to_string(),
            location: Some("id_rsa".to_string()),
        }];
        assert_eq!(compute_risk_level(&findings), "Critical");
    }

    #[test]
    fn security_risk_level_high_for_env_file() {
        let findings = vec![SecurityFinding {
            severity: "high".to_string(),
            kind: "tracked_env_file".to_string(),
            message: ".env tracked".to_string(),
            location: Some(".env".to_string()),
        }];
        assert_eq!(compute_risk_level(&findings), "High");
    }

    #[test]
    fn security_risk_level_none_when_clean() {
        assert_eq!(compute_risk_level(&[]), "None");
    }

    #[test]
    fn security_recommendations_include_history_cleanup_for_high_findings() {
        let findings = vec![SecurityFinding {
            severity: "high".to_string(),
            kind: "openai_key".to_string(),
            message: "OpenAI key in config.js".to_string(),
            location: Some("config.js".to_string()),
        }];
        let recs = build_security_recommendations(&findings);
        let has_filter_repo = recs.iter().any(|r| r.contains("filter-repo") || r.contains("BFG"));
        assert!(has_filter_repo, "should recommend git history cleanup for high-severity findings");
    }

    #[test]
    fn security_json_report_has_correct_shape() {
        let report = SecurityReport {
            risk_level: "High".to_string(),
            findings: vec![SecurityFinding {
                severity: "high".to_string(),
                kind: "tracked_env_file".to_string(),
                message: ".env tracked".to_string(),
                location: Some(".env".to_string()),
            }],
            recommendations: vec!["Add .env to .gitignore.".to_string()],
            scanned_file_count: 42,
        };
        let json = serde_json::to_value(&report).unwrap();
        assert!(json.get("risk_level").is_some());
        assert!(json.get("findings").is_some());
        assert!(json.get("recommendations").is_some());
        assert!(json.get("scanned_file_count").is_some());
        assert_eq!(json["findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn security_text_report_starts_with_security_report_header() {
        let report = SecurityReport::empty();
        let text = render_security_text(&report);
        assert!(text.starts_with("Security Notes\n"), "security text must start with 'Security Notes'");
        assert!(text.contains("✓  No credentials or secrets detected."));
    }

    #[test]
    fn doctor_text_always_contains_security_section() {
        let run = make_run(0.75, 0.85, 9, 8, 1, 8.5);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(8)).collect();
        let report = build_doctor_report(&run, &features, SecurityReport::empty(), RepoKind::Unknown);
        let text = render_doctor_text(&report, &RepoKind::Unknown);
        assert!(text.contains("Security Notes"), "doctor text must include Security Notes section");
        assert!(text.contains("✓  No credentials or secrets detected."), "clean repo should show no issues");
    }

    #[test]
    fn doctor_text_shows_security_findings_when_present() {
        let run = make_run(0.75, 0.85, 9, 8, 1, 8.5);
        let features: Vec<Feature> = (0..9).map(|_| make_feature_with_commits(8)).collect();
        let sec = SecurityReport {
            risk_level: "High".to_string(),
            findings: vec![SecurityFinding {
                severity: "high".to_string(),
                kind: "tracked_env_file".to_string(),
                message: ".env.production is tracked by git — secrets may be exposed.".to_string(),
                location: Some(".env.production".to_string()),
            }],
            recommendations: vec!["Remove .env.production from git tracking.".to_string()],
            scanned_file_count: 10,
        };
        let report = build_doctor_report(&run, &features, sec, RepoKind::Unknown);
        let text = render_doctor_text(&report, &RepoKind::Unknown);
        assert!(text.contains("Needs Attention") || text.contains("Secrets Exposed"), "should show attention status when findings exist");
        assert!(text.contains(".env.production is tracked by git"), "should show finding message");
        // Security alert leads when High/Critical, health appears after
        let sec_pos = text.find("Security Alert").unwrap();
        let story_pos = text.find("Project Story  ·").unwrap();
        assert!(sec_pos < story_pos, "security alert should appear before project story section when severity is high");
    }
}
