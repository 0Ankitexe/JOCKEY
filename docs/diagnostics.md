# JOCKY diagnostics

Version 2 preserves the original syntax diagnostics and adds semantic checking,
warnings, and schema-constrained human/JSON check reports.

The frontend owns all user-facing diagnostics. Pest errors are internal
location evidence and its rule names or messages are never emitted. A failed
parse returns a non-empty typed `DiagnosticSet` and no partial AST.

## Categories

| Rank | Identifier                   | Meaning                                                                          |
| ---: | ---------------------------- | -------------------------------------------------------------------------------- |
|   10 | `unexpected-token`           | Source does not match Version 1 and no more specific category applies.           |
|   20 | `missing-delimiter`          | A string, block comment, parenthesis, or brace is unclosed.                      |
|   30 | `invalid-target-declaration` | A selector is not `windows`, `ubuntu`, or `windows \| ubuntu`.                   |
|   40 | `malformed-type`             | A named, `list<T>`, `option<T>`, or `result<T, E>` type is malformed.            |
|   50 | `duplicate-top-level-run`    | A second otherwise complete top-level `run` appears.                             |
|   90 | `resource-limit`             | Delimiter nesting exceeds the local limit of 256; this is not a syntax judgment. |

At one location the most specific applicable category wins: duplicate complete
run, invalid target island, malformed type context, missing non-type delimiter,
then unexpected token. A missing generic `>` is `malformed-type` with a
`missing closing >` label.

## Examples and corrections

Unexpected token:

```jocky
fn inspect(target: endpoint,) -> report {}
```

Remove the trailing comma:

```jocky
fn inspect(target: endpoint) -> report {}
```

Missing delimiter:

```jocky
fn inspect() -> report { return report()
```

Add the closing `}`. Unterminated strings, comments, and parentheses are
corrected similarly.

Invalid target declaration:

```jocky
target ubuntu | windows
```

Use `target windows | ubuntu`, `target windows`, or `target ubuntu`.

Malformed type:

```jocky
fn inspect() -> result<report failure> {}
```

Use `result<report, failure>`.

Duplicate run:

```jocky
run inspect on selected_endpoints
run inspect on selected_endpoints
```

Retain at most one final `run` statement.

Resource limit failures are corrected by reducing nested parentheses, angle
brackets, or braces. The CLI returns exit 2 for this category; ordinary syntax
diagnostics return exit 1.

## Stable human format

```text
error[invalid-target-declaration]: target must be windows, ubuntu, or windows | ubuntu
  --> bad-target.jky:2:8
   |
2 | target macos
   |        ^^^^^ unsupported target
```

Output is uncolored, uses LF on all platforms, separates diagnostics with one
blank line, and ends with LF. Diagnostics sort by start byte, category rank,
end byte, and closed message key; exact duplicates are removed. At most 32 are
retained. When more are found, output ends with:

```text
note: additional diagnostics suppressed after 32 errors
```

## Locations and escaping

Structured spans are zero-based, half-open UTF-8 byte offsets. Rendered lines
and columns are one-based. LF and CRLF each advance one line. Each Unicode
scalar advances one column, while a tab advances to columns 1, 5, 9, 13, and so
on. A zero-width position has one caret. A multi-line span marks only the first
displayed line, and an unclosed delimiter is anchored at its opener.

Tabs in snippets expand to the next four-column stop. Printable Unicode and
backslash remain visible. Other C0/C1 controls render as uppercase
`\u{XXXX}`. Filename labels use `\t`, `\n`, and `\r` for those three controls
and the same uppercase form for other C0/C1 values. The caller's filename is a
label only: the pure parser never opens or canonicalizes it.

## Recovery and determinism

Recovery runs only after strict full-input parsing fails. It uses depth-aware
top-level, statement, and type synchronization while treating strings and
comments as opaque. Every scan advances by a token or complete Unicode scalar.
Recovery can report independent later errors but can never produce an AST or
turn malformed source into success.

The invalid fixture corpus is compared byte-for-byte and each snapshot is
repeated ten times. LF/CRLF variants retain convention-specific byte spans but
equivalent categories and logical locations. Fixed-seed 10,000-input campaigns
exercise parser/checker robustness; these finite tests are not exhaustive proofs.

## Version 2 semantic diagnostics

`jockyc check <file>` and `jockyc check <file> --diagnostic-format human` first
parse, then check names/types/flow against a validated inert registry.
`parse` remains syntax-only. Profile-aware parsing additionally reports
`invalid-profile-declaration` (rank 60) for invalid static lab declarations.

Semantic errors are `duplicate-definition`, `unknown-name`, `unknown-type`,
`not-callable`, `argument-count-mismatch`, `argument-type-mismatch`,
`return-type-mismatch`, `missing-return`, `condition-not-bool`,
`not-a-collection`, `missing-run`, `invalid-entry-point`, `unsupported-target`
and `lab-profile-required`. Errors have source spans and prevent a checked model.

Warnings are `unreachable-code` (each dead statement) and `unused-binding`
(unused let/loop bindings, including underscore names). Parameters do not receive
unused warnings, and dead reads do not count as uses. Warnings alone exit `0`.
No source expression is executed to compute reachability.

```text
error[condition-not-bool]: condition must have type bool; found int
  --> type-error.jky:4:8
   |
4 |     if 1 {
   |        ^ expected bool
```

Human checks write diagnostics to stderr and leave stdout empty. Success without
warnings is silent. Syntax/semantic errors exit `1`; command, input, registry,
resource or output failures exit `2`.

Semantic diagnostics sort by `(start, severity, category, end, message, label)`,
with errors first at equal positions. Exact duplicates are removed; the first
32 errors and 32 warnings are retained independently. Truncation does not turn
failure into success. Suppression notes distinguish the two caps. Syntax
diagnostics retain their original ordering, cap and snapshots.

Resource failures replace partial diagnostics/metadata with a single failure.
Limits are 4 MiB source, 1,024 functions, 100,000 syntax nodes, 20,000 bindings,
32,768 distinct source-call edges, type depth 64, 100,000 attempted type visits
and retained type nodes, and an 8 MiB report buffer. The original parser's
256-delimiter guard remains. Check reads files with a byte bound; the legacy
syntax APIs do not inherit semantic-only budgets.

Report output is buffered before writing. A report that cannot fit produces a
small `resource-limit` error; an actual writer failure cannot guarantee partial
output recovery. Native I/O and registry dependency messages are not exposed.

## JSON reports and output channels

`jockyc check <file> --diagnostic-format json` writes one complete report to
stdout and leaves stderr empty whenever a report can be delivered. The source
of truth is [check-report.schema.json](../shared/compiler-contracts/check-report.schema.json).
Schema version `1.0.0` is independent of product and AST versions.

| Status    | Exit | Meaning                                         | Metadata |
| --------- | ---: | ----------------------------------------------- | -------- |
| `valid`   |    0 | Full syntax/semantic success; warnings allowed  | Complete |
| `invalid` |    1 | Syntax or semantic source errors                | `null`   |
| `failure` |    2 | Input, registry, resource, or reporting failure | `null`   |

Root fields have fixed order: `schema_version`, `status`, `file`, `diagnostics`,
`truncated`, `metadata`. JSON uses two-space indentation and one final LF.
No timestamps, random IDs, ANSI coloring or runtime results are generated.
Each source diagnostic includes severity, code, message, location and label.
Location contains original byte span, logical start/exclusive-end positions,
escaped first-line snippet, zero-based visual marker start and positive width.
Syntax JSON retains the legacy renderer's CR/LF-boundary convention; semantic
locations use the original source map. Source is never rewritten for reporting.

Input/registry diagnostics have `location: null` and `label: null`. A correctly
shaped JSON request with a non-UTF-8 path has `file: null`; no fake line 1 is
invented. Missing/unreadable files, invalid UTF-8 bytes and invalid registry data
produce stable codes, not platform error prose. Unknown formats or malformed
argument order return human usage on stderr, exit 2, without guessing JSON.

The two `truncated` booleans describe the independent error/warning caps.
Resource failure supersedes partial diagnostics and metadata. Overlarge output
is replaced with a small `failure` report before any stdout write. Serialization
or consistency failures likewise become `output-failure` when a fallback can be
serialized. A supplied label too large even for that fallback produces a stderr
output error instead of truncating or inventing a filename.

Write or flush failure returns exit 2, with an output error on stderr if writable.
A broken stream may contain an incomplete document (or a complete buffered
document whose flush failed); neither is promised delivered. No second JSON
document is appended to repair partial output.

```sh
# Static success with complete unavailable-dependency metadata (exit 0)
target/debug/jockyc check examples/triage.jky --diagnostic-format json
# Source error (exit 1); add --diagnostic-format human for readable carets
target/debug/jockyc check language/tests/fixtures/semantic/invalid/non-bool-condition.jky --diagnostic-format json
# Warning-only valid report (exit 0)
target/debug/jockyc check language/tests/fixtures/semantic/warnings/unused.jky --diagnostic-format json
# Inspect a planned unavailable contract, without collecting anything
target/debug/jockyc modules describe forensic.process.list
```

Successful metadata belongs only to the exact source bytes that produced the
checked program. Functions retain declaration order; capabilities, platforms and
dependencies are canonical, and the entry equals its function summary. Selected
targets are separate from inferred platform support. No successful check or
module description means a collector exists. Version 3 adds only the explicitly
requested fixture interpreter described below.

## Version 3 IR and fixture diagnostics

`build-ir <source> --output <new.json>` and `verify-ir <artifact>` are silent
on success (build warnings use stderr). `run-fixture <source> --fixture <dir>`
emits one explicitly simulated JSON report after execution starts. Source and
setup failures emit no report. None of these commands enables real collection.

| Category                             | Codes / meaning                                                                                                                         |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| JSON/schema/version                  | IR_JSON, IR_SCHEMA, IR_VERSION                                                                                                          |
| Identity/references/types            | IR_IDENTITY, IR_REFERENCE, IR_TYPE                                                                                                      |
| Data/control flow                    | IR_INITIALIZATION, IR_CONTROL, IR_RETURN                                                                                                |
| Contract/static metadata             | IR_CONTRACT, IR_METADATA                                                                                                                |
| Bounded verification/lowering        | IR_RESOURCE, LOWER_RESOURCE, LOWER_INVARIANT                                                                                            |
| File/setup/delivery                  | V3_INPUT, V3_FIXTURE, V3_VERSION, V3_REGISTRY, V3_RESOURCE, V3_OUTPUT, V3_UNSUPPORTED_IO                                                |
| Execution resources                  | EXEC_INSTRUCTION_LIMIT, EXEC_WORK_LIMIT, EXEC_COLLECTION_LIMIT, EXEC_CALL_DEPTH, EXEC_VALUE_DEPTH, EXEC_MEMORY_LIMIT, EXEC_OUTPUT_LIMIT |
| Invalid simulated values/composition | EXEC_INVALID_VALUE, EXEC_INVALID_COMPOSITION                                                                                            |

Provider codes remain the called contract's declared typed errors; a provider
ResourceLimit is a resource failure, not ordinary result<T,E> data. The report
separates provider and interpreter categories and preserves the originating
instruction UUID/optional span through nested calls. It never contains a partial
successful forensic value on failure.

| Result                                     | Exit | Output                                          |
| ------------------------------------------ | ---: | ----------------------------------------------- |
| Build/verify success                       |    0 | No stdout; build warnings only                  |
| Fixture success                            |    0 | One success report, warnings on stderr          |
| Source or supported malformed IR           |    1 | Human stderr only                               |
| Ordinary fixture execution failure         |    1 | One failure report plus human stderr            |
| Usage/input/setup/version/registry failure |    2 | Human stderr only                               |
| Execution resource failure                 |    2 | One failure report plus human stderr            |
| Failed output/write/flush/publication      |    2 | Delivery not promised; fixed stderr if writable |

IR errors sort deterministically by artifact position, code and pointer and retain
at most 20 distinct diagnostics plus a truncation flag. Resource failures replace
partial diagnostics. Standalone verification reports artifact labels, JSON
pointers and available UUIDs/byte spans; it never opens embedded source labels
or fabricates line/column/snippets. Lowering and fixture errors with matching
source retain filename, line, column and source snippet. Runtime markers say
“fixture execution stopped here”, not that static compilation rejected the call.

Diagnostics use fixed messages, escaped identifiers and no native exception or
fixture payload text. Report serialization uses fixed field order and one final
LF. An oversized success becomes a small EXEC_OUTPUT_LIMIT report before exit
selection. Broken stdout may contain an incomplete document; no second document
is appended. Failed warning delivery prevents interpretation/publication.

See [IR rules](ir-specification.md), [fixture rules](reference-interpreter.md) and
the [README command demonstration](../README.md).
