# JOCKY architecture

## Linux-first delivery policy

The [2026-09-21 roadmap](linux-first-roadmap.md) and ADR-0013 defer Windows
operational work into Version 16. Revised Version 5 creates shared collector
interfaces/policies and `native/linux/` adapters; Versions 6–15 deliver the Linux
workflow first. This is future architecture, not implemented collection.
Version 3 remains complete and fixture-only. Existing Windows/Ubuntu contracts
remain unchanged until Version 5's explicit versioned `linux` migration.

## Version 0 boundary

Version 0 establishes package boundaries and cross-language data contracts. It
does not parse JOCKY source, generate target packages, register endpoints,
dispatch tasks, collect forensic data, or implement transport and sensitive lab
adapters.

## Version 1 boundary

Version 1 implements only the platform-neutral syntax frontend in `language/`:
the Pest grammar, lexical depth guard, owned AST, deterministic diagnostics,
and the `jockyc` file/stdio adapter. Strict whole-source recognition is the
only AST path. Recovery reports failures but cannot create a partial AST or
accept source. At that checkpoint semantic checking was future work; Version 2
adds it below. IR, package generation, execution, forensic access, endpoint
registration and networking remain future work.

## End-to-end flow

The planned data flow is:

```text
JOCKY source
    -> parser (Version 1)
    -> type checker and inert module registry (Version 2)
    -> IR
    -> target package
    -> controller
    -> endpoint runtime
    -> forensic results
    -> controller/dashboard
```

The build path ends at a signed, integrity-addressed target package. The
management path begins when the controller assigns that package to an
authorised endpoint. Endpoint status and forensic results return as separate
typed messages so evidence data does not enter operational status logs.

## Component responsibilities

| Area            | Responsibility                                                | Current state                                                                                          |
| --------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `language/`     | Grammar, AST, semantic analysis, diagnostics and CLI          | Version 2 frontend plus Version 3 lowering, build/verify/fixture commands                              |
| `ir/`           | Platform-independent intermediate representation              | Typed IR, full verifier and guarded data-only fixture interpreter                                      |
| `transforms/`   | Safe transformation research                                  | Empty typed Rust crate boundary                                                                        |
| `codegen/`      | Linux-first, future Windows target-package backends           | Empty typed Rust crate boundary                                                                        |
| `forensic-lib/` | Read-only forensic contracts; future collectors               | Ten versioned, unavailable-only contracts in an immutable registry; no invocation API                  |
| `runtime/`      | Endpoint task lifecycle and package execution                 | Empty typed Rust crate boundary                                                                        |
| `native/`       | Explicit Linux and future Windows native adapters             | Documentation-only boundary                                                                            |
| `kernel-lab/`   | Detection-only inventory and external lab-evidence interfaces | Documentation-only, disabled boundary                                                                  |
| `transport/`    | Controller/endpoint transport adapters                        | Documentation-only boundary                                                                            |
| `controller/`   | Management API and future persistence/queue coordination      | `/health` and `/version` only                                                                          |
| `dashboard/`    | Operator interface                                            | Controller health/status only                                                                          |
| `shared/`       | Operational and separately versioned compiler contracts       | Six unchanged operational schemas plus compiler vocabulary, registry, AST/check and IR/fixture schemas |

## Contract boundaries

The JSON Schema documents in `shared/schemas/` are the source of truth.
Language-specific types must remain compatible with them and are checked
against the same fixtures.

| Boundary                                | Contracts                            |
| --------------------------------------- | ------------------------------------ |
| Controller and endpoint identity        | `EndpointIdentity`                   |
| Controller to endpoint                  | `TaskEnvelope`, `BuildManifest`      |
| Endpoint to controller lifecycle        | `TaskStatusEvent`                    |
| Endpoint to evidence storage/controller | `ForensicRecord`, `EvidenceManifest` |

Every contract carries `schema_version`. Stable identifiers are canonical UUID
strings. Timestamps are RFC 3339 date-times normalized to UTC and ending in
`Z`. Unknown properties and enum values are rejected except within explicitly
open `parameters` and forensic `payload` objects.

## Trust zones

- The development/build zone contains the compiler workspace and CI.
- The management zone contains PostgreSQL, Redis, the controller, and dashboard.
- The transport zone will contain explicitly configured adapters; none exist in
  Version 0.
- Linux and later Windows endpoints will contain the runtime, forensic library, and
  platform adapter selected through an explicit interface.
- Any future high-risk research remains in an isolated, owner-authorised lab
  zone and behind an auditable, disabled-by-default gate.

Platform-specific imports and operating-system calls must stay behind separate
Linux and Windows adapters. Linux facility-specific adapters handle distribution
differences; a distribution name never grants a capability. Cross-platform crates may depend on adapter
interfaces but not on platform implementations.

## Source-to-AST flow

```text
caller-supplied UTF-8 + filename label
    -> iterative lexical preflight (local depth guard)
    -> strict SOI/EOI Pest parse
       -> success -> owned span-preserving AST -> optional JSON document
       -> failure -> deterministic recovery -> typed DiagnosticSet
```

The pure frontend performs no file or host operation. Only `language/src/cli/`
reads the one explicitly supplied path for parse/check and writes standard
output/error; those commands never execute parsed calls. The AST JSON
schema deployed under `shared/compiler-contracts/` is a compiler-output contract and
does not replace or modify the six operational schemas in `shared/schemas/`.

## Version 2 static analysis boundary

The source parser wraps the unchanged AST with an optional contextual lab profile.
No profile emits legacy AST 1.0.0; `profile lab` emits AST 2.0.0. Legacy library
`parse`/`check` remain syntax-only, while CLI `check` performs semantic analysis.

```text
original UTF-8 source + validated inert registry
    -> bounded strict parse and AST census
    -> predeclared signatures + lexical name/type/flow resolution
    -> target/lab checks for every call (including dead and uncalled code)
    -> finite iterative call-graph metadata
    -> complete CheckedProgram + warnings, or typed failure without metadata
    -> bounded human/JSON report -> explicit stdio adapter
```

The shared compiler vocabulary owns 18 named types, list/option/result and the
platform/privilege/capability enums. Language-owned side tables associate resolved
references and exact types with source spans and compiler-local IDs, without
mutating the AST. CheckedProgram also retains a bounded source copy to bind
success reports to the original bytes. Private report constructors validate
source and metadata consistency before serialization.

Dependency direction is language -> forensic contracts -> shared, with a direct
language -> shared edge. No compiler component depends on native/runtime execution.
Registry schemas and the catalogue are embedded. Validators resolve only explicitly
registered local resources and reject unknown URI retrieval. New tests use deployed
contracts; an offline fixed-path CI utility prepares the preserved V1 test loader.

Source modules have lexical scopes, exact types, conservative all-path returns,
bool conditions, bounded collection loops and one valid run entry. Recursive calls
form strongly connected components for metadata union/intersection; they are never
evaluated. Uncalled functions are checked but excluded from unrelated entry summaries.

Privileges and `profile lab` are descriptive static gates only, not authority or
elevation. Every contract remains unavailable. IR, interpretation, code generation,
collectors, endpoint registration and transport are outside Version 2. Controller
and dashboard still provide only the Version 0 health/status foundation.

## Version 3: verified IR and fixture-only execution

All seven Version 3 phases are implemented. See [release evidence](version-3-verification.md),
the [IR specification](ir-specification.md) and [interpreter guide](reference-interpreter.md).

```text
AST -> typed AST -> IR -> verifier -> fixture interpreter

typed AST = unchanged parser AST plus resolved Version 2 semantic side tables

checked source -> typed IR -> full verifier -> VerifiedProgram
                                                            |
fixture.json -> bounded offline validation -> ValidatedFixture
                                                            |
                   explicit limits -> prepare -> bounded fixture interpreter
                                                -> synthetic result or typed failure
```

Dependency direction is `language -> ir -> forensic contracts -> shared`.
The pure IR crate receives immutable verified data and one validated synthetic
endpoint, never file handles, provider callbacks or native adapter capabilities.
Explicit frames avoid recursive Rust execution; immutable handles preserve
ordinary generic values. Four signature-matched data mocks do not change real
catalogue availability. Restricted/unmocked calls cannot fall back to the host.

The CLI alone opens explicit source/IR/fixture paths through separate no-follow
platform adapters. `verify-ir` does not open source labels. `run-fixture` checks,
lowers and verifies source, reads only `fixture.json`, prepares the invocation,
delivers warnings, then emits one bounded report. Setup failure emits no report;
execution failure never includes a successful value. Controller messages and
fixture reports are separate contracts; there is no endpoint execution or
transport. These data-flow constraints are not an OS sandbox for arbitrary native
replacement code. Privilege and lab-profile metadata remain descriptive only.
