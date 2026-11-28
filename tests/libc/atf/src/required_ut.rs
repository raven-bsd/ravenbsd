// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::Case;
use crate::Outcome;
use crate::table::TableRow;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-required-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

fn row(suite: &str, program: &str, case: &str, verdict: Verdict, reason: &str) -> TableRow {
    TableRow {
        outcome: Outcome::new(Case::new(suite, program, case), verdict, reason, "S750"),
        oracle_pass: verdict == Verdict::Pass,
    }
}

fn table(rows: Vec<TableRow>) -> ConformanceTable {
    ConformanceTable {
        rows,
        stats: Vec::new(),
    }
}

fn required(id: &str, suite: &str, program: &str, case: &str) -> RequiredRow {
    RequiredRow {
        id: id.to_string(),
        suite: suite.to_string(),
        program: program.to_string(),
        case: case.to_string(),
        donor: "donor.c".to_string(),
        expect: "the donor assertion".to_string(),
        environment: "x86_64".to_string(),
        regression: "tests/regression.rs".to_string(),
        prerequisite: Vec::new(),
    }
}

#[test]
fn a_passing_exact_body_meets_the_row() {
    let config = RequiredConfig {
        schema: 1,
        rows: vec![required(
            "descriptors-dup-close",
            "freebsd-libc-sys",
            "sys/t_dup",
            "dup2_basic",
        )],
    };
    let outcomes = evaluate(
        &config,
        &table(vec![row(
            "freebsd-libc-sys",
            "sys/t_dup",
            "dup2_basic",
            Verdict::Pass,
            "",
        )]),
    );
    assert!(met(&outcomes));
    assert_eq!(outcomes[0].note, "");
}

#[test]
fn an_empty_matrix_is_never_a_pass() {
    let config = RequiredConfig {
        schema: 1,
        rows: Vec::new(),
    };
    let outcomes = evaluate(&config, &table(Vec::new()));
    assert!(outcomes.is_empty());
    assert!(!met(&outcomes));
    assert!(render(&outcomes).contains("required: FAIL (0 of 0 met)"));
}

#[test]
fn a_missing_required_body_fails_the_row() {
    let config = RequiredConfig {
        schema: 1,
        rows: vec![required(
            "descriptors-dup-close",
            "freebsd-libc-sys",
            "sys/t_dup",
            "dup2_basic",
        )],
    };
    let outcomes = evaluate(&config, &table(Vec::new()));
    assert!(!met(&outcomes));
    assert!(outcomes[0].note.contains("missing: no result row"));
}

#[test]
fn fail_skip_not_run_excluded_and_oracle_fail_all_fail_the_row() {
    for verdict in [
        Verdict::Fail,
        Verdict::Skip,
        Verdict::NotRun,
        Verdict::Excluded,
        Verdict::OracleFail,
    ] {
        let config = RequiredConfig {
            schema: 1,
            rows: vec![required("row", "s", "p", "c")],
        };
        let outcomes = evaluate(&config, &table(vec![row("s", "p", "c", verdict, "why")]));
        assert!(!met(&outcomes), "{verdict} must not satisfy a required row");
        assert!(outcomes[0].note.starts_with("not pass:"));
    }
}

#[test]
fn a_wildcard_requires_every_matching_body_to_pass() {
    let config = RequiredConfig {
        schema: 1,
        rows: vec![required("row", "s", "p/*", "*")],
    };
    let passing = evaluate(
        &config,
        &table(vec![
            row("s", "p/a", "one", Verdict::Pass, ""),
            row("s", "p/b", "two", Verdict::Pass, ""),
        ]),
    );
    assert!(met(&passing));
    let failing = evaluate(
        &config,
        &table(vec![
            row("s", "p/a", "one", Verdict::Pass, ""),
            row("s", "p/b", "two", Verdict::Fail, "behaviour:bad"),
        ]),
    );
    assert!(!met(&failing));
}

#[test]
fn a_failed_prerequisite_fails_the_row() {
    let mut required = required("wait", "s", "t_wait", "wait6_exited");
    required.prerequisite.push(Prerequisite {
        suite: "s".to_string(),
        program: "t_kill".to_string(),
        case: "kill_basic".to_string(),
    });
    let config = RequiredConfig {
        schema: 1,
        rows: vec![required],
    };
    let outcomes = evaluate(
        &config,
        &table(vec![
            row("s", "t_wait", "wait6_exited", Verdict::Pass, ""),
            row("s", "t_kill", "kill_basic", Verdict::Fail, "fork failed"),
        ]),
    );
    assert!(!met(&outcomes));
    assert!(outcomes[0].note.contains("prerequisite not pass"));
}

#[test]
fn a_missing_prerequisite_fails_the_row() {
    let mut required = required("wait", "s", "t_wait", "wait6_exited");
    required.prerequisite.push(Prerequisite {
        suite: "s".to_string(),
        program: "t_umask".to_string(),
        case: "umask_fork".to_string(),
    });
    let config = RequiredConfig {
        schema: 1,
        rows: vec![required],
    };
    let outcomes = evaluate(
        &config,
        &table(vec![row("s", "t_wait", "wait6_exited", Verdict::Pass, "")]),
    );
    assert!(!met(&outcomes));
    assert!(outcomes[0].note.contains("prerequisite missing"));
}

#[test]
fn every_row_must_pass_independently() {
    let config = RequiredConfig {
        schema: 1,
        rows: vec![
            required("one", "s", "p", "a"),
            required("two", "s", "p", "b"),
        ],
    };
    let outcomes = evaluate(
        &config,
        &table(vec![
            row("s", "p", "a", Verdict::Pass, ""),
            row("s", "p", "b", Verdict::NotRun, "no-run-backend"),
        ]),
    );
    assert!(!met(&outcomes));
    assert_eq!(outcomes.iter().filter(|outcome| outcome.met).count(), 1);
    assert!(render(&outcomes).contains("required: FAIL (1 of 2 met)"));
}

#[test]
fn render_names_every_row_and_the_final_verdict() {
    let outcomes = vec![
        RequiredOutcome {
            id: "one".to_string(),
            met: true,
            note: String::new(),
        },
        RequiredOutcome {
            id: "two".to_string(),
            met: false,
            note: "missing: no result row".to_string(),
        },
    ];
    assert_eq!(
        render(&outcomes),
        "REQUIRED one: MET\nREQUIRED two: NOT MET (missing: no result row)\nrequired: FAIL (1 of 2 met)\n"
    );
}

#[test]
fn loading_rejects_a_wrong_schema_an_empty_row_and_a_duplicate_id() {
    let root = temp("load");
    let wrong = root.join("wrong.toml");
    fs::write(&wrong, "schema = 2\n").expect("write");
    assert!(RequiredConfig::load(&wrong).is_err());

    let empty = root.join("empty.toml");
    fs::write(
        &empty,
        "schema = 1\n\n[[required]]\nid = \"\"\nsuite = \"s\"\nprogram = \"p\"\ndonor = \"d\"\nenvironment = \"e\"\nregression = \"r\"\n",
    )
    .expect("write");
    assert!(RequiredConfig::load(&empty).is_err());

    let duplicate = root.join("duplicate.toml");
    fs::write(
        &duplicate,
        "schema = 1\n\n[[required]]\nid = \"x\"\nsuite = \"s\"\nprogram = \"p\"\ndonor = \"d\"\nenvironment = \"e\"\nregression = \"r\"\n\n[[required]]\nid = \"x\"\nsuite = \"s\"\nprogram = \"q\"\ndonor = \"d\"\nenvironment = \"e\"\nregression = \"r\"\n",
    )
    .expect("write");
    assert!(RequiredConfig::load(&duplicate).is_err());
}

#[test]
fn loading_rejects_a_row_without_an_expected_behaviour() {
    let root = temp("expect");
    let path = root.join("no-expect.toml");
    fs::write(
        &path,
        "schema = 1\n\n[[required]]\nid = \"x\"\nsuite = \"s\"\nprogram = \"p\"\ndonor = \"d\"\nenvironment = \"e\"\nregression = \"r\"\n",
    )
    .expect("write");
    assert!(RequiredConfig::load(&path).is_err());
}

#[test]
fn loading_accepts_the_committed_matrix() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libc-oracle/required.toml");
    let config = RequiredConfig::load(&path).expect("committed required matrix must parse");
    assert!(!config.rows.is_empty(), "the matrix must not be empty");
}
