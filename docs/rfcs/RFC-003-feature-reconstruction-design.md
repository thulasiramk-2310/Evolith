# RFC-003: Feature Reconstruction Engine Design

## Status
Proposed

## Version
1.0.0

## Author
[Author Placeholder]

## Date
May 31, 2026

---

## Abstract
Evolith aggregates immutable developer events to project "how software is built." While Sessions (RFC-002) chunk this activity chronologically, they lack semantic context. This document proposes the design for a Feature Reconstruction Engine: an asynchronous, retrospective mechanism for grouping events and sessions into cross-cutting, semantic "Features." Features represent the overarching goal of a body of work. This RFC establishes the models for append-only, versioned feature assignments, gracefully supporting algorithmic heuristics, multiple event-to-feature overlaps, and manual corrections over immutable events.

## Context
Following **ADR-001**, events are the immutable source of truth. Raw events capture isolated steps (e.g., file edits, git commits). Sessions represent chronological proximity ("developer sat down for 3 hours"). However, these views are inadequate for understanding the structural and goal-oriented lifecycle of a codebase.

### Why Feature Reconstruction is Needed
Developers don't think in terms of "session 42"; they think in terms of semantic goals—like "Implement OAuth2 login." To replay and comprehend software building across days or weeks, we must re-cluster raw events semantically into Features.

### Why Sessions are not Enough
A single 4-hour session might involve work on "OAuth2" (Feature A) alongside a quick visual tweak for "Homepage Header" (Feature B), and a sudden fix for "Memory Leak" (Feature C). Conversely, Feature A might span across multiple interleaved sessions spread over weeks.

### Why this is a Retrospective Analysis Problem
Context is rarely pristine at the moment of typing. Realizing that a specific change belonged to a wider feature may only become algebraically clear later—once a branch completes, a commit message is written, or an issue is closed. Determining feature assignment is intrinsically a heuristic projection applied retrospectively, not a concrete recording.

## Goals
- Formally define semantic Features within Evolith.
- Introduce a robust schema to project events onto Features without mutating original event data.
- Manage conflict resolution and overlapping assignments when an event contributes to multiple features.
- Permit versioned algorithmic updates where improved heuristics can supersede older feature groupings.
- Enable instantaneous retrieval of Feature projections for timeline replay.

## Non-Goals
- Real-time prediction of the active feature during event ingestion.
- Modifying the primary event payloads or storing source code contents natively in Evolith to determine context.
- Hardcoding explicit AST or language parsing directly within this design document.
- Implementing the engine logic code in this phase.

## Definition of a Feature
A **Feature** is a semantic projection constructed over immutable events and sessions, representing a unified goal or piece of work.
- It operates strictly as a secondary view projected from the primary event truth.
- Features **MAY** span multiple chronological sessions.
- Features **MAY** overlap loosely, given that foundational work (e.g., refactoring a utility) naturally touches multiple concurrent features.

## Feature Assignment Model
To map the immutable events to their conceptual Features, we introduce the `feature_assignments` layer. 
- **Append-only History:** The assignment table relies on an append-only mapping structure. As the clustering heuristics upgrade, assignments are not updated; new records supersede old ones.
- **Multi-Assignment:** An event can be mapped to multiple Features simultaneously.
  - *One Event -> Many Feature Assignments.* 
  - For example, `auth/utils.rs` can belong to `Authentication Rewrite`, `User Management`, and `SSO Migration` simultaneously.
- **Confidence Ownership:** The feature with the highest algorithm/user confidence score acts as the "primary owner" of the event for timeline visualization, while secondary features retain their assignments for cross-referenced queries.

## Proposed Schema

Features and their granular event bindings will persist in SQLite tables.

### `features` Table
Defines the high-level semantic entities.
- `id`: UUID (Primary Key)
- `project_id`: String (Foreign Key linking to projects)
- `title`: String (e.g., "OAuth2 Implementation")
- `status`: String (Enum-like: `draft`, `active`, `merged`, `archived`)
- `created_at`: UTC Datetime

### `feature_assignments` Table
Maps events to features using an append-only, versioned strategy.
- `id`: UUID (Primary Key)
- `feature_id`: UUID (Foreign Key)
- `event_id`: UUID (Foreign Key)
- `confidence_score`: Real (Used as a ranking signal, not a pure probability. e.g., Feature A = 0.91, Feature B = 0.42 means A is stronger/more likely than B, not that there is a 91% chance A is correct. Manual corrections use 1.0.)
- `source`: String (Enum-like: `heuristic_temporal`, `heuristic_git`, `manual_correction`, etc.)
- `algorithm_version`: String (REQUIRED. The version block of the engine that generated this edge)
- `created_at`: UTC Datetime
- `superseded_by`: UUID, Nullable (Points to the `id` of a newer assignment record replacing this one if the algorithmic mapping changes)

### Nullable vs Required
- Event and Feature IDs MUST be strictly required to build the graph constraints.
- `superseded_by` MUST default to `NULL`. An active, undisputed projection is one where `superseded_by IS NULL`. 

### Why Versioning in Assignments is Needed
As algorithmic heuristics improve over successive Evolith engine releases, the clustering shape of prior events may shift. We cannot destructively modify historical graphs. Versioned `algorithm_version` tags and `superseded_by` pointers maintain analytical integrity.

## Conflict Resolution
When an event probabilistically matches multiple features (e.g., a shared `utils.ts` export modified in a session supporting two parallel branch features):
- **Primary Owner Selection:** Evolith determines the primary feature for a specific event by choosing the active (`superseded_by IS NULL`) assignment record with the highest `confidence_score`.
- **Secondary Preservation:** Lower confidence assignments to the same event are NOT deleted. They remain attached so users can pivot to the "overlapping feature context" from the UI.
- **Developer Ground Truth:** Manual correction supersedes all algorithmic assignments. If an Algorithm v1 assigns an event to Feature A, and a developer moves the event to Feature B, Feature B wins. The system stores the old assignment as superseded. It never argues with the user.

## Scoring Model
The heuristic algorithms calculate `confidence_score` rankings by scaling signals. Weights MUST be dynamically configurable rather than hardcoded. Sample signals include:
1. **Branch Similarity:** If an event occurred entirely inside `feature/oauth`, probability scaling biases aggressively toward the derived OAuth feature.
2. **File Path Locality:** Modification clusters inside `src/auth/*` denote feature proximity.
3. **Temporal Proximity:** Events performed rapidly in adjacent bursts likely share intent.
4. **Git Metadata:** Commit message references, issue ticket linkage.
5. **Manual Overrides:** Developer-supplied corrections (inserted with `source = 'manual_correction'`) immediately push `confidence_score` to absolute (1.0).

## Assignment Lifecycle
1. **Initial Assignment:** The reconstruction engine scans unassigned events and writes fresh `feature_assignments` pointing to newly generated `features`.
2. **Algorithmic Reassignment:** A newer engine pass (`algorithm_version = 'v2'`) concludes an event actually belongs to Feature B instead of Feature A. It inserts the new Feature B assignment edge and sets the Feature A assignment's `superseded_by` pointer to the newly generated `id`.
3. **Manual Correction:** A user reviews the replay and forces the event into Feature C. A top-tier manual assignment is inserted, superseding any prior algorithmic rows.
4. **Version Comparison:** Using the `algorithm_version`, debugging views can show how feature boundaries shifted across engine upgrades.

## Example Scenarios
- **Single focus:** `auth.ts` is iteratively modified. The feature engine detects branch tracking and tight temporal bounds, exclusively mapping it to "Auth Service" with `confidence = 0.9`. 
- **Cross-cutting utility:** `utils.ts` is edited in the middle of an "Auth Service" session but is necessary for a later "Billing UI" feature. The engine inserts associations to both features, relying on temporal spacing to score "Auth Service" as `0.6` and "Billing UI" as `0.45`.
- **Pause and Resume:** Work on a feature halts on Tuesday and resumes on Thursday. Despite distinct Sessions, Git metadata and filepath correlation securely merge both sessions' events into one overarching Feature projection.
- **Hotfix Overlap:** While writing a massive feature, a 5-minute hotfix on a completely separate namespace interrupts the timeline. The temporal bounds might lightly confuse the algorithm, but the abrupt file-graph distance properly sections the hotfix into its own mini-feature, preventing contamination of the main branch projections.

## Query Examples

- **All active events for a Feature:**
  ```sql
  SELECT e.*, a.confidence FROM events e 
  INNER JOIN feature_assignments a ON e.id = a.event_id 
  WHERE a.feature_id = ? AND a.superseded_by IS NULL 
  ORDER BY e.timestamp ASC;
  ```

- **All active assignments for a specific Event:**
  ```sql
  SELECT * FROM feature_assignments 
  WHERE event_id = ? AND superseded_by IS NULL 
  ORDER BY confidence DESC;
  ```

- **Tracing algorithm history for an assignment:**
  ```sql
  -- Assuming we know an old assignment ID
  WITH RECURSIVE supersessions(id, superseded_by, algorithm_version) AS (
    SELECT id, superseded_by, algorithm_version FROM feature_assignments WHERE id = ?
    UNION ALL
    SELECT f.id, f.superseded_by, f.algorithm_version FROM feature_assignments f
    INNER JOIN supersessions s ON s.superseded_by = f.id
  )
  SELECT * FROM supersessions;
  ```

- **Querying the primary feature for a shared file event:**
  (Top 1 row grouped by event, sorted by confidence down)

## Performance Considerations
Feature boundaries span vast networks of events. Querying these dynamically without persistent assignments would cripple replays. Caching the projections in `feature_assignments` enables `O(1)` table scanning for rapid timeline generation. The system handles replay instantly via simple index-backed SQL joins without engaging heavy graph/heuristic logic algorithms at query time.

## Privacy Considerations
Feature projections MUST inherit the strict data models of raw events. **Under no circumstances should the engine parse, request, or store physical source code boundaries / contents into the heuristic assignment outputs.** All logic is applied strictly over metadata forms (path strings, timestamps, hashes).

## Implementation Checklist
- [ ] Define SQLAlchemy / SQLite structural updates for `features` and `feature_assignments`.
- [ ] Define the insert logic to respect `superseded_by` immutable updates.
- [ ] Define basic heuristic scoring functions (Path Similarity, Temporal Bounds).
- [ ] Incorporate weighting configurations for algorithmic passes.
- [ ] Provide CLI wrappers for forcing manual reassignments of distinct event blocks.
- [ ] Ensure comprehensive test cases validating conflict supersession patterns.

## Changelog
- **1.0.0**: Initial comprehensive design RFC for robust feature reconstruction. Replaces draft RFC-003.
