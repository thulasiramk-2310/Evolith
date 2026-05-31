# RFC-002: Session Reconstruction

## Status
Proposed

## Context
Raw events map to every minor developer action, resulting in thousands of unorganized data points. To make Evolith useful, we need a higher-level grouping mechanism called "Sessions". A session represents a contiguous block of developer activity.

## Definition of a Session
For Milestone 0, a Session is defined by time-proximity:
- A new session is started if the gap between the last event and the current event exceeds 30 minutes.
- Events within that 30-minute threshold are grouped into the active session.

In the future, sessions may be bounded by heuristics, context switches, or explicit developer commands.

## Schema Changes

To accommodate persistent sessions, the database requires two new tables:

### 1. sessions Table
Persists the lifecycle metrics of a session.
- id (UUID, Primary Key)
- project_id (Text)
- start_time (Text - RFC3339)
- end_time (Text - RFC3339)
- event_count (Integer)

### 2. session_assignments Table
Maps events to their corresponding session. This acts as a persistent projection of the event stream.
- session_id (UUID, Foreign Key)
- event_id (UUID, Foreign Key)
- project_id (Text)
- 	imestamp (Text - RFC3339, for ordering)

## Operations

1. **Rebuild Sessions**: 
   - Scans all unassigned events.
   - Calculates time deltas.
   - Persists groups into sessions.
   - Populates session_assignments.

2. **Querying**:
   - get_sessions(project_id) - Retrieves all recorded sessions.
   - get_session_events(session_id) - Retrieves the ordered timeline of events within a specific session using an INNER JOIN between session_assignments and events.
