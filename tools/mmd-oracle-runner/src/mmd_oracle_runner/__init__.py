"""Validation foundation for the MMD oracle runner."""

from .case import (
    CaseValidationError,
    GeneratorBackend,
    OracleCase,
    ValidationIssue,
    load_case,
)

__all__ = [
    "CaseValidationError",
    "GeneratorBackend",
    "OracleCase",
    "ValidationIssue",
    "load_case",
]

__version__ = "0.1.0"
