"""Freeze a deterministic, local-only PMX/VMD campaign selection."""

from __future__ import annotations

import hashlib
import json
import math
import os
import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from .artifacts import reject_reparse

SELECTION_SCHEMA_VERSION = 1
DEFAULT_MAX_PMX_BYTES = 256 * 1024 * 1024
DEFAULT_MAX_VMD_BYTES = 128 * 1024 * 1024
_THRESHOLD_NAMES = {
    "translationMaxError",
    "translationRmsError",
    "rotationMaxAngleRad",
    "rotationRmsAngleRad",
    "maxAbsError",
}


class SelectionError(ValueError):
    """Stable validation or discovery failure for a frozen selection."""

    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(message)

    def as_dict(self) -> dict[str, str]:
        return {"kind": "selection", "code": self.code, "message": self.message}


@dataclass(frozen=True)
class _Candidate:
    path: Path
    relative_path: str
    size: int


def freeze_selection(
    pmx_root: Path,
    vmd_root: Path,
    output_path: Path,
    *,
    count: int,
    seed: str,
    frames: tuple[int, ...] = (0, 15, 30, 60, 120),
    max_pmx_bytes: int = DEFAULT_MAX_PMX_BYTES,
    max_vmd_bytes: int = DEFAULT_MAX_VMD_BYTES,
) -> dict[str, Any]:
    """Discover, select, content-hash, and exclusively write one local list."""

    pmx_root = _absolute_directory(pmx_root, "pmx-root")
    vmd_root = _absolute_directory(vmd_root, "vmd-root")
    output_path = _absolute_output(output_path)
    _validate_options(count, seed, frames, max_pmx_bytes, max_vmd_bytes)

    pmx_files = list(_walk_files(pmx_root, ".pmx"))
    vmd_files = list(_walk_files(vmd_root, ".vmd"))
    pmx_candidates = [candidate for candidate in pmx_files if _eligible_pmx(candidate, max_pmx_bytes)]
    vmd_candidates = [candidate for candidate in vmd_files if _eligible_body_vmd(candidate, max_vmd_bytes)]
    if len(pmx_candidates) < count or len(vmd_candidates) < count:
        raise SelectionError(
            "insufficient-assets",
            f"requested {count} cases but only {len(pmx_candidates)} eligible PMX and "
            f"{len(vmd_candidates)} eligible body VMD files were found",
        )

    selected_pmx = _rank(pmx_candidates, seed, "pmx")[:count]
    selected_vmd = _rank(vmd_candidates, seed, "vmd")[:count]
    cases = []
    for index, (pmx, vmd) in enumerate(zip(selected_pmx, selected_vmd, strict=True), start=1):
        pair_hash = hashlib.sha256(
            f"{pmx.relative_path}\0{vmd.relative_path}".encode("utf-8")
        ).hexdigest()[:12]
        cases.append(
            {
                "caseId": f"library-{index:04d}-{pair_hash}",
                "pmx": _asset_entry(pmx),
                "bodyVmd": _asset_entry(vmd),
                "features": ["bone-motion"],
                "categories": ["deterministic-library-sample"],
            }
        )

    payload: dict[str, Any] = {
        "schemaVersion": SELECTION_SCHEMA_VERSION,
        "seed": seed,
        "requestedCases": count,
        "frames": list(frames),
        "roots": {"pmx": str(pmx_root), "vmd": str(vmd_root)},
        "policy": {
            "pmxExtension": ".pmx",
            "vmdExtension": ".vmd",
            "maxPmxBytes": max_pmx_bytes,
            "maxVmdBytes": max_vmd_bytes,
            "requireBodyBoneFrames": True,
            "ordering": "sha256(seed-kind-relative-path-v1)",
        },
        "discovery": {
            "pmxFiles": len(pmx_files),
            "vmdFiles": len(vmd_files),
            "eligiblePmx": len(pmx_candidates),
            "eligibleBodyVmd": len(vmd_candidates),
            "selected": count,
        },
        "cases": cases,
    }
    payload["selectionHash"] = _selection_hash(payload)
    _exclusive_write_json(output_path, payload)
    return payload


def load_selection(path: Path) -> dict[str, Any]:
    """Strictly read and validate a frozen selection and its content hashes."""

    path = Path(path)
    if not path.is_absolute() or not path.is_file():
        raise SelectionError("selection-path", "selection must be an existing absolute file")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SelectionError("selection-json", f"cannot read selection JSON: {error}") from error
    if not isinstance(value, dict):
        raise SelectionError("selection-schema", "selection root must be an object")
    expected = {
        "schemaVersion",
        "seed",
        "requestedCases",
        "frames",
        "roots",
        "policy",
        "discovery",
        "cases",
        "selectionHash",
    }
    if set(value) != expected or value.get("schemaVersion") != SELECTION_SCHEMA_VERSION:
        raise SelectionError("selection-schema", "selection fields or schemaVersion are unsupported")
    claimed_hash = value.get("selectionHash")
    without_hash = {key: item for key, item in value.items() if key != "selectionHash"}
    try:
        actual_hash = _selection_hash(without_hash)
    except (TypeError, ValueError) as error:
        raise SelectionError("selection-schema", "selection contains non-canonical JSON values") from error
    if not _is_sha256(claimed_hash) or claimed_hash != actual_hash:
        raise SelectionError("selection-hash", "selectionHash does not match the frozen content")
    requested_cases = value.get("requestedCases")
    seed = value.get("seed")
    frames = value.get("frames")
    if isinstance(requested_cases, bool) or not isinstance(requested_cases, int) or requested_cases <= 0:
        raise SelectionError("selection-schema", "requestedCases must be a positive integer")
    if not isinstance(seed, str) or not seed or "\r" in seed or "\n" in seed:
        raise SelectionError("selection-schema", "seed must be a non-empty single-line string")
    if not isinstance(frames, list):
        raise SelectionError("selection-schema", "frames must be an array")
    _validate_options(requested_cases, seed, tuple(frames), 1, 1)
    roots = value.get("roots")
    if not isinstance(roots, dict) or set(roots) != {"pmx", "vmd"}:
        raise SelectionError("selection-schema", "roots must contain exactly pmx and vmd")
    if any(not isinstance(root, str) or not Path(root).is_absolute() for root in roots.values()):
        raise SelectionError("selection-schema", "selection roots must be absolute paths")
    _validate_loaded_policy(value.get("policy"))
    _validate_loaded_discovery(value.get("discovery"), requested_cases)
    cases = value.get("cases")
    if not isinstance(cases, list) or len(cases) != requested_cases:
        raise SelectionError("selection-schema", "cases must match requestedCases")
    seen_ids: set[str] = set()
    for index, case in enumerate(cases):
        if not isinstance(case, dict) or set(case) != {
            "caseId",
            "pmx",
            "bodyVmd",
            "features",
            "categories",
        }:
            raise SelectionError("selection-schema", f"cases[{index}] is invalid")
        case_id = case.get("caseId")
        if not _is_safe_case_id(case_id) or case_id in seen_ids:
            raise SelectionError("selection-schema", f"cases[{index}].caseId is invalid or duplicated")
        seen_ids.add(case_id)
        _validate_asset_entry(case.get("pmx"), f"cases[{index}].pmx")
        _validate_asset_entry(case.get("bodyVmd"), f"cases[{index}].bodyVmd")
        _validate_tags(case.get("features"), f"cases[{index}].features")
        _validate_tags(case.get("categories"), f"cases[{index}].categories")
        _validate_asset_root(case["pmx"], Path(roots["pmx"]), f"cases[{index}].pmx")
        _validate_asset_root(case["bodyVmd"], Path(roots["vmd"]), f"cases[{index}].bodyVmd")
    return value


def verify_selection(path: Path) -> dict[str, Any]:
    """Re-hash every frozen asset and fail if the local library drifted."""

    selection = load_selection(path)
    return _verify_loaded_selection(selection)


def _verify_loaded_selection(selection: dict[str, Any]) -> dict[str, Any]:
    """Verify assets for the exact already-validated selection object."""

    checked = 0
    for case in selection["cases"]:
        for field in ("pmx", "bodyVmd"):
            asset = case[field]
            asset_path = Path(asset["path"])
            if not asset_path.is_file():
                raise SelectionError("asset-missing", f"frozen asset is missing: {asset_path}")
            try:
                reject_reparse(asset_path)
                size = asset_path.stat().st_size
                digest = _hash_file(asset_path)
            except OSError as error:
                raise SelectionError("asset-read", f"cannot verify frozen asset: {asset_path}") from error
            if size != asset["size"] or digest != asset["sha256"]:
                raise SelectionError("asset-drift", f"frozen asset changed: {asset_path}")
            checked += 1
    return {
        "ok": True,
        "selectionHash": selection["selectionHash"],
        "cases": len(selection["cases"]),
        "assetsChecked": checked,
    }


def materialize_selection(selection_path: Path, template_path: Path, output_dir: Path) -> dict[str, Any]:
    """Create local case contracts and one campaign config from a frozen list."""

    output_dir = Path(output_dir)
    if not output_dir.is_absolute() or not output_dir.parent.is_dir():
        raise SelectionError("materialize-path", "output-dir must have an existing absolute parent")
    if output_dir.exists():
        raise SelectionError("materialize-exists", "output-dir already exists and is not overwritten")
    selection = load_selection(selection_path)
    verification = _verify_loaded_selection(selection)
    template = _load_materialization_template(template_path)
    try:
        reject_reparse(output_dir.parent)
        output_dir.mkdir()
        cases_dir = output_dir / "cases"
        cases_dir.mkdir()
        campaign_cases = []
        for selected_case in selection["cases"]:
            case_path = cases_dir / f"{selected_case['caseId']}.json"
            if case_path.resolve().parent != cases_dir.resolve():
                raise SelectionError("materialize-path", "case path escapes the exact cases directory")
            case_payload = {
                "schemaVersion": 1,
                "name": selected_case["caseId"],
                "input": {
                    "pmx": selected_case["pmx"]["path"],
                    "bodyVmd": selected_case["bodyVmd"]["path"],
                },
                "frames": selection["frames"],
                "outputRoot": template["outputRoot"],
                "generatorBackend": "python-rust",
                "recordOptIn": True,
                "dialogOptIn": False,
                "requestedFeatures": [],
            }
            _exclusive_write_json(case_path, case_payload)
            campaign_cases.append(
                {
                    "caseFile": str(case_path.resolve()),
                    "caseId": selected_case["caseId"],
                    "features": selected_case["features"],
                    "categories": selected_case["categories"],
                }
            )
        campaign_payload = {
            "schemaVersion": 1,
            "selectionFile": str(Path(selection_path).resolve()),
            "selectionHash": selection["selectionHash"],
            "run": template["run"],
            "discovered": min(
                selection["discovery"]["eligiblePmx"],
                selection["discovery"]["eligibleBodyVmd"],
            ),
            "compare": template["compare"],
            "cases": campaign_cases,
        }
        campaign_path = output_dir / "campaign.json"
        _exclusive_write_json(campaign_path, campaign_payload)
    except (OSError, SelectionError) as error:
        if isinstance(error, SelectionError):
            raise
        raise SelectionError(
            "materialize-write",
            f"cannot materialize selection; partial output may remain at {output_dir}: {error}",
        ) from error
    return {
        "ok": True,
        "selectionHash": verification["selectionHash"],
        "cases": len(campaign_cases),
        "campaign": str(campaign_path.resolve()),
    }


def _absolute_directory(path: Path, label: str) -> Path:
    path = Path(path)
    if not path.is_absolute() or not path.is_dir():
        raise SelectionError("selection-path", f"{label} must be an existing absolute directory")
    try:
        reject_reparse(path)
    except OSError as error:
        raise SelectionError("selection-path", f"{label} is unsafe: {error}") from error
    return path.resolve()


def _load_materialization_template(path: Path) -> dict[str, Any]:
    path = Path(path)
    if not path.is_absolute() or not path.is_file():
        raise SelectionError("materialize-template", "template must be an existing absolute JSON file")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SelectionError("materialize-template", f"cannot read template JSON: {error}") from error
    if not isinstance(value, dict) or set(value) != {"schemaVersion", "run", "compare", "outputRoot"}:
        raise SelectionError("materialize-template", "template fields are unsupported")
    if value.get("schemaVersion") != 1:
        raise SelectionError("materialize-template", "template schemaVersion must be 1")
    run = value.get("run")
    if not isinstance(run, dict) or set(run) != {"mmdVersion", "dumperVersion", "timestamp", "samplingPolicy"}:
        raise SelectionError("materialize-template", "template run fields are unsupported")
    if any(not isinstance(item, str) or not item or "\r" in item or "\n" in item for item in run.values()):
        raise SelectionError("materialize-template", "template run values must be non-empty single-line strings")
    compare = value.get("compare")
    if not isinstance(compare, dict) or set(compare) != {"focusBones", "thresholds"}:
        raise SelectionError("materialize-template", "template compare fields are unsupported")
    _validate_tags(compare.get("focusBones"), "template.compare.focusBones")
    thresholds = compare.get("thresholds")
    if not isinstance(thresholds, dict) or set(thresholds) != _THRESHOLD_NAMES:
        raise SelectionError("materialize-template", "template thresholds are incomplete")
    if any(
        isinstance(item, bool)
        or not isinstance(item, (int, float))
        or not math.isfinite(item)
        or item < 0
        for item in thresholds.values()
    ):
        raise SelectionError("materialize-template", "template thresholds must be finite and non-negative")
    output_root = value.get("outputRoot")
    if not isinstance(output_root, str) or not Path(output_root).is_absolute():
        raise SelectionError("materialize-template", "template outputRoot must be absolute")
    return value


def _absolute_output(path: Path) -> Path:
    path = Path(path)
    if not path.is_absolute() or not path.parent.is_dir():
        raise SelectionError("selection-path", "output must have an existing absolute parent directory")
    if path.exists():
        raise SelectionError("selection-exists", "output already exists; frozen selections are not overwritten")
    try:
        reject_reparse(path.parent)
    except OSError as error:
        raise SelectionError("selection-path", f"output path is unsafe: {error}") from error
    return path.resolve()


def _validate_options(
    count: int,
    seed: str,
    frames: tuple[int, ...],
    max_pmx_bytes: int,
    max_vmd_bytes: int,
) -> None:
    if isinstance(count, bool) or not isinstance(count, int) or count <= 0:
        raise SelectionError("selection-option", "count must be a positive integer")
    if not isinstance(seed, str) or not seed.strip() or "\n" in seed or "\r" in seed:
        raise SelectionError("selection-option", "seed must be a non-empty single-line string")
    if not frames or any(isinstance(frame, bool) or not isinstance(frame, int) or frame < 0 for frame in frames):
        raise SelectionError("selection-option", "frames must contain non-negative integers")
    if tuple(sorted(set(frames))) != frames:
        raise SelectionError("selection-option", "frames must be unique and ascending")
    if any(isinstance(limit, bool) or not isinstance(limit, int) or limit <= 0 for limit in (max_pmx_bytes, max_vmd_bytes)):
        raise SelectionError("selection-option", "asset byte limits must be positive integers")


def _walk_files(root: Path, extension: str) -> Iterable[_Candidate]:
    for directory, names, files in os.walk(root, topdown=True, followlinks=False):
        directory_path = Path(directory)
        safe_names = []
        for name in names:
            child = directory_path / name
            try:
                reject_reparse(child)
            except OSError:
                continue
            safe_names.append(name)
        names[:] = sorted(safe_names, key=str.casefold)
        for name in sorted(files, key=str.casefold):
            if Path(name).suffix.casefold() != extension:
                continue
            path = directory_path / name
            try:
                reject_reparse(path)
                stat_result = path.stat()
            except OSError:
                continue
            if not path.is_file():
                continue
            relative = path.relative_to(root).as_posix()
            yield _Candidate(path.resolve(), relative, stat_result.st_size)


def _eligible_pmx(candidate: _Candidate, max_bytes: int) -> bool:
    if candidate.size < 9 or candidate.size > max_bytes:
        return False
    try:
        with candidate.path.open("rb") as stream:
            return stream.read(4) == b"PMX "
    except OSError:
        return False


def _eligible_body_vmd(candidate: _Candidate, max_bytes: int) -> bool:
    if candidate.size < 54 or candidate.size > max_bytes:
        return False
    try:
        with candidate.path.open("rb") as stream:
            header = stream.read(54)
    except OSError:
        return False
    return header.startswith(b"Vocaloid Motion Data") and struct.unpack_from("<I", header, 50)[0] > 0


def _rank(candidates: list[_Candidate], seed: str, kind: str) -> list[_Candidate]:
    return sorted(
        candidates,
        key=lambda candidate: (
            hashlib.sha256(f"{seed}\0{kind}\0{candidate.relative_path.casefold()}".encode("utf-8")).digest(),
            candidate.relative_path.casefold(),
        ),
    )


def _asset_entry(candidate: _Candidate) -> dict[str, Any]:
    try:
        size_before = candidate.path.stat().st_size
        digest = _hash_file(candidate.path)
        size_after = candidate.path.stat().st_size
    except OSError as error:
        raise SelectionError("asset-read", f"cannot hash selected asset: {candidate.path}") from error
    if size_before != candidate.size or size_after != candidate.size:
        raise SelectionError("asset-drift", f"selected asset changed during freezing: {candidate.path}")
    return {
        "path": str(candidate.path),
        "relativePath": candidate.relative_path,
        "size": candidate.size,
        "sha256": digest,
    }


def _hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _selection_hash(payload: dict[str, Any]) -> str:
    canonical = json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def _exclusive_write_json(path: Path, payload: dict[str, Any]) -> None:
    created = False
    try:
        text = json.dumps(payload, ensure_ascii=True, indent=2, allow_nan=False) + "\n"
        with path.open("x", encoding="utf-8", newline="\n") as stream:
            created = True
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
    except OSError as error:
        if created:
            try:
                path.unlink(missing_ok=True)
            except OSError:
                pass
        raise SelectionError("selection-write", f"cannot write frozen selection: {error}") from error


def _validate_asset_entry(value: object, label: str) -> None:
    if not isinstance(value, dict) or set(value) != {"path", "relativePath", "size", "sha256"}:
        raise SelectionError("selection-schema", f"{label} is invalid")
    path = value.get("path")
    relative = value.get("relativePath")
    size = value.get("size")
    if not isinstance(path, str) or not Path(path).is_absolute():
        raise SelectionError("selection-schema", f"{label}.path must be absolute")
    if not _is_safe_relative_path(relative):
        raise SelectionError("selection-schema", f"{label}.relativePath must be relative")
    if isinstance(size, bool) or not isinstance(size, int) or size < 0:
        raise SelectionError("selection-schema", f"{label}.size must be a non-negative integer")
    if not _is_sha256(value.get("sha256")):
        raise SelectionError("selection-schema", f"{label}.sha256 must be SHA-256")


def _validate_loaded_policy(value: object) -> None:
    expected = {
        "pmxExtension",
        "vmdExtension",
        "maxPmxBytes",
        "maxVmdBytes",
        "requireBodyBoneFrames",
        "ordering",
    }
    if not isinstance(value, dict) or set(value) != expected:
        raise SelectionError("selection-schema", "policy fields are unsupported")
    if value.get("pmxExtension") != ".pmx" or value.get("vmdExtension") != ".vmd":
        raise SelectionError("selection-schema", "policy extensions are unsupported")
    if value.get("requireBodyBoneFrames") is not True:
        raise SelectionError("selection-schema", "policy must require body bone frames")
    if value.get("ordering") != "sha256(seed-kind-relative-path-v1)":
        raise SelectionError("selection-schema", "policy ordering is unsupported")
    for field in ("maxPmxBytes", "maxVmdBytes"):
        item = value.get(field)
        if isinstance(item, bool) or not isinstance(item, int) or item <= 0:
            raise SelectionError("selection-schema", f"policy.{field} must be a positive integer")


def _validate_loaded_discovery(value: object, requested_cases: int) -> None:
    expected = {"pmxFiles", "vmdFiles", "eligiblePmx", "eligibleBodyVmd", "selected"}
    if not isinstance(value, dict) or set(value) != expected:
        raise SelectionError("selection-schema", "discovery fields are unsupported")
    if any(isinstance(item, bool) or not isinstance(item, int) or item < 0 for item in value.values()):
        raise SelectionError("selection-schema", "discovery counts must be non-negative integers")
    if value["selected"] != requested_cases:
        raise SelectionError("selection-schema", "discovery.selected must match requestedCases")
    if value["eligiblePmx"] > value["pmxFiles"] or value["eligibleBodyVmd"] > value["vmdFiles"]:
        raise SelectionError("selection-schema", "eligible discovery counts exceed discovered files")
    if requested_cases > min(value["eligiblePmx"], value["eligibleBodyVmd"]):
        raise SelectionError("selection-schema", "selected cases exceed eligible assets")


def _validate_asset_root(value: dict[str, Any], root: Path, label: str) -> None:
    resolved_root = root.resolve()
    expected = (resolved_root / value["relativePath"]).resolve()
    resolved_path = Path(value["path"]).resolve()
    if not expected.is_relative_to(resolved_root) or not resolved_path.is_relative_to(resolved_root):
        raise SelectionError("selection-schema", f"{label} escapes its frozen root")
    if resolved_path != expected:
        raise SelectionError("selection-schema", f"{label} path does not match its root and relativePath")


def _validate_tags(value: object, label: str) -> None:
    if not isinstance(value, list) or not value:
        raise SelectionError("selection-schema", f"{label} must be a non-empty array")
    if any(not isinstance(item, str) or not item or "\r" in item or "\n" in item for item in value):
        raise SelectionError("selection-schema", f"{label} contains an invalid tag")
    if len(set(value)) != len(value):
        raise SelectionError("selection-schema", f"{label} contains duplicate tags")


def _is_safe_case_id(value: object) -> bool:
    return (
        isinstance(value, str)
        and bool(value)
        and value not in {".", ".."}
        and ".." not in value
        and not any(character in value for character in ("/", "\\", ":", "\r", "\n"))
    )


def _is_safe_relative_path(value: object) -> bool:
    if (
        not isinstance(value, str)
        or not value
        or "\\" in value
        or ":" in value
        or "\r" in value
        or "\n" in value
    ):
        return False
    path = Path(value)
    return not path.is_absolute() and not path.drive and all(part not in {"", ".", ".."} for part in path.parts)


def _is_sha256(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)
