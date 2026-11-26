// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Negative controls: every verification must fail, naming the mutated item.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::{repo_root, require_resources};

const GROUPS: [&str; 14] = [
    "errno", "signal", "fcntl", "mman", "stat", "socket", "netinet", "poll", "time", "resource",
    "limits", "sysconf", "pthread", "misc",
];

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

fn setup(tag: &str) -> Option<(Temp, PathBuf)> {
    let resources = require_resources("mutation")?;
    Some((Temp::new(tag), resources))
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_raven-libc-hdrgen"))
        .args(args)
        .output()
        .expect("run raven-libc-hdrgen")
}

fn generate(temp: &Temp, resources: &Path) -> Output {
    let root = repo_root();
    run(&[
        "generate",
        "--abi",
        temp.abi().to_str().unwrap(),
        "--rust",
        temp.rust().to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        resources.to_str().unwrap(),
    ])
}

fn verify(temp: &Temp, resources: &Path) -> Output {
    let root = repo_root();
    run(&[
        "verify",
        "--target",
        "x86_64",
        "--abi",
        temp.abi().to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        resources.to_str().unwrap(),
    ])
}

fn verify_freebsd(temp: &Temp, resources: &Path, families: &str) -> Output {
    let root = repo_root();
    run(&[
        "verify",
        "--profile",
        "freebsd",
        "--target",
        "all",
        "--families",
        families,
        "--abi",
        temp.abi().to_str().unwrap(),
        "--root",
        root.to_str().unwrap(),
        "--resources",
        resources.to_str().unwrap(),
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
    let Some((temp, resources)) = setup("offset") else {
        return;
    };
    temp.mutate(
        "structs.toml",
        "{ name = \"st_size\", ty = \"i64\", off = 48 }",
        "{ name = \"st_size\", ty = \"i64\", off = 56 }",
    );
    let output = generate(&temp, &resources);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("st_size"), "{}", stderr(&output));
}

#[test]
fn a_flipped_constant_fails_verify_and_names_the_item() {
    let Some((temp, resources)) = setup("constant") else {
        return;
    };
    // The signal and fcntl tables are FreeBSD-leased and measured by the FreeBSD oracle; the
    // musl verify measures the musl-derived groups, so mutate a limits row here.
    temp.mutate(
        "limits.toml",
        "{ name = \"PATH_MAX\", value = 4096 }",
        "{ name = \"PATH_MAX\", value = 4097 }",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp, &resources);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("PATH_MAX"), "{}", stderr(&output));
}

#[test]
fn a_changed_opaque_size_fails_verify_and_names_the_type() {
    let Some((temp, resources)) = setup("opaque") else {
        return;
    };
    temp.mutate(
        "structs.toml",
        "size = 392\nalign = 8",
        "size = 400\nalign = 8",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp, &resources);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("jmp_buf"), "{}", stderr(&output));
}

#[test]
fn an_undocumented_musl_difference_fails_verify() {
    let Some((temp, resources)) = setup("undocumented") else {
        return;
    };
    temp.mutate(
        "socket.toml",
        "{ name = \"SOMAXCONN\", value = 4096",
        "{ name = \"SOMAXCONN\", value = 8192",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp, &resources);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("SOMAXCONN"), "{}", stderr(&output));
}

#[test]
fn a_flipped_shift_fails_verify_and_names_the_item() {
    let Some((temp, resources)) = setup("shift") else {
        return;
    };
    temp.mutate(
        "mman.toml",
        "{ name = \"MAP_ANON\", value = 0x1000 }",
        "{ name = \"MAP_ANON\", value = 0x2000 }",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify_freebsd(&temp, &resources, "mman");
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("MAP_ANON"), "{}", stderr(&output));
}

#[test]
fn a_stale_deviation_row_fails_verify() {
    let Some((temp, resources)) = setup("stale") else {
        return;
    };
    temp.mutate(
        "limits.toml",
        "{ name = \"NGROUPS_MAX\", value = 1023 }",
        "{ name = \"NGROUPS_MAX\", value = 32 }",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp, &resources);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("NGROUPS_MAX"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_undocumented_bsd_family_value_fails_verify() {
    let Some((temp, resources)) = setup("route") else {
        return;
    };
    temp.mutate(
        "socket.toml",
        "{ name = \"AF_ROUTE\", value = 128",
        "{ name = \"AF_ROUTE\", value = 129",
    );
    let generated = generate(&temp, &resources);
    assert!(generated.status.success(), "{}", stderr(&generated));
    let output = verify(&temp, &resources);
    if skip(&output) {
        return;
    }
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("AF_ROUTE"), "{}", stderr(&output));
}

#[test]
fn a_bad_header_name_fails_generate() {
    let Some((temp, resources)) = setup("header") else {
        return;
    };
    temp.mutate(
        "errno.toml",
        "c_header = \"bits/errno.h\"",
        "c_header = \"bits/wrong.h\"",
    );
    let output = generate(&temp, &resources);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("c_header"), "{}", stderr(&output));
}
