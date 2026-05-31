# RFC-003: Feature Reconstruction Engine

## Status
Proposed

## Context
Raw events tell us what happened at the keystroke or file level. Sessions cluster these events chronologically to give a contiguous slice of developer time. However, to achieve Evolith's vision ("how software was built"), we need semantic grouping. We call this grouping "Features." Features allow a user to understand the goal-oriented lifecycle of a block of work rather than just reading a raw sequential diary.

## Definition of a Feature
A "Feature" is an aggregated projection of multiple sessions (or slices of sessions) correlated by a common semantic goal. 

Features answer questions like:
- Which files were created and evolved specifically for "Authentication"?
- When were bugs related to this goal introduced and patched?
- What was the lifecycle of the PR implementing this?

## Heuristics for Reconstruction (Milestone 0.5+)

Since we aim to automatically reconstruct features without requiring heavy manual developer tagging, we will employ reconstruction heuristics:

1. **Git Linkage**: Correlating events around specific Git branches, commits, or issue references inside commit bodies.
2. **File Graph**: Path-based locality (e.g., changes exclusively targeting src/auth/* denote an Auth feature).
3. **Temporal Linkage**: Events clustered tightly across multiple sessions indicating an ongoing, paused, and resumed focus on the same subset of systems.
4. **Agent Analysis (Future)**: Employing intelligent heuristic models or LLMs to parse context, payloads, and commit trails to label features automatically.

## Architectural Requirements

### 1. eatures Table
Tracks the reconstructed semantic entity.
- id (UUID, Primary Key)
- project_id (Text)
- 	itle (Text)
- confidence_score (Real)

### 2. eature_assignments Table
Maps underlying events (or sessions) into the semantic feature scope.
- eature_id (UUID, Foreign Key)
- event_id (UUID, Foreign Key)

### 3. Engine Operations
- 
econstruct_features(): Operates over unassigned sessions/events and performs contextual grouping logic.
- get_feature(id): Reads the projection back from persistent storage to replay solely the slices of changes that directly pertained to that feature.
