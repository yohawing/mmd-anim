from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .batch import run_batch
from .campaign import run_campaign
from .case import CaseValidationError, load_case
from .prepare import prepare_case
from .record import record_case
from .report import ReportValidationError, write_report


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    if args.command == "prepare-batch":
        summary = run_batch(
            args.case,
            action=prepare_case,
            action_name="prepare",
            case_loader=load_case,
        )
        summary["command"] = args.command
        _print_json(summary)
        return 0 if summary["ok"] else 1
    if args.command == "record-batch":
        summary = run_batch(
            args.case,
            action=lambda case: record_case(case, args.mmd_exe),
            action_name="record",
            case_loader=load_case,
        )
        summary["command"] = args.command
        _print_json(summary)
        return 0 if summary["ok"] else 1
    if args.command == "quality-report":
        try:
            write_report(Path(args.snapshot), Path(args.output))
        except ReportValidationError as error:
            _print_json(
                {"ok": False, "command": args.command, "error": error.as_dict()},
                stream=sys.stderr,
            )
            return 2
        _print_json({"ok": True, "command": args.command, "output": str(Path(args.output).resolve())})
        return 0
    if args.command == "campaign":
        result = run_campaign(Path(args.config), Path(args.snapshot), args.state, args.mmd_exe)
        result["command"] = args.command
        _print_json(result)
        if result.get("ok") is True:
            return 0
        error = result.get("error")
        if isinstance(error, dict) and error.get("kind") == "campaign":
            return 2
        return 1
    case_path = Path(args.case)
    try:
        case = load_case(case_path)
    except CaseValidationError as error:
        _print_json(
            {
                "ok": False,
                "command": args.command,
                "caseFile": str(case_path.resolve()),
                "error": error.as_dict(),
            },
            stream=sys.stderr,
        )
        return 2

    if args.command == "prepare":
        result = prepare_case(case)
        _print_json(result)
        return 0 if result["ok"] else 1
    if args.command == "record":
        result = record_case(case, args.mmd_exe)
        _print_json(result)
        return 0 if result["ok"] else 1

    _print_json(
        {
            "ok": True,
            "command": "validate",
            "caseFile": str(case.source_path),
            "case": case.as_dict(),
        }
    )
    return 0


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="mmd-oracle-runner")
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate", help="validate one case contract")
    validate.add_argument("--case", required=True, help="absolute case JSON path")
    prepare = commands.add_parser("prepare", help="prepare one case without launching MMD")
    prepare.add_argument("--case", required=True, help="absolute case JSON path")
    record = commands.add_parser("record", help="record one prepared case through MMD")
    record.add_argument("--case", required=True, help="absolute case JSON path")
    record.add_argument(
        "--mmd-exe",
        help="absolute MikuMikuDance executable path (defaults to MMD_DUMPER_MMD_EXE)",
    )
    prepare_batch = commands.add_parser("prepare-batch", help="prepare multiple cases without launching MMD")
    prepare_batch.add_argument("--case", action="append", required=True, help="case JSON path (repeatable)")
    record_batch = commands.add_parser("record-batch", help="record multiple prepared cases through MMD")
    record_batch.add_argument("--case", action="append", required=True, help="case JSON path (repeatable)")
    record_batch.add_argument(
        "--mmd-exe",
        help="absolute MikuMikuDance executable path (defaults to MMD_DUMPER_MMD_EXE)",
    )
    quality_report = commands.add_parser(
        "quality-report", help="generate deterministic Markdown from one quality snapshot"
    )
    quality_report.add_argument("--snapshot", required=True, help="compact quality snapshot JSON path")
    quality_report.add_argument("--output", required=True, help="Markdown report output path")
    campaign = commands.add_parser("campaign", help="run a sequential local motion quality campaign")
    campaign.add_argument("--config", required=True, help="absolute campaign config JSON path")
    campaign.add_argument("--snapshot", required=True, help="absolute compact snapshot JSON output path")
    campaign.add_argument("--state", help="absolute local campaign state JSON path")
    campaign.add_argument("--mmd-exe", help="absolute MikuMikuDance executable path")
    return parser


def _print_json(payload: dict[str, object], *, stream=None) -> None:
    if stream is None:
        stream = sys.stdout
    print(json.dumps(payload, ensure_ascii=True, sort_keys=True, allow_nan=False), file=stream)


if __name__ == "__main__":
    raise SystemExit(main())
