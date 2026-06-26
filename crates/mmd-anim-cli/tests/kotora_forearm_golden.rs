use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const KOTORA_MANIFEST_ENV: &str = "MMD_ANIM_KOTORA_FOREARM_GOLDEN_MANIFEST";

#[test]
#[ignore = "requires local Kotora GoldenOracle data generated from MMD/MMDDumper"]
fn kotora_forearm_golden_compare_passes_with_ordered_pmx_runtime() {
    let manifest = local_manifest();
    require_local_manifest(&manifest);

    let output = mmd_anim()
        .arg("verify")
        .arg(&manifest)
        .args(["--mode", "numeric"])
        .output()
        .expect("run mmd-anim verify");

    assert!(
        output.status.success(),
        "Kotora focused compare should pass once the forearm runtime is fixed\n{}",
        combined_output(&output)
    );
    let combined = combined_output(&output);
    assert_contains(
        &combined,
        "Numeric compare: ok",
        "Kotora compare did not report success",
    );
    assert_contains(
        &combined,
        "motionMaxAbsError=0.001101",
        "unexpected Kotora max error",
    );
    assert_contains(
        &combined,
        "motionWorst=kotora-gym-weekender-forearm:4800:左肘捩抽出",
        "unexpected Kotora worst bone/frame",
    );
}

#[test]
#[ignore = "requires local Kotora GoldenOracle data generated from MMD/MMDDumper"]
fn kotora_forearm_golden_diagnosis_stays_within_epsilon_after_fix() {
    let manifest = local_manifest();
    require_local_manifest(&manifest);

    let output = mmd_anim()
        .arg("verify")
        .arg(&manifest)
        .args([
            "--mode",
            "numeric",
            "--diagnose",
            "kotora-gym-weekender-forearm",
            "4800",
            "左肘捩抽出",
            "--eval-frame",
            "4801",
        ])
        .output()
        .expect("run mmd-anim verify --diagnose");

    assert!(
        output.status.success(),
        "Kotora diagnosis command failed\n{}",
        combined_output(&output)
    );
    let combined = combined_output(&output);
    assert_contains(
        &combined,
        "bone=左肘捩抽出 index=69 oracleIndex=69",
        "diagnosis no longer points at the expected extraction bone",
    );
    assert_contains(
        &combined,
        "postMaxDelta=0.00110",
        "diagnosis max delta changed",
    );
    assert_contains(
        &combined,
        "vmdKeys=0 exactVmdKeys=0",
        "diagnosis should remain focused on an unkeyed extraction bone",
    );
}

fn local_manifest() -> PathBuf {
    std::env::var_os(KOTORA_MANIFEST_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{KOTORA_MANIFEST_ENV} must point to the local Kotora manifest"))
}

fn require_local_manifest(path: &Path) {
    assert!(
        path.exists(),
        "missing local Kotora GoldenOracle manifest: {}; set {KOTORA_MANIFEST_ENV}",
        path.display()
    );
}

fn mmd_anim() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mmd-anim"))
}

fn combined_output(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_contains(haystack: &str, needle: &str, context: &str) {
    assert!(
        haystack.contains(needle),
        "{context}: missing `{needle}`\n{haystack}"
    );
}
