# RESEARCH-001: Feature Reconstruction via Graph Community Detection

## Context
Following the initial RFC-003 design, we implemented a Python simulator (`research/feature_reconstruction_simulator.py`) to test heuristic clustering logic before committing to a costly Rust/SQLite persistence implementation.

The initial assumption was that calculating a similarity score between events and applying a basic threshold (Connected Components) would cleanly reconstruct features.

## Dataset Profile
- **Total Events**: 1,000
- **Underlying Features**: 4 ("Authentication", "Payments", "SSO Migration", "Memory Leak Hotfix")
- **Noise Injected**:
  - Missing branch tracking (10% drop to `main`)
  - Missing commit messages (80% absent)
  - Time gaps and context-switching
- **Overlap Profile**: "Authentication" and "SSO Migration" intentionally share files (`auth.ts`).

## Algorithms Tested
Three algorithms were tested against the identical synthetic dataset:
1. **Connected Components (Baseline)**
2. **DBSCAN** (Density-Based Spatial Clustering of Applications with Noise)
3. **Louvain Community Detection** (Graph Modularity Maximization)

## Results

| Algorithm            | Purity |
| -------------------- | ------ |
| Connected Components | ~78.7% |
| DBSCAN               | ~73.5% |
| Louvain              | ~99.5% |

## Observations
- **Shared Files Create Bridge Contamination:** In Connected Components, the presence of `auth.ts` in both "Authentication" and "SSO Migration" created edges that were too strong to break, merging both distinct features into one massive blob.
- **DBSCAN Fails on Edges:** DBSCAN discarded many smaller, isolated events as noise rather than keeping them with their parent features, and still struggled to split the overlapping clusters.
- **Louvain Thrives on Density:** By representing the event stream as a weighted graph, Louvain maximizes modularity. Even though `auth.ts` connects two clusters, Louvain realizes that the density of interactions within `jwt.ts` and `sso.ts` forms distinct communities. It confidently severs the weak cross-feature boundary.

## Conclusion
Feature reconstruction **MUST** be modeled as a graph community detection problem, not simple coordinate distance or threshold limits. 

The architecture for RFC-003 should transition from simple Similarity Threshold Clustering to:
`Weighted Event Graph -> Community Detection -> Feature Projection`.

Before writing the Engine in Rust, Louvain will be stress-tested against adversarial datasets.
