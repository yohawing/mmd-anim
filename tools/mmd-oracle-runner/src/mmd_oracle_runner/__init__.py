"""Validation foundation for the MMD oracle runner."""

from .case import (
    CaseValidationError,
    GeneratorBackend,
    OracleCase,
    ValidationIssue,
    load_case,
)
from .batch import run_batch
from .campaign import CampaignConfig, CampaignValidationError, load_campaign_config, run_campaign
from .campaign_artifacts import cleanup_completed_case_run, cleanup_prepared_case_run
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
    "CampaignConfig",
    "CampaignValidationError",
    "load_campaign_config",
    "run_campaign",
    "cleanup_completed_case_run",
    "cleanup_prepared_case_run",
    "prepare_case",
    "record_case",
    "ReportValidationError",
    "generate_report",
    "load_snapshot",
    "write_report",
]

__version__ = "0.1.0"
