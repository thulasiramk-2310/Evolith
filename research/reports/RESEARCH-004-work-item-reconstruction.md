# RESEARCH-004: Work Item Reconstruction

## Status
Proposed

## Date
May 31, 2026

## Motivation
Research-002 and Research-003 showed that community detection is useful but insufficient.

Key failures:
- Feature Twins: ARI near 0
- Utility Hub: ARI near 0

Conclusion:
- The missing signal is not primarily graph topology.
- Feature identity behaves more like a work narrative than a file community.

## Updated Hypothesis
Features are reconstructed from narrative continuity across work items, with graph structure as a secondary signal.

Pipeline candidate:
1. Events
2. Episode Detection
3. Work Item Candidate Reconstruction
4. Feature Projection

## Research Questions
1. Can episode-level grouping improve separability for twin features that share file/branch/timing signals?
2. Can narrative signals (commit chains, issue refs, PR refs, branch episodes) reduce hub collapse?
3. What combination of narrative and graph signals achieves robust ARI without introducing non-determinism?

## Candidate Signals
- Episode continuity: time windows + branch continuity + commit continuity
- Branch episodes: stable branch segments over time
- Commit chains: semantic sequence of commit messages
- Issue references: ticket IDs as strong anchors
- PR boundaries: explicit review/merge scope
- Graph context: weighted co-change edges, with hub penalty

## Experimental Design
### Inputs
- Existing adversarial scenarios A-G
- New narrative-rich variants:
  - E2: Feature Twins with distinct ticket/commit narratives
  - G2: Utility Hub with shared files but divergent issue/PR boundaries

### Models to Compare
1. Graph-only baseline (current Louvain stack)
2. Episode + graph hybrid
3. Work-item-first hybrid (episode -> narrative candidate -> graph refinement)

### Metrics
- Primary: ARI
- Secondary: NMI, Purity
- Structural: cluster count stability, over-segmentation rate

## Acceptance Gates
- Normal dataset ARI > 0.90
- Monorepo ARI > 0.85
- Feature Twins ARI > 0.60
- Utility Hub ARI > 0.60

## Non-Goals
- Production persistence tables in Rust
- UI work
- Real-time inference

## Deliverables
1. Narrative-aware simulator module
2. Extended adversarial scenarios (E2, G2)
3. Benchmark report with gate pass/fail
4. Decision memo:
   - Continue research, or
   - Promote architecture to implementation phase

## Decision Rule
Do not implement `features`, `feature_assignments`, or `feature_runs` persistence until Research-004 gates pass.
