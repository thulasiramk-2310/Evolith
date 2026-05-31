# Roadmap

Naming decision:
- Product name: `Evolith`
- Primary CLI command: `evolith`
- Backward-compatible alias: `devreplay` (deprecated in alpha, removal planned after transition window).

## v0.1.0-alpha (Current)

Scope:
- Git-only collection
- CLI-only workflows
- Single-user local usage
- Explainable feature reconstruction

Included:
- Event collection and persistence
- Anchor extraction and episode fallback
- Feature reconstruction and assignment writing
- Run metrics and run health
- Feature explainability and feature health
- JSON output for run and feature explainability

Exit criterion:
- `3+` external developers run the CLI on their own repositories, share `runs show` output, and provide structured feedback on usefulness/trust/confusion points.

## v0.2.0

Focus:
- Stronger repository diagnostics and operator workflows
- Better reconstruction quality on sparse-anchor histories

Planned:
- Collector plugin architecture:
  - pluggable collector interface
  - collector registration/config via CLI
  - versioned collector metadata persisted per event
- Repository health command and recommendations:
  - `evolith doctor`
  - repository-level health grade + actionable guidance
- Deterministic feature merge suggestions:
  - detect tiny low-signal feature fragments
  - propose merge candidates with explicit deterministic reasons
- Extended run-level reporting:
  - run-to-run comparison view
  - trend reporting for anchor coverage and dominant anchor ratio

## v0.3.0

Focus:
- Better developer UX for daily use

Planned:
- Terminal UI (TUI)
- VS Code integration
- Guided investigation flows over features and runs

## v1.0

Focus:
- Durable project memory and deeper engineering insights

Planned:
- Long-horizon project memory
- Cross-repository comparisons and insights
- Stability hardening and broader external adoption
