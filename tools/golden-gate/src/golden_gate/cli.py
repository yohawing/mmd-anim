from __future__ import annotations

import argparse
import sys

from .compare import compare_reports
from .config import ConfigError, resolve_config
from .report import format_failures, load_report, save_report, summarize_report
from .runner import RunnerError, generate_current_report


def main(argv: list[str] | None = None) -> int:
    _configure_utf8_console()
    parser = _build_parser()
    args = parser.parse_args(argv)
    try:
        config = resolve_config(args).require_paths()
        if args.command == "baseline":
            return _baseline(args, config)
        if args.command == "gate":
            return _gate(config)
    except (ConfigError, RunnerError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    parser.error("missing command")
    return 2


def _configure_utf8_console() -> None:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, OSError, ValueError):
            pass


def _baseline(args: argparse.Namespace, config) -> int:
    if config.baseline.exists() and not args.replace:
        raise ConfigError(f"baseline already exists; pass --replace to update it: {config.baseline}")
    report, report_path = generate_current_report(config)
    save_report(config.baseline, report)
    print(f"Current report: {report_path}")
    print(f"Baseline saved: {config.baseline}")
    print(summarize_report(report))
    return 0


def _gate(config) -> int:
    if not config.baseline.exists():
        raise ConfigError(f"baseline does not exist: {config.baseline}")
    baseline = load_report(config.baseline)
    current, report_path = generate_current_report(config)
    failures = compare_reports(baseline, current, config.tolerances, config.options)
    print(f"Current report: {report_path}")
    print(f"Baseline: {config.baseline}")
    print(summarize_report(current))
    print(format_failures(failures))
    return 1 if failures else 0


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="golden-gate")
    subparsers = parser.add_subparsers(dest="command", required=True)

    baseline = subparsers.add_parser("baseline", help="write the current report as the accepted baseline")
    _add_common_options(baseline)
    baseline.add_argument("--replace", action="store_true", help="replace an existing baseline")

    gate = subparsers.add_parser("gate", help="compare the current report against the accepted baseline")
    _add_common_options(gate)
    return parser


def _add_common_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--config", help="local TOML config path")
    parser.add_argument("--repo-root", help="mmd-anim repository root")
    parser.add_argument("--manifest", help="NumericMotion manifest path")
    parser.add_argument("--baseline", help="accepted baseline report path")
    parser.add_argument("--report-dir", help="directory for timestamped current reports")
    parser.add_argument("--mmd-anim-bin", help="direct mmd-anim CLI binary path")
    parser.add_argument("--physics-penetration", action="store_true", default=None)
    parser.add_argument("--diagnose-case", default=None)
    parser.add_argument("--diagnose-frame", default=None)
    parser.add_argument("--diagnose-bone", default=None)
    parser.add_argument("--diagnose-eval-frame", default=None)
    parser.add_argument("--max-abs-error-tolerance", type=float, default=None)
    parser.add_argument("--translation-max-error-tolerance", type=float, default=None)
    parser.add_argument("--translation-rms-error-tolerance", type=float, default=None)
    parser.add_argument("--rotation-max-angle-rad-tolerance", type=float, default=None)
    parser.add_argument("--rotation-rms-angle-rad-tolerance", type=float, default=None)
    parser.add_argument("--penetration-max-depth-tolerance", type=float, default=None)
    parser.add_argument("--bullet-penetration-max-depth-tolerance", type=float, default=None)
    parser.add_argument("--penetrating-pair-count-tolerance", type=int, default=None)
    parser.add_argument("--severe-pair-count-tolerance", type=int, default=None)
    parser.add_argument("--penetrating-contact-count-tolerance", type=int, default=None)
    parser.add_argument("--mismatch-count-tolerance", type=int, default=None)
    parser.add_argument("--missing-tolerance", type=int, default=None)
    parser.add_argument("--import-error-tolerance", type=int, default=None)
    parser.add_argument("--allow-count-changes", action="store_true", default=None)
    parser.add_argument("--allow-skipped-target-changes", action="store_true", default=None)
    parser.add_argument(
        "--required-physics-backend",
        default=None,
        help="required physicsBackend for physics cases; disabled when omitted or empty",
    )


if __name__ == "__main__":
    raise SystemExit(main())
