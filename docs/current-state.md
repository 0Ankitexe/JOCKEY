# Current state

Latest release: **Version 3 — Intermediate Representation and Safe Reference
Interpreter, complete** (2026-09-20, T001–T100). Earlier version and incremental
records below are historical. The final section records the current capabilities,
verification status, limitations and exact next hand-off. The 2026-09-21
Linux-first amendment supersedes older hand-offs: **next is revised Version 5**;
Version 4 is deferred into Version 16. No collector version has started.

## Version 0: Repository Foundation and Project Contracts

- Status: Complete
- Completed: 2026-08-31
- Contract version: 1.0.0
- Next version: Version 1 — Grammar, Lexer, Parser and Diagnostics

Version 0 establishes the monorepo, Rust workspace boundaries, Python 3.12
controller foundation, React/TypeScript dashboard foundation, local Compose
services, shared JSON Schema contracts, offline fixtures, documentation, and CI
jobs for Ubuntu and Windows.

### Implemented

- Seven Rust crates exist for language, IR, code generation, transforms,
  forensics, runtime, and shared contracts. Only the shared task-state type has
  public behavior; the feature crates are intentionally empty boundaries.
- Six Draft 2020-12 JSON Schemas define `EndpointIdentity`, `TaskEnvelope`,
  `TaskStatusEvent`, `ForensicRecord`, `EvidenceManifest`, and `BuildManifest`.
- Contract validators enforce schema versioning, canonical UUID-shaped IDs,
  semantically valid RFC 3339 date-times normalized to UTC `Z`, closed enums,
  required identifiers, and closed message objects.
- Valid fixtures exist for all six schemas. Targeted invalid fixtures cover one
  missing identifier per schema, a malformed UUID, an impossible date-time, a
  non-UTC date-time, and an unknown task state.
- The FastAPI application exposes only `GET /health` and `GET /version`, with
  strict response models, typed error bodies, narrow configurable CORS, and no
  database or Redis connection at startup.
- The dashboard displays its own Version 0 state and one controller
  health/version snapshot. It safely handles invalid, failed, and unreachable
  responses.
- Compose defines PostgreSQL, Redis, and the development controller without
  committed credentials. CI defines Rust jobs on Ubuntu and Windows plus
  Python, TypeScript, and Compose validation jobs.

### Verification snapshot

- `cargo fmt --check`: passed with stable rustfmt 1.98.0.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace --locked`: passed; 5 shared unit/contract tests plus
  all workspace compile and doc-test smoke suites.
- Python Ruff format/lint and strict mypy: passed.
- Python pytest: 52 passed.
- Dashboard Prettier, ESLint, and TypeScript checks: passed.
- Dashboard Vitest: 18 passed across 2 files.
- Dashboard production build: passed (30 modules transformed).
- Docker Compose configuration validation with `.env.example`: passed with the
  official standalone Docker Compose v5.4.0 binary. The host Docker CLI/daemon
  was not installed, so containers were not started locally.

### Intentionally not implemented

- JOCKY grammar, parser, AST, diagnostics, semantic analysis, IR, transforms,
  or code generation;
- forensic collectors or operating-system access;
- endpoint registration, task APIs, scheduling, transport, or runtime
  execution;
- database tables, Alembic revisions, or Redis queues;
- task controls or forensic-result views in the dashboard;
- native, kernel, EDR, BYOVD, in-memory, persistence, privilege, or evasion
  behavior.

The dashboard performs a one-time health check rather than polling. The CI
workflow is defined but was not executed on GitHub-hosted Windows/Ubuntu runners
within this local verification session.

## Version 1 pre-change baseline (2026-09-03)

Before Version 1 implementation, the Version 0 suites were re-run against the
existing worktree:

- Rust formatting and Clippy passed with an isolated Rust 1.98.0 toolchain;
  `cargo test --workspace --locked` passed all 5 existing tests.
- Controller formatting, linting, strict mypy, and all 52 pytest tests passed.
- Dashboard formatting, linting, type checking, all 18 Vitest tests, and the
  production build passed.
- Compose configuration validation passed with the standalone Compose v5.4.0
  binary because the host Docker CLI is unavailable.

The authoritative Version 0 schema checksums before implementation are:

```text
5fc5d8699a6523b469597cc454ad137c642e84e4a45d21fd61339afc4de0c1ed  build-manifest.schema.json
b1092fc58aab2a981b9ce5f88916abeb13164115bc7440f992e3c46cdd0fe726  endpoint-identity.schema.json
2cc2cb32d576a12723c58aa55317e1fdeda2d10e151676505973e71f81b5285c  evidence-manifest.schema.json
1f9af8a0d6bd7ba47afa4b9d4130918dffd908fdea79ffffbce5b290ab0df3db  forensic-record.schema.json
3fe59861b1e13b50ccdbe9f920810abac59b91842606bd3e5243fcf5fd6cde10  task-envelope.schema.json
b28d3bbc4ae68fe08c8675fa8968cc2d9240b759b5d92c86ba53ea2c83f4289c  task-status-event.schema.json
```

## Version 1: Grammar, Lexer, Parser and Diagnostics

- Status: Complete
- Completed: 2026-09-15
- AST output schema version: 1.0.0
- Next version: Version 2 — Semantic Analysis, Type System and Module Contracts;
  prompt-pack scope is known, awaiting an explicit feature request

Version 1 implements the platform-neutral, syntax-only frontend in
`jocky-language`. It adds the explicit-trivia Pest grammar, iterative lexical
preflight and 256-delimiter guard, owned Serde AST with half-open UTF-8 byte
spans, strict parser, bounded deterministic diagnostic recovery, JSON AST
emission, and the three-command `jockyc` adapter.

### Implemented

- Module/target declarations, typed functions, parameters and return types,
  let/call/if/else/for/return statements, all required literals and generic
  type forms, qualified calls, optional final run, and line/block comments.
- Complete spans for programs, declarations, blocks, types, statements,
  expressions, identifiers, and qualified-name segments. Integer and duration
  magnitudes remain unbounded source text; supported string escapes are decoded.
- Stable `unexpected-token`, `missing-delimiter`,
  `invalid-target-declaration`, `malformed-type`, and
  `duplicate-top-level-run` syntax categories plus typed `resource-limit`.
- Deterministic sorting, exact deduplication, 32-error capping, LF-only human
  rendering, UTF-8/CRLF/tab/control-aware locations, and committed snapshots.
- Exact CLI forms `parse <file>`, `parse <file> --emit-ast json`, and
  `check <file>` with exit statuses 0, 1, and 2 for success, syntax failure,
  and command/input/resource/output failure respectively.
- Nineteen valid source/AST pairs, including six syntax-valid semantic anomaly
  cases; twenty invalid source/stderr pairs; and five runnable examples.
- Language reference, diagnostic reference, architecture, decisions,
  traceability, README, and CI-boundary documentation.

The required compatibility example revealed a feature-contract inconsistency:
`target` is used as a parameter and `list` as a qualified-name segment despite
the draft treating them as globally reserved. ADR-0005 records the narrow
resolution: `target`, `list`, `option`, and `result` are contextual while their
declaration/type-position meanings stay fixed. The feature-local EBNF and AST
schema were updated together. No operational schema under `shared/schemas/`
changed.

### Final verification snapshot

- `cargo fmt --all` and `cargo fmt --all --check`: passed with Rust 1.97.1
  (`rustfmt` 1.9.0; Clippy 0.1.98).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace --locked`: passed, 80 tests total: 75 language
  unit/integration tests and 5 existing shared-contract tests.
- `CARGO_NET_OFFLINE=true cargo test --workspace --locked`: passed with the
  same 80 tests and no network access.
- Dedicated robustness run: 10,000 generated inputs plus long/deep cases
  passed in 0.17 seconds; slowest generated case was 217 microseconds on this
  host, with no panic.
- All five examples passed plain parse, JSON parse, and check: 15 successful
  direct CLI commands. The CLI integration test also validates every invalid
  fixture in all three modes.
- `cargo tree -p jocky-language --edges normal --depth 1`: direct runtime
  dependencies are only Pest/Pest derive and Serde/Serde JSON.
- Controller Ruff format/lint, strict mypy, and all 52 pytest tests passed on
  Python 3.12.13.
- Dashboard Prettier, ESLint, TypeScript, all 18 Vitest tests, and the Vite
  production build passed (30 modules transformed).
- Docker Compose configuration validation passed with standalone Compose
  v5.4.0.

The final authoritative shared-schema SHA-256 values exactly match the
pre-change Version 0 baseline:

```text
5fc5d8699a6523b469597cc454ad137c642e84e4a45d21fd61339afc4de0c1ed  build-manifest.schema.json
b1092fc58aab2a981b9ce5f88916abeb13164115bc7440f992e3c46cdd0fe726  endpoint-identity.schema.json
2cc2cb32d576a12723c58aa55317e1fdeda2d10e151676505973e71f81b5285c  evidence-manifest.schema.json
1f9af8a0d6bd7ba47afa4b9d4130918dffd908fdea79ffffbce5b290ab0df3db  forensic-record.schema.json
3fe59861b1e13b50ccdbe9f920810abac59b91842606bd3e5243fcf5fd6cde10  task-envelope.schema.json
b28d3bbc4ae68fe08c8675fa8968cc2d9240b759b5d92c86ba53ea2c83f4289c  task-status-event.schema.json
```

### Acceptance criteria

- [x] Rust formatting, Clippy, and workspace tests pass.
- [x] Controller formatting, linting, type checking, and pytest pass.
- [x] Dashboard tests and production build pass.
- [x] Compose configuration validates.
- [x] Every required example parses through the CLI.
- [x] Nineteen valid fixtures match exact schema-valid AST goldens.
- [x] Twenty invalid fixtures fail with stable diagnostics and non-zero status.
- [x] Parser robustness, LF/CRLF behavior, and offline execution are tested.
- [x] Core parser performs no OS, file, environment, network, runtime, or
      forensic operation; the CLI reads only its explicit path.
- [x] No semantic/type checking or sensitive execution behavior was added.
- [x] All six Version 0 shared schemas are byte-identical to baseline.

### Known limitations

- Validation was local on Linux; the checked-in Windows and Ubuntu CI jobs were
  not executed on hosted runners in this session.
- Diagnostics recover multiple independent target/type/run and incomplete
  statement islands, but intentionally do not attempt semantic suggestions.
- `check` remains syntax-only. Name resolution, type checking, IR lowering,
  collectors, registration, transport, endpoint execution, and dashboard
  language features are not implemented.
- Docker Compose was configuration-validated only; services were not started.

## Constitution ratification and retrospective review (2026-09-15)

- Constitution status: Ratified, version `1.0.0`; first adoption after Versions
  0 and 1, independent of product and schema version numbers.
- Scope: Governance/template/documentation alignment and a review of prior work.
- Result: No application-source, parser-behavior, schema, or fixture changes were
  required by this review. See [constitution-audit.md](constitution-audit.md)
  for findings, commands, evidence, remaining obligations, and changed files.
- Corrections: Mandatory tests in generated tasks, persistent constitution
  references, Version 1 FR-036/data-model contextual keywords, accurate hosted-CI
  status, and required task/endpoint IDs in operational log guidance.
- Fresh checks: Rust format/Clippy and all 80 workspace tests passed offline;
  controller Ruff/mypy and 52 pytest tests passed; dashboard format/lint/type
  checks, 18 Vitest tests, and build passed; Compose configuration passed using
  the existing standalone binary. Shared-schema hashes still match Version 0.
- Document validation: 14 Markdown files pass formatting/content checks; 16
  local links resolve, 24 SIH rows are preserved, and FR-036 matches the grammar
  and AST schema. Hash checks confirm no source/test/schema changes and no
  changes to unrelated tooling files.
- Remaining limits: Hosted Windows/Ubuntu CI and live Compose services have not
  been exercised here. Most Version 0/1 files still lack a Git checkpoint.
- Workflow: The constitution persists across versions. Use the constitution
  command again only when project rules change, not before each specification.

## Historical Version 1 hand-off

Version 1 and the requested constitution review are complete. The next feature
request is **JOCKY Version 2: Semantic Analysis, Type System and Module Contracts**
from `JOCKY_AI_Agent_Prompt_Pack.md`, beginning with `$speckit-specify`. Reuse the
ratified constitution, review/checkpoint the current worktree, and run Version 2's
required baseline before implementation. Do not start Version 2 until requested.

## Version 2 MVP: Semantic Analysis, Type System and Module Contracts

- Scope: setup, foundations and User Story 1 only (T001–T044).
- Status: MVP complete and locally verified (2026-09-17).
- Full Version 2 acceptance: pending; US2/US3/US4 and release tasks are not done.
- Constitution stays at 1.0.0; no package or operational schema version bump.

### Implemented in this increment

- All 18 named types, exact list/option/result identity, pure name resolution,
  lexical scopes, argument/return checks, bool conditions, list-only loops and
  one `(endpoint) -> forensic_result` entry. No implicit constructors or coercions.
- Source-located errors, unreachable/unused warnings, deterministic separate
  32/32 caps, and typed resource failures with no partial checked program.
- A validated, bounded, versioned registry with ten unavailable read-only
  planned contracts. No collector or callable runtime stub exists.
- Target/lab restrictions on every direct call, plus complete internal capability,
  privilege, platform and unavailable-dependency metadata under recursion.
  This metadata is computed but its public inspection story is still pending.
- Contextual `profile lab`, original spans, separate profile AST version 2.0.0,
  unchanged ordinary AST 1.0.0 and legacy syntax-only library entry points.
- `jockyc check <file>` and explicit human format now check syntax and semantics.
  Both parse modes stay syntax-only. Errors exit 1, warnings-only checks exit 0,
  and host/resource failures exit 2. Output and input buffers are bounded.
- Four teaching examples updated; triage and all original parser fixtures/tests
  remain unchanged. No controller, dashboard, IR or adapter feature was added.

### Verification and limitations

Local gates passed: Rustfmt/Clippy and **144 Rust tests** (121 language, 13 forensic,
10 shared), controller Ruff/mypy and **52 pytest tests**, dashboard format/lint/
typecheck, **18 tests** and production build. All five examples parse and check.

See [version-2-verification.md](version-2-verification.md) for the commands,
file manifest and acceptance checklist. New evidence covers 27 distinct
syntax-valid semantic negatives, valid type/scope programs, binary snapshots,
resource boundaries, registry schema cases and the old parser regressions.
SHA-256 comparisons preserve 94 protected files, including the six operational
schemas, original parser corpus, triage, original CLI test and security boundary.

Controller and dashboard retain only their Version 0 health/status behavior.
There is no code generation, interpreter, collection, endpoint contact or
sensitive execution. Privileges/profile declarations never grant permissions.
JSON check reports and module discovery are not available in the MVP.
Docker is unavailable here, so current Compose configuration validation is
pending. Hosted Windows/Ubuntu CI has not run. The legacy parser schema loader
still requires an ignored local schema path; clean-checkout preparation/CI
integration is a remaining Version 2 release task, not silently bypassed.

### Exact next hand-off

Continue **Version 2 User Story 2: Know targets and permissions**, tasks
**T045–T053**, using the existing specification/plan and ratified constitution.
Expose and verify the already-computed metadata without adding execution.
Suggested request: `$speckit-implement US2 only (T045–T053); stop after US2`.
Then US3/US4 and release gates remain. Do not start Version 3 yet.

## Version 2 Phases 4 and 5: Metadata and module discovery

- Scope: US2/Phase 4 (T045–T053) and US3/Phase 5 (T054–T061), requested together.
- Status: both phases complete and locally verified on 2026-09-17.
- Completed through T061; Phase 6 (T062–T077) and Phase 7 remain pending.
- This is not full Version 2 acceptance. Constitution remains 1.0.0.

### Added capabilities

- Read-only Rust access to complete per-function and run-entry metadata,
  source-selected targets, and explicit lab-profile declaration spans.
- Canonical capability union, strongest privilege, supported-platform
  intersection and sorted unavailable dependencies, including recursive/dead
  calls. Uncalled helpers are checked but excluded from unrelated entry summaries.
- Exhaustive platform/lab fixture matrices, mixed privilege metadata, deep and
  shared call graphs, registry permutation, and source/AST preservation evidence.
- `jockyc modules list` and `jockyc modules describe <qualified-name>` show all ten
  embedded planned contracts and every field. All implementations remain unavailable.
  Discovery works without a source file or working-directory catalogue. Typed
  lookup/registry/write/flush failures exit 2, never a fake successful collection.
- Three new Windows/Ubuntu/cross-platform examples and the module/platform guides.
  The event examples intentionally have uncalled timestamp-parameter helpers.

### Fresh verification

Rustfmt and offline locked Clippy passed. The workspace passed **176 Rust
unit/integration tests** (150 language, 16 forensic, 10 shared), plus one
compile-fail documentation test proving metadata fields cannot be mutated.
Controller Ruff/mypy and **52 pytest tests** passed; dashboard formatting,
linting, type checks, **18 tests** and production build passed.

All eight distributed examples pass parse, JSON AST, default check and explicit
human check. The original parser fixtures/tests, operational schemas, deployed
compiler schemas, bundled catalogue and security boundary are unchanged.
The 94-file original preservation manifest still matches. No dependency,
controller/dashboard code, CI, Git exclusion or unrelated work changed.
See [version-2-verification.md](version-2-verification.md) for the command ledger,
file list and detailed acceptance evidence.

### Remaining limitations and exact next hand-off

No execution, IR, code generation, collector, elevation, endpoint contact or
sensitive adapter was added. Lab profiles and privileges are descriptive only.
Human checking does not print metadata; the public Rust accessors expose it.
JSON check reports are not implemented yet. Current Compose validation is
pending because Docker is absent; hosted Windows/Ubuntu CI was not run. The
legacy parser-test schema preparation and clean-checkout validation remain
Phase 7 tasks (T081/T082/T089), not bypassed here.

Next request: **Version 2 Phase 6 — Predictable reports and parser compatibility,
T062–T077 only**. Suggested prompt:
`$speckit-implement Phase 6 only (T062–T077); stop before Phase 7`.
Reuse the current specification/plan/tasks and constitution; do not start
Version 3. No commit or push has been performed.

## Version 2 complete: Semantic Analysis, Type System and Module Contracts

- Status: complete and locally verified on 2026-09-17; T001–T093 complete.
- Final increment: Phases 6/7, T062–T093, following the verified MVP and Phases 4/5.
- Constitution: 1.0.0. Operational schemas and package versions are unchanged.
- Next product version: **Version 3: Intermediate Representation and Safe Reference Interpreter**.

### Available now

The frontend parses and statically checks JOCKY programs without executing them.
It resolves names/scopes, checks all 18 named types and three generic forms,
arguments/returns, boolean conditions, collection loops and exactly one valid
run entry. All bodies are checked, including unreachable and uncalled code.
Warnings identify unused bindings and unreachable statements.

Target and lab-profile rules use a validated, inert registry. All ten planned
forensic contracts remain unavailable; none is an implemented collector.
Read-only per-function and entry metadata includes transitive capabilities,
strongest privilege, supported-platform intersection and unavailable dependencies.
Profiles and privileges are descriptive, never permission grants.

All seven CLI forms work: plain/JSON-AST parse, default/human/JSON check, module
list and module describe. Check reports use schema version 1.0.0, ordered
source-located diagnostics and complete metadata only on success. Exit codes
are 0 for valid (including warnings), 1 for source rejection and 2 for
host/resource/output failure. Host failures do not invent source locations.
Input is bounded to 4 MiB and serialized reports to 8 MiB; partial writes cannot
claim successful delivery. Legacy parser/library contracts remain syntax-only.

The fixed-path, offline compatibility tool prepares the unchanged legacy AST
schema when absent, no-ops when identical and refuses differing contents.
Both Rust CI jobs run this step; Python CI covers the tool separately. Controller
and dashboard retain only their Version 0 health/version and status behavior.

### Final verification

All applicable local release gates pass. Exact commands, file changes and
requirement mapping are in [version-2-verification.md](version-2-verification.md).

| Gate                                                         | Observed result                                                                                                                  |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| Rustfmt, locked offline Clippy and workspace tests           | Pass: 203 unit/integration tests (177 language, 16 forensic, 10 shared), plus 1 compile-fail doctest                             |
| Controller Ruff format/lint, mypy and pytest                 | Pass: 19 typed files; 52 tests                                                                                                   |
| Compatibility-tool Ruff format/lint, strict mypy and pytest  | Pass: 2 typed files; 11 tests                                                                                                    |
| Dashboard format/lint/typecheck, Vitest and production build | Pass: 18 tests in 2 files; 30 build modules                                                                                      |
| Compose configuration                                        | Pass: official checksum-verified standalone Compose v5.5.0; host Docker launcher absent                                          |
| Direct example/module smoke checks                           | Pass: 53 commands; 16 emitted JSON documents schema-validated; slowest example command 401 ms                                    |
| Repository-local clean-copy setup                            | Pass: 476 deployable files, no planning files initially; preparation copy/no-op; 203 Rust tests plus 1 doctest and 11 tool tests |

The new full-checker robustness campaign covers 10,000 arbitrary UTF-8 inputs,
256 structured programs and three adversarial cases. A focused run completed
in 569 ms with a 44 ms worst case, within the test budgets. Twenty-five frozen
human/JSON diagnostic pairs are compared over ten repeated renders. These are
finite verification results, not exhaustive proofs.

All 94 originally protected files retain their hashes, including the original
19 AST goldens, 20 invalid parser snapshots, triage example and six operational
schemas. Deployed compiler schemas, catalogue, security boundary and Git
exclusions are also unchanged in the final increment. Earlier uncommitted work
is preserved; no staging, commit or push was performed.

### Acceptance and limitations

- [x] Valid cross-platform and platform-specific examples parse, resolve and type-check.
- [x] At least 20 semantic-negative programs reject for source-located, stable reasons.
- [x] Both platform-mismatch directions, strict registry validation and privilege/lab rules are tested.
- [x] Human/JSON diagnostics, warnings, metadata and CLI exit/output failures are covered.
- [x] Original parser suites and all Version 0 application tests still pass.
- [x] Offline analysis performs no collection, endpoint contact, program execution or elevation.
- [x] README setup, schema preparation, CI definitions and required documents are updated.

Hosted Windows/Ubuntu CI was not run or observed; verification used local Fedora
with Rust/Cargo 1.98.1, Python 3.12.13 and Node 22.22.2. The declared Rust 1.85
minimum was not independently tested. The clean-copy test uses cached dependencies,
not a fresh OS installation. Compose was configuration-validated only; no daemon,
containers or live services were started. There is no IR, interpreter, code
generation, collector, endpoint registration, task transport or sensitive adapter.

### Exact next hand-off

Version 2 is complete. The next request is **JOCKY Version 3: Intermediate
Representation and Safe Reference Interpreter** from the prompt pack, beginning
with `$speckit-specify` and the Version 3 prompt. Reuse constitution 1.0.0, inspect
the current state and decisions, and rerun the recorded baseline before changes.
Preserve the parser/semantic/report contracts and unavailable-only registry.
Stop here until that request; do not infer authority to execute forensic operations,
commit or push.

## Version 3 MVP: verified intermediate representation and build-ir

- Status: complete within the requested MVP scope, locally verified 2026-09-19.
- Tasks: T001–T045 complete; T046–T100 remain open. **Full Version 3 is not complete.**
- Constitution: unchanged at 1.0.0. Operational and existing compiler schemas,
  unavailable catalogue, original examples and parser/report fixtures are preserved.

### Available now

`jockyc build-ir <file> --output <file.json>` checks source, lowers it to typed
IR, performs complete verification and publishes a new deterministic JSON file.
IR contains exact constants, typed slots, calls, branches, collection loops,
returns, original spans, stable UUIDv8 instruction IDs and static requirements.
It is an inspectable program description, not executable target code or evidence.
The registry snapshot captured during analysis cannot be rebound during lowering.

The pure IR library strictly decodes, verifies and serializes supplied memory.
All verification passes are required before immutable VerifiedProgram success:
schema/version, identity/reference/ownership, exact types, initialization/returns,
registry provenance and recursive metadata. Invalid or excessive input fails typed.
Deep programmatically constructed rejected values/types are released iteratively.

CLI file adapters bound reads and use retained no-follow directory handles.
They reject links/reparse paths, special files, parent traversal and existing
outputs; publication uses a complete staged buffer and an exclusive hard link.
Warnings must be delivered before publication. No old command reader is changed.

All ten forensic contracts remain unavailable. No provider, interpreter,
collector, endpoint contact, elevation, code generation or sensitive adapter was
added. `verify-ir` and `run-fixture` are not exposed. Controller/dashboard remain
their health/version/status foundations.

### Verification

See [Version 3 verification](version-3-verification.md) for commands, files,
red/green evidence, artifact review and remaining limitations.

- Rustfmt and locked offline workspace Clippy: pass.
- Locked offline workspace suite: **264 unit/integration tests + 3 compile-fail
  doctests pass**, retaining every previous test.
- Twenty reviewed source/IR goldens: ten-repeat deterministic builds and
  decode/reverify round trips pass. All eight unchanged examples build through
  the actual CLI. Triage has 10 instructions; no execution result is claimed.
- Controller Ruff/mypy and 52 pytest tests, setup-tool Ruff/mypy and 11 tests,
  dashboard format/lint/type checks, 18 Vitest tests and production build: pass.
- Compose configuration: pass with the existing standalone binary. No containers
  or live services were started. Windows/Ubuntu hosted CI is not an observed pass.

### Limitations and exact next hand-off

Source/IR verification does not prove termination or authenticate source. The
local adapter assumes user-controlled directories and hard-link support; it is
not an OS sandbox or disk deadline. Cleanup failure may leave owned staging or
a complete final artifact and is reported as failure. Windows adapter/junction
tests exist but were not run on this Linux host.

The dependency audit found an older MSRV mismatch: Cargo.toml declares 1.85,
but pre-existing locked ICU packages require 1.88 (idna_adapter requires 1.86).
Checks passed on installed Rust 1.98.1; no minimum-toolchain pass is claimed.
No unrelated dependency, constitution, Git history or presentation file was changed.

Next request: **`$speckit-implement rest` for Version 3**, beginning with
**Phase 4 / User Story 2, T046–T053** (standalone verification and its negative
artifact corpus), followed by the remaining fixture interpreter and release
tasks through T100. Preserve this MVP and rerun its baseline. **Do not start
Version 4** until all Version 3 acceptance criteria are actually met and the
user explicitly requests the next product version. No commit or push was made.

## Version 3 Phases 4–5: standalone verifier and synthetic interpreter

- Status: complete within the requested next-two-phase scope, locally verified
  2026-09-20. T046–T074 complete; T075–T100 remain open.
- Full Version 3 is **not** complete. No Phase 6/7 or Version 4 work was started.
- Constitution 1.0.0, deployed schemas, unavailable catalogue, old examples and
  parser/AST/report goldens remain unchanged. No dependencies were added.

### Current capabilities

`jockyc verify-ir <file.json>` independently checks a bounded supplied artifact.
It reports deterministic codes, JSON pointers, instruction IDs and optional byte
spans, never opening embedded source labels or inventing source snippets.
Success is silent; malformed supported IR exits 1; version/resource/host errors
exit 2. Thirty-six frozen malformed artifacts reject for their expected reasons.

`jockyc run-fixture <source.jky> --fixture <directory>` checks, lowers and verifies
source, loads only the directory's `fixture.json`, and executes once against one
synthetic endpoint. Four exact-signature data mocks provide system profile,
processes, connections and indicator correlation. Whole-bundle validation covers
unused outcomes, context, references, declared errors and collection limits.
Unmocked calls return declared Unsupported/NotImplemented; there is no real
collector, callback, host fallback or sensitive operation. Module discovery
remains unavailable-only; parse/check/build/verify remain non-executing commands.

All six IR operations execute with explicit frames/cursors and immutable value
handles. Instruction/work/collection/call/control/value/type/storage/output guards
apply from preparation onward. Ordinary option/result values remain data.
Errors preserve the original provider instruction/span and stop execution.
Consistent bounded reports separate success from failure; output overflow cannot
leave a success exit status, and counters are never silently repaired.

Both `examples/fixtures/triage/{ubuntu,windows}` demonstrations return the authored
forensic result. Each executes 10 instructions. Reviewed actual reports record
work 1,629/1,649 and peak logical storage 67,975/67,990 bytes respectively.
These counters are logical accounting, not process RSS or real collection costs.
The fixed observation time and indicator come from data. Controller/dashboard
and runtime/codegen/transport boundaries have no new features.

### Verification and limitations

See [Phases 4–5 evidence](version-3-phases-4-5-verification.md) for exact commands,
changed files, corrections, preservation and acceptance mapping.

- Rustfmt, locked offline Clippy and workspace suite pass: **313 unit/integration
  tests plus 5 compile-fail doctests**, retaining Version 0–2 and MVP coverage.
- All 20 golden source programs have expected fixture behavior, including typed
  unsupported and recursion failures. New tests cover argument/collection order,
  fresh calls, selected branches, empty loops, early returns, generic transport,
  original error locations, limits and stream failures.
- Controller/tool Ruff and mypy pass; pytest passes 52 + 11 tests.
- Dashboard format/lint/type checks, 18 tests and production build pass.
- Compose configuration passes; no services or lab endpoints were contacted.

Phase 6's expanded boundary/stress/determinism campaigns and Phase 7's full
documentation/release/clean-copy closure remain pending. Local evidence is Fedora
Linux, not an observed Windows/Ubuntu hosted pass. The pre-existing Rust 1.85
MSRV declaration conflicts with locked dependencies; stable 1.98.1 was tested.
CLI inputs assume user-controlled directories; no OS sandbox or I/O deadline is
claimed. Synthetic data does not establish detection accuracy or real forensic
collection. No Git staging, commit or push was performed.

### Exact next hand-off

Request **`$speckit-implement Phase 6 (T075–T085) for Version 3 only; stop before
Phase 7`**. Inspect this checkpoint/ADR-0011, rerun the baseline and preserve
T001–T074. Alternatively request the remaining Version 3 work explicitly, but
do not start Version 4 until Version 3 is genuinely complete and separately
authorised. Stop here and wait for the next prompt.

## Version 3 complete — 2026-09-20

All 100 Version 3 tasks are complete under constitution 1.0.0. The final request
completed T075–T100 only, preserving the previously working frontend, IR, verifier
and guarded fixture interpreter. See [full release evidence](version-3-verification.md),
[IR specification](ir-specification.md) and [reference interpreter](reference-interpreter.md).

### Current capabilities

- Version 1 parsing/AST/diagnostics and Version 2 semantic checking/contracts remain.
- `jockyc build-ir <source> --output <new.json>` lowers checked source to typed,
  deterministic, source-located IR and fully verifies it before publication.
- `jockyc verify-ir <file.json>` independently validates IR without opening source
  labels or invoking any provider. Malformed/version/resource failures are typed.
- `jockyc run-fixture <source> --fixture <directory>` executes verified IR once
  against one synthetic endpoint using only four exact-signature fixture mocks.
- Explicit frames, immutable handles and fixed instruction/work/collection/depth/
  node/storage/output limits bound execution. Errors propagate original UUID/spans
  without partial success. Ordinary generic result/option values remain data.
- Both unchanged triage fixtures return their exact authored forensic_result.
  Repeated builds/runs are byte-identical. Real catalogue entries remain unavailable.
- CLI file boundaries reject traversal, symlinks/reparse points, special files,
  overwrite/aliasing and unsafe publication; failed delivery never means success.

### Final verification

Local Fedora/Linux, Rust 1.98.1, Python 3.12.13, Node 22.22.2:

- Rustfmt and locked offline Clippy pass; **342 unit/integration tests plus five
  compile-fail doctests** pass (77 IR, 239 language, 16 forensic-contract, 10 shared).
- Controller/tool Ruff and mypy pass; **52 + 11 pytest tests** pass.
- Dashboard format/lint/type checks, **18 tests** and production build pass.
- Standalone Compose 5.5.0 validates `.env.example` configuration; no services start.
- Twenty IR goldens, 36 malformed artifacts, all eight example CLI builds,
  ten-repeat builds/runs and three separate 10,000-case IR campaigns pass.
- A fresh repository-local copy without planning files builds and passes the full
  **342 + 5** Rust suite and both demos. The documented utility prepares only the
  legacy AST schema. A socket-test long-path issue was fixed in the source test,
  then verified in a second fresh copy, not hidden by patching copied artifacts.
- Protected schemas/catalogue, parser/AST/report/IR fixtures, all examples,
  controller/dashboard code, dependencies and unrelated user changes are intact.

### Remaining limitations and exact next hand-off

Only fixture simulation exists: no real collectors, generated packages, endpoint
registration/execution, task transport, elevation or sensitive adapter. Controller
and dashboard still provide health/status only. Synthetic data proves data flow,
not detection accuracy. File safety assumes user-controlled local directories;
it is not an OS sandbox, hostile-filesystem guarantee or wall-clock I/O deadline.

Windows/Ubuntu hosted CI remains unobserved; native Windows tests are configured,
not locally executed. The old Rust 1.85 MSRV declaration conflicts with existing
locked dependencies and is not verified; use tested current stable. No container
deployment or lab endpoint contact occurred. No Git staging, commit or push occurred.

Historical release hand-off (superseded by the amendment below): use
**`$speckit-specify` with Version 4: Windows Read-Only Forensic
Collector Library** from `JOCKY_AI_Agent_Prompt_Pack.md`. First inspect this release,
ADR-0012, schemas and test evidence; preserve Version 0–3 behavior and the security
boundary. **Stop here; Version 4 requires a separate explicit prompt.**

## Linux-first governance amendment — 2026-09-21

- Constitution: **1.1.0**, original ratification date retained. See ADR-0013 and
  [the approved roadmap](linux-first-roadmap.md).
- Product remains Version 3, complete. No source, schemas, dependencies, examples,
  fixture data or CI jobs are changed by this amendment; no live collection occurs.
- Version 4's Windows collector work is deferred into new Version 16, which also
  owns Windows packaging, runtime and end-to-end validation. Windows contracts and
  existing compiler/fixture regressions remain intact.
- Revised Version 5 now owns shared collector records/interfaces/policies,
  versioned Linux vocabulary migration and portable read-only Linux collection.
- Initial planned evidence matrix: pinned x86_64/glibc Debian, Ubuntu, Fedora,
  a named RHEL-compatible distribution and Arch VM configurations. None is claimed
  live-collector-tested yet. Missing facilities must yield typed unsupported errors.
- First useful checkpoint: a separately authorised standalone Linux lab report
  for system/process/network/scoped-file facts, independently checked against benign
  ground truth. It is not full Version 5 acceptance or JOCKY package execution.
- Versions 6–15 deliver the Linux workflow; no manual Windows host work is required.
- Pure parse/check/IR/fixture behavior and all security prohibitions are preserved.
  The development workstation is not an implicitly authorised collection endpoint.

Documentation, preservation and regression results are in
[the governance verification record](linux-first-governance-verification.md).
Existing hosted-CI and Rust minimum-version limitations remain unresolved; this
roadmap change does not certify another distribution or fix the toolchain mismatch.

Exact next request: **`$speckit-specify` with Version 5: Portable Linux Read-Only
Forensic Collector Library** from the revised prompt pack. Read constitution 1.1.0,
ADR-0013, the Version 3 evidence and Linux-first roadmap first. Stop after this
governance amendment; specification/implementation requires the next explicit prompt.
