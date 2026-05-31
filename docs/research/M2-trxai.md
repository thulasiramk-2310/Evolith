# M2.3 Validation: TRX-AI Candidate (`D:\chatcli`)

Date: May 30, 2026

## Repository Summary
- Repository: `D:\chatcli`
- Git commit events collected: `57`
- Reconstruction algorithm: `v0.1-anchor-hybrid`

## Run Metrics
- Features: `9`
- Anchor Coverage: `22.8%`
- Anchored Features: `5`
- Unanchored Features: `4`
- Avg Events per Feature: `6.33`
- Dominant Anchor Ratio: `0.56`
- Run Status: `completed`

## Top Reconstructed Features
1. `Pattern cache` -> `PARTIAL`
2. `Pattern db` -> `PARTIAL`
3. `Pattern refactor` -> `PARTIAL`
4. `Pattern test` -> `PARTIAL`
5. `Episode Candidate` -> `PARTIAL`
6. `Episode Candidate` -> `PARTIAL`
7. `Pattern ci` -> `YES`
8. `Episode Candidate` -> `PARTIAL`
9. `Episode Candidate` -> `PARTIAL`

## Observations
- Compared to Bravely M2.2, this repo shows lower anchor coverage but materially better consolidation (`9` features over `57` commits; `6.33` avg events/feature).
- Pattern anchors (`cache`, `db`, `refactor`, `test`, `ci`) are meaningful and cluster-like, but several still need human interpretation.
- Dominant anchor ratio (`0.56`) is below the M3 target (`0.60`), but substantially higher than early Bravely baseline (`0.35`).

## Failure Cases
- `Episode Candidate` remains present (`4/9` features), indicating anchor scarcity in portions of history.
- Pattern anchors like `refactor` are broad and can span multiple real work streams.

## Next Actions
1. Add stronger pattern taxonomy and weighting (domain-specific patterns > generic patterns like `refactor`).
2. Add small deterministic merge rules for low-signal episode groups.
3. Re-run this repo after enrichment and target dominant anchor ratio `>= 0.60`.

## Cross-Repository Comparison

| Metric | Evolith | Bravely (M2.2) | TRX-AI Candidate |
| --- | ---: | ---: | ---: |
| Commits | 11 | 64 | 57 |
| Features | 8 | 21 | 9 |
| Anchor Coverage | 26.7% | 39.1% | 22.8% |
| Dominant Anchor Ratio | 0.50 | 0.67 | 0.56 |
| Anchored Features | 4 | 14 | 5 |
| Unanchored Features | 4 | 7 | 4 |
| Avg Events/Feature | 1.88 | 3.05 | 6.33 |

## M3 Signal Check
- `2+` real repositories: `PASS` (Bravely + TRX-AI candidate)
- Anchor Coverage `> 30%`: `PARTIAL` (Bravely pass, TRX-AI candidate below)
- Dominant Anchor Ratio `> 0.60`: `PARTIAL` (Bravely pass, TRX-AI candidate below)
- Anchored Features > Unanchored Features: `PASS` (both real repositories)
