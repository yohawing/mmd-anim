from __future__ import annotations

from golden_gate.report import summarize_report


def test_summarize_report_includes_physics_penetration_metrics():
    summary = summarize_report(
        {
            "summary": {
                "pairCount": 4,
                "penetratingPairCount": 2,
                "severePairCount": 2,
                "jointConnectedPairCount": 2,
                "jointConnectedPenetratingPairCount": 2,
                "jointConnectedSeverePairCount": 2,
                "unconnectedPairCount": 2,
                "unconnectedPenetratingPairCount": 0,
                "unconnectedSeverePairCount": 0,
                "bulletContactCount": 0,
                "penetratingContactCount": 0,
                "maxPenetrationDepth": 0.7402609,
                "maxBulletPenetrationDepth": 0.0,
            },
            "rigidBodies": [
                {
                    "index": 234,
                    "name": "左HA20",
                    "positionWorld": [5.0, 19.0, 1.0],
                    "rotationXyzw": [0.5, -0.4, -0.1, 0.7],
                }
            ],
        }
    )

    assert "pairCount=4" in summary
    assert "severePairCount=2" in summary
    assert "jointConnectedSeverePairCount=2" in summary
    assert "unconnectedSeverePairCount=0" in summary
    assert "bulletContactCount=0" in summary
    assert "maxBulletPenetrationDepth=0.0" in summary
    assert "rigidBodies=1" in summary
