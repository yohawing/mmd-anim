"""Validation foundation for the MMD oracle runner."""

from .case import (
    CaseValidationError,
    GeneratorBackend,
    OracleCase,
    ValidationIssue,
    load_case,
)
from .prepare import prepare_case

__all__ = [
    "CaseValidationError",
    "GeneratorBackend",
    "OracleCase",
    "ValidationIssue",
    "load_case",
    "prepare_case",
]

__version__ = "0.1.0"
