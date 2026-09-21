# Linux-first governance amendment verification

Date: **2026-09-21**. Scope: roadmap, constitution and dependent guidance only.
Product implementation remains **Version 3 complete**. No Version 5 specification,
tasks, collector code or live endpoint work is included.

## A. Outcome

Constitution **1.0.0 -> 1.1.0** adds materially expanded Linux support and delivery
guidance while retaining all principles, fixed technologies, security prohibitions
and the original 2026-09-15 ratification date. ADR-0013 records the user's requested
Linux-first direction, compatibility impact and deferred Windows obligations.

Revised order: **0–3 complete -> 5 -> 6–15 Linux-first -> 16 Windows completion**.
Version 4 is deferred into Version 16, not silently completed or deleted.
Version 5 now owns shared collector foundations, versioned Linux vocabulary
migration and the first independently checked standalone Linux lab JSON report.
No Windows workstation is required for the Linux milestones. Full Linux support
claims require actual evidence; the first one-VM demo is only an early checkpoint.

## B. Files changed

Created:

- `docs/linux-first-roadmap.md`
- `docs/linux-first-governance-verification.md`

Updated:

- `JOCKY_AI_Agent_Prompt_Pack.md`
- `.specify/memory/constitution.md`
- `.specify/templates/plan-template.md`
- `.specify/templates/spec-template.md`
- `.specify/templates/tasks-template.md`
- `AGENTS.md`
- `README.md`
- `ci/README.md`
- `native/README.md`
- `docs/architecture.md`
- `docs/current-state.md`
- `docs/decisions.md`
- `docs/traceability.md`
- `docs/security-boundary.md`
- `docs/platform-capabilities.md`
- `docs/module-contracts.md`

Completed feature plans, old decisions, historical release evidence, source,
dependencies, schemas, catalogue, examples and fixtures are preserved. The README's
stale Version 3 Phase 6 hand-off is corrected; no historical test is relabelled.
Existing unrelated worktree modifications/deletions are untouched. No files removed.

## C. Commands executed

Inspection used `pwd`, `git status --short`, `git diff --cached --stat`,
`git rev-parse HEAD`, `rg`, `wc`, `sed`, `cat` and read-only Node SHA-256 checks.
Read the constitution skill, its Git-init hook skill/script, current constitution,
plan/spec/tasks templates, generic constitution/checklist templates, current V3
plan, architecture/decisions/current-state, roadmap and release evidence.
There are no `.specify/templates/commands/` templates to synchronise.

The required constitution pre-hook ran:

```sh
bash .specify/extensions/git/scripts/bash/initialize-repo.sh
```

It reported that Git already exists and skipped initialization. The optional
`/speckit-git-commit` post-hook was not run. No staging, commit, push or Git
configuration change occurred; the existing index stayed empty.

Regression commands (run after initial inspection, while documentation was updated):

```sh
# Repository root
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline

# controller/
.venv/bin/python -m ruff format --check .
.venv/bin/python -m ruff check .
.venv/bin/python -m mypy
.venv/bin/python -m pytest

# dashboard/
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build

# Repository root
controller/.venv/bin/python -m ruff format --check --config controller/pyproject.toml ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m ruff check --config controller/pyproject.toml ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m mypy --strict --explicit-package-bases ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m pytest tests/test_prepare_legacy_ast_schema.py
target/compose-verification/docker-compose --env-file .env.example -f compose.yaml config --quiet
```

Formatting and final inspection:

```sh
dashboard/node_modules/.bin/prettier --write JOCKY_AI_Agent_Prompt_Pack.md .specify/memory/constitution.md .specify/templates/plan-template.md .specify/templates/spec-template.md .specify/templates/tasks-template.md AGENTS.md README.md ci/README.md native/README.md docs/architecture.md docs/current-state.md docs/decisions.md docs/traceability.md docs/security-boundary.md docs/platform-capabilities.md docs/module-contracts.md docs/linux-first-roadmap.md docs/linux-first-governance-verification.md
dashboard/node_modules/.bin/prettier --check JOCKY_AI_Agent_Prompt_Pack.md .specify/memory/constitution.md .specify/templates/plan-template.md .specify/templates/spec-template.md .specify/templates/tasks-template.md AGENTS.md README.md ci/README.md native/README.md docs/architecture.md docs/current-state.md docs/decisions.md docs/traceability.md docs/security-boundary.md docs/platform-capabilities.md docs/module-contracts.md docs/linux-first-roadmap.md docs/linux-first-governance-verification.md
git diff --check
git diff --cached --stat
git status --short
```

Read-only inline Node assertions check local Markdown links/anchors, constitution
version/dates, all 24 implementation traceability rows, Version 5 ownership,
Version 16 deferred acceptance, required safety/migration clauses and preserved
source/schema/test fingerprints. They introduce no public code or new test tool.

## D. Tests and results

Local Linux environment, installed dependencies; no package download or live endpoint.

| Check                                                 | Result                                                                          |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- |
| Rust formatting and locked offline Clippy             | Pass                                                                            |
| Full workspace unit/integration tests                 | 342 passed                                                                      |
| Compile-fail doctests                                 | 5 passed                                                                        |
| Controller Ruff format/lint and mypy                  | Pass; 19 checked files                                                          |
| Controller pytest                                     | 52 passed                                                                       |
| Setup-tool Ruff format/lint and strict mypy           | Pass; 2 checked files                                                           |
| Setup-tool pytest                                     | 11 passed                                                                       |
| Dashboard formatting, lint and type checking          | Pass                                                                            |
| Vitest and Vite production build                      | 18 passed; build passed                                                         |
| Standalone Compose configuration                      | Pass; no services started                                                       |
| Changed Markdown formatting and whitespace            | Pass; 18 changed documents                                                      |
| Documentation consistency and preservation assertions | Pass; 61 local links, one anchor, 24 implementation rows, 17 milestone headings |

A 609-file pre-edit fingerprint set covers source, tests, fixtures, schemas,
catalogue, manifests, lockfiles, adapters and CI. **607 files are byte-identical**;
the only two changed files in that set are the intentionally updated
`ci/README.md` and `native/README.md`. No new files appeared in those protected
areas. The 2 new and 16 updated files listed above are documentation/guidance only.
HEAD remains `48f0c0bfc33bf1cae57396948921906f5a350c90`; the index is unchanged.

No public runtime behavior is added, so governance validation and existing
unit/integration/negative suites are appropriate; no fake collector or artificial
feature was added merely to create new tests. One documentation patch initially
failed its context check without applying changes; it was corrected and reapplied.

## E. Acceptance checklist

- [x] Linux-first order is explicit and consistent across current guidance.
- [x] Shared collector foundations move from deferred Version 4 to revised Version 5.
- [x] A standalone real-data Linux lab checkpoint precedes package/dashboard work.
- [x] Distribution support is bounded, versioned and evidence-based, not universal.
- [x] Legacy Ubuntu/Windows contracts and fixture-only execution remain preserved.
- [x] Windows collector/package/runtime/end-to-end work is assigned to Version 16.
- [x] Security rules, fixed stack and authorised-VM boundaries are unchanged.
- [x] No product code, schema, dependency, CI job or live collection is introduced.
- [x] Existing local Rust/Python/dashboard gates and Compose configuration pass.
- [x] Final Markdown, link, scope and preservation inspection completed.

## F. Limitations

The product still collects no real evidence. Generic `linux`, live collection,
support-matrix testing and the standalone report are future Version 5 work.
Exact VM releases/images and endpoint authorisations must be established during
that feature's planning/lab setup. No workstation is designated by this amendment.
Windows hosted CI remains unobserved; existing Ubuntu hosted results likewise
remain unverified here. The old Rust 1.85 declaration/dependency mismatch remains.
No deployment, live services, Windows run or cross-distribution collector test occurred.

## G. Exact hand-off

Next request: **`$speckit-specify` with Version 5: Portable Linux Read-Only Forensic
Collector Library** from the revised prompt pack. Read constitution **1.1.0**,
ADR-0013, the Linux-first roadmap and Version 3 verification first. Then request
the plan, tasks and scoped implementation. Do not execute deferred Version 4,
change old schemas without migration, or begin live collection automatically.

Suggested commit message (not executed):
`docs: amend constitution to 1.1.0 and adopt Linux-first roadmap`
