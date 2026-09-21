# Platforms and capability metadata

Version 2 exposes static requirements through the Rust checker API and JSON reports.
It does not collect evidence, run source, query the host OS, change privileges
or grant operational authorisation. Every forensic implementation is unavailable.

## Planned Linux expansion, not yet implemented

Under [ADR-0013 and the Linux-first roadmap](linux-first-roadmap.md), revised
Version 5 will add a generic `linux` target through an explicit versioned migration.
The target rules below describe current Version 2–3 behavior and stay unchanged
today. `ubuntu` must retain Ubuntu-specific scope; no blind alias broadening is
allowed. The future model distinguishes OS family, distribution/release,
architecture, available facilities and tested collector support. Windows live
collection and package/runtime integration are deferred to Version 16.
No Linux distribution has live-collector evidence yet.

## Selected and supported targets

Source selects `target windows`, `target ubuntu`, or `target windows | ubuntu`.
Every direct contract call must support **every selected target**, including
calls in uncalled helpers, branches, and statements after a return. `if true`
does not narrow targets. An Ubuntu-only call in Windows source fails, and vice
versa; either single-platform call also fails in dual-target source.

Selected targets are the author's choice. Supported platforms are the intersection
of a function's direct and transitive dependencies. A function with no restrictive
dependencies supports both platforms even if its source selects only Windows.
Support describes planned contracts, not implemented or tested collectors.

## How summaries combine

| Field                      | Rule                                                                |
| -------------------------- | ------------------------------------------------------------------- |
| `required_privilege`       | Strongest requirement: `user < elevated < lab_only`; default `user` |
| `capabilities`             | Unique union in the fixed order below; default empty                |
| `supported_platforms`      | Intersection; default Windows then Ubuntu                           |
| `unavailable_dependencies` | Unique external names in ASCII order; default empty                 |

The eight capability categories, in canonical order, are:

1. `system`: planned system profile.
2. `processes`: planned process inventory.
3. `network`: planned connection inventory.
4. `files`: planned file inspection.
5. `events`: planned bounded event queries.
6. `persistence`: planned persistence inventory, never installation.
7. `drivers`: planned driver inventory, never driver loading or exploitation.
8. `indicators`: planned correlation of supplied records.

Summaries include dead call sites. Iterative call-graph processing handles shared
helpers and recursive cycles without following them as executable calls. This
does not prove a program terminates. Source functions appear in declaration order;
their names are not unavailable dependencies. The compiler-only `forensic_result`
composition signature contributes its bare name, user privilege and no capability.

The entry summary equals the run function's summary, not the union of every
function in the file. Uncalled helpers are still validated, but cannot inflate
the entry's requirements.

## Read-only Rust API

Call `jocky_language::semantic::analyze(SourceFile, &Registry)`. Only success
returns a `CheckedProgram`; syntax, semantic and resource failures carry no
partial checked program. The result exposes:

- `function_metadata()`: immutable declaration-ordered summaries.
- `entry_metadata()`: a borrowed reference to the run function's summary.
- `selected_targets()`: an immutable canonical platform slice.
- `profile()`: `None` for standard profile or the explicit lab declaration
  with original UTF-8 byte spans.

Each `FunctionMetadata` has `name()`, `required_privilege()`, `capabilities()`,
`supported_platforms()` and `unavailable_dependencies()`. `name()` formats an
owned qualified-name string on demand; the other methods borrow data or return
a small enum. Dependency iteration exposes borrowed strings, not mutable storage.
Metadata compares by value, independently of registry input order or filename.
There is no unchecked public constructor, evaluator or collector method.

Human `jockyc check` is silent on clean success and reports restrictions on
failure. To print these summaries use `jockyc check <file> --diagnostic-format json`.
Only a valid report has metadata; invalid source and host/resource failures have
`metadata: null`. JSON metadata uses the same canonical ordering and entry identity
as the Rust API. No report is an execution or evidence result.

## Lab profile and privileges

`profile lab` may appear once after target and before functions. Its absence
never opts in. Comments, strings, filenames and the host's privilege cannot
enable it. Unknown, duplicate and misplaced declarations are syntax errors.

A contract's `lab_only: true` must agree with `required_privilege: lab_only`.
Contradictions fail registry validation. Lab-only references without an explicit
profile fail even in unused/dead helpers. An explicit profile allows static
validation only; it neither overrides platform/type checks nor makes anything
executable. Synthetic lab contracts exist only in tests, not in the shipped
catalogue. No CLI option loads another registry.

`elevated` likewise means descriptive metadata, not a request to elevate.
The operational authorisation gates in [security-boundary.md](security-boundary.md)
remain separate and unimplemented. They must never enable prohibited behavior.

## Offline demonstration

From the repository root after dependencies are installed:

```sh
CARGO_NET_OFFLINE=true cargo build --locked -p jocky-language --bin jockyc
target/debug/jockyc check examples/windows-events.jky
target/debug/jockyc check examples/ubuntu-events.jky
target/debug/jockyc check examples/cross-platform.jky
target/debug/jockyc check language/tests/fixtures/semantic/platforms/windows-call-on-ubuntu.jky
target/debug/jockyc check language/tests/fixtures/semantic/platforms/ubuntu-call-on-windows.jky
```

The first three checks exit 0. Each event example has an **uncalled** event helper
so opaque timestamps can flow through typed parameters without invented values.
That helper requires `events/elevated` and its single platform, but its run entry
requires only `system/user` and supports both. The cross-platform entry includes
`system/processes/drivers`, requires `elevated` through its helper, and supports
both platforms. Nothing runs or reads event logs.

The final two checks exit 1 with `unsupported-target` at the actual restricted
callee. They are deliberate negative fixtures, not installation failures.

Tests: `semantic_metadata`, `semantic_lab`, `semantic_graph`,
`semantic_metadata_cli` and `semantic_purity`. The tests and code/dependency
review provide evidence of the in-memory boundary, not a formal proof of purity.
