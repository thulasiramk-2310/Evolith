# RFC-001: Event Model

## Status
Accepted

## Context
DevReplay needs a structured format to capture all development activity in a standardized way. These events are immutable records of state changes over time. They form the foundation of our event-sourced architecture (ADR-001).

## Event Schema

Every event MUST adhere to the following schema:

- id: UUID (v4 currently, aiming to migrate to UUIDv7)
- 	imestamp: UTC datetime representing when the event occurred.
- event_type: The classification of the event (e.g., ile.modified, git.commit, 	est.run).
- project_id: Identifier for the project this event belongs to.
- payload: JSON payload containing event-specific data (e.g., file contents, git messages, test results).
- path: String (optional). The relevant file path, if applicable.
- ranch: String (optional). The git branch active when the event occurred.
- metadata: JSON (optional). Additional contextual data like OS, IDE, etc.
- collector_version: String. Version of the agent/collector that recorded the event.
- created_at: UTC datetime representing when the event was ingested by the storage layer.

## Event Types

Currently supported and planned event types include:

### File Events
- ile.created
- ile.modified
- ile.deleted

### Git Events
- git.commit
- git.branch.created
- git.branch.switched
- git.merge

### Execution Events
- 	est.run
- uild.run

## Database Considerations
Events are persisted in SQLite. The database schema must provide efficient querying via indexes on 	imestamp, project_id, and event_type.
