// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end fixture builds: five package kinds against a fake sysroot.
//!
//! The produced static libraries are never executed. When the toolchain is absent the test prints
//! `SKIP`; `RL_WORLD_REQUIRED=1` turns that into a failure (section 7).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libc_world::sha256;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().is_ok()
}

fn require_tools() -> bool {
    for tool in ["clang", "llvm-ar", "tar", "patch", "make", "cmake", "ninja"] {
        if !have(tool) {
            if std::env::var("RL_WORLD_REQUIRED").is_ok_and(|value| value == "1") {
                panic!("missing required tool `{tool}` (RL_WORLD_REQUIRED=1)");
            }
            eprintln!("SKIP: {tool} not found");
            return false;
        }
    }
    true
}

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("libc-world-e2e-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("dir");
    for entry in fs::read_dir(source).expect("read") {
        let entry = entry.expect("entry");
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

fn compile(sysroot: &Path, source: &Path, object: &Path) {
    let status = Command::new("clang")
        .args([
            "--target=x86_64-unknown-ravenbsd",
            "-I",
            sysroot.join("usr/include").to_str().expect("path"),
            "-c",
            source.to_str().expect("path"),
            "-o",
            object.to_str().expect("path"),
        ])
        .status()
        .expect("clang");
    assert!(status.success(), "clang failed for {}", source.display());
}

fn build_sysroot(temp: &Path) -> PathBuf {
    let sysroot = temp.join("sysroot");
    copy_tree(
        &fixtures().join("sysroot/usr/include"),
        &sysroot.join("usr/include"),
    );
    fs::create_dir_all(sysroot.join("usr/lib/pkgconfig")).expect("pkgconfig");
    fs::create_dir_all(sysroot.join("lib")).expect("lib");
    let work = temp.join("sysroot-work");
    fs::create_dir_all(&work).expect("work");
    for source in ["fake-libc.c", "crt1.c", "crti.c", "crtn.c"] {
        let object = work.join(format!("{source}.o"));
        compile(&sysroot, &fixtures().join("sysroot").join(source), &object);
        if source == "fake-libc.c" {
            let status = Command::new("llvm-ar")
                .arg("rcs")
                .arg(sysroot.join("usr/lib/libc.a"))
                .arg(&object)
                .status()
                .expect("ar");
            assert!(status.success());
        } else {
            let name = source.trim_end_matches(".c");
            fs::copy(&object, sysroot.join("usr/lib").join(format!("{name}.o"))).expect("copy");
        }
    }
    sysroot
}

fn make_tarball(name: &str, version: &str, tarballs: &Path) -> String {
    let tarball = tarballs.join(format!("{name}-{version}.tar.gz"));
    let status = Command::new("tar")
        .arg("czf")
        .arg(&tarball)
        .arg("-C")
        .arg(fixtures().join("pkgs"))
        .arg(format!("{name}-{version}"))
        .status()
        .expect("tar");
    assert!(status.success(), "tar failed for {name}");
    sha256::hex_digest(&fs::read(&tarball).expect("read tarball"))
}

fn fill_row(world: &str, name: &str, version: &str, tarball: &str, sha: &str) -> String {
    let mut parts: Vec<String> = world.split("[[package]]").map(str::to_string).collect();
    for part in &mut parts {
        if part.contains(&format!("name = \"{name}\"")) {
            *part = part
                .replace(
                    "version = \"PLACEHOLDER\"",
                    &format!("version = \"{version}\""),
                )
                .replace(
                    "url = \"PLACEHOLDER\"",
                    &format!("url = \"https://example.invalid/{tarball}\""),
                )
                .replace("sha256 = \"PLACEHOLDER\"", &format!("sha256 = \"{sha}\""))
                .replace(
                    "tarball = \"PLACEHOLDER\"",
                    &format!("tarball = \"{tarball}\""),
                );
        }
    }
    parts.join("[[package]]")
}

fn fake_lld_bin(temp: &Path) -> PathBuf {
    let bin = temp.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    let lld = bin.join("ld.lld");
    fs::write(&lld, "#!/bin/sh\nexit 0\n").expect("lld");
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = fs::metadata(&lld).expect("meta").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&lld, permissions).expect("chmod");
    bin
}

fn path_with(bin: &Path) -> String {
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn build_subset(temp: &Path, subset: &str) -> (std::process::Output, PathBuf, PathBuf) {
    let sysroot = build_sysroot(temp);
    let tarballs = temp.join("tarballs");
    fs::create_dir_all(&tarballs).expect("tarballs");
    let rows = [
        ("zlib", "1.0"),
        ("lua", "1.0"),
        ("sqlite", "1.0"),
        ("pcre2", "1.0"),
        ("toybox", "1.0"),
    ];
    let mut world =
        fs::read_to_string(repo_root().join("tools/libc-world/world.toml")).expect("world");
    for (name, version) in rows {
        let sha = make_tarball(name, version, &tarballs);
        world = fill_row(
            &world,
            name,
            version,
            &format!("{name}-{version}.tar.gz"),
            &sha,
        );
    }
    let world_path = temp.join("world.toml");
    fs::write(&world_path, world).expect("world");
    let bin = fake_lld_bin(temp);
    let out = temp.join("out");
    let results = temp.join("results");
    let output = Command::new(env!("CARGO_BIN_EXE_libc-world"))
        .current_dir(repo_root())
        .args(["build", "--subset", subset])
        .arg("--world")
        .arg(&world_path)
        .arg("--sysroot")
        .arg(&sysroot)
        .arg("--tarballs")
        .arg(&tarballs)
        .arg("--out")
        .arg(&out)
        .arg("--results")
        .arg(&results)
        .args(["--jobs", "2"])
        .env("PATH", path_with(&bin))
        .output()
        .expect("build");
    (output, out, results)
}

#[test]
fn the_five_smoke_packages_build_against_the_fake_sysroot() {
    if !require_tools() {
        return;
    }
    let temp = temp("smoke5");
    let (output, _out, results) = build_subset(&temp, "smoke5");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for name in ["zlib", "lua", "sqlite", "pcre2", "toybox"] {
        assert!(stdout.contains(&format!("{name}\tbuilt")), "{stdout}");
        let text = fs::read_to_string(results.join(format!("x86_64/{name}.json"))).expect("result");
        assert!(text.contains("\"status\":\"built\""), "{name}: {text}");
        assert!(text.contains("\"steps\":[{"), "{name}: {text}");
    }
    let undef = fs::read_to_string(results.join("x86_64/undef/zlib.undef")).expect("undef");
    assert!(undef.contains("strlcpy\tU"), "{undef}");
    let report = Command::new(env!("CARGO_BIN_EXE_libc-world"))
        .current_dir(repo_root())
        .args(["report", "--results"])
        .arg(&results)
        .args(["--arch", "x86_64"])
        .output()
        .expect("report");
    assert!(report.status.success());
    let md = fs::read_to_string(results.join("x86_64/world.md")).expect("world.md");
    assert!(md.contains("| 25 | zlib | 1 | built |"));
}

#[test]
fn a_repeated_single_package_build_is_byte_identical() {
    if !require_tools() {
        return;
    }
    let first = temp("determinism-a");
    let second = temp("determinism-b");
    let (status_a, _, results_a) = build_subset(&first, "smoke5");
    let (status_b, _, results_b) = build_subset(&second, "smoke5");
    assert_eq!(status_a.status.code(), Some(0));
    assert_eq!(status_b.status.code(), Some(0));
    let a = fs::read(results_a.join("x86_64/zlib.json")).expect("a");
    let b = fs::read(results_b.join("x86_64/zlib.json")).expect("b");
    assert_eq!(a, b);
}

#[test]
fn a_built_result_has_no_absolute_paths() {
    if !require_tools() {
        return;
    }
    let temp = temp("paths");
    let (output, _, results) = build_subset(&temp, "smoke5");
    assert_eq!(output.status.code(), Some(0));
    let text = fs::read_to_string(results.join("x86_64/zlib.json")).expect("result");
    assert!(!text.contains(temp.to_str().expect("path")));
    assert!(text.contains("\"tests\":\"not-run\""));
}
