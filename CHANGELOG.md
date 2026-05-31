# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning pre-release tags.

## [0.1.0-alpha] - 2026-05-31

### Added
- Git collector command: `devreplay collect git`.
- Feature reconstruction pipeline: `events -> anchors -> episodes -> features`.
- Feature persistence tables: `features`, `feature_assignments`, `feature_runs`.
- Run-level metrics:
  - `anchor_coverage`
  - `anchored_feature_count`
  - `unanchored_feature_count`
  - `avg_events_per_feature`
  - `dominant_anchor_ratio`
- Run lifecycle status tracking: `running`, `completed`, `failed`.
- CLI workflows:
  - `devreplay features rebuild`
  - `devreplay features list`
  - `devreplay features show <id>`
  - `devreplay features explain <id>`
  - `devreplay runs list`
  - `devreplay runs show <id>`
- Explainability and trust layer:
  - Anchor Confidence
  - Evidence Strength
  - Feature Health
  - Run Health
- JSON output support:
  - `devreplay features explain <id> --json`
  - `devreplay runs show <id> --json`
- Real repository validation reports:
  - Bravely (`docs/research/M2-bravely.md`)
  - TRX-AI candidate (`docs/research/M2-trxai.md`)

### Changed
- Anchor extraction enriched with branch normalization and commit-pattern signals.
- Feature grouping improved to reduce unanchored fragmentation.

### Fixed
- Git collection idempotency (duplicate commit import prevention).
- Feature run status updates on rebuild failures.

### Testing
- Added migration, persistence, reconstruction, and idempotency tests.
- Added anchor-dominance and explainability behavior tests.
