# Motion Golden Oracle Quality Report

This report is generated from a compact quality snapshot. It contains aggregate results only; per-frame and per-bone details are not included.

## Run and provenance

| Field | Value |
| --- | --- |
| Commit SHA | c946304d1c3760ec7951a05a06d055485e30ee0d |
| Repository state | clean |
| MMD version (self-reported) | 9.32-x64 |
| MMDDumper version (self-reported) | MMDDumper-e51a439 |
| MMD version source | config-self-reported |
| MMDDumper version source | config-self-reported |
| MMD executable SHA-256 | 07516fd3bf1e6b1339836b6773a156f61bdd6f848eeb621fdda012375df313a1 |
| Timestamp | 2026-08-25T09:06:45+09:00 |
| Sampling policy | frozen-library-128x5-v1 |
| Manifest SHA-256 | 281616cb18a24ff6194c7ab3e514b9581e9c4a8c59bbcc31da484970c366727c |

## Execution funnel

| Stage | Cases |
| --- | ---: |
| Discovered | 128 |
| Selected | 128 |
| Prepared | 56 |
| Recorded | 31 |
| Compared | 21 |
| Passed | 1 |

## Parity thresholds

| Metric | Threshold |
| --- | ---: |
| translationMaxError | 0.003 |
| translationRmsError | 0.001 |
| rotationMaxAngleRad | 0.003 |
| rotationRmsAngleRad | 0.001 |
| maxAbsError | 0.003 |

## Metric distributions

| Metric | p50 | p95 | p99 | max |
| --- | ---: | ---: | ---: | ---: |
| translationMaxError | 2.53557300568 | 12.3038606644 | 12.5500001907 | 12.5500001907 |
| translationRmsError | 0.46462298869 | 2.90261227065 | 4.00963327758 | 4.00963327758 |
| rotationMaxAngleRad | 0.605169653893 | 1.7592805624 | 2.92146682739 | 2.92146682739 |
| rotationRmsAngleRad | 0.239958300256 | 0.658205959268 | 2.29738123317 | 2.29738123317 |
| maxAbsError | 2.53557300568 | 12.3038606644 | 12.5500001907 | 12.5500001907 |

## Failure classifications

| Classification | Cases |
| --- | ---: |
| compare-fields | 10 |
| prepare | 72 |
| record | 25 |
| threshold | 20 |

## Feature summaries

| Tag | Selected | Compared | Passed |
| --- | ---: | ---: | ---: |
| bone-motion | 128 | 21 | 1 |

## Category summaries

| Tag | Selected | Compared | Passed |
| --- | ---: | ---: | ---: |
| deterministic-library-sample | 128 | 21 | 1 |

## Worst cases

| Case | Category | Metric | Value | Result |
| --- | --- | --- | ---: | --- |
| library-0085-93262b8f9d40 | deterministic-library-sample | maxAbsError | 12.5500001907 | fail |
| library-0085-93262b8f9d40 | deterministic-library-sample | translationMaxError | 12.5500001907 | fail |
| library-0016-ec39e66d60d2 | deterministic-library-sample | maxAbsError | 12.3038606644 | fail |
| library-0016-ec39e66d60d2 | deterministic-library-sample | translationMaxError | 12.3038606644 | fail |
| library-0049-edef068672d0 | deterministic-library-sample | maxAbsError | 12.0810251236 | fail |
| library-0049-edef068672d0 | deterministic-library-sample | translationMaxError | 12.0810251236 | fail |
| library-0016-ec39e66d60d2 | deterministic-library-sample | translationRmsError | 4.00963327758 | fail |
| library-0104-c6b4e7f5ac37 | deterministic-library-sample | maxAbsError | 12 | fail |
| library-0104-c6b4e7f5ac37 | deterministic-library-sample | translationMaxError | 12 | fail |
| library-0095-ec5f5988903f | deterministic-library-sample | maxAbsError | 10.6228475571 | fail |
| library-0095-ec5f5988903f | deterministic-library-sample | translationMaxError | 10.6228475571 | fail |
| library-0085-93262b8f9d40 | deterministic-library-sample | translationRmsError | 2.90261227065 | fail |
| library-0104-c6b4e7f5ac37 | deterministic-library-sample | translationRmsError | 2.82842712475 | fail |
| library-0095-ec5f5988903f | deterministic-library-sample | translationRmsError | 2.80790049566 | fail |
| library-0059-90466e9f8648 | deterministic-library-sample | maxAbsError | 7.78625917435 | fail |
| library-0059-90466e9f8648 | deterministic-library-sample | translationMaxError | 7.78625917435 | fail |
| library-0049-edef068672d0 | deterministic-library-sample | translationRmsError | 2.49887418775 | fail |
| library-0078-318cec9536e2 | deterministic-library-sample | rotationRmsAngleRad | 2.29738123317 | fail |
| library-0059-90466e9f8648 | deterministic-library-sample | translationRmsError | 2.08055186763 | fail |
| library-0108-9fd80b2a545d | deterministic-library-sample | maxAbsError | 3.49297142029 | fail |

## Raw artifact retention

Raw PMM, JSONL, and log artifacts are not retained by this quality-report workflow.
Snapshot retention flag: `false`.
