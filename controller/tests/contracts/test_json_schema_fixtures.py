"""Cross-language JSON Schema source-of-truth smoke and negative tests."""

import json
from collections.abc import Iterator
from pathlib import Path
from typing import Any, cast

import pytest
from jsonschema import FormatChecker, ValidationError
from jsonschema.protocols import Validator
from jsonschema.validators import validator_for

REPOSITORY_ROOT = Path(__file__).resolve().parents[3]
SCHEMA_DIRECTORY = REPOSITORY_ROOT / "shared" / "schemas"
VALID_FIXTURE_DIRECTORY = REPOSITORY_ROOT / "fixtures" / "valid"
INVALID_FIXTURE_DIRECTORY = REPOSITORY_ROOT / "fixtures" / "invalid"

EXPECTED_CONTRACTS = {
    "endpoint-identity",
    "task-envelope",
    "task-status-event",
    "forensic-record",
    "evidence-manifest",
    "build-manifest",
}
DISCOVERED_CONTRACTS = tuple(
    sorted(
        path.name.removesuffix(".schema.json") for path in SCHEMA_DIRECTORY.glob("*.schema.json")
    )
)
CONTRACT_IDENTIFIERS = {
    "endpoint-identity": "endpoint_id",
    "task-envelope": "task_id",
    "task-status-event": "event_id",
    "forensic-record": "record_id",
    "evidence-manifest": "manifest_id",
    "build-manifest": "build_id",
}
TASK_STATES = {
    "queued",
    "assigned",
    "delivered",
    "running",
    "completed",
    "failed",
    "cancelled",
    "expired",
}


def _load_object(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as source:
        value = json.load(source)
    assert isinstance(value, dict), f"{path} must contain a JSON object"
    return cast(dict[str, Any], value)


def _schema_path(contract: str) -> Path:
    return SCHEMA_DIRECTORY / f"{contract}.schema.json"


def _fixture_path(directory: Path, contract: str, suffix: str = "") -> Path:
    return directory / f"{contract}{suffix}.json"


def _validator(contract: str) -> Validator:
    schema = _load_object(_schema_path(contract))
    validator_class = validator_for(schema)
    validator_class.check_schema(schema)
    return validator_class(schema, format_checker=FormatChecker())


def _errors(contract: str, fixture: Path) -> list[ValidationError]:
    return sorted(_validator(contract).iter_errors(_load_object(fixture)), key=str)


def _required_identifier(contract: str, schema: dict[str, Any], instance: dict[str, Any]) -> str:
    required = schema.get("required")
    assert isinstance(required, list), "contract schema must declare required fields"
    identifier_fields = [
        field
        for field in required
        if isinstance(field, str) and (field == "id" or field.endswith("_id"))
    ]
    missing = [field for field in identifier_fields if field not in instance]
    assert len(missing) == 1, "missing-id fixture must omit exactly one required identifier"
    expected_identifier = CONTRACT_IDENTIFIERS[contract]
    assert missing[0] == expected_identifier
    return expected_identifier


def _required_error(errors: list[ValidationError], identifier: str) -> ValidationError:
    matches = [
        error
        for error in errors
        if error.validator == "required"
        and not error.absolute_path
        and f"'{identifier}' is a required property" in error.message
    ]
    assert len(matches) == 1, f"expected one root required error for {identifier}"
    return matches[0]


def test_schema_catalog_contains_exact_version_zero_contracts() -> None:
    assert set(DISCOVERED_CONTRACTS) == EXPECTED_CONTRACTS


@pytest.mark.parametrize("contract", DISCOVERED_CONTRACTS)
def test_contract_schema_is_valid_and_fixtures_exist(contract: str) -> None:
    validator = _validator(contract)

    assert validator.META_SCHEMA
    assert _fixture_path(VALID_FIXTURE_DIRECTORY, contract).is_file()
    assert _fixture_path(INVALID_FIXTURE_DIRECTORY, contract, "--missing-id").is_file()


@pytest.mark.parametrize("contract", DISCOVERED_CONTRACTS)
def test_valid_contract_fixture_passes_validation(contract: str) -> None:
    fixture = _fixture_path(VALID_FIXTURE_DIRECTORY, contract)

    assert list(_validator(contract).iter_errors(_load_object(fixture))) == []


@pytest.mark.parametrize("contract", DISCOVERED_CONTRACTS)
def test_missing_identifier_fixture_fails_required_keyword_at_root(contract: str) -> None:
    schema = _load_object(_schema_path(contract))
    fixture_path = _fixture_path(INVALID_FIXTURE_DIRECTORY, contract, "--missing-id")
    instance = _load_object(fixture_path)
    identifier = _required_identifier(contract, schema, instance)

    error = _required_error(_errors(contract, fixture_path), identifier)

    assert error.validator_value == schema["required"]
    assert list(error.absolute_schema_path)[-1] == "required"


def test_unknown_task_state_fails_enum_keyword_at_state_path() -> None:
    fixture = _fixture_path(
        INVALID_FIXTURE_DIRECTORY,
        "task-status-event",
        "--unknown-state",
    )
    errors = _errors("task-status-event", fixture)
    enum_errors = [
        error
        for error in errors
        if error.validator == "enum" and list(error.absolute_path) == ["state"]
    ]

    assert len(enum_errors) == 1
    assert set(enum_errors[0].validator_value) == TASK_STATES
    assert enum_errors[0].instance not in TASK_STATES


def test_malformed_endpoint_id_fails_uuid_contract_at_endpoint_id_path() -> None:
    fixture = _fixture_path(
        INVALID_FIXTURE_DIRECTORY,
        "endpoint-identity",
        "--malformed-id",
    )
    errors = _errors("endpoint-identity", fixture)
    identifier_errors = [
        error
        for error in errors
        if error.validator in {"format", "pattern"} and list(error.absolute_path) == ["endpoint_id"]
    ]

    assert {error.validator for error in identifier_errors} == {"format", "pattern"}
    assert all(error.instance == "endpoint-1" for error in identifier_errors)


def test_non_utc_timestamp_fails_pattern_contract_at_issued_at_path() -> None:
    fixture = _fixture_path(
        INVALID_FIXTURE_DIRECTORY,
        "task-envelope",
        "--non-utc-timestamp",
    )
    errors = _errors("task-envelope", fixture)
    utc_errors = [
        error
        for error in errors
        if error.validator == "pattern" and list(error.absolute_path) == ["issued_at"]
    ]

    assert len(utc_errors) == 1
    assert utc_errors[0].instance == "2026-08-30T17:30:00+05:30"
    assert utc_errors[0].validator_value.endswith("Z$")


def test_semantically_invalid_timestamp_fails_date_time_format() -> None:
    assert "date-time" in FormatChecker.checkers
    fixture = _fixture_path(
        INVALID_FIXTURE_DIRECTORY,
        "task-envelope",
        "--invalid-timestamp",
    )
    errors = _errors("task-envelope", fixture)
    format_errors = [
        error
        for error in errors
        if error.validator == "format" and list(error.absolute_path) == ["issued_at"]
    ]

    assert len(format_errors) == 1
    assert format_errors[0].instance == "2026-99-99T99:99:99Z"
    assert format_errors[0].validator_value == "date-time"


def test_fixture_directories_contain_only_declared_contract_examples() -> None:
    expected_valid = {f"{contract}.json" for contract in DISCOVERED_CONTRACTS}
    expected_invalid = {
        *(f"{contract}--missing-id.json" for contract in DISCOVERED_CONTRACTS),
        "endpoint-identity--malformed-id.json",
        "task-envelope--invalid-timestamp.json",
        "task-envelope--non-utc-timestamp.json",
        "task-status-event--unknown-state.json",
    }

    def json_names(directory: Path) -> Iterator[str]:
        return (path.name for path in directory.glob("*.json"))

    assert set(json_names(VALID_FIXTURE_DIRECTORY)) == expected_valid
    assert set(json_names(INVALID_FIXTURE_DIRECTORY)) == expected_invalid
