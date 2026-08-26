"""Validation foundation for the MMD oracle runner."""

from .case import (
    CaseValidationError,
    GeneratorBackend,
    OracleCase,
    ValidationIssue,
    load_case,
)
from .batch import run_batch
from .campaign import run_campaign
from .prepare import prepare_case
from .record import record_case
from .report import ReportValidationError, generate_report, load_snapshot, write_report

__all__ = [
    "CaseValidationError",
    "GeneratorBackend",
    "OracleCase",
    "ValidationIssue",
    "load_case",
    "run_batch",
    "run_campaign",
    "prepare_case",
    "record_case",
    "ReportValidationError",
    "generate_report",
    "load_snapshot",
    "write_report",
]

__version__ = "0.1.0"
