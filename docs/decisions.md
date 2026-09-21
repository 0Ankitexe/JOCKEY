# Architecture decisions

Decisions are append-only. A superseding decision must reference the decision
it replaces and describe contract migration.

## ADR-0001: Fixed implementation stack

- Status: Accepted
- Date: 2026-08-30
- Requirements: SIH-01, SIH-02, SIH-08, SIH-09, SIH-24

Use a Rust stable workspace for the language, IR, transforms, code generation,
forensic library, shared types, and endpoint runtime. Pest is the parser
technology; Serde/Serde JSON provide typed serialization; Tokio provides the
asynchronous Rust runtime. Use Python 3.12 with FastAPI, Pydantic, SQLAlchemy,
Alembic, PostgreSQL, and Redis for central management. Use React, TypeScript,
Vite, Vitest, and React Testing Library for the dashboard. Use Docker Compose
for local services and GitHub Actions-compatible Windows and Ubuntu CI.

This fixes integration expectations before feature implementation begins.

## ADR-0002: Schema-first cross-language integration

- Status: Accepted
- Date: 2026-08-30
- Requirements: SIH-02, SIH-03, SIH-08, SIH-09, SIH-24

The Draft 2020-12 JSON Schema files under `shared/schemas/` are authoritative
for messages crossing language or process boundaries. Contracts use explicit
versions, closed objects, exact enums, canonical UUID strings, and RFC 3339 UTC
timestamps ending in `Z`. Rust and Python validate the same fixture corpus.

Open-ended data is allowed only in fields that are explicitly intended to carry
typed function parameters or a collector-specific payload. Contract changes
require compatible versioning, fixtures, tests, traceability, and a migration
note.

## ADR-0003: Separate platform adapters

- Status: Accepted
- Date: 2026-08-30
- Requirements: SIH-02, SIH-03, SIH-07, SIH-22, SIH-24

Cross-platform code depends on explicit adapter interfaces. Windows and Ubuntu
implementations remain separate and are selected at a narrow boundary. No
platform-specific operating-system import belongs in shared language, IR,
controller, or message-contract modules.

This keeps platform behavior testable and prevents one target's assumptions
from leaking into another target.

## ADR-0004: Sensitive adapters are disabled and auditable

- Status: Accepted
- Date: 2026-08-30
- Requirements: SIH-04, SIH-07, SIH-10, SIH-11, SIH-13 through SIH-23

High-risk requirement names remain traceable, but operational bypass or abuse
code is outside the project boundary. Future safe interfaces must default to a
typed unsupported result and, where applicable, require an explicit auditable
lab gate. Permitted implementations are limited to detection-only inventory,
same-process simulation, loopback-only tests, and external lab-evidence records
as detailed in `docs/security-boundary.md`.

## ADR-0005: Strict, pure, schema-constrained Version 1 frontend

- Status: Accepted
- Date: 2026-09-15
- Requirements: SIH-01, SIH-03, SIH-12

Implement Version 1 in `jocky-language` with an explicit-trivia, SOI/EOI Pest
grammar; an owned Serde AST with zero-based half-open UTF-8 byte spans; a local
iterative delimiter guard; and project-owned deterministic diagnostics. A
strict successful Pest parse is the sole source of an AST. Recovery runs only
after failure and can never return partial success. The core accepts borrowed
text plus a filename label and performs no file, environment, OS, network,
runtime, or forensic operation. `jockyc` is a separate thin adapter limited to
the supplied file and standard streams.

AST JSON is wrapped with schema version `1.0.0`, uses ordered structs/vectors,
pretty serialization, and exactly one final LF. The feature-local schema in
`specs/001-language-parser-diagnostics/contracts/` governs this compiler
output; the six cross-process schemas in `shared/schemas/` remain byte-identical.

During strict compatibility testing, the required program exposed a conflict
in the draft feature contract: it used `target` as a parameter and `list` as a
qualified call segment while both were globally reserved. The accepted narrow
resolution makes `target`, `list`, `option`, and `result` contextual. Their
declaration/type meanings remain fixed in those grammar positions, while they
are permitted as ordinary identifiers elsewhere. The feature EBNF and AST JSON
schema were corrected together; no shared operational schema changed.

## ADR-0006: Ratify the constitution and review existing versions

- Status: Accepted
- Date: 2026-09-15
- Requirements: SIH-01 through SIH-24 (project governance; no capability upgrade)

Ratify `.specify/memory/constitution.md` as version `1.0.0` using the user's
existing working rules, fixed stack, quality requirements, and security boundary.
The prior file was a template without an adopted version. Ratification is dated
today and does not change the completion dates of Versions 0 and 1, the package
versions, or the shared/AST schema versions.

The constitution persists across feature specifications and implementations;
starting Version 2 does not require ratification again. Amend it only when the
user changes project rules. Propagate amendments to planning/specification/task
templates and contributor guidance. New public behavior always requires unit,
integration, and negative tests, including when generic skill text calls tests
optional. See `docs/constitution-audit.md` for the retrospective review.

Align Version 1's FR-036 and data-model prose with the already accepted
contextual-keyword decision in ADR-0005. Correct the plan's hosted-platform
verification claim and strengthen operational log wording to require associated
task/endpoint IDs. These are documentation corrections; ADR-0001 through
ADR-0005 remain accepted and no source, public behavior, fixture, or schema
migration is required by this review.

Project-wide HTTPS/WebSocket/OpenAPI and Compose dashboard requirements remain
explicit architecture obligations delivered by their scoped versions. Version 0
specifically requested three Compose services; Version 1 is syntax-only. This
governance change does not add later operational infrastructure or start Version 2.

## ADR-0007: Version 2 MVP is static semantic analysis, not execution

- Status: Accepted for the MVP increment (T001–T044)
- Date: 2026-09-17
- Requirements: SIH-01, SIH-02, SIH-03, SIH-12; metadata-only SIH-18/SIH-24

Preserve the Version 1 AST, library parse/check APIs, parser goldens and six
operational schemas. Revise only CLI checking to validate names, exact types,
scopes, calls, returns, bool/list flow and one valid run entry. Source spans
remain original half-open UTF-8 offsets; resolved types/references/calls live in
compiler-local side tables. Warnings do not fail otherwise valid checks, and
resource failures discard partial diagnostics and metadata.

Deploy separate, byte-identical compiler contract schemas under
`shared/compiler-contracts/`. A closed, bounded, immutable registry rejects
duplicate keys and identities, unknown fields, contradictory metadata and
unknown schema retrieval. The ten planned contracts are unavailable-only,
with no invocation interface. The dependency direction is language -> forensic
contracts -> shared vocabulary, without runtime/native dependencies or new
third-party versions. The composition helper is one explicit unavailable
signature, never an implicit record constructor.

Add contextual `profile lab` through a separate parser entry and syntax wrapper;
do not mask source text or widen legacy acceptance. Profile-bearing AST JSON is
`2.0.0`; other sources preserve `1.0.0`. Profile opt-in and user/elevated/lab_only
privileges are metadata, not permission grants. Check every direct restricted
call, including unused/dead helpers. Iterative SCC processing with bounded
dependency bitsets computes complete metadata before success; names are shared
to avoid repeating large prefixes. No execution, collection or elevation occurs.

The user requested the task plan's MVP, not the whole Version 2 release. Deliver
human checking and preserve parse modes now. US2 metadata inspection, US3 module
commands, US4 JSON reports, and release tasks remain unimplemented. In particular,
the unchanged parser-test loader's ignored legacy schema path still needs the
planned clean-checkout preparation step (T081/T082); do not silently change the
old tests, Git exclusions, CI or schema bytes to conceal it. See
[MVP verification](version-2-verification.md) for evidence and remaining gates.

## ADR-0008: Read-only metadata and embedded module discovery

- Status: Accepted for Version 2 Phases 4/5 (T045–T061)
- Date: 2026-09-17
- Requirements: SIH-01, SIH-02, SIH-03, SIH-12; metadata-only SIH-18/SIH-24

Expose the complete metadata already computed under ADR-0007 using immutable
CheckedProgram/FunctionMetadata accessors. Preserve declaration order, exact
run-entry identity, canonical metadata ordering, original source/profile spans,
and value equality independently of registry input order. Store names with
shared prefixes; format a full name on demand. No unchecked public success
constructor, mutable metadata field, collector or executor is introduced.

Selected targets remain separate from inferred support. All call sites, including
dead statements and uncalled helpers, retain platform/lab checks; unrelated
helpers do not inflate entry requirements. Recursion is analyzed, never executed.
The Windows/Ubuntu event examples pass opaque timestamps through uncalled helpers
and explicitly explain this entry-summary distinction.

Implement module list/describe only over the validated embedded catalogue.
Dispatch before source I/O, retain the fallible cache, and expose every contract
field in stable text order. Unknown/malformed names map to module-not-found;
bad registry data, output writes and flush failures cannot report success.
Non-UTF-8 module arguments map to InvalidCommand/usage without lossy conversion.
Synthetic registries remain in analyzer tests/private adapter seams, never an
external-registry option. All ten contracts remain unavailable and unchanged.

CLI API migration: Command gains module variants and Command::path returns
Option<&str>; callers must handle None for commands without a source path.
Both parse modes stay registry-independent. Only the prior CLI-specific assertions
that rejected modules list are revised. Original parser tests/fixtures and all
schemas are byte-preserved; no schema inconsistency or migration was needed.

This fulfills the user's two-phase request, not the remaining report/release
work. Phase 6 JSON checking and Phase 7 preparation/CI/clean-checkout gates remain
pending. ADR-0001–0007 remain intact; no constitution amendment, execution authority,
dependency installation, Git commit or push follows from this increment.

## ADR-0009: Complete Version 2 reports and reproducible offline contracts

- Status: Accepted for Version 2 (T062–T093)
- Date: 2026-09-17
- Requirements: SIH-01/02/03/12; metadata-only SIH-18/SIH-24

Complete the staged CLI-check revision in ADR-0007/0008: all seven command
forms now work, including explicit human/JSON checking. Legacy library
parse/check remain syntax-only; profile-aware parsing preserves contextual
keywords and AST 1.0.0/2.0.0 output. This does not broaden execution authority.

The deployed compiler schemas remain unchanged and separate from the six
operational schemas. Registry/report schema version 1.0.0 is independent of
product version. Registry validation remains strict, bounded and offline;
all ten bundled forensic implementations and the compiler composition helper
remain unavailable. Privileges, profiles and capabilities describe static
requirements only. There is no collector or invocation API.

Construct successful CheckReport values only from completed CheckedProgram
values. Retain one bounded original-source copy in CheckedProgram so an unrelated
source/report pair is rejected, not presented as valid. Borrow large names and
snippets during serialization. Fixed-order JSON includes a final LF within its
8 MiB budget; input bytes remain limited to 4 MiB. Resource errors discard partial
diagnostics/metadata. Host errors have no invented source location. Writer/flush
failure cannot claim delivery and never triggers a second stdout document.

The unchanged V1 test loader still expects an excluded local schema path.
Resolve this deployment issue with a Python-3.12-compatible, stdlib-only,
fixed-path preparation utility that copies the deployed byte-identical schema
only when absent, no-ops when identical and refuses different contents. It
accepts no path/force/URL flags or environment overrides, rejects symlink
components and uses exclusive creation. New tests reference deployable contracts
directly. Run preparation before Rust checks in both platform CI jobs.

This supersedes the pending-report/preparation milestones in ADR-0007/0008,
not their security or compatibility decisions. The only previous CLI assertion
changed now was JSON rejection (replaced with unsupported XML). Original parser
tests, fixtures, triage source, operational/compiler schemas and catalogue bytes
remain unchanged; no actual schema inconsistency or migration was necessary.

Local regression, configuration validation and clean-checkout simulation are
separate evidence from hosted CI or live service operation. Preserve that
distinction in the hand-off. No constitution amendment, Git commit/push or
Version 3 implementation is part of completing this request.

## ADR-0010: Verified IR and exclusive local artifact publication

- Status: Accepted for Version 3 MVP (T001–T045 only)
- Date: 2026-09-19
- Requirements: SIH-12/SIH-13 compiler infrastructure only; no evasion capability

Keep source/semantic/operational contracts and the ten unavailable descriptors
unchanged. Deploy four additional compiler-only wire contracts: IR, fixture
values, fixture bundles and fixture reports. Their schema versions are independent
of product version. Bundle/report examples are shape tests, not execution evidence.

Retain exact original source/label and a cloned immutable validated registry in
CheckedProgram. Add bounded structured lookup views rather than reparsing displayed
types or resolving names again. Lower only this private-constructor success type,
using iterative traversal, exact decimal strings, explicit typed slots and owned
structured regions. Preserve every statement, including dead/uncalled code.

Use software-only SHA-256, explicit seed and length-framed header/position inputs
for UUIDv8 instruction IDs. Full-registry fingerprints bind complete descriptors;
they detect inconsistency, not source authenticity. Full verification checks schema,
identity, references, types, initialization, return flow, registry and SCC metadata
before private immutable VerifiedProgram success. Bounded JSON and typed failures
are mandatory for decoded and programmatically supplied data alike.

Only CLI adapters receive file capabilities. Pin cap-std/cap-fs-ext 4.0.3 and
sha2 0.10.9 with reviewed features. Walk retained no-follow directory handles,
bound UTF-8 reads, reject non-regular/reparse paths, serialize completely and flush
warnings before staging. Publish a new final name with a same-parent hard link;
never overwrite, rename as fallback, or delete final output on failure. Cleanup
failure is still failure, even if a complete final file exists. This assumes
user-controlled local directories; it is not an OS sandbox or crash-proof transaction.

Add only build-ir to the CLI in this increment. The verifier is a library API;
verify-ir, execution/providers, fixture reports and final Version 3 release tasks
remain pending. No parser, AST, old report/schema golden or catalogue migration
is needed. Existing CLI usage simply gains one line. No constitution amendment,
collector, code generation, sensitive adapter, Git commit or push is implied.

The dependency audit found the pre-existing 1.85 MSRV declaration conflicts with
locked ICU 2.3 (minimum 1.88) and idna_adapter 1.2.2 (minimum 1.86). Preserve unrelated
locked dependencies and record the mismatch; current stable is the supported
local setup for this checkpoint, not a newly verified minimum. Hosted Windows
and Ubuntu runs remain unobserved. See [MVP evidence](version-3-verification.md).

## ADR-0011: Standalone verification and bounded fixture-only interpretation

- Status: Accepted for Version 3 Phases 4–5 (T046–T074 only)
- Date: 2026-09-20
- Requirements: SIH-12/SIH-13 compiler infrastructure only; no evasion capability

Expose the MVP verifier through `verify-ir`, with bounded explicit artifact reads
and deterministic pointer/UUID/optional-byte-span diagnostics. An embedded source
label is display data, never a file to open. Supported malformed IR fails with
exit 1; unavailable versions, resource limits and host/delivery errors exit 2.
This advances the standalone-command milestone in ADR-0010 without changing its
verification or file-publication rules.

Add only four closed, data-backed mocks inside `ir/`, separate from the unavailable
forensic registry and endpoint runtime. Match complete canonical descriptors,
including signatures, versions and restrictions; changed, unmocked or lab-only
descriptors never select a mock. Reached unavailable calls use Unsupported first,
then NotImplemented only if declared. No callback, plugin, native fallback,
retry, endpoint contact or elevation interface is introduced. All supplied
provider outcomes, including unused ones, must validate before preparation.

Only verified IR and a privately validated one-endpoint fixture can produce a
PreparedInvocation. Enforce finite instruction, collection, work, value/type,
call/control, storage and output guards from the first callable executor.
Function frames and region/loop cursors are explicit stacks; values travel as
immutable arena handles. Loop progress charges N+2 instructions, excluding body
instructions; comparisons visit schema-ordered values even for identical handles.
Composition retains a shared result shell after context/reference validation.
Ordinary option/result data never implicitly unwraps or becomes a raised error.

Reports bind the fixture identity/platform and original provider failure UUID/span,
validate accounting, and buffer one bounded JSON document. Output overflow replaces
success with a reserved typed failure before the CLI selects an exit status.
Invalid counters cannot be clamped or concealed; report delivery fails instead.
Setup emits no execution report. Diagnostics exclude fixture payloads and do not
invent task IDs. Source-located runtime labels describe execution, not rejection
by the static compiler; existing compiler diagnostic bytes stay unchanged.

All deployed schemas, the catalogue, original examples and old goldens remain
byte-identical. No migration, dependency or constitution amendment is required.
The CLI alone reads source plus fixed `fixture.json`; `expected-result.json` is
test-only. Controller/dashboard remain health/status foundations. This boundary
is a pure data interpreter, not an OS sandbox for arbitrary native replacement
code. Phase 6 stress evidence and Phase 7 release closure remain open; no Version 4
or Git mutation is implied. See [checkpoint evidence](version-3-phases-4-5-verification.md).

## ADR-0012: Version 3 fixture interpreter release and adversarial verification

- Status: Accepted for Version 3; closes the remaining validation/documentation scope
- Date: 2026-09-20
- Requirements: SIH-01/SIH-02 frontend support; SIH-12/SIH-13 compiler infrastructure only

Retain ADR-0010/0011's checked-source provenance, full verification, UUIDv8 framing,
private immutable success wrappers, six-opcode structured IR, four exact-descriptor
fixture mocks and exclusive CLI file boundary. The four compiler-only 1.0.0
schemas are additive to frozen operational/Version 1–2 contracts. Source AST,
span semantics, catalogue availability and source examples require no migration.

Close release with deterministic library/CLI repetition, three fixed-seed
10,000-case campaigns, exact/+1 resource tests, deep ordinary generic transport,
inert data tests and file/stream fault injection. Instructions, loop N+2/work
charging and logical storage caps remain unchanged. Shared-handle comparison
must cost the same as independent equal values; repeated composition retains
bounded shells, not recursively duplicated results. Programmatic raised limits
cannot exceed fixed caps or authorize host behavior.

An actual near-8-MiB result test verifies oversized success is replaced by one
typed EXEC_OUTPUT_LIMIT failure before CLI exit selection. Partial writes and
failed stderr delivery take exit 2 precedence without a second stdout document.
No production runtime change was justified by the final adversarial tests;
two initial red probes had incorrect starting-depth/handle assumptions, corrected
in tests without changing their published boundaries or existing goldens.
The first nested clean-copy run exposed the Unix socket test's absolute-path
length assumption. Rebase the same test-owned socket name relative to the current
directory, without changing global cwd or creating files outside the repository;
retain the complete no-follow/special-file assertions and rerun a fresh copy.

Normal dependency and control/data-flow review found no interpreter provider
callback, arbitrary file/process/network/elevation path or native fallback.
Software hashing and embedded deny-retrieval schemas preserve host independence.
The CLI alone owns explicit local file capabilities, with per-platform adapters,
retained handles and no-overwrite staging/hard-link publication. These guarantees
assume user-controlled local directories, not a malicious privileged filesystem,
wall-clock deadline or OS sandbox for replacement code.

Keep controller/dashboard as health/status foundations and real forensic
implementations unavailable. The release evidence distinguishes local Fedora
passes from unobserved Windows/Ubuntu hosted jobs, Compose config from running
services, and current stable from the pre-existing unverified/incompatible 1.85
MSRV declaration. No constitution amendment, dependency update, Git mutation or
Version 4 implementation follows automatically. See [release evidence](version-3-verification.md).

## ADR-0013: Linux-first delivery and deferred Windows completion

- Status: Accepted for governance and future version scope; no collector implemented
- Date: 2026-09-21
- Requirements: SIH-01/02/03/08/09/12/24 delivery planning; no implementation-status upgrade
- Supersedes: ADR-0003's Ubuntu-only Linux adapter scope and the old Windows-first
  hand-off following ADR-0012; other prior decisions and historical evidence remain.

The user requests progress and demonstrable Linux results without working on a
Windows system yet. Amend constitution 1.0.0 to **1.1.0**: materially expanded
platform/support guidance, not a removed principle or weakened security rule.
Keep original ratification 2026-09-15; amendment date is 2026-09-21. Fixed stack,
separate Windows/Ubuntu regression CI, version stopping rules and all prohibited
behavior remain unchanged.

Preserve completed Versions 0–3. Defer Version 4 Windows collector requirements
into new Version 16. Revised Version 5 creates platform-neutral collector
interfaces/records/policies plus native/linux adapters, without depending on
Windows code. Versions 6–15 deliver Linux packages, managed execution and the
integrated demonstration. Version 16 must additionally complete Windows packages,
runtime, mixed-platform results and all deferred acceptance, not collectors alone.
See [the roadmap](linux-first-roadmap.md) for exact ownership and checkpoints.

Initial planning matrix is x86_64/glibc Debian, Ubuntu, Fedora, a named
RHEL-compatible distribution (initial candidate Rocky Linux), and Arch Linux.
Pin releases/images at planning time and record actual native VM evidence.
Do not claim universal Linux or derivative support. Use common Linux collectors
and small facility adapters; unsupported, denied, partial and empty-success
outcomes are distinct. Preserve least privilege; never disable security controls.

Version 5 prioritises a separately authorised standalone lab report for system,
process, connection and scoped-file facts with benign independent ground truth.
This provides visible progress before native JOCKY packages or managed endpoints.
Missing authorised VMs leave live acceptance unverified; they do not justify
using the developer's workstation or substituting fixtures for real evidence.

Compatibility: this amendment changes no schemas, source, fixtures, dependencies
or CI jobs. Version 5 must explicitly version the `linux` vocabulary addition
across grammar/AST, checker, registry, shared types, IR/report and affected
operational contracts. Retain legacy schemas and artifact validity, Ubuntu-specific
source meaning, Windows-only restrictions and old regression fixtures. Keep OS
family separate from distribution/release, architecture and facility availability.
Reject Ubuntu-only calls from generic Linux source unless a new portable contract
actually supports them. Planned module declarations remain distinct from live
adapter availability and authorisation.

The pure compiler/IR fixture core never gains native collection or a host fallback.
Live collection belongs behind a separate explicit CLI/adapter and policy gate.
This governance turn stops before Version 5 specification or implementation; no
endpoint contact, Windows work, Git staging/commit/push or history rewrite follows.
Historical plans stay unchanged; current state and contributor guidance identify
the superseding hand-off. Validation is recorded in
[linux-first-governance-verification.md](linux-first-governance-verification.md).
