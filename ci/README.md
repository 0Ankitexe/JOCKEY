# Continuous integration

## Linux-first scheduling

The [roadmap amendment](../docs/linux-first-roadmap.md) changes no CI job today.
Keep existing Windows/Ubuntu compiler/fixture regressions; they do not require
manual Windows work or establish real collector support. Revised Version 5 must
add exact Linux distribution/release/architecture/facility evidence. Native VM
ground truth is distinct from container/userspace checks. Windows artifact,
collector and agent acceptance is deferred into Version 16, not reported as passed.

GitHub Actions jobs live in `.github/workflows/ci.yml`. The Ubuntu and Windows
Rust jobs prepare the legacy schema, fetch locked dependencies, then run formatting,
offline Clippy and `cargo test --offline --workspace --locked`; the
workspace command includes the complete Version 0–3 suites and actual
`jockyc` binary tests on both newline/path platforms. The dedicated robustness
target uses a fixed seed and standard-library generation, so it is repeatable
without a fuzzing service.

Python jobs retain the Version 0 controller Ruff, mypy, and pytest gates and
explicitly check the preparation utility below with Ruff, strict mypy and pytest.
TypeScript jobs retain dashboard formatting, ESLint, type checking, Vitest, and
build gates. Compose validation checks configuration only and does not enable a
lab or endpoint adapter.

After dependencies are cached, all parser and fixture tests run offline. The
language core accepts caller-supplied text and performs no file, environment,
OS, network, runtime, forensic, or platform operation. Only the tested CLI
adapter reads the explicit input path and writes standard streams. Windows and
Ubuntu use the same grammar and AST contracts; there is no platform-specific
parser behavior.

## Offline legacy AST schema preparation

Before Rust checks on both Windows and Ubuntu, Python 3.12 runs:

```sh
python ci/prepare_legacy_ast_schema.py
```

This standard-library-only tool copies
`shared/compiler-contracts/ast-v1.schema.json` to the fixed path
`specs/001-language-parser-diagnostics/contracts/ast-json.schema.json` required
by the unchanged legacy test loader. Paths are resolved from the script's own
location, not the working directory. Missing destination: byte-exact copy;
identical destination: no-op; different destination: refuse without overwrite.
The tool has no flags, downloads, environment settings or Git operations. It
rejects symbolic-link path components and uses exclusive destination creation.

Typed setup failures exit 2 and have stable stderr without native exception
contents; success exits 0 with `prepared` or `unchanged`. If a failed write leaves
a partial file that cannot be removed, the next run refuses it. Inspect and
resolve such a file manually; there is deliberately no force option.

From the repository root, after controller development dependencies are installed:

```sh
controller/.venv/bin/python -m ruff format --check --config controller/pyproject.toml ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m ruff check --config controller/pyproject.toml ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m mypy --strict --explicit-package-bases ci/prepare_legacy_ast_schema.py tests/test_prepare_legacy_ast_schema.py
controller/.venv/bin/python -m pytest tests/test_prepare_legacy_ast_schema.py
```

New compiler-contract tests use deployed schemas directly; they do not depend
on planning artifacts. Test temporary directories stay under `target/`.
Version 3 adds verified IR and fixture-only interpretation, not collection,
code generation, transport or endpoint execution.
Configured hosted jobs are not evidence of a hosted run; consult the release
verification record for checks actually executed.

## Version 3 gates

Both Rust jobs explicitly repeat deployed schema/report/safety tests and the
actual fixture/file-adapter integrations after the full regression suite. The
workspace suite includes 20 IR goldens, 36 verifier-negative artifacts, ten-run
library/CLI determinism, resource boundaries and three fixed-seed 10,000-case IR
campaigns. Each campaign has a five-minute harness budget; jobs have a 20-minute
ceiling. No fuzzing service, native collector or live endpoint is used.

Windows runs its junction/reparse/device/ADS and retained-parent cases; Ubuntu
runs Unix symlink/FIFO/socket cases. Shared path-rule/unsupported-adapter tests
are not a substitute for executing the native platform tests. Only dependencies
are fetched before tests; test commands use `--locked --offline`.

Current stable is the verified local setup. The pre-existing workspace declaration
`rust-version = "1.85"` conflicts with locked ICU 2.3 (minimum 1.88) and
idna_adapter 1.2.2 (minimum 1.86); this release does not claim an MSRV pass or
silently change dependencies. Windows/Ubuntu hosted execution remains unobserved
locally. See [Version 3 verification](../docs/version-3-verification.md).
