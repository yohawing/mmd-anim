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

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
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

fn path_text(path: &Path) -> &str {
    path.to_str().expect("temporary path should be valid UTF-8")
}
