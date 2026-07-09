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
    translation_max_error_tolerance: float = 0.0
    translation_rms_error_tolerance: float = 0.0
    rotation_max_angle_rad_tolerance: float = 0.0
    rotation_rms_angle_rad_tolerance: float = 0.0
    penetration_max_depth_tolerance: float = 0.0
    bullet_penetration_max_depth_tolerance: float = 0.0
    penetrating_pair_count_tolerance: int = 0
    severe_pair_count_tolerance: int = 0
    penetrating_contact_count_tolerance: int = 0
    mismatch_count_tolerance: int = 0
    missing_tolerance: int = 0
    import_error_tolerance: int = 0


@dataclass(frozen=True)
class GateOptions:
    allow_count_changes: bool = False
    allow_skipped_target_changes: bool = False
    required_physics_backend: str | None = None


@dataclass(frozen=True)
class GoldenGateConfig:
    repo_root: Path
    manifest: Path | None
    baseline: Path | None
    report_dir: Path | None
    mmd_anim_bin: Path | None
    physics_penetration: bool
    diagnose_case: str | None
    diagnose_frame: str | None
    diagnose_bone: str | None
    diagnose_eval_frame: str | None
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
        if self.physics_penetration and (self.diagnose_case is None or self.diagnose_frame is None):
            raise ConfigError("physics_penetration requires diagnose_case and diagnose_frame")
        if not self.physics_penetration and any(
            value is not None
            for value in (self.diagnose_case, self.diagnose_frame, self.diagnose_bone, self.diagnose_eval_frame)
        ):
            raise ConfigError("diagnose_* config values require physics_penetration")
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
    physics_penetration = _bool_value(
        "physics_penetration",
        getattr(args, "physics_penetration", None),
        "MMD_ANIM_GOLDEN_PHYSICS_PENETRATION",
        raw_config,
        False,
    )
    diagnose_case = _optional_scalar_string_value(
        "diagnose_case",
        getattr(args, "diagnose_case", None),
        "MMD_ANIM_GOLDEN_DIAGNOSE_CASE",
        raw_config,
        None,
    )
    diagnose_frame = _optional_scalar_string_value(
        "diagnose_frame",
        getattr(args, "diagnose_frame", None),
        "MMD_ANIM_GOLDEN_DIAGNOSE_FRAME",
        raw_config,
        None,
    )
    diagnose_bone = _optional_scalar_string_value(
        "diagnose_bone",
        getattr(args, "diagnose_bone", None),
        "MMD_ANIM_GOLDEN_DIAGNOSE_BONE",
        raw_config,
        None,
    )
    diagnose_eval_frame = _optional_scalar_string_value(
        "diagnose_eval_frame",
        getattr(args, "diagnose_eval_frame", None),
        "MMD_ANIM_GOLDEN_DIAGNOSE_EVAL_FRAME",
        raw_config,
        None,
    )

    tolerances = Tolerances(
        max_abs_error_tolerance=_float_value(
            "max_abs_error_tolerance",
            getattr(args, "max_abs_error_tolerance", None),
            "MMD_ANIM_GOLDEN_MAX_ABS_ERROR_TOLERANCE",
            raw_config,
            0.0,
        ),
        translation_max_error_tolerance=_float_value(
            "translation_max_error_tolerance",
            getattr(args, "translation_max_error_tolerance", None),
            "MMD_ANIM_GOLDEN_TRANSLATION_MAX_ERROR_TOLERANCE",
            raw_config,
            0.0,
        ),
        translation_rms_error_tolerance=_float_value(
            "translation_rms_error_tolerance",
            getattr(args, "translation_rms_error_tolerance", None),
            "MMD_ANIM_GOLDEN_TRANSLATION_RMS_ERROR_TOLERANCE",
            raw_config,
            0.0,
        ),
        rotation_max_angle_rad_tolerance=_float_value(
            "rotation_max_angle_rad_tolerance",
            getattr(args, "rotation_max_angle_rad_tolerance", None),
            "MMD_ANIM_GOLDEN_ROTATION_MAX_ANGLE_RAD_TOLERANCE",
            raw_config,
            0.0,
        ),
        rotation_rms_angle_rad_tolerance=_float_value(
            "rotation_rms_angle_rad_tolerance",
            getattr(args, "rotation_rms_angle_rad_tolerance", None),
            "MMD_ANIM_GOLDEN_ROTATION_RMS_ANGLE_RAD_TOLERANCE",
            raw_config,
            0.0,
        ),
        penetration_max_depth_tolerance=_float_value(
            "penetration_max_depth_tolerance",
            getattr(args, "penetration_max_depth_tolerance", None),
            "MMD_ANIM_GOLDEN_PENETRATION_MAX_DEPTH_TOLERANCE",
            raw_config,
            0.0,
        ),
        bullet_penetration_max_depth_tolerance=_float_value(
            "bullet_penetration_max_depth_tolerance",
            getattr(args, "bullet_penetration_max_depth_tolerance", None),
            "MMD_ANIM_GOLDEN_BULLET_PENETRATION_MAX_DEPTH_TOLERANCE",
            raw_config,
            0.0,
        ),
        penetrating_pair_count_tolerance=_int_value(
            "penetrating_pair_count_tolerance",
            getattr(args, "penetrating_pair_count_tolerance", None),
            "MMD_ANIM_GOLDEN_PENETRATING_PAIR_COUNT_TOLERANCE",
            raw_config,
            0,
        ),
        severe_pair_count_tolerance=_int_value(
            "severe_pair_count_tolerance",
            getattr(args, "severe_pair_count_tolerance", None),
            "MMD_ANIM_GOLDEN_SEVERE_PAIR_COUNT_TOLERANCE",
            raw_config,
            0,
        ),
        penetrating_contact_count_tolerance=_int_value(
            "penetrating_contact_count_tolerance",
            getattr(args, "penetrating_contact_count_tolerance", None),
            "MMD_ANIM_GOLDEN_PENETRATING_CONTACT_COUNT_TOLERANCE",
            raw_config,
            0,
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
        required_physics_backend=_optional_string_value(
            "required_physics_backend",
            getattr(args, "required_physics_backend", None),
            "MMD_ANIM_GOLDEN_REQUIRED_PHYSICS_BACKEND",
            raw_config,
            None,
        ),
    )

    return GoldenGateConfig(
        repo_root=repo_root,
        manifest=manifest,
        baseline=baseline,
        report_dir=report_dir,
        mmd_anim_bin=mmd_anim_bin,
        physics_penetration=physics_penetration,
        diagnose_case=diagnose_case,
        diagnose_frame=diagnose_frame,
        diagnose_bone=diagnose_bone,
        diagnose_eval_frame=diagnose_eval_frame,
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
    if key in {"allow_count_changes", "allow_skipped_target_changes", "required_physics_backend"}:
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


def _optional_string_value(
    key: str,
    cli_value: Any,
    env_name: str,
    raw_config: dict[str, Any],
    default: str | None,
) -> str | None:
    if cli_value is not None:
        value = cli_value
    elif env_name in os.environ:
        value = os.environ.get(env_name)
    elif key in raw_config:
        value = raw_config[key]
    else:
        value = raw_config.get("options", {}).get(key)
    if value is None:
        return default
    if value == "":
        return None
    if not isinstance(value, str):
        raise ConfigError(f"{key} must be a string")
    return value


def _optional_scalar_string_value(
    key: str,
    cli_value: Any,
    env_name: str,
    raw_config: dict[str, Any],
    default: str | None,
) -> str | None:
    value = _choose(key, cli_value, env_name, raw_config)
    if value is None:
        return default
    if value == "":
        return None
    if isinstance(value, bool) or not isinstance(value, (str, int, float)):
        raise ConfigError(f"{key} must be a string or number")
    return str(value)
