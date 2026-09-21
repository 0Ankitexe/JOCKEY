# JOCKY language reference

Version 1's library `parse` and `check` remain syntax-only compatibility APIs.
Version 2 adds `parse_source`, `semantic::analyze` and semantic CLI
checking. None performs forensic work or executes calls. The CLI alone reads
the explicitly supplied file; core APIs treat the filename as a label.
The syntax examples below describe parsing; the semantic checking rules follow them.

## Program structure

A source file contains exactly one module declaration, one target declaration,
one or more functions, and optionally one final `run` statement, in that order.

```jocky
module triage
target windows | ubuntu

fn investigate(target: endpoint) -> forensic_result {
    return forensic.system.profile(target)
}

run investigate on selected_endpoints
```

The target selector is exactly `windows`, `ubuntu`, or the canonical dual form
`windows | ubuntu`. Reversed, repeated, dangling, or additional platforms are
invalid. A file without `run` is a valid library-style program. More than one
top-level `run` is invalid, and a present `run` must be last. Version 1 does not
verify that its function exists or has a particular signature.

## Identifiers and keywords

Identifiers use ASCII letters or `_` first, followed by ASCII letters, digits,
or `_`. They are case-sensitive. Keywords require a token boundary, so
`module_name` is one identifier rather than `module` plus `_name`.

`module`, `windows`, `ubuntu`, `fn`, `let`, `if`, `else`, `for`, `in`,
`return`, `run`, `on`, `selected_endpoints`, `true`, and `false` are reserved.
The spellings `target`, `list`, `option`, and `result` are contextual: they keep
their declaration or type-constructor meaning where the grammar expects it,
and may otherwise be identifiers. This is required for the compatibility forms
`target: endpoint` and `forensic.process.list(target)`.

Qualified names contain one or more identifier segments separated by dots:

```jocky
forensic.network.connections(target)
```

Each segment and the complete qualified name has an independent source span.

## Functions and types

Functions always declare parameter types and a return type. Parameters are
positional and may be empty; trailing commas are not accepted.

```jocky
fn collect(
    targets: list<endpoint>,
    prior: option<result<report, failure>>
) -> result<list<report>, failure> {
    return build(targets, prior)
}
```

The supported type forms are:

- a named type such as `endpoint`;
- `list<T>`;
- `option<T>`;
- `result<T, E>`.

Generic types may nest. Bare `list`, `option`, and `result` are not named types
in type position. The frontend applies a local maximum delimiter nesting depth
of 256 before invoking the recursive grammar.

## Blocks and statements

Blocks are braced, may be empty, and preserve statement order. Version 1 has
five statement forms:

```jocky
let profile = forensic.system.profile(target)
forensic.audit.record(profile)

if ready {
    return accepted()
} else {
    return rejected()
}

for item in items {
    forensic.process.list(item)
}

return report(profile)
```

`else` is optional. `for` iterates syntactically over an expression; Version 1
does not prove the value is a collection or finite. `while` and other loop
forms are not supported. Statements have no semicolon terminator.

## Expressions and literals

Expressions are identifiers, calls with positional arguments, strings,
integers, booleans, or durations. Calls may be simple or qualified and may have
zero arguments. Argument lists do not accept trailing commas.

Strings use double quotes and support only `\"`, `\\`, `\n`, `\r`, and `\t`.
Raw CR or LF is not permitted inside a string. The AST contains the decoded
value while its span identifies the original spelling.

Integers are `0`, a non-zero decimal magnitude, or a minus sign followed by a
non-zero decimal magnitude. Leading `+`, leading zeroes, fractions, and `-0`
are invalid. Integer lexemes are retained as strings and are not limited to a
machine integer range.

Booleans are exactly `true` and `false`. Durations combine a non-negative
canonical decimal magnitude with `ms`, `s`, `m`, `h`, or `d`, for example
`250ms` or `4h`. Duration magnitudes also remain strings and may exceed machine
ranges.

## Trivia, newlines, and spans

Spaces, tabs, LF, CRLF, `//` line comments, and non-nested `/* ... */` block
comments are trivia between tokens. Required keyword gaps must contain at least
one trivia item. Comments are not emitted into the AST. Bare CR is not a line
ending, and block comments cannot nest.

Every module, function, statement, expression, identifier, type, target, block,
qualified name, and run node carries a half-open `{start, end}` span using
zero-based UTF-8 byte offsets into the original source. `Program.span` covers
all input, including leading and trailing trivia. Original CRLF bytes are never
normalized in spans.

`jockyc parse <file> --emit-ast json` wraps the AST with
`"schema_version": "1.0.0"`, emits deterministic pretty JSON, and ends with
one LF. The contract is
`shared/compiler-contracts/ast-v1.schema.json`. The historical regression loader's
local-path requirement is prepared offline by `python3.12 ci/prepare_legacy_ast_schema.py`.

## Historical syntax-only exclusions

Version 1 has no name resolution, duplicate-function rejection, arity checks,
type inference or compatibility checks, target compatibility checks, return
correctness checks, IR generation, package generation, source execution,
forensic collectors, endpoint registration, network access, or OS adapter
behavior. The fixtures prefixed `semantic-` intentionally prove these concerns
do not enter syntax validation.

## Version 2 checking rules

`jockyc parse` preserves syntax-only acceptance, including historical semantic
anomalies. `jockyc check` additionally enforces the following static rules.

The closed named types are `bool`, `int`, `string`, `bytes`, `duration`,
`timestamp`, `endpoint`, `platform`, `ip_address`, `path`, `process_record`,
`connection_record`, `file_record`, `event_record`, `persistence_record`,
`driver_record`, `forensic_result` and `diagnostic_error`. Generic forms are
`list<T>`, `option<T>` and `result<T, E>`, with exact nested equality and a
maximum type depth of 64. There are no implicit conversions, wrappers or
domain constructors. `string` is not `path`; `bytes` is not `list<int>`.
Only bool/int/string/duration have literals; opaque values flow through typed
parameters and declared results. No numeric range or timestamp-order evaluation
is performed.

Functions resolve before their bodies, so forward calls and recursion are
valid statically. Parameters share the outer function-body scope. Each branch
and loop body has its own scope; nested shadowing is allowed, same-scope
duplicates are not. A let initializer sees outer bindings, not its new binding.
Locals shadow unqualified functions and are not callable. Qualified calls are
either `source_module.function` or an exact registered module function, never
object fields. Functions are not first-class values. `forensic_result` cannot
be redefined, and source modules cannot shadow registry roots or `builtin`.

Conditions must be `bool`; `for` accepts only `list<T>`. Every function must
return its exact result on all paths; loops may execute zero times. There is
no constant folding. Dead code is still checked. Exactly one run entry must
name a source function with signature `(endpoint) -> forensic_result`.

The ten planned contracts in `forensic-lib/contracts/catalogue.v1.json` are
statically usable but unavailable for execution. The only composition helper is
`forensic_result(forensic_result, forensic_result) -> forensic_result`, also
unavailable. Calls must support every declared target, even in unused helpers
or unreachable code. Privileges/capabilities are metadata only.

An optional contextual declaration can appear between the target and functions:

```jocky
module lab_example
target ubuntu
profile lab
fn inspect(target: endpoint) -> forensic_result {
    return forensic.system.profile(target)
}
run inspect on selected_endpoints
```

`profile` and `lab` remain usable as ordinary identifiers elsewhere. Unknown,
duplicate or misplaced profiles are syntax errors. The declaration is static
opt-in for synthetic lab contract tests, not operational authorisation. No lab
operation is shipped. Profile AST output uses version `2.0.0`; other AST output
remains byte-compatible `1.0.0`. Schemas live in `shared/compiler-contracts/`.

Version 2 provides [module discovery](module-contracts.md),
[read-only metadata access](platform-capabilities.md) and
[human/JSON check reports](diagnostics.md). IR, code generation, an interpreter
and collectors remain unimplemented.

Resolved names, calls and exact types live in semantic side tables indexed by
compiler-local IDs; the syntax AST is not rewritten. A successful checked
program retains one bounded original-source copy so report metadata cannot
be paired with different source. Semantic failures carry no partial success.
Iterative call-graph analysis handles recursion as a finite graph, not by
executing function bodies; it makes no runtime-termination promise.

```sh
target/debug/jockyc check examples/triage.jky --diagnostic-format human
target/debug/jockyc check examples/triage.jky --diagnostic-format json
target/debug/jockyc modules list
target/debug/jockyc modules describe forensic.process.list
```

Only the JSON check emits success metadata. It describes static requirements,
including unavailable dependencies, not evidence collected from selected endpoints.
See [Version 2 verification](version-2-verification.md) for release evidence.
