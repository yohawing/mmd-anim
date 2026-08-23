from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, Mapping

from .artifacts import reject_reparse

GeneratorBackend = Literal["node-mmddumper", "rust-build-pmm"]
_GENERATOR_BACKENDS = frozenset(("node-mmddumper", "rust-build-pmm"))
_UNSUPPORTED_FEATURES = frozenset(("multi-model", "accessory"))
_NODE_OPT_IN_FEATURES = frozenset(("property-ik",))


@dataclass(frozen=True)
class ValidationIssue:
    """One stable, user-readable case contract violation."""

    field: str
    reason: str

    def as_dict(self) -> dict[str, str]:
        return {"field": self.field, "reason": self.reason}


class CaseValidationError(ValueError):
    """Raised when a case cannot be accepted without an implicit fallback."""

    def __init__(self, issues: list[ValidationIssue] | tuple[ValidationIssue, ...]):
        self.issues = tuple(issues)
        super().__init__(self._message())

    def _message(self) -> str:
        return "; ".join(f"{issue.field}: {issue.reason}" for issue in self.issues)

    def as_dict(self) -> dict[str, Any]:
        return {
            "code": "case-validation-error",
            "issues": [issue.as_dict() for issue in self.issues],
        }


@dataclass(frozen=True)
class OracleCase:
    """Validated case input shared by future prepare and record phases."""

    schema_version: int
    name: str
    pmx: Path
    body_vmd: Path
    camera_vmd: Path | None
    frames: tuple[int, ...]
    output_root: Path
    generator_backend: GeneratorBackend
    record_opt_in: bool
    dialog_opt_in: bool
    requested_features: tuple[str, ...]
    source_path: Path

    def as_dict(self) -> dict[str, Any]:
        input_data: dict[str, str] = {
            "pmx": str(self.pmx),
            "bodyVmd": str(self.body_vmd),
        }
        if self.camera_vmd is not None:
            input_data["cameraVmd"] = str(self.camera_vmd)
        return {
            "schemaVersion": self.schema_version,
            "name": self.name,
            "input": input_data,
            "frames": list(self.frames),
            "outputRoot": str(self.output_root),
            "generatorBackend": self.generator_backend,
            "recordOptIn": self.record_opt_in,
            "dialogOptIn": self.dialog_opt_in,
            "requestedFeatures": list(self.requested_features),
        }


def load_case(case_path: str | Path) -> OracleCase:
    """Read and validate one absolute case JSON file."""

    path = Path(case_path)
    issues: list[ValidationIssue] = []
    if not path.is_absolute():
        issues.append(ValidationIssue("casePath", "must be an absolute path"))
    if not path.exists():
        issues.append(ValidationIssue("casePath", "file does not exist"))
    elif not path.is_file():
        issues.append(ValidationIssue("casePath", "must be a file"))
    if issues:
        raise CaseValidationError(issues)

    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CaseValidationError((ValidationIssue("case", f"cannot read JSON: {error}"),)) from error
    return _validate_payload(payload, path.resolve())


def _validate_payload(payload: Any, source_path: Path) -> OracleCase:
    if not isinstance(payload, Mapping):
        raise CaseValidationError((ValidationIssue("case", "must be a JSON object"),))

    issues: list[ValidationIssue] = []
    schema_version = _required_int(payload, "schemaVersion", issues)
    if schema_version is not None and schema_version != 1:
        issues.append(ValidationIssue("schemaVersion", "must be 1"))
    name = _required_nonempty_string(payload, "name", issues)
    input_data = payload.get("input")
    if not isinstance(input_data, Mapping):
        issues.append(ValidationIssue("input", "must be a JSON object"))
        input_data = {}

    pmx = _required_absolute_file(input_data, "pmx", issues)
    body_vmd = _required_absolute_file(input_data, "bodyVmd", issues)
    camera_vmd = _optional_absolute_file(input_data, "cameraVmd", issues)
    frames = _frames(payload.get("frames"), issues)
    output_root = _required_absolute_path(payload, "outputRoot", issues)
    backend = _backend(payload.get("generatorBackend"), issues)
    record_opt_in = _required_bool(payload, "recordOptIn", issues)
    dialog_opt_in = _required_bool(payload, "dialogOptIn", issues)
    requested_features = _requested_features(payload.get("requestedFeatures"), issues)

    if backend == "rust-build-pmm" and camera_vmd is not None:
        issues.append(
            ValidationIssue(
                "input.cameraVmd",
                "capability unsupported for generatorBackend rust-build-pmm",
            )
        )
    for feature in requested_features:
        if feature in _UNSUPPORTED_FEATURES:
            issues.append(ValidationIssue("requestedFeatures", f"unsupported capability: {feature}"))
        elif feature in _NODE_OPT_IN_FEATURES and backend == "rust-build-pmm":
            issues.append(
                ValidationIssue(
                    "requestedFeatures",
                    "capability unsupported for generatorBackend rust-build-pmm: property-ik",
                )
            )
        elif feature not in _NODE_OPT_IN_FEATURES:
            issues.append(ValidationIssue("requestedFeatures", f"unknown or unsupported capability: {feature}"))

    if issues:
        raise CaseValidationError(issues)
    assert schema_version == 1
    assert name is not None
    assert pmx is not None and body_vmd is not None
    assert output_root is not None
    assert backend is not None
    assert record_opt_in is not None and dialog_opt_in is not None
    return OracleCase(
        schema_version=schema_version,
        name=name,
        pmx=pmx,
        body_vmd=body_vmd,
        camera_vmd=camera_vmd,
        frames=frames,
        output_root=output_root,
        generator_backend=backend,
        record_opt_in=record_opt_in,
        dialog_opt_in=dialog_opt_in,
        requested_features=requested_features,
        source_path=source_path,
    )


def _required_int(payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]) -> int | None:
    value = payload.get(field)
    if isinstance(value, bool) or not isinstance(value, int):
        issues.append(ValidationIssue(field, "must be an integer"))
        return None
    return value


def _required_nonempty_string(payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]) -> str | None:
    value = payload.get(field)
    if not isinstance(value, str) or not value.strip():
        issues.append(ValidationIssue(field, "must be a non-empty string"))
        return None
    return value


def _required_absolute_file(
    payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]
) -> Path | None:
    value = payload.get(field)
    path = _absolute_path(value, field, issues)
    if path is None:
        return None
    if not path.exists():
        issues.append(ValidationIssue(field, "file does not exist"))
    elif not path.is_file():
        issues.append(ValidationIssue(field, "must be a file"))
    return path


def _optional_absolute_file(
    payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]
) -> Path | None:
    if field not in payload or payload[field] is None:
        return None
    return _required_absolute_file(payload, field, issues)


def _required_absolute_path(payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]) -> Path | None:
    raw_path = _absolute_path(payload.get(field), field, issues, resolve=False)
    if raw_path is None:
        return None
    try:
        reject_reparse(raw_path)
    except OSError as error:
        issues.append(ValidationIssue(field, str(error)))
        return None
    path = raw_path.resolve()
    if path is not None and path.exists() and not path.is_dir():
        issues.append(ValidationIssue(field, "must be a directory when the path already exists"))
    return path


def _absolute_path(value: Any, field: str, issues: list[ValidationIssue], *, resolve: bool = True) -> Path | None:
    if not isinstance(value, str) or not value.strip():
        issues.append(ValidationIssue(field, "must be a non-empty absolute path"))
        return None
    path = Path(value)
    if not path.is_absolute():
        issues.append(ValidationIssue(field, "must be an absolute path"))
        return None
    return path.resolve() if resolve else path


def _frames(value: Any, issues: list[ValidationIssue]) -> tuple[int, ...]:
    if not isinstance(value, list) or not value:
        issues.append(ValidationIssue("frames", "must be a non-empty array"))
        return ()
    if any(isinstance(frame, bool) or not isinstance(frame, int) for frame in value):
        issues.append(ValidationIssue("frames", "each frame must be a non-negative integer"))
    if any(isinstance(frame, int) and not isinstance(frame, bool) and frame < 0 for frame in value):
        issues.append(ValidationIssue("frames", "each frame must be non-negative"))
    valid_frames = [frame for frame in value if isinstance(frame, int) and not isinstance(frame, bool)]
    if len(set(valid_frames)) != len(valid_frames):
        issues.append(ValidationIssue("frames", "duplicate frame values are not allowed"))
    return tuple(valid_frames)


def _backend(value: Any, issues: list[ValidationIssue]) -> GeneratorBackend | None:
    if not isinstance(value, str) or value not in _GENERATOR_BACKENDS:
        issues.append(
            ValidationIssue(
                "generatorBackend",
                "must be one of node-mmddumper or rust-build-pmm",
            )
        )
        return None
    return value  # type: ignore[return-value]


def _required_bool(payload: Mapping[str, Any], field: str, issues: list[ValidationIssue]) -> bool | None:
    value = payload.get(field)
    if not isinstance(value, bool):
        issues.append(ValidationIssue(field, "must be a boolean"))
        return None
    return value


def _requested_features(value: Any, issues: list[ValidationIssue]) -> tuple[str, ...]:
    if value is None:
        return ()
    if not isinstance(value, list) or any(not isinstance(feature, str) or not feature for feature in value):
        issues.append(ValidationIssue("requestedFeatures", "must be an array of non-empty strings"))
        return ()
    return tuple(value)
