# Safe reference interpreter

Version 3 runs verified IR against authored fixture data, never against your
computer. It has no host collector, process/network/filesystem/clock/environment
interface, native callback, executable plugin or elevation path. `runtime/` is
still a placeholder. Four mocks do not change the ten real module contracts'
`unavailable` status. This design is not an OS sandbox for replacement native code.

## Run the demonstration

From the repository root, with Rust dependencies already cached:

```sh
cargo build --locked --offline -p jocky-language --bin jockyc
target/debug/jockyc run-fixture examples/triage.jky --fixture examples/fixtures/triage/ubuntu
target/debug/jockyc run-fixture examples/triage.jky --fixture examples/fixtures/triage/windows
```

On Windows use `target/debug/jockyc.exe`. Both fixtures run on either supported
development host. The argument is JOCKY source, not prebuilt IR. The CLI checks,
lowers and verifies it, reads only the fixed regular `fixture.json` leaf, validates
all data, prepares the invocation, flushes warnings, then executes and emits one
report. Invalid setup produces no execution report.

`selected_endpoints` means exactly the single synthetic endpoint in the bundle.
Its platform must match source targets independently of the host OS. Its UUID and
observation time come from data, not discovery or the current clock. There is no
task scheduling, controller request, operational task ID or real evidence.

Exact data and result oracles:

- Ubuntu: [fixture](../examples/fixtures/triage/ubuntu/fixture.json),
  [forensic_result](../examples/fixtures/triage/ubuntu/expected-result.json),
  [complete measured report](../ir/tests/fixtures/golden/triage-ubuntu.report.json).
- Windows: [fixture](../examples/fixtures/triage/windows/fixture.json),
  [forensic_result](../examples/fixtures/triage/windows/expected-result.json),
  [complete measured report](../ir/tests/fixtures/golden/triage-windows.report.json).

The runner never reads `expected-result.json`. It returns `outcome.value` equal
to that oracle: an authored system profile and one indicator saying
“Authored fixture match; not live detection.” The observation time is
`2026-09-18T00:00:00Z`. Ubuntu endpoint ends `0001`, Windows `0101`; these are
synthetic IDs, not the demo computer's identity.

| Fixture        | Instructions |  Work | Peak frames | Peak logical storage |
| -------------- | -----------: | ----: | ----------: | -------------------: |
| Ubuntu triage  |           10 | 1,629 |           1 |         67,975 bytes |
| Windows triage |           10 | 1,649 |           1 |         67,990 bytes |

Ten repeated runs compare complete JSON bytes and accounting. Different explicit
library build seeds retain successful values; failure instruction IDs may change.

## Typed interfaces and contracts

```text
decode_fixture(bytes, &ExecutionLimits) -> Result<ValidatedFixture, FixtureFailure>
ExecutionLimits::new(instructions, collection, work, call_depth) -> Result<ExecutionLimits, LimitConfigurationFailure>
prepare(&VerifiedProgram, &ValidatedFixture, &ExecutionLimits) -> Result<PreparedInvocation, FixtureFailure>
execute(PreparedInvocation) -> ExecutionReport
ExecutionReport::to_json() -> Result<Vec<u8>, ReportWriteFailure>
```

Success constructors are private and immutable. Preparation checks platform,
whole-fixture context, all nested collection limits and combined retained values,
and reserves failure-report capacity. Limits are explicit validated programmatic
inputs, never CLI flags/environment overrides. No live-provider selector exists.

Compiler-only schema versions are `1.0.0`:
[values](../shared/compiler-contracts/fixture-values.schema.json),
[bundle](../shared/compiler-contracts/fixture-bundle.schema.json),
[report](../shared/compiler-contracts/fixture-report.schema.json).
They do not redefine the six operational message schemas.

| Exact mock signature                                                                                              | Declared provider errors                                       |
| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `forensic.system.profile(endpoint) -> forensic_result`                                                            | Unsupported, PermissionDenied, ResourceLimit, CollectionFailed |
| `forensic.process.list(endpoint) -> list<process_record>`                                                         | Unsupported, PermissionDenied, ResourceLimit, CollectionFailed |
| `forensic.network.connections(endpoint) -> list<connection_record>`                                               | Unsupported, PermissionDenied, ResourceLimit, CollectionFailed |
| `forensic.indicator.correlate(forensic_result, list<process_record>, list<connection_record>) -> forensic_result` | NotImplemented, InvalidInput, ResourceLimit                    |

Dispatch matches the complete canonical descriptor/version/restrictions, not
just its name. Changed, lab-only and unmocked descriptors do not select a mock.
Reached unavailable calls use declared Unsupported first, otherwise declared
NotImplemented; inconsistent contracts fail typed, never supply empty success.
An uncalled/untaken call is not invoked. Privilege/profile metadata never elevates.

The first three mocks require an exact match of the supplied endpoint value.
Correlation compares actual system/process/connection arguments, in order, with
the fixture's `expected` values before using its configured outcome. Mismatch is
declared InvalidInput; it is not a detection algorithm. All four provider slots
are mandatory even if unused. Explicit empty typed lists differ from missing data.

Validate endpoint/time/platform consistency, unique record/indicator IDs,
connection-to-process references and indicator references. Repeated copies of a
record must agree. Profile has a non-null system and no indicators; correlation
has null system plus authored indicators. The existing bare
`forensic_result(profile, indicators)` helper requires matching context and these
roles, then combines the system and indicators using shared immutable children.
It is not a general record constructor; disagreement is EXEC_INVALID_COMPOSITION.

## Values, control flow and errors

All 18 named types and list/option/result values are exact tagged data. Integers
and duration magnitudes are canonical arbitrary-length decimal strings; no fixed-
width conversion occurs. Bytes use lowercase even-length hex. UUIDs and valid
UTC timestamps are checked. Paths, command-looking strings and IP addresses
remain inert data, never a request to open, execute, resolve or contact anything.

Immutable arena handles carry values through explicit function frames and region/
loop cursors. Function calls do not recurse on the Rust stack. Each call/body
activation has fresh slots; return unwinds the whole function. Generic None,
Some, Ok and Error variants remain ordinary data, including a diagnostic_error
whose code happens to be ResourceLimit. They do not implicitly unwrap or raise.

Raised provider/interpreter failures stop all later instructions and retain the
original instruction UUID and available source span across calls. A failure
report has no successful partial value. There is no catch, retry or host fallback.
Interpreter codes are EXEC_INSTRUCTION_LIMIT, EXEC_WORK_LIMIT,
EXEC_COLLECTION_LIMIT, EXEC_CALL_DEPTH, EXEC_VALUE_DEPTH, EXEC_MEMORY_LIMIT,
EXEC_OUTPUT_LIMIT, EXEC_INVALID_VALUE and EXEC_INVALID_COMPOSITION.

## Charging and limits

Every executed instruction costs one before execution. Loop entry costs one,
then each advance test costs one, including the final exhausted test: an exhausted
N-element loop costs N+2 plus its body. Early return charges only visited tests.
Source call and callee instructions charge separately; the budget never resets.
Exhausted/overflowing charges stop before the action without incrementing usage.

Comparisons traverse JSON nodes in schema property/collection order: one work
unit per paired node, plus the greater UTF-8 length for scalar string pairs.
Tags/types count; object keys do not. Equal handles receive the same charge as
independent equal values. Comparison stops at first mismatch. One-value traversal
charges nodes and string bytes. Composition charges both inputs and constructed
logical result before retaining its 80-byte shared shell. Preflight/schema/report
serialization have separate caps; serialization never changes report accounting.

| Resource                                         |           Default |   Hard cap |
| ------------------------------------------------ | ----------------: | ---------: |
| Instructions                                     |           100,000 |  1,000,000 |
| Elements per collection, including nested arrays |            10,000 |     10,000 |
| Work units                                       |         1,000,000 | 10,000,000 |
| Function frames                                  |               128 |        256 |
| Typed value / type depth                         |                64 |         64 |
| Control cursors per frame                        |               256 |        256 |
| Fixture input / report, including final LF       |             8 MiB |      8 MiB |
| Raw JSON depth / nodes                           |      96 / 200,000 |       Same |
| Retained value nodes / child handles             | 200,000 / 200,000 |       Same |
| Retained scalar bytes                            |             8 MiB |      8 MiB |
| Live slot handles                                |         1,000,000 |       Same |
| Logical invocation storage                       |            64 MiB |       Same |
| Reserved failure buffer                          |            64 KiB |       Same |

Only the first four limits are configurable, positively and within hard caps.
Lowest applicable bound wins (for example, nested lists can reach JSON depth
before typed depth). Storage includes constants/fixtures before execution:
64 bytes per retained value/type node, actual scalar bytes, 8 per child/slot/
index handle, 64 per frame, 32 per active control cursor and dispatch/index costs.
Returned frames release live costs; arena storage is monotonic. Counters are
logical bounds, not process RSS. Validation has a separate 10,000,000-visit cap.

## Output and safety limitations

Reports use fixed field order, two-space indentation and a final LF. Result
context, error membership/origin and accounting are checked before delivery.
Oversized success becomes one small EXEC_OUTPUT_LIMIT failure; inconsistent
counters are not silently clamped. Serialization/delivery can explicitly fail.
Partial stdout writes cannot be retracted; no replacement document is appended.

Exit 0 means successful simulation, 1 an ordinary execution/source rejection,
2 usage/setup/version/resource/output failure. After execution starts, a writable
output receives one report; diagnostics/warnings use stderr without fixture
payloads. Setup errors emit no report. See [diagnostics](diagnostics.md).

CLI inputs must be explicit user-controlled local regular files/directories:
no traversal, symlink/reparse escape, special file, external reference, discovery
or provider loading. These controls are not a wall-clock I/O deadline, protection
from privileged filesystem attackers, or a sandbox for arbitrary native code.
Local Fedora tests do not constitute Windows/Ubuntu hosted passes. Synthetic
results establish deterministic data flow, not collection or detection accuracy.
See [release evidence](version-3-verification.md) for exact tested scope.
