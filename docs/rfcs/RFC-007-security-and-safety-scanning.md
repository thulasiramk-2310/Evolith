# RFC-007: Security & Safety Scanning

**Status:** Implemented (v0.1.0-alpha)
**Command:** `evolith doctor --security`
**Ruleset version:** v0.1

---

## Problem

Developers sometimes accidentally commit secrets to git. `.env` files, API keys, private key
files, and database credentials with embedded passwords occasionally end up tracked. Once in git
history, they're exposed — even after a subsequent `git rm`.

Evolith already reads the repository to reconstruct project history. Adding offline, deterministic
security detection on top of that data costs nothing extra and adds significant defensive value.

---

## Design Principles

- **Detect and warn, not fix.** Evolith does not delete files, rotate keys, or modify git history.
- **Offline, deterministic.** No network calls. No uploads. No cloud scanning. Runs entirely from
  the local file system and `git ls-files` output.
- **Read-only.** Evolith never writes to the repository being scanned.
- **Privacy-preserving.** File content is read locally to detect patterns; nothing leaves the
  machine.
- **Not a replacement for dedicated tools.** Evolith's security check is a first-pass "smoke
  test." `git-secrets`, `truffleHog`, or `gitleaks` provide deeper history scanning.

---

## What Is NOT Done (By Design)

- ❌ Upload files or content to VirusTotal, OpenAI, or any cloud service
- ❌ Send code to LLM APIs for analysis
- ❌ Auto-fix secrets or delete files
- ❌ Scan full git history (only currently tracked files)
- ❌ Auto-rotate credentials

---

## Scope: What Is Scanned

**File names** (via `git ls-files`):
- `.env`, `.env.production`, `.env.local`, `.env.staging`, and other `.env.*` variants
- `.env.example`, `.env.sample`, `.env.template` (tracked as MEDIUM — likely intentional templates)
- Private key file names: `id_rsa`, `id_ed25519`, `*.pem`, `*.key`, `*.p12`, `*.pfx`
- Exclusions: files in `.venv/`, `site-packages/`, `node_modules/`, `target/`, `vendor/`
- Exclusions: known CA bundle filenames (`cacert.pem`, `ca-bundle.crt`, etc.)

**File content** (files ≤100KB, skipping binary/vendor paths):
- Private key headers: `-----BEGIN (RSA|OPENSSH|EC|DSA|PGP)? PRIVATE KEY-----`
- OpenAI API keys: `sk-[A-Za-z0-9_-]{20,}`
- GitHub tokens: `gh[pors]_[A-Za-z0-9]{36,}`
- AWS access keys: `AKIA[0-9A-Z]{16}`
- Google API keys: `AIza[0-9A-Za-z\-_]{35}`
- Slack tokens: `xox[baprs]-[0-9A-Za-z\-_]{10,}`
- Database URLs with embedded credentials: `(postgres|mysql|mongodb|redis)://user:pass@host`

**`.gitignore` completeness:**
- No `.gitignore` present
- `.env*` pattern missing from `.gitignore`
- Common directories tracked that should be gitignored (`__pycache__`, `.venv`, `node_modules`,
  `dist`, `build`)

---

## Risk Levels

| Level    | Triggered by                                          |
|----------|-------------------------------------------------------|
| Critical | Any private key file or private key content in code   |
| High     | API keys, tokens, non-template `.env.*`, DB creds     |
| Medium   | `.env.example`/template files, missing `.gitignore` rules, vendor dirs tracked |
| None     | No findings                                           |

---

## Output

### Text mode (default)

```
Security Report
===============

Findings

🚨 [CRITICAL]  Private key file tracked by git: deploy/server.pem
⚠  [HIGH]      .env.production is tracked by git — secrets may be exposed.
ℹ  [MEDIUM]   .env.example is tracked — verify it contains only placeholder values.

Risk: CRITICAL

Secrets Summary
---------------
⚠  .env files tracked      : 1
⚠  Private keys             : 1
✓  OpenAI keys              : none
...

Recommended Actions
1. Revoke and rotate the private key at deploy/server.pem immediately.
2. Remove .env.production from git tracking: `git rm --cached .env.production`
3. Use `git filter-repo` or BFG Repo Cleaner to purge secrets from git history.
```

### JSON mode (`evolith doctor --security --json`)

```json
{
  "risk_level": "Critical",
  "findings": [
    {
      "severity": "critical",
      "kind": "tracked_key_file",
      "message": "Private key file tracked by git: deploy/server.pem",
      "location": "deploy/server.pem"
    }
  ],
  "recommendations": ["..."],
  "scanned_file_count": 142
}
```

---

## Integration with `evolith doctor`

When running `evolith doctor` (full health report), a Security & Safety section is appended
automatically between the Health section and the Recommended Actions section. Security
recommendations appear first in the combined action list.

`evolith doctor --security` runs security scanning only (no feature run database required).

---

## Implementation Notes

- `scan_tracked_file_names(files)` — filename-pattern matching, returns `Vec<SecurityFinding>`
- `check_content_patterns(content, file)` — regex matching on file content; `pub(crate)` for tests
- `scan_tracked_file_contents(files)` — reads ≤100KB files and calls `check_content_patterns`
- `check_gitignore_completeness()` — reads `.gitignore` for common missing patterns
- `scan_security()` — orchestrates all checks, returns `SecurityReport`
- `render_security_text(report)` — human-readable text output
- Ruleset version constant: `DOCTOR_RULESET_VERSION = "v0.1"` (shared with RFC-006)

---

## Test Coverage (15 tests)

- `security_detects_env_file_by_name` — .env variants, severity split high/medium
- `security_detects_private_key_file_by_name` — key files, vendor-path exclusion
- `security_content_detects_aws_key_pattern`
- `security_content_detects_private_key_header`
- `security_content_detects_openssh_private_key`
- `security_content_detects_db_url_with_credentials`
- `security_content_no_false_positive_for_clean_db_url`
- `security_risk_level_critical_for_private_key`
- `security_risk_level_high_for_env_file`
- `security_risk_level_none_when_clean`
- `security_recommendations_include_history_cleanup_for_high_findings`
- `security_json_report_has_correct_shape`
- `security_text_report_starts_with_security_report_header`
- `doctor_text_always_contains_security_section`
- `doctor_text_shows_security_findings_when_present`

---

## Future Work (Not in v0.1)

- `--history` flag: scan git log for secrets added and later removed (requires `git log -p`)
- SBOM integration: flag dependencies with known CVEs
- Per-file severity override via `.evolith-ignore`
- Webhook mode: post findings to a security dashboard
