# RFC-006: Repository Health & Recommendations Engine

## Status
Proposed

## Version
1.0.0

## Author
Evolith Maintainers

## Date
May 31, 2026

---

## Abstract
Evolith can reconstruct features, score trust, and explain why events were grouped. It cannot yet answer the next user question: "What should I change in my workflow to improve reconstruction quality?" This RFC introduces a deterministic Repository Health & Recommendations Engine exposed through `evolith doctor` and `evolith doctor --json`.

The engine converts existing reconstruction metrics into actionable guidance. It closes the loop:

`Collect -> Reconstruct -> Explain -> Assess -> Recommend`

## Context
From M2/M3 validation:
- Feature/run explainability works.
- Health classification works.
- Accuracy strongly correlates with anchor quality/coverage.

Current gap:
- Evolith reports weak evidence and fragmentation, but does not prescribe operator actions.

## Goals
- Provide repository-level health classification from persisted run + feature signals.
- Produce deterministic recommendations (no LLM dependency).
- Offer human-readable and JSON outputs for CLI and automation.
- Make advice operational and measurable (expected impact ranges).

## Non-Goals
- Automatic modification of commit history or branch names.
- AI-generated natural-language coaching.
- Replacing feature-level explainability.
- Solving all monorepo boundary problems in this phase.

## Command Interface

### Human Output
```bash
evolith doctor
```

### Structured Output
```bash
evolith doctor --json
```

### Optional Scope Flags (future-compatible)
- `--run <id|index>`: evaluate a specific run.
- Default behavior: evaluate latest completed run.

## Example Output
```text
Repository Health: Moderate

Anchor Coverage: 22.8%
Dominant Anchor Ratio: 0.56
Weak Evidence Features: 4/9

Issues:
- 77.2% of events lack anchors
- 4 features are weakly supported
- fragmentation risk is moderate (avg events/feature 6.33)

Recommendations:
1. Include ticket IDs in commit messages (AUTH-123 style)
2. Use consistent branch naming (feature/<domain>-<goal>)
3. Reference PR IDs in merge/commit messages

Expected Impact:
Anchor Coverage +15-25%
Dominant Anchor Ratio +0.05 to +0.15
```

## Health Signals

### 1) Anchor Coverage
Source: latest `feature_runs.anchor_coverage`.

Thresholds:
- `>= 0.40`: Good
- `0.20 - 0.39`: Moderate
- `< 0.20`: Poor

### 2) Dominant Anchor Ratio
Source: latest `feature_runs.dominant_anchor_ratio`.

Thresholds:
- `>= 0.70`: Good
- `0.50 - 0.69`: Moderate
- `< 0.50`: Poor

### 3) Fragmentation
Source: latest `feature_runs.avg_events_per_feature`.

Interpretation:
- `< 2.0`: Fragmented
- `2.0 - 25.0`: Healthy range
- `> 25.0`: Potential over-merge

### 4) Weak Evidence Rate
Source: derived by evaluating active features with existing Feature Health rules.

Computation:
- `weak_evidence_rate = weak_features / total_features`

Thresholds:
- `<= 0.20`: Good
- `0.21 - 0.40`: Moderate
- `> 0.40`: Poor

## Repository Health Classification
Deterministic weighted score:

```text
score = 0.35 * anchor_coverage_score
      + 0.30 * dominant_anchor_ratio_score
      + 0.20 * fragmentation_score
      + 0.15 * weak_evidence_score
```

Where each component is normalized to `[0, 1]`.

Final labels:
- `score >= 0.75`: Good
- `0.50 <= score < 0.75`: Moderate
- `score < 0.50`: Poor

## Recommendation Rules
Rules are deterministic and additive.

### Rule A: Low Anchor Coverage
If `anchor_coverage < 0.20`:
- Recommend commit ticket references (`AUTH-123`, `ABC-42`).
- Recommend PR references (`#123`, `PR-123`) in merge/commit messages.
- Expected impact: `+15%` to `+25%` anchor coverage.

If `0.20 <= anchor_coverage < 0.40`:
- Recommend branch naming normalization + ticket references.
- Expected impact: `+8%` to `+15%` anchor coverage.

### Rule B: Low Dominant Anchor Ratio
If `dominant_anchor_ratio < 0.50`:
- Recommend stronger anchor consistency per work stream.
- Recommend avoiding mixed-intent commits on the same branch.

### Rule C: High Weak-Evidence Rate
If `weak_evidence_rate > 0.30`:
- Recommend larger coherent commits over many single-event commits.
- Recommend branch naming by work item rather than generic names.

### Rule D: Fragmentation
If `avg_events_per_feature < 2.0`:
- Recommend reducing micro-commits without anchors.
- Recommend explicit issue/ticket references for short-lived commits.

### Rule E: Over-Merge Risk
If `avg_events_per_feature > 25.0`:
- Recommend splitting broad branches by feature intent.
- Recommend distinct ticket/PR anchors across concurrent streams.

## JSON Output Contract (`doctor --json`)
```json
{
  "run_id": "uuid",
  "status": "completed",
  "repository_health": "Moderate",
  "metrics": {
    "anchor_coverage": 0.228,
    "dominant_anchor_ratio": 0.56,
    "feature_count": 9,
    "anchored_feature_count": 5,
    "unanchored_feature_count": 4,
    "avg_events_per_feature": 6.33,
    "weak_evidence_feature_count": 4,
    "weak_evidence_rate": 0.444
  },
  "issues": [
    "77.2% of events lack anchors",
    "4 features are weakly supported"
  ],
  "recommendations": [
    {
      "id": "low_anchor_coverage_commit_anchors",
      "priority": "high",
      "message": "Include ticket IDs in commit messages.",
      "expected_impact": "Anchor Coverage +15-25%"
    }
  ],
  "scoring": {
    "overall_score": 0.62,
    "band": "Moderate"
  }
}
```

## Data Dependencies
- `feature_runs` (latest completed run metrics).
- `features` + evidence + existing feature health classification.
- Optional: event counts for additional issue diagnostics.

No schema migration is required for initial version.

## Implementation Plan
1. Add `Doctor` command in CLI.
2. Read latest completed run (or selected run).
3. Compute weak evidence count/rate from active features.
4. Compute repository score + health band.
5. Apply deterministic recommendation rules.
6. Render text and JSON output.
7. Add tests with Bravely/TRX-AI-like fixtures.

## Acceptance Criteria
- `evolith doctor` provides non-empty deterministic recommendations.
- `evolith doctor --json` returns stable parseable contract.
- Bravely M2.2-like metrics classify as `Repository Health: Good`.
- Lower-coverage TRX-AI-like fixture classifies as `Moderate` or `Poor` with recommendations.
- Command never mutates repository data.

## Risks
- Overly rigid thresholds may not fit all repositories.
- Early expected-impact ranges are heuristic and should be calibrated with external user feedback.

## Mitigations
- Keep thresholds configurable in code constants.
- Version rule sets (`doctor_ruleset_version`) for reproducibility.
- Track recommendation efficacy in future run comparisons.

## Future Extensions
- `evolith doctor --run <id>` deep diagnostics.
- Run-to-run improvement tracking from prior doctor outputs.
- Monorepo-aware recommendation rules.
- Collector-aware recommendations once plugin system lands (v0.2.0+).

