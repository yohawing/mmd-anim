from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import prepare_case
from prepare_test_support import REPO_ROOT, write_case


@pytest.mark.integration
def test_real_python_rust_prepare_is_repository_local(tmp_path: Path):
    if shutil.which("cargo") is None:
        pytest.skip("Cargo is not installed")
    result = prepare_case(load_case(write_case(tmp_path)), repo_root=REPO_ROOT)
    assert result["ok"] is True
    assert result["comparison"]["status"] == "generated"
    assert Path(result["artifacts"]["project"]["path"]).is_file()
    assert Path(result["artifacts"]["fixture"]["path"]).is_file()
