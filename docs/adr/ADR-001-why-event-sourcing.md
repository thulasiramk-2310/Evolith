# ADR-001: Event Sourcing

## Status
Accepted

## Context
We need a way to store historical data of how a codebase evolved.

## Decision
Events are the immutable source of truth.
Sessions, Features, and Stories are projections based on events.
