// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn root() -> &'static Path {
    Path::new("/repo")
}

fn real_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask parent")
        .to_path_buf()
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn two_plain_arguments_are_an_adhoc_build() {
    let planned = plan(root(), &args(&["openbsd", "lib/libc/string/strlcpy.c"])).expect("plan");
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].spec.program, "cargo");
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "run",
            "-q",
            "-p",
            "libc-oracle",
            "--",
            "adhoc",
            "openbsd",
            "lib/libc/string/strlcpy.c",
        ])
    );
    assert_eq!(planned[0].spec.cwd, Path::new("/repo"));
    assert!(planned[0].spec.env.is_empty());
}

#[test]
fn build_list_check_and_vendor_are_forwarded_verbatim() {
    for command in ["build", "list", "check", "vendor"] {
        let planned = plan(root(), &args(&[command, "--flag", "value"])).expect("plan");
        assert_eq!(planned.len(), 1);
        assert_eq!(
            planned[0].spec.args,
            args(&[
                "run",
                "-q",
                "-p",
                "libc-oracle",
                "--",
                command,
                "--flag",
                "value",
            ])
        );
    }
}

#[test]
fn diff_builds_both_optimisation_levels_and_sets_the_environment() {
    let planned = plan(root(), &args(&["diff", "string-basic"])).expect("plan");
    assert_eq!(planned.len(), 3);
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "run",
            "-q",
            "-p",
            "libc-oracle",
            "--",
            "build",
            "string-basic",
            "--opt",
            "O2,O0",
        ])
    );
    assert_eq!(
        planned[1].spec.args,
        args(&[
            "test",
            "-p",
            "raven-libc-oracle",
            "--features",
            "oracle-string-basic",
        ])
    );
    let o2 = "/repo/build/libc-oracle/string-basic/O2";
    assert_eq!(
        planned[1].spec.env,
        vec![
            ("LIBRARY_PATH".to_string(), o2.to_string()),
            ("LD_LIBRARY_PATH".to_string(), o2.to_string()),
            ("RAVEN_ORACLE_DIR".to_string(), o2.to_string()),
        ]
    );
    let o0 = "/repo/build/libc-oracle/string-basic/O0";
    assert!(
        planned[2]
            .spec
            .env
            .contains(&("RAVEN_ORACLE_DIR".to_string(), o0.to_string()))
    );
    assert_eq!(
        planned[1].label.as_deref(),
        Some("libc-oracle diff string-basic O2")
    );
    assert_eq!(
        planned[2].label.as_deref(),
        Some("libc-oracle diff string-basic O0")
    );
}

#[test]
fn diff_flags_reach_both_the_build_and_the_test() {
    let planned = plan(
        root(),
        &args(&[
            "diff",
            "string-basic",
            "--seeds",
            "5",
            "--exhaustive",
            "--opt",
            "O0",
            "--sanitize",
            "--no-pin-check",
        ]),
    )
    .expect("plan");
    assert_eq!(planned.len(), 2);
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "run",
            "-q",
            "-p",
            "libc-oracle",
            "--",
            "build",
            "string-basic",
            "--opt",
            "O0",
            "--sanitize",
            "--no-pin-check",
        ])
    );
    assert!(
        planned[1]
            .spec
            .env
            .contains(&("RL_DIFF_SEEDS".to_string(), "5".to_string()))
    );
    assert!(
        planned[1]
            .spec
            .env
            .contains(&("RL_DIFF_EXHAUSTIVE".to_string(), "1".to_string()))
    );
}

#[test]
fn malformed_shapes_are_usage_errors() {
    assert!(plan(root(), &[]).is_err());
    assert!(plan(root(), &args(&["openbsd"])).is_err());
    assert!(plan(root(), &args(&["openbsd", "a.c", "extra"])).is_err());
    assert!(plan(root(), &args(&["--wat"])).is_err());
    assert!(plan(root(), &args(&["diff"])).is_err());
    assert!(plan(root(), &args(&["diff", "x", "--seeds", "many"])).is_err());
    assert!(plan(root(), &args(&["diff", "x", "--opt", "O9"])).is_err());
}

#[test]
fn help_prints_and_succeeds_without_spawning() {
    assert_eq!(run(root(), &args(&["--help"])).expect("help"), 0);
}

#[test]
fn model_family_has_one_test_step_and_no_build() {
    let planned = plan(&real_root(), &args(&["diff", "tls-layout"])).expect("plan");
    assert_eq!(planned.len(), 1);
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "test",
            "-p",
            "raven-libc-oracle",
            "--features",
            "oracle-tls-layout",
        ])
    );
    assert_eq!(
        planned[0].label.as_deref(),
        Some("libc-oracle diff tls-layout")
    );
}

fn suite_forwards_to_raven_atf_run() {
    let planned = plan(
        root(),
        &args(&["suite", "--suite", "netbsd-libc", "--mode", "oracle"]),
    )
    .expect("plan");
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].spec.program, "cargo");
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "run",
            "-q",
            "-p",
            "raven-atf-run",
            "--",
            "suite",
            "--suite",
            "netbsd-libc",
            "--mode",
            "oracle",
        ])
    );
}

#[test]
fn doc17_suite_spelling_is_mapped_to_the_suite_subcommand() {
    let planned = plan(
        root(),
        &args(&["--suite", "netbsd", "--target", "host-oracle"]),
    )
    .expect("plan");
    assert_eq!(
        planned[0].spec.args,
        args(&[
            "run",
            "-q",
            "-p",
            "raven-atf-run",
            "--",
            "suite",
            "--suite",
            "netbsd",
            "--target",
            "host-oracle",
        ])
    );
}

#[test]
fn gate_forwards_to_raven_atf_run() {
    let planned = plan(root(), &args(&["gate", "--g1"])).expect("plan");
    assert_eq!(
        planned[0].spec.args,
        args(&["run", "-q", "-p", "raven-atf-run", "--", "gate", "--g1"])
    );
}

#[test]
fn the_doc17_g1_spelling_is_mapped_to_the_gate_subcommand() {
    let planned = plan(root(), &args(&["--g1"])).expect("plan");
    assert_eq!(
        planned[0].spec.args,
        args(&["run", "-q", "-p", "raven-atf-run", "--", "gate", "--g1"])
    );
}

#[test]
fn qemu_target_is_passed_through_for_the_runner_to_refuse() {
    let planned = plan(
        root(),
        &args(&["suite", "--suite", "netbsd", "--target", "qemu-x86_64"]),
    )
    .expect("plan");
    assert!(planned[0].spec.args.iter().any(|arg| arg == "qemu-x86_64"));
}
