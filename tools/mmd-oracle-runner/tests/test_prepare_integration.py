from __future__ import annotations

import json
import shutil
from pathlib import Path

import pytest

from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import prepare_case
from prepare_test_support import M0_BASELINE, REPO_ROOT, write_case


@pytest.mark.integration
def test_real_m0_node_cases_match_baseline(tmp_path: Path):
    if shutil.which("node") is None or not (REPO_ROOT / "tools" / "mmd-dumper" / "node_modules" / "iconv-lite").exists():
        pytest.skip("Node backend dependencies are not installed")
    for name, kwargs in (
        ("body-only", {}),
        ("body-camera", {"camera": True}),
        ("body-property-ik", {"property_opt_in": True}),
    ):
        case = load_case(write_case(tmp_path, name=name, **kwargs))
        result = prepare_case(case, repo_root=REPO_ROOT)
        expected = M0_BASELINE["cases"][name]
        assert result["ok"] is True
        assert result["frames"] == expected["frames"]
        body_counts = expected["sourceVmdCounts"]
        assert {key: result["preflight"]["bodyVmd"][key] for key in body_counts} == body_counts
        if "sourceCameraCounts" in expected:
            assert result["preflight"]["cameraVmd"]["cameraFrames"] == expected["sourceCameraCounts"]["cameraFrames"]
        assert result["comparison"]["patchCounts"] == expected["patchCounts"]
        assert result["skippedBoneNames"] == expected["filter"]["skippedBoneNames"]
        assert result["skippedMorphNames"] == expected["filter"]["skippedMorphNames"]
        assert result["droppedUnsupportedChannels"]["propertyFrames"] == expected["filter"]["propertyFrames"]
        assert result["classifications"] == expected["classifications"]
        assert all(artifact["exists"] for artifact in result["artifacts"].values())
        fixture = json.loads(Path(result["artifacts"]["fixture"]["path"]).read_text(encoding="utf-8"))
        assert Path(fixture["output"]) == case.output_root / name / "oracle.actual.jsonl"
        assert result["recorded"] is False
        assert not (case.output_root / name / "oracle.actual.jsonl").exists()
        stored = json.loads((case.output_root / name / "prepare-result.json").read_text(encoding="utf-8"))
        assert stored["ok"] is True
        assert stored["recorded"] is False
        assert stored["artifacts"]["result"]["exists"] is True


@pytest.mark.integration
def test_real_rust_body_only_prepare(tmp_path: Path):
    if shutil.which("cargo") is None or shutil.which("node") is None or not (REPO_ROOT / "tools" / "mmd-dumper" / "node_modules" / "iconv-lite").exists():
        pytest.skip("Cargo and the Node backend dependencies are required")
    result = prepare_case(load_case(write_case(tmp_path, backend="rust-build-pmm")), repo_root=REPO_ROOT)

    assert result["ok"] is False and result["phase"] == "artifacts"
    assert result["recorded"] is False
    assert result["comparison"]["status"] == "not-verified"
    assert all(artifact["exists"] for artifact in result["artifacts"].values())
