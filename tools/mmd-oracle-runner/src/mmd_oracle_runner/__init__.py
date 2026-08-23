"""Validation foundation for the MMD oracle runner."""

from .case import (
    CaseValidationError,
    GeneratorBackend,
    OracleCase,
    ValidationIssue,
    load_case,
)
from .batch import run_batch
from .prepare import prepare_case
from .record import record_case

__all__ = [
    "CaseValidationError",
    "GeneratorBackend",
    "OracleCase",
    "ValidationIssue",
    "load_case",
    "run_batch",
    "prepare_case",
    "record_case",
]

__version__ = "0.1.0"
