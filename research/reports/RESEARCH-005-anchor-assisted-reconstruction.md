# RESEARCH-005: Anchor-Assisted Reconstruction

## Status
Proposed

## Date
May 31, 2026

## Motivation
Research-004 showed a clear fingerprint:
- Narrative Hub (G2) and Interrupted Narrative (E3) can be solved.
- Narrative Twins (E2) remains near-random.

Interpretation:
- Some twin cases are underdetermined with event-only signals.
- Weak intent anchors may provide the missing separability.

## Hypothesis
Feature reconstruction quality improves dramatically when optional work-item anchors are present:
- Ticket IDs (e.g., `AUTH-123`)
- PR IDs (e.g., `PR-91`)
- Distinct branch episodes (e.g., `feature/auth-refactor`)

## Experimental Plan
Twin baseline remains identical except for one anchor type:
1. E2A: Twins + Ticket anchors
2. E2B: Twins + Branch episode anchors
3. E2C: Twins + PR anchors

## Metrics
- Primary: ARI
- Secondary: NMI, Purity

## Gates
- E2A ARI > 0.80
- E2B ARI > 0.80
- E2C ARI > 0.80

## Decision Rule
If anchor-assisted twins consistently exceed ARI 0.80, proceed with architecture that is:
- automatic by default
- anchor-amplified when available

Do not implement persistence tables until this gate is validated.

---

## Addendum: Gate Results Are Invalid (audit, August 19, 2026)

The E2A/E2B/E2C gates all report ARI 1.0000 and all three PASS. **These numbers
do not measure method performance and must not be cited as such.**

### The defect

In each anchor scenario the label and the anchor are assigned from the same dict
literal, in the same loop iteration, as two arguments to the same call:

```python
features = [
    {"name": "Authentication Refactor", "ticket": "AUTH-123"},
    {"name": "Authentication Migration", "ticket": "AUTH-456"},
]
build_event(feat["name"], ..., ticket_id=feat["ticket"])
#           ^ ground_truth              ^ the anchor the method reads
```

The mapping is bijective over a 2-element list, so `ticket_id` is `ground_truth`
under a different spelling. Measured `NMI(ground_truth, ticket_id) = 1.0000`.
`calculate_similarity_hybrid` adds +0.45 on anchor match and −0.35 on mismatch,
the largest term in the function, so the clustering is reading the answer key.

Confirmed by ablation: removing the anchor field drops E2A from 1.0000 to
0.0000. The anchor supplies 100% of the signal. A one-line
`groupby('ticket_id')` also scores 1.0000, tying the entire graph pipeline.

E2A, E2B and E2C are not three independent confirmations. Under identical
corruption seeds they return bit-identical ARI at every noise level
(0.8170 / 0.5772 / 0.1981 at 5/10/25% corruption). They are one experiment run
three times with the leaking field renamed.

### The defect is not limited to RESEARCH-005

The same pattern exists in the base generators, where `{"name": ..., "branch": ...}`
is also one-to-one. Every scenario that scores ARI 1.0000 is solved exactly by
`GROUP BY branch`:

| Scenario | Louvain ARI | Best trivial `groupby` | Field |
| --- | --- | --- | --- |
| A shared infrastructure | 1.0000 | 1.0000 | `branch` |
| B context switching | 0.9299 | 1.0000 | `branch` |
| D long-lived feature | 1.0000 | 1.0000 | `branch` |
| F monorepo | 1.0000 | 1.0000 | `branch` |
| E3 interrupted narrative | 1.0000 | 1.0000 | `branch` |
| E2A / E2B / E2C | 1.0000 | 1.0000 | anchor |
| G giant utility file | 0.1486 | 0.0049 | `file_path` |
| E2 narrative twins | 0.0079 | **0.3345** | `commit_msg` |

13 of 14 scenarios fail the check "does the graph method beat a one-line
groupby by more than 0.05?". Only Scenario G passes, and both scores there are
poor in absolute terms.

### The 0.018 twins result is also mischaracterised

RESEARCH-004 records Narrative Twins as "near-random", implying the scenario is
underdetermined. It is not. `commit_msg` carries real signal on E2
(`NMI = 0.558`), and a trivial `groupby('commit_msg')` scores **0.3345** where
Louvain scores 0.0079 — the baseline beats the method by roughly 40x. E2 is an
*algorithmic* failure to use narrative signal that is present in the data, not
an information-theoretic floor.

### What was actually established

- Anchors that map one-to-one onto features make reconstruction trivially
  separable. This is an upper bound, not a result.
- Under realistic anchors (partial coverage, shared epics, mislabeling) the
  method scores ARI ~0.07, barely distinguishable from `groupby('ticket_id')`
  at ~0.08. See `scenario_e2d_realistic_ticket_anchors`.
- The current similarity function extracts no usable signal from commit
  narrative, which is where the remaining headroom is.

### Corrective tooling added

- `research/adversarial/leakage_guard.py` measures `NMI(ground_truth, field)`
  for every input field a similarity function may read, and fails any scenario
  where a field determines the label.
- `research/adversarial/benchmark_fair.py` runs Louvain, DBSCAN, connected
  components and a trivial `groupby` on the *same* events with the *same*
  similarity function, over multiple seeds with standard deviation, and prints
  the leakage report before any score.
- `scenario_e2d_realistic_ticket_anchors` provides anchors that correlate with
  features without being a relabeling of them.

### Required before these gates mean anything

1. Re-derive gates from `benchmark_fair.py`, not `benchmark.py`.
2. Treat any scenario the leakage guard marks LEAK as an upper bound only.
3. Require the graph method to beat the best trivial `groupby` by a stated
   margin; a scenario where it does not cannot support a claim about the method.
4. Label ground truth on at least one real repository (squashed-merge PR
   boundaries, or `Fixes #N` trailers) so labels are not written by the same
   code that writes the features.

Status of this RFC's decision rule: **not satisfied**. The gates as written
cannot distinguish a working method from a lookup table.
