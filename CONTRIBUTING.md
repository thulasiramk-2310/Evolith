# Contributing to Evolith

Read ADR-001 before contributing.
Read RFC-001 before implementing collectors.

## Current Phase

`v0.1.0-alpha` is feature-frozen for core surface area. Prioritize:
- bug fixes
- test coverage
- explainability quality
- real-repository validation reports

Avoid adding new product surfaces unless directly required for alpha stability.

## Development Workflow

1. Initialize local workspace:
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- init`
2. Collect and rebuild:
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- collect git`
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- features rebuild`
3. Inspect run and feature outputs:
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- runs list`
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- runs show 0`
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- features list`
   - `cargo run --manifest-path crates/cli/Cargo.toml --bin evolith -- features explain <feature-id>`

## Quality Gates

Before opening a PR:
- `cargo test` must pass.
- Git collection must remain idempotent.
- Explainability output must remain coherent for both:
  - human-readable mode
  - `--json` mode

## Reporting

For repository validations, add or update reports in:
- `docs/research/`

Use the existing M2 report format and include:
- repository summary
- run metrics
- top reconstructed features
- observations
- failure cases
- next actions
