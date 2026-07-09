from __future__ import annotations

from golden_gate.report import summarize_report


def test_summarize_report_includes_physics_penetration_metrics():
    summary = summarize_report(
        {
            "summary": {
                "pairCount": 4,
                "penetratingPairCount": 2,
                "severePairCount": 2,
                "bulletContactCount": 0,
                "penetratingContactCount": 0,
                "maxPenetrationDepth": 0.7402609,
                "maxBulletPenetrationDepth": 0.0,
            }
        }
    )

    assert "pairCount=4" in summary
    assert "severePairCount=2" in summary
    assert "bulletContactCount=0" in summary
    assert "maxBulletPenetrationDepth=0.0" in summary
