use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn run_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mmd-anim"))
        .args(args)
        .output()
        .expect("mmd-anim CLI should run")
}

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    env::temp_dir().join(format!(
        "mmd-anim-cli-exit-{}-{nanos}-{name}",
        std::process::id()
    ))
}

struct TempFileGuard {
    path: PathBuf,
    cleaned: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleaned: false,
        }
    }

    fn cleanup(mut self) {
        fs::remove_file(&self.path).expect("temporary fixture should be removed");
        self.cleaned = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

const VMD_MAGIC: &[u8] = b"Vocaloid Motion Data 0002\0\0\0\0\0";

fn vmd_header() -> Vec<u8> {
    let mut data = VMD_MAGIC.to_vec();
    data.extend_from_slice(&[0; 20]);
    data
}

fn minimal_vmd() -> Vec<u8> {
    let mut data = vmd_header();
    data.extend_from_slice(&0u32.to_le_bytes());
    data
}

fn run_inspect(path: &Path, json: bool) -> Output {
    let path = path_text(path);
    if json {
        run_cli(&["inspect", path, "--json"])
    } else {
        run_cli(&["inspect", path])
    }
}

fn assert_failed_inspect(path: &Path, json: bool) {
    let output = run_inspect(path, json);
    let stderr = stderr_text(&output);

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        output.stdout.is_empty(),
        "stdout={:?} stderr={stderr}",
        output.stdout
    );
    assert!(stderr.contains(path_text(path)), "{stderr}");
    assert!(stderr.contains("format=VMD"), "{stderr}");
}

#[test]
fn success_exits_with_zero() {
    let output = run_cli(&["--version"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr_text(&output));
}

#[test]
fn clap_argument_error_exits_with_two() {
    let output = run_cli(&["inspect"]);

    assert_eq!(output.status.code(), Some(2), "{}", stderr_text(&output));
}

#[test]
fn execution_error_exits_with_one_and_reports_input_path() {
    let missing = unique_temp_path("missing.pmx");
    let output = run_cli(&["inspect", path_text(&missing)]);
    let stderr = stderr_text(&output);

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains(path_text(&missing)), "{stderr}");
}

#[test]
fn detected_format_parse_error_reports_path_and_format() {
    let malformed = unique_temp_path("malformed.pmx");
    fs::write(&malformed, b"PMX ").expect("malformed PMX fixture should be written");

    let output = run_cli(&["inspect", path_text(&malformed)]);
    let stderr = stderr_text(&output);
    let _ = fs::remove_file(&malformed);

    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains(path_text(&malformed)), "{stderr}");
    assert!(stderr.contains("format=PMX"), "{stderr}");
    assert!(!stderr.contains("detected="), "{stderr}");
}

#[test]
fn malformed_vmd_files_fail_closed_for_text_and_json_inspect() {
    let mut bone_record_truncation = minimal_vmd();
    bone_record_truncation[50..54].copy_from_slice(&1u32.to_le_bytes());
    bone_record_truncation.extend_from_slice(&[0; 110]);

    let mut partial_optional_count = minimal_vmd();
    partial_optional_count.push(0);

    let fixtures = [
        ("invalid-magic.vmd", b"not a VMD".to_vec()),
        ("header-truncation.vmd", VMD_MAGIC.to_vec()),
        ("bone-record-truncation.vmd", bone_record_truncation),
        ("partial-optional-count.vmd", partial_optional_count),
    ];

    for (name, data) in fixtures {
        let fixture = TempFileGuard::new(unique_temp_path(name));
        fs::write(&fixture.path, data).expect("malformed VMD fixture should be written");
        assert_failed_inspect(&fixture.path, false);
        assert_failed_inspect(&fixture.path, true);
        fixture.cleanup();
    }
}

#[test]
fn truncated_optional_tail_remains_compatibility_tolerated() {
    let mut data = minimal_vmd();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());

    let guard = TempFileGuard::new(unique_temp_path("truncated-optional-camera-tail.vmd"));
    fs::write(&guard.path, data).expect("truncated optional VMD fixture should be written");

    for json in [false, true] {
        let output = run_inspect(&guard.path, json);
        let stderr = stderr_text(&output);

        assert_eq!(output.status.code(), Some(0), "{stderr}");
        assert!(stderr.is_empty(), "{stderr}");
        assert!(!output.stdout.is_empty(), "json={json}");
    }

    guard.cleanup();
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("temporary path should be valid UTF-8")
}
