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
from .selection import SelectionError, freeze_selection, materialize_selection, verify_selection


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
    if args.command == "freeze-selection":
        try:
            selection = freeze_selection(
                Path(args.pmx_root),
                Path(args.vmd_root),
                Path(args.output),
                count=args.count,
                seed=args.seed,
                frames=tuple(args.frame or (0, 15, 30, 60, 120)),
            )
        except SelectionError as error:
            _print_json({"ok": False, "command": args.command, "error": error.as_dict()}, stream=sys.stderr)
            return 2
        _print_json(
            {
                "ok": True,
                "command": args.command,
                "output": str(Path(args.output).resolve()),
                "selected": selection["discovery"]["selected"],
                "selectionHash": selection["selectionHash"],
            }
        )
        return 0
    if args.command == "verify-selection":
        try:
            result = verify_selection(Path(args.selection))
        except SelectionError as error:
            _print_json({"ok": False, "command": args.command, "error": error.as_dict()}, stream=sys.stderr)
            return 2
        result["command"] = args.command
        _print_json(result)
        return 0
    if args.command == "materialize-selection":
        try:
            result = materialize_selection(Path(args.selection), Path(args.template), Path(args.output_dir))
        except SelectionError as error:
            _print_json({"ok": False, "command": args.command, "error": error.as_dict()}, stream=sys.stderr)
            return 2
        result["command"] = args.command
        _print_json(result)
        return 0

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
    selection = commands.add_parser("freeze-selection", help="freeze a deterministic local PMX/VMD selection")
    selection.add_argument("--pmx-root", required=True, help="absolute PMX library directory")
    selection.add_argument("--vmd-root", required=True, help="absolute VMD library directory")
    selection.add_argument("--output", required=True, help="new local selection JSON path")
    selection.add_argument("--count", required=True, type=int, help="number of fixed PMX/VMD pairs")
    selection.add_argument("--seed", required=True, help="stable deterministic selection seed")
    selection.add_argument(
        "--frame",
        action="append",
        type=int,
        default=None,
        help="sample frame (repeatable; defaults to 0,15,30,60,120)",
    )
    verify_selection_parser = commands.add_parser("verify-selection", help="verify every frozen asset hash")
    verify_selection_parser.add_argument("--selection", required=True, help="absolute frozen selection JSON path")
    materialize = commands.add_parser("materialize-selection", help="create local cases and campaign config")
    materialize.add_argument("--selection", required=True, help="absolute frozen selection JSON path")
    materialize.add_argument("--template", required=True, help="absolute local campaign template JSON path")
    materialize.add_argument("--output-dir", required=True, help="new local materialized campaign directory")
    return parser


def _print_json(payload: dict[str, object], *, stream=None) -> None:
    if stream is None:
        stream = sys.stdout
    print(json.dumps(payload, ensure_ascii=True, sort_keys=True, allow_nan=False), file=stream)


if __name__ == "__main__":
    raise SystemExit(main())
