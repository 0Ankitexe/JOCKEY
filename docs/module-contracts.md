# Module contracts

Version 2 provides an embedded catalogue of ten planned forensic
functions. You can inspect their interfaces and statically check calls, but
**all implementations are unavailable**. No invocation method, collector,
plugin loader, source execution or empty successful forensic result exists.

The [Linux-first roadmap](linux-first-roadmap.md) assigns shared collector
interfaces and a versioned Linux vocabulary/availability migration to revised
Version 5. The existing registry below is unchanged; do not interpret the roadmap
as making these functions executable. Windows operational support is deferred.

## Authority and versions

The deployable source of truth is
[module-registry.schema.json](../shared/compiler-contracts/module-registry.schema.json),
using the closed vocabulary in
[compiler-types.schema.json](../shared/compiler-contracts/compiler-types.schema.json).
The bundled instance is
[catalogue.v1.json](../forensic-lib/contracts/catalogue.v1.json).
The schemas and catalogue are compiled into the Rust library, not opened relative
to the working directory. HTTPS schema identifiers are identities, never downloads.
Unknown schema resource retrieval is denied.

Registry format `1.0.0` is independent of the product milestone and each
function's `0.1.0` semantic version. Full lexical SemVer, including prerelease
and build suffixes, is accepted within 64 ASCII characters. No version solving
is performed: two records with the same qualified name are invalid even if
their version numbers differ. A later schema revision is required before any
implemented availability status can be represented.

## Every function field

The root has exactly `schema_version` and `functions`. Each function requires
these twelve fields; no executable or extension fields are accepted:

| Field                 | Meaning                                                                          |
| --------------------- | -------------------------------------------------------------------------------- |
| `module`              | Case-sensitive dotted module identifier                                          |
| `name`                | Simple identifier; joined to module with a dot for lookup                        |
| `version`             | Function contract's semantic version                                             |
| `parameters`          | Ordered named, typed arguments; empty array means zero arguments                 |
| `result`              | One exact type; not implicitly wrapped in a result type                          |
| `supported_platforms` | Nonempty unique Windows/Ubuntu set; planned support only                         |
| `required_privilege`  | `user`, `elevated` or `lab_only`; descriptive, never elevation                   |
| `capability`          | One of the eight [capability categories](platform-capabilities.md)               |
| `read_only`           | Declaration of intended behavior; all shipped entries are true                   |
| `lab_only`            | Must be true exactly when privilege is `lab_only`; all shipped entries are false |
| `possible_errors`     | Ordered unique codes paired with the exact type `diagnostic_error`               |
| `availability`        | Only `unavailable` in registry format 1.0.0                                      |

Types are tagged JSON objects: named, list, option, or result. Nested types use
the same exact identities as the language. `path` is not `string`, and `bytes`
is not `list<int>`. Parameter names/order are part of the contract.

## Planned signatures

These are signatures for static checking, not executable declarations:

```text
forensic.driver.list(target: endpoint) -> list<driver_record>
forensic.event.list(target: endpoint, since: timestamp, until: timestamp, limit: int) -> list<event_record>
forensic.event.ubuntu_journal(target: endpoint, since: timestamp, until: timestamp, limit: int) -> list<event_record>
forensic.event.windows_log(target: endpoint, channel: string, since: timestamp, until: timestamp, limit: int) -> list<event_record>
forensic.file.inspect(target: endpoint, location: path) -> file_record
forensic.indicator.correlate(system: forensic_result, processes: list<process_record>, connections: list<connection_record>) -> forensic_result
forensic.network.connections(target: endpoint) -> list<connection_record>
forensic.persistence.list(target: endpoint) -> list<persistence_record>
forensic.process.list(target: endpoint) -> list<process_record>
forensic.system.profile(target: endpoint) -> forensic_result
```

`windows_log` supports Windows only; `ubuntu_journal` supports Ubuntu only.
The other eight support both as planned contracts. Driver/event calls require
`elevated`; all others require `user`. All are version `0.1.0`, read-only,
non-lab and unavailable. Driver/persistence signatures describe inventory only,
never loading drivers or installing persistence. Event parameters describe a
future bounded query; checking does not evaluate timestamp ordering or limit
values. Opaque values can flow through typed helper parameters without new
constructors. See the [platform examples](platform-capabilities.md).

The compiler separately recognises just one bare composition signature:

```text
forensic_result(system: forensic_result, indicators: forensic_result) -> forensic_result
```

It is unavailable, requires user privilege, supports both targets and adds no
capability. It cannot be redefined. It is not a registry entry or lookup alias,
and it does not construct runtime data. Its bare name appears in dependency
metadata only when used.

## Possible typed errors

The closed codes are `NotImplemented`, `Unsupported`, `PermissionDenied`,
`InvalidInput`, `NotFound`, `ResourceLimit`, `Timeout` and `CollectionFailed`.
Each is paired with `diagnostic_error`. Every record must include
`NotImplemented` or `Unsupported`. These are possible future outcomes, not
exceptions raised by checking a well-typed call to an unavailable contract.
They do not implicitly change `T` into `result<T, diagnostic_error>`.

Descriptions preserve each record's declared error order. For example, process
listing declares Unsupported, PermissionDenied, ResourceLimit, CollectionFailed;
indicator correlation declares NotImplemented, InvalidInput, ResourceLimit.
The catalogue tests pin all ten records' complete ordered error lists.

## Strict validation and limits

`Registry::from_json(&str)` is pure and fallible. It bounds input and rejects
duplicate JSON object keys before ordinary maps can discard them. It validates
the entire schema, decodes closed typed structures, checks consistency, then
creates an immutable sorted registry. Failed validation returns no usable partial
registry. `builtin_registry()` caches a fallible result, never an empty fallback.

| Resource                         |                     Maximum |
| -------------------------------- | --------------------------: |
| JSON bytes                       |                   1,048,576 |
| Nested JSON containers           |                          96 |
| Functions                        | 512 (at least one required) |
| Parameters per function          |                          64 |
| Bytes per identifier segment     |                          64 |
| Bytes per module path            |                         128 |
| ASCII characters per version     |                          64 |
| Type depth, including named leaf |                          64 |
| Total type-node occurrences      |                      32,768 |

Missing fields, unknown properties/enums/types, malformed names or versions,
empty/duplicate platforms, duplicate identities/parameters/error codes, and
contradictory lab metadata all fail. A false `read_only` flag is representable
in an inert test registry but never enables execution or relaxes the security
boundary. No such record is shipped.

Registry errors are typed: `InvalidJson`, `DuplicateJsonKey`,
`UnsupportedVersion`, `SchemaViolation`, `DuplicateFunction`,
`DuplicateParameter`, `DuplicateErrorCode`, `InvalidIdentifier`, `ResourceLimit`.
They carry a fixed reason key and optional JSON pointer, not native/dependency
error prose. Lookup distinguishes `InvalidName` and `UnknownFunction`.

## Demonstration and CLI results

From the repository root, with dependencies installed:

```sh
CARGO_NET_OFFLINE=true cargo build --locked -p jocky-language --bin jockyc
target/debug/jockyc modules list
target/debug/jockyc modules describe forensic.process.list
target/debug/jockyc modules describe forensic.event.windows_log
target/debug/jockyc modules describe forensic.event.ubuntu_journal
target/debug/jockyc modules describe forensic.missing.function
target/debug/jockyc modules describe forensic_result
```

List prints all ten qualified names in ASCII order, each followed by version
and `unavailable`. Describe prints the twelve fields above, in that order,
one LF-terminated line each. Parameters retain signature order (`()` for none);
platforms display Windows before Ubuntu; errors retain declaration order.
Success uses stdout, empty stderr and exit 0. These commands work from any
directory and never call the source reader.

The last two examples deliberately exit 2 with `error[module-not-found]` on
stderr and empty stdout. Malformed names get the same stable error, with control
characters visibly escaped. Bad embedded registry data gives
`error[invalid-registry]: module registry is invalid`. Extra/reordered options,
unsupported flags and non-UTF-8 module arguments produce usage and exit 2.
There is no external-registry option. Write/flush failures exit 2 without a
complete-output promise.

Verification: `cargo test --locked -p jocky-forensic` checks schema, consistency
and exact catalogue values; `cargo test --locked -p jocky-language --test module_cli`
checks the real binary, including an empty repository-local working directory
and source-reader/write/flush failure sentinels. Unit tests inject invalid and
synthetic registries only through private seams. See
[verification evidence](version-2-verification.md) for actual commands/results.
