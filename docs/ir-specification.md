# JOCKY IR 1.0.0

Version 3 introduces compiler infrastructure, not native code generation or
endpoint execution. The source of truth for the closed wire format is
[ir.schema.json](../shared/compiler-contracts/ir.schema.json). Cross-field rules
below are additionally enforced by `jocky_ir::verify`.

## Pipeline and trust boundary

```text
AST -> typed AST -> IR -> verifier -> fixture interpreter
       original AST +                verified input + authored fixture only
       semantic side tables
```

`jocky_language::lowering::lower(&CheckedProgram, BuildOptions { seed })` accepts
only a successful Version 2 analysis. The checked value retains original source,
logical filename, spans, resolved bindings/types/calls and its immutable validated
registry snapshot. Lowering cannot substitute another registry or guess names
from display strings. It invokes the full verifier before returning.

`decode_ir(bytes)` returns untrusted `IrDocument`, not executable approval.
`verify(document, &Registry)` is the only constructor of immutable
`VerifiedProgram`. Its `document()` view is borrowed; `to_json()` is bounded and
fallible. These APIs use supplied data, not files. Verification checks internal
consistency; it neither authenticates the author/source nor proves termination.

## Document and deterministic serialization

Root fields, in emitted order: `schema_version`, `language_version`, `build_seed`,
`source`, `registry`, `module`, `targets`, `profile`, `entry`, `entry_metadata`,
`contracts`, `constants`, `functions`. IR is `1.0.0`, language compatibility is
`2.0.0`, registry schema is `1.0.0`. Product version is independent of these.

The seed is a canonical decimal u64 string. CLI seed is zero; the library accepts
an explicit u64. Source metadata contains logical label, original UTF-8 byte
length and lowercase SHA-256, not source text. CLI labels replace backslashes
with `/` without resolving paths, changing case or normalizing newlines. Library
labels remain exactly supplied. Cwd, output filename, host platform, clock and
environment are not build inputs. LF and CRLF are different source bytes with
their own correct spans and hashes.

Spans are half-open UTF-8 byte offsets. Lowered modules, functions, identifiers/
slots and instructions retain original spans; genuinely source-less hand-authored
IR may use null spans. A label embedded in IR is never a file to open.

Targets are Windows-before-Ubuntu; profile is null or `lab`. Function/region/slot/
constant/contract references are zero-based bounded table indices, not operational
UUIDs. Functions retain declaration order, regions lexical preorder, slots first
allocation order, constants first occurrence without deduplication. Serialization
uses schema field order, two-space indentation, Serde JSON escaping, UTF-8 and
one final LF. Identical inputs/seed give identical complete bytes.

## Stable instruction identity

Each instruction has a lowercase UUIDv8. The full validated registry is hashed
as compact UTF-8 JSON with lexicographically sorted object keys, functions sorted
by qualified name/version, canonical platform/error sets and preserved parameter
order. No final LF enters that fingerprint. Referenced contracts, including dead
calls, are an exact sorted subset of this registry with no duplicates/unused rows.

For each instruction, start with ASCII `JOCKY-IR-ID-v1` and a zero byte. Append,
in order, source SHA-256 hex, logical label, language version, IR version, registry
schema version, registry fingerprint hex, decimal seed, function index, region
index and instruction index within that region. Prefix each UTF-8 field with
its byte length as an unsigned 64-bit big-endian integer. Hash with SHA-256,
take the first 16 bytes, set byte 6 to `(byte & 0x0f) | 0x80` and byte 8 to
`(byte & 0x3f) | 0x80`, then format as lowercase 8-4-4-4-12 UUID text.

Verification recomputes IDs and rejects duplicates/disagreement. Changed seed
can change identities, never instruction semantics or successful fixture values.
This is deterministic identity, not a signature or a collision-impossibility claim.

## Functions, typed slots and control

Functions contain signatures, ordered parameter-slot references, typed slots,
root region zero, owned regions and static metadata. A slot has type, owning
region, kind (`parameter`, `local`, `temporary`, `loop_binding`), optional name
and span. Every non-root region has exactly one parent control instruction;
detached, shared, cyclic or cross-function regions are invalid.

| Opcode     | Effect                                                         |
| ---------- | -------------------------------------------------------------- |
| `const`    | Initialize destination from a typed constant                   |
| `copy`     | Initialize destination with an immutable value handle          |
| `call`     | Call a source function, exact external contract or composition |
| `if`       | Select a child region using a bool slot                        |
| `for_each` | Traverse an already evaluated list with a fresh item binding   |
| `return`   | Return a typed slot from the whole current function            |

Every static destination is defined once. Parameters are initialized on entry;
loop items belong to their body region. Children may read initialized ancestor
slots but cannot write them. Sibling/child bindings cannot escape. New calls and
loop iterations have fresh activation state. Recursive calls are allowed but
bounded at execution; arbitrary control-flow cycles are not allowed.

Lowering evaluates statements in source order and call arguments left-to-right.
Let bindings emit distinct local copies; conditions/collections evaluate once.
Dead instructions and uncalled functions remain present and statically checked.
No optimizations or new source syntax are introduced. Strings, booleans and exact
decimal integer/duration magnitudes are retained without numeric narrowing.
Hand-authored constants support every validated domain/generic fixture value;
see [the interpreter guide](reference-interpreter.md).

Calls explicitly use `on_error: "propagate"`. Provider/interpreter failures stop
the invocation and retain the originating instruction/span across source calls.
Ordinary option/result values are data, not implicit unwrap/catch/retry behavior.

Metadata recomputes maximum privilege, capability union, platform intersection,
unavailable dependencies, read-only conjunction and lab-only disjunction through
source-call SCCs. Dead/uncalled calls still obey selected targets/lab gates;
unrelated helpers do not inflate entry metadata. These flags grant no authority.

## Verification and diagnostics

Both decoded and directly constructed documents undergo bounded census and
closed-schema/format validation, identity/reference/span/region ownership checks,
exact types and lexical visibility, definite initialization/all-path returns,
then complete contract/fingerprint and transitive metadata checks. Branches require
bool, iteration requires list/item agreement, and an empty loop may fall through.
Earlier unsafe structures suppress dependent passes, not allow partial success.

Closed codes: `IR_JSON`, `IR_SCHEMA`, `IR_VERSION`, `IR_IDENTITY`, `IR_REFERENCE`,
`IR_TYPE`, `IR_INITIALIZATION`, `IR_CONTROL`, `IR_RETURN`, `IR_CONTRACT`,
`IR_METADATA`, `IR_RESOURCE`; lowering adds `LOWER_INVARIANT`/`LOWER_RESOURCE`.
Diagnostics use fixed text, available JSON pointer/UUID/byte span and deterministic
position/code/pointer ordering. At most 20 distinct diagnostics plus truncation
are retained. Resource failure replaces partial diagnostics. No source snippet
is invented for an independently supplied IR artifact.

## Hard limits and file boundary

| Resource                                |                  Hard cap |
| --------------------------------------- | ------------------------: |
| Source / IR bytes                       |            4 MiB / 16 MiB |
| Logical label                           |         4,096 UTF-8 bytes |
| Raw JSON depth / nodes                  |              96 / 200,000 |
| Functions / external contracts          |               1,024 / 512 |
| Instructions, regions, slots, constants | 100,000 each per artifact |
| Distinct source-call edges              |                    32,768 |
| Type depth / type occurrences           |            64 / 1,000,000 |
| Project validation visits               |                10,000,000 |
| Logical auxiliary storage               |                    64 MiB |

Check before growth using checked arithmetic. Storage charges 64 bytes per
structural/type/diagnostic entry, retained UTF-8 bytes, 8 per reference/index,
16 per distinct call edge and rounded dependency-bitset bytes. Temporary indexes
are included; this is not allocator overhead or host RSS. Lowest applicable
limits win. Third-party schema validation uses fixed closed schemas and bounded
input, not a claimed internal step counter.

Only CLI adapters open explicit local source/IR paths. They reject traversal,
symlinks/junctions, reparse points and non-regular files, retain parent handles
and bound reads. IR output must have a new name: serialize and flush warnings,
exclusively create one of 64 sibling staging names, write/flush/sync, hard-link
to the final name without replacement, then remove only the owned stage.
Unsupported hard links have no overwrite/rename fallback. Failure can leave an
owned stage or a complete final file; no fake delivery success is returned.
Use user-controlled local directories. This is not a hostile-filesystem sandbox.

The [README](../README.md) demonstrates all three CLI forms. See
[verification evidence](version-3-verification.md) and [diagnostics](diagnostics.md).
