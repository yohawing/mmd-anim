fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", git_path("HEAD"));
    println!("cargo:rerun-if-changed={}", git_path("index"));

    let rustc_version =
        command_stdout("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MMD_ANIM_CLI_RUSTC_VERSION={rustc_version}");

    let build_target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=MMD_ANIM_CLI_BUILD_TARGET={build_target}");

    let git_commit = command_stdout("git", &["rev-parse", "--short", "HEAD"])
        .filter(|commit| !commit.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=MMD_ANIM_CLI_GIT_COMMIT={git_commit}");
}

fn git_path(path: &str) -> String {
    command_stdout("git", &["rev-parse", "--git-path", path])
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!(".git/{path}"))
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
}
