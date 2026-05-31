# Issue #1: RFC-006 - Implement `evolith doctor` (Repository Health & Recommendations)

## Summary

Implement repository-level health assessment and actionable recommendations based on existing reconstruction metrics.

This command should help developers understand how trustworthy reconstruction is and what changes would improve future reconstruction quality.

## Commands

```bash
evolith doctor
```

```bash
evolith doctor --json
```

## Scope

Read-only analysis.

No event mutation.

No feature mutation.

No AI dependency.

Recommendations must be deterministic and reproducible.

Recommendations are always shown; severity is indicated by health tier.

## Inputs

Use existing metrics already persisted in `feature_runs`:

- `anchor_coverage`
- `dominant_anchor_ratio`
- `anchored_feature_count`
- `unanchored_feature_count`
- `avg_events_per_feature`

## Repository Health Classification

### Good

- `anchor_coverage >= 30%`
- `dominant_anchor_ratio >= 0.60`
- `anchored_feature_count > unanchored_feature_count`

### Moderate

Default classification when repository is neither Good nor Poor.

### Poor

One or more of:

- `anchor_coverage < 20%`
- `dominant_anchor_ratio < 0.50`
- `unanchored_feature_count > anchored_feature_count`

## Recommendation Rules

### Low Anchor Coverage

Condition:

`anchor_coverage < 30%`

Recommendation:

- Include ticket IDs in commit messages
- Reference PR numbers consistently
- Use descriptive branch names

Expected Impact:

`+10% to +25% anchor coverage`

---

### Weak Feature Identity

Condition:

`dominant_anchor_ratio < 0.60`

Recommendation:

- Standardize commit naming
- Use stable work-item references

Expected Impact:

`+0.10 to +0.25 dominant anchor ratio`

---

### Fragmentation

Condition:

`avg_events_per_feature < 2.0`

Recommendation:

- Consolidate short-lived branches
- Reduce one-commit feature branches

Expected Impact:

Larger and more coherent reconstructed features.

## Text Output Example

```text
Repository Health: Moderate

Metrics:
- Anchor Coverage: 22.8%
- Dominant Anchor Ratio: 0.56
- Anchored Features: 5
- Unanchored Features: 4

Issues:
- Low anchor coverage
- Weak feature identity

Recommendations:
1. Include ticket references in commit messages.
2. Reference PR numbers consistently.
3. Use descriptive branch naming.

Expected Impact:
- Anchor Coverage +10% to +25%
- Dominant Anchor Ratio +0.10 to +0.25
```

## JSON Output

Must expose:

- `repository_health`
- `metrics`
- `issues`
- `recommendations`
- `expected_impacts`

Use stable field names suitable for future TUI or desktop UI consumption.

## Validation

Validate against:

- Bravely dataset
- TRX-AI dataset

Expected behavior:

- Bravely classified as `Good` or `Moderate`
- TRX-AI classified as `Moderate`
- Recommendations consistent across repeated runs

## Acceptance Criteria

- `evolith doctor` implemented
- `evolith doctor --json` implemented
- Deterministic health classification
- Deterministic recommendations
- Fragmentation rule triggers when `avg_events_per_feature < 2.0` and produces consolidation recommendations
- JSON contract documented
- Unit tests added
- Bravely and TRX-AI validation results recorded
- No mutation of repository data

## Implementation Notes

- Prefer latest completed run from `feature_runs` when run id is not provided.
- Keep rule thresholds as constants in one location for easy tuning.
- Reuse existing feature/run health helpers when possible.
- Add CLI tests for both text and JSON output paths.
