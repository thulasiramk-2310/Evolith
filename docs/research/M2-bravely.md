# M2.1 Validation: Bravely

Date: May 30, 2026

## Repository Summary
- Repository: `D:\Softskills\Bravely`
- Git commit events collected: `64`
- Reconstruction algorithm: `v0.1-anchor-hybrid`

## Run Metrics
- Features: `23`
- Anchor Coverage: `14.1%`
- Anchored Features: `8`
- Unanchored Features: `15`
- Avg Events per Feature: `2.78`
- Dominant Anchor Ratio: `0.35`
- Run Status: `completed`

## Top Reconstructed Features (First 10)
1. `Episode Candidate` -> `PARTIAL`
2. `Episode Candidate` -> `PARTIAL`
3. `Episode Candidate` -> `PARTIAL`
4. `Episode Candidate` -> `PARTIAL`
5. `PR PR-6` -> `YES`
6. `Branch feature/backend-auth` -> `YES`
7. `PR PR-5` -> `YES`
8. `Episode Candidate` -> `PARTIAL`
9. `Episode Candidate` -> `PARTIAL`
10. `Episode Candidate` -> `PARTIAL`

## Observations
- Anchor identity is useful when present (`PR-*`, branch names are immediately recognizable).
- Unanchored output dominates (`15/23`), so many groups still collapse into generic `Episode Candidate`.
- Anchor coverage is currently low (`14.1%`), consistent with sparse explicit anchor usage in commit messages.
- Average events per feature (`2.78`) indicates fragmentation remains significant on this repository.

## Failure Cases
- Too many non-descriptive reconstructed features (`Episode Candidate`) to be directly actionable.
- Low dominant anchor ratio (`0.35`) indicates weak agreement on one identity within many reconstructed groups.
- Branch-only and episode-only groups likely need post-processing/merge logic for better human interpretability.

## Next Actions
1. Improve anchor extraction coverage (ticket + PR parsing variants, branch normalization).
2. Add deterministic merge pass for tiny unanchored episode groups.
3. Re-run Bravely after extraction improvements and compare against this baseline.
4. Run the same M2.1 protocol on `TRX-AI` for cross-repo signal validation.

## M2.2 Re-run (Anchor Enrichment)
- Date: May 30, 2026
- Features: `21` (from `23`)
- Anchor Coverage: `39.1%` (from `14.1%`)
- Anchored Features: `14` (from `8`)
- Unanchored Features: `7` (from `15`)
- Avg Events per Feature: `3.05` (from `2.78`)
- Dominant Anchor Ratio: `0.67` (from `0.35`)

Observed improvement:
- Anchor scarcity was the bottleneck; enrichment substantially increased anchor signal.
- `Episode Candidate` groups were reduced, and descriptive pattern/branch anchors emerged (`Pattern auth`, `Pattern backend`, `Branch auth`, `Branch aiml`).
