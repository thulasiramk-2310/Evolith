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
