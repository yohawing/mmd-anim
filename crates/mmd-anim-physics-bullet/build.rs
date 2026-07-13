use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=MMD_ANIM_BULLET3_DIR");
    println!("cargo:rerun-if-changed=native/mmd_bullet_api.cpp");
    println!("cargo:rerun-if-changed=native/mmd_bullet_api.h");

    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let bullet_dir = env::var_os("MMD_ANIM_BULLET3_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("vendor/bullet3"));
    let bullet_src = bullet_dir.join("src");
    println!("cargo:rerun-if-changed={}", bullet_src.display());

    if !bullet_src.is_dir() {
        panic!(
            "Bullet sources not found at {}. Restore vendor/bullet3 or set MMD_ANIM_BULLET3_DIR to a Bullet3 checkout.",
            bullet_dir.display()
        );
    }

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(&bullet_src)
        .file("native/mmd_bullet_api.cpp")
        .flag_if_supported("-std=c++17")
        .flag_if_supported("/std:c++17")
        .define("BT_NO_PROFILE", None);

    for dir in ["LinearMath", "BulletCollision", "BulletDynamics"] {
        add_cpp_files(&mut build, &bullet_src.join(dir));
    }

    build.compile("mmd_anim_bullet");
}

fn add_cpp_files(build: &mut cc::Build, dir: &Path) {
    let entries = fs::read_dir(dir).unwrap_or_else(|err| {
        panic!(
            "failed to read Bullet source directory {}: {err}",
            dir.display()
        )
    });

    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            let path_text = path.to_string_lossy();
            if path_text.contains("TaskScheduler") {
                continue;
            }
            add_cpp_files(build, &path);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("cpp") {
            build.file(path);
        }
    }
}
