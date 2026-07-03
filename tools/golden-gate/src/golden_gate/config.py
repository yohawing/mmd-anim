from __future__ import annotations

import json
import os
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class Tolerances:
    max_abs_error_tolerance: float = 0.0
    mismatch_count_tolerance: int = 0
    missing_tolerance: int = 0
    import_error_tolerance: int = 0


@dataclass(frozen=True)
class GateOptions:
    allow_count_changes: bool = False
    allow_skipped_target_changes: bool = False


@dataclass(frozen=True)
class GoldenGateConfig:
    repo_root: Path
    manifest: Path | None
    baseline: Path | None
    report_dir: Path | None
    mmd_anim_bin: Path | None
    tolerances: Tolerances
    options: GateOptions

    def require_paths(self) -> "GoldenGateConfig":
        missing = []
        if self.manifest is None:
            missing.append("manifest")
        if self.baseline is None:
            missing.append("baseline")
        if missing:
            raise ConfigError(f"missing required config value(s): {', '.join(missing)}")
        report_dir = self.report_dir
        if report_dir is None and self.baseline is not None:
            report_dir = self.baseline.parent
        return replace(self, report_dir=report_dir)


def resolve_config(args: Any) -> GoldenGateConfig:
    config_path = _select_config_path(getattr(args, "config", None))
    raw_config, config_base = _load_config(config_path)

    repo_root = _resolve_path(
        _choose("repo_root", getattr(args, "repo_root", None), "MMD_ANIM_GOLDEN_REPO_ROOT", raw_config),
        base=config_base,
        default=_default_repo_root(),
    )
    manifest = _resolve_path(
        _choose("manifest", getattr(args, "manifest", None), "MMD_ANIM_GOLDEN_MANIFEST", raw_config),
        base=config_base,
    )
    baseline = _resolve_path(
        _choose("baseline", getattr(args, "baseline", None), "MMD_ANIM_GOLDEN_BASELINE", raw_config),
        base=config_base,
    )
    report_dir = _resolve_path(
        _choose("report_dir", getattr(args, "report_dir", None), "MMD_ANIM_GOLDEN_REPORT_DIR", raw_config),
        base=config_base,
    )
    mmd_anim_bin = _resolve_path(
        _choose("mmd_anim_bin", getattr(args, "mmd_anim_bin", None), "MMD_ANIM_BIN", raw_config),
        base=config_base,
    )

    tolerances = Tolerances(
        max_abs_error_tolerance=_float_value(
            "max_abs_error_tolerance",
            getattr(args, "max_abs_error_tolerance", None),
            "MMD_ANIM_GOLDEN_MAX_ABS_ERROR_TOLERANCE",
            raw_config,
            0.0,
        ),
        mismatch_count_tolerance=_int_value(
            "mismatch_count_tolerance",
            getattr(args, "mismatch_count_tolerance", None),
            "MMD_ANIM_GOLDEN_MISMATCH_COUNT_TOLERANCE",
            raw_config,
            0,
        ),
        missing_tolerance=_int_value(
            "missing_tolerance",
            getattr(args, "missing_tolerance", None),
            "MMD_ANIM_GOLDEN_MISSING_TOLERANCE",
            raw_config,
            0,
        ),
        import_error_tolerance=_int_value(
            "import_error_tolerance",
            getattr(args, "import_error_tolerance", None),
            "MMD_ANIM_GOLDEN_IMPORT_ERROR_TOLERANCE",
            raw_config,
            0,
        ),
    )
    options = GateOptions(
        allow_count_changes=_bool_value(
            "allow_count_changes",
            getattr(args, "allow_count_changes", None),
            "MMD_ANIM_GOLDEN_ALLOW_COUNT_CHANGES",
            raw_config,
            False,
        ),
        allow_skipped_target_changes=_bool_value(
            "allow_skipped_target_changes",
            getattr(args, "allow_skipped_target_changes", None),
            "MMD_ANIM_GOLDEN_ALLOW_SKIPPED_TARGET_CHANGES",
            raw_config,
            False,
        ),
    )

    return GoldenGateConfig(
        repo_root=repo_root,
        manifest=manifest,
        baseline=baseline,
        report_dir=report_dir,
        mmd_anim_bin=mmd_anim_bin,
        tolerances=tolerances,
        options=options,
    )


def _default_repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def _select_config_path(cli_config: str | None) -> Path | None:
    if cli_config:
        return Path(cli_config)
    env_config = os.environ.get("MMD_ANIM_GOLDEN_CONFIG")
    if env_config:
        return Path(env_config)
    local_config = Path("golden-gate.local.json")
    if local_config.exists():
        return local_config
    return None


def _load_config(config_path: Path | None) -> tuple[dict[str, Any], Path | None]:
    if config_path is None:
        return {}, None
    if not config_path.exists():
        raise ConfigError(f"config file does not exist: {config_path}")
    try:
        with config_path.open("r", encoding="utf-8") as handle:
            value = json.load(handle)
    except OSError as error:
        raise ConfigError(f"failed to read config file {config_path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ConfigError(f"config file is not valid JSON: {config_path}: {error}") from error
    if not isinstance(value, dict):
        raise ConfigError(f"config file must contain a JSON object: {config_path}")
    return value, config_path.resolve().parent


def _choose(key: str, cli_value: Any, env_name: str, raw_config: dict[str, Any]) -> Any:
    if cli_value is not None:
        return cli_value
    env_value = os.environ.get(env_name)
    if env_value not in (None, ""):
        return env_value
    if key in raw_config:
        return raw_config[key]
    if key.endswith("_tolerance"):
        return raw_config.get("tolerances", {}).get(key)
    if key in {"allow_count_changes", "allow_skipped_target_changes"}:
        return raw_config.get("options", {}).get(key)
    return None


def _resolve_path(value: Any, *, base: Path | None, default: Path | None = None) -> Path | None:
    if value in (None, ""):
        return default
    path = Path(value).expanduser()
    if path.is_absolute():
        return path
    if base is not None:
        return (base / path).resolve()
    return path.resolve()


def _float_value(key: str, cli_value: Any, env_name: str, raw_config: dict[str, Any], default: float) -> float:
    value = _choose(key, cli_value, env_name, raw_config)
    if value in (None, ""):
        return default
    try:
        parsed = float(value)
    except (TypeError, ValueError) as error:
        raise ConfigError(f"{key} must be a number") from error
    if parsed < 0:
        raise ConfigError(f"{key} must be non-negative")
    return parsed


def _int_value(key: str, cli_value: Any, env_name: str, raw_config: dict[str, Any], default: int) -> int:
    value = _choose(key, cli_value, env_name, raw_config)
    if value in (None, ""):
        return default
    try:
        parsed = int(value)
    except (TypeError, ValueError) as error:
        raise ConfigError(f"{key} must be an integer") from error
    if parsed < 0:
        raise ConfigError(f"{key} must be non-negative")
    return parsed


def _bool_value(key: str, cli_value: Any, env_name: str, raw_config: dict[str, Any], default: bool) -> bool:
    value = _choose(key, cli_value, env_name, raw_config)
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    normalized = str(value).strip().lower()
    if normalized in {"1", "true", "yes", "on"}:
        return True
    if normalized in {"0", "false", "no", "off"}:
        return False
    raise ConfigError(f"{key} must be a boolean")
