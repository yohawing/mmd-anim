from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .case import CaseValidationError, load_case


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    case_path = Path(args.case)
    try:
        case = load_case(case_path)
    except CaseValidationError as error:
        _print_json(
            {
                "ok": False,
                "command": "validate",
                "caseFile": str(case_path.resolve()),
                "error": error.as_dict(),
            },
            stream=sys.stderr,
        )
        return 2

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
    return parser


def _print_json(payload: dict[str, object], *, stream=None) -> None:
    if stream is None:
        stream = sys.stdout
    print(json.dumps(payload, ensure_ascii=True, sort_keys=True), file=stream)


if __name__ == "__main__":
    raise SystemExit(main())
