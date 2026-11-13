// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Negative controls: every verification must fail, naming the mutated item.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GROUPS: [&str; 14] = [
    "errno", "signal", "fcntl", "mman", "stat", "socket", "netinet", "poll", "time", "resource",
    "limits", "sysconf", "pthread", "misc",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn resources() -> PathBuf {
    let mut directory = Some(repo_root());
    while let Some(current) = directory {
        let candidate = current.join("ravenbsd-resources");
        if candidate.join("donors.lock").is_file() {
            return candidate;
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    panic!("cannot find ravenbsd-resources");
}

struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "hdrgen-mut-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        std::fs::create_dir_all(path.join("abi")).expect("temp dir");
        for name in ["types.toml", "structs.toml", "deviations.toml"] {
            std::fs::copy(
                repo_root().join("libc/ravenlibc/abi").join(name),
                path.join("abi").join(name),
            )
            .expect("copy");
        }
        for group in GROUPS {
            let name = format!("{group}.toml");
            std::fs::copy(
                repo_root().join("libc/ravenlibc/abi").join(&name),
                path.join("abi").join(name),
            )
            .expect("copy");
        }
        Self { path }
    }

    fn abi(&self) -> PathBuf {
        self.path.join("abi")
    }

    fn rust(&self) -> PathBuf {
        self.path.join("rust")
    }

    fn mutate(&self, file: &str, from: &str, to: &str) {
        let path = self.abi().join(file);
        let text = std::fs::read_to_string(&path).expect("read table");
        assert!(
            text.contains(from),
            "mutation anchor {from:?} missing in {file}"
        );
        std::fs::write(&path, text.replacen(from, to, 1)).expect("write table");
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_raven-libc-hdrgen"))
        .args(args)
        .output()
        .expect("run raven-libc-hdrgen")
}

fn generate(temp: &Temp) -> Output {
    let root = repo_root();
    let res = resources();
    run(&[
        "generate",
        "--abi",
        temp.abi().to_str().unwrap(),
        "--rust",
        temp.rust().to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ])
}

fn verify(temp: &Temp) -> Output {
    let root = repo_root();
    let res = resources();
    run(&[
        "verify",
        "--target",
        "x86_64",
        "--abi",
        temp.abi().to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        res.to_str().unwrap(),
    ])
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn skip(output: &Output) -> bool {
    output.status.code() == Some(77)
}

#[test]
fn a_flipped_offset_fails_generate_and_names_the_field() {
    let temp = Temp::new("offset");
    temp.mutate(
        "structs.toml",
        "{ name = \"st_size\", ty = \"i64\", off = 48 }",
        "{ name = \"st_size\", ty = \"i64\", off = 56 }",
    );
    let output = generate(&temp);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("st_size"), "{}", stderr(&output));
}

#[test]
fn a_flipped_constant_fails_verify_and_names_the_item() {
    let temp = Temp::new("constant");
    temp.mutate(
        "errno.toml",
        "{ name = \"EPERM\", value = 1 }",
        "{ name = \"EPERM\", value = 2 }",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("EPERM"), "{}", stderr(&output));
}

#[test]
fn a_changed_opaque_size_fails_verify_and_names_the_type() {
    let temp = Temp::new("opaque");
    temp.mutate(
        "structs.toml",
        "size = 392\nalign = 8",
        "size = 400\nalign = 8",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("jmp_buf"), "{}", stderr(&output));
}

#[test]
fn an_undocumented_musl_difference_fails_verify() {
    let temp = Temp::new("undocumented");
    temp.mutate(
        "socket.toml",
        "{ name = \"SOMAXCONN\", value = 4096",
        "{ name = \"SOMAXCONN\", value = 8192",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("SOMAXCONN"), "{}", stderr(&output));
}

#[test]
fn a_flipped_shift_fails_verify_and_names_the_item() {
    let temp = Temp::new("shift");
    temp.mutate(
        "mman.toml",
        "{ name = \"MAP_HUGE_16KB\", shl = [14, 26] }",
        "{ name = \"MAP_HUGE_16KB\", shl = [15, 26] }",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("MAP_HUGE_16KB"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_stale_deviation_row_fails_verify() {
    let temp = Temp::new("stale");
    temp.mutate(
        "fcntl.toml",
        "{ name = \"O_LARGEFILE\", value = 0",
        "{ name = \"O_LARGEFILE\", value = 4096",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("O_LARGEFILE"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_undocumented_bsd_family_value_fails_verify() {
    let temp = Temp::new("route");
    temp.mutate(
        "socket.toml",
        "{ name = \"AF_ROUTE\", value = 128",
        "{ name = \"AF_ROUTE\", value = 129",
    );
    let generated = generate(&temp);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("AF_ROUTE"), "{}", stderr(&output));
}

#[test]
fn a_bad_header_name_fails_generate() {
    let temp = Temp::new("header");
    temp.mutate(
        "errno.toml",
        "c_header = \"bits/errno.h\"",
        "c_header = \"bits/wrong.h\"",
    );
    let output = generate(&temp);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("c_header"), "{}", stderr(&output));
}
