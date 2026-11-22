// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the owner evidence overlay.

use std::path::{Path, PathBuf};

use crate::model::{HeaderRow, Meta, PlatformStatus, Profile, STANDARD, Status};
use crate::overlay::{Claim, Overlay};

fn root(label: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("posix-overlay-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let path = base.join("root");
    std::fs::create_dir_all(base.join("ravenbsd-prjmgmt/architecture/product-data/posix/evidence"))
        .unwrap();
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn posix(root: &Path) -> PathBuf {
    root.parent()
        .unwrap()
        .join("ravenbsd-prjmgmt/architecture/product-data/posix")
}

fn cleanup(path: &Path) {
    if let Some(base) = path.parent() {
        let _ = std::fs::remove_dir_all(base);
    }
}

fn write_owner(root: &Path, owner: &str, body: &str) {
    std::fs::write(
        posix(root).join("evidence").join(format!("{owner}.toml")),
        format!("schema = 1\nstandard = \"posix-2024\"\n\n{body}"),
    )
    .unwrap();
}

fn write_test(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn profile_with_assert_header() -> Profile {
    Profile {
        schema: 1,
        standard: STANDARD.to_string(),
        platform: PlatformStatus::new(),
        header: vec![HeaderRow {
            meta: Meta {
                id: "header.assert.h".to_string(),
                standard: STANDARD.to_string(),
                status: Status::Partial,
                evidence: "libc/ravenlibc/include".to_string(),
                owner: "P1".to_string(),
                waiver: String::new(),
                notes: String::new(),
                pg: Vec::new(),
            },
            name: "assert.h".to_string(),
            required_macros: Vec::new(),
            required_types: Vec::new(),
            required_functions: Vec::new(),
        }],
        interface: Vec::new(),
        utility: Vec::new(),
        shell_feature: Vec::new(),
        filesystem: Vec::new(),
        process: Vec::new(),
        tty: Vec::new(),
    }
}

#[test]
fn merge_precedence_overrides_the_derived_status() {
    let path = root("merge");
    write_test(&path, "tests/posix/check-headers.sh", "#!/bin/sh\nexit 0\n");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/check-headers.sh\"\n\
         notes = \"assert.h compiles under the POSIX feature macros\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.unsupported.is_empty(), "{outcome:?}");
    assert_eq!(outcome.claims.len(), 1);
    assert!(matches!(outcome.claims[0], Claim::Test { .. }));
    assert_eq!(outcome.claims[0].id(), "header.assert.h");
    assert_eq!(
        outcome.claims[0].command(),
        "test:tests/posix/check-headers.sh"
    );
    let meta = profile.meta_mut("header.assert.h").unwrap();
    assert_eq!(meta.status, Status::Present);
    assert_eq!(meta.evidence, "test:tests/posix/check-headers.sh");
    assert_eq!(
        meta.notes,
        "assert.h compiles under the POSIX feature macros"
    );
    cleanup(&path);
}

#[test]
fn qtest_evidence_resolves_to_a_case() {
    let path = root("qtest");
    write_test(
        &path,
        "tests/qtest/cases/overlay-pass.toml",
        "name = \"overlay-pass\"\n",
    );
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
         evidence = \"qtest:overlay-pass\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.unsupported.is_empty(), "{outcome:?}");
    assert_eq!(outcome.claims.len(), 1);
    assert!(matches!(
        &outcome.claims[0],
        Claim::Qtest { case, .. } if case == "overlay-pass"
    ));
    cleanup(&path);
}

#[test]
fn present_without_executable_evidence_is_unsupported() {
    let path = root("missing-evidence");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\nevidence = \"none\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(
        outcome.unsupported[0]
            .reason
            .contains("qtest:<case>` or `test:")
    );
    assert_eq!(
        profile.meta_mut("header.assert.h").unwrap().status,
        Status::Partial
    );
    cleanup(&path);
}

#[test]
fn fake_test_path_is_unsupported() {
    let path = root("fake-test");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/does-not-exist.sh\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(
        outcome.unsupported[0].reason.contains("does not exist"),
        "{outcome:?}"
    );
    assert_eq!(
        profile.meta_mut("header.assert.h").unwrap().status,
        Status::Partial
    );
    cleanup(&path);
}

#[test]
fn missing_qtest_case_is_unsupported() {
    let path = root("missing-qtest");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
         evidence = \"qtest:no-such-case\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(
        outcome.unsupported[0].reason.contains("does not exist"),
        "{outcome:?}"
    );
    cleanup(&path);
}

#[test]
fn duplicate_ids_across_owner_files_are_rejected() {
    let path = root("duplicate");
    let row = "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
               evidence = \"test:tests/posix/check-headers.sh\"\n";
    write_test(&path, "tests/posix/check-headers.sh", "#!/bin/sh\nexit 0\n");
    write_owner(&path, "P1", row);
    write_owner(&path, "P2", row);
    let error = Overlay::load_at(&posix(&path)).unwrap_err().to_string();
    assert!(
        error.contains("duplicate overlay id `header.assert.h`"),
        "{error}"
    );
    cleanup(&path);
}

#[test]
fn waived_row_keeps_the_written_waiver() {
    let path = root("waived");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"waived\"\n\
         evidence = \"none\"\nnotes = \"OWNER WAIVER: P1 omits assert.h for 0.1.0\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.unsupported.is_empty(), "{outcome:?}");
    assert_eq!(outcome.claims.len(), 1);
    assert!(matches!(
        &outcome.claims[0],
        Claim::Waived { waiver, .. } if waiver.contains("OWNER WAIVER")
    ));
    assert_eq!(outcome.claims[0].command(), "waiver");
    let meta = profile.meta_mut("header.assert.h").unwrap();
    assert_eq!(meta.status, Status::Waived);
    assert_eq!(meta.waiver, "OWNER WAIVER: P1 omits assert.h for 0.1.0");
    assert!(meta.notes.is_empty());
    cleanup(&path);
}

#[test]
fn waived_without_a_waiver_is_unsupported() {
    let path = root("waived-empty");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"waived\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(
        outcome.unsupported[0]
            .reason
            .contains("waived without a written waiver")
    );
    cleanup(&path);
}

#[test]
fn unknown_id_is_unsupported() {
    let path = root("unknown");
    write_test(&path, "tests/posix/check-headers.sh", "#!/bin/sh\nexit 0\n");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"header.not-real.h\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/check-headers.sh\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(outcome.unsupported[0].reason.contains("no profile row"));
    cleanup(&path);
}

#[test]
fn overlay_file_must_match_the_row_owner() {
    let path = root("owner");
    write_test(&path, "tests/posix/check-headers.sh", "#!/bin/sh\nexit 0\n");
    write_owner(
        &path,
        "P2",
        "[[row]]\nid = \"header.assert.h\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/check-headers.sh\"\n",
    );
    let overlay = Overlay::load_at(&posix(&path)).unwrap();
    let mut profile = profile_with_assert_header();
    let outcome = overlay.apply(&path, &mut profile);
    assert!(outcome.claims.is_empty());
    assert_eq!(outcome.unsupported.len(), 1);
    assert!(outcome.unsupported[0].reason.contains("owner mismatch"));
    cleanup(&path);
}

#[test]
fn seed_includes_supported_overlay_rows() {
    let path = root("seed");
    std::fs::create_dir_all(path.join("libc/ravenlibc/abi/census")).unwrap();
    std::fs::create_dir_all(path.join("libc/ravenlibc/include")).unwrap();
    std::fs::create_dir_all(path.join("userland/bin")).unwrap();
    std::fs::write(path.join("libc/ravenlibc/abi/Symbols.list"), "foo\n").unwrap();
    std::fs::write(
        path.join("libc/ravenlibc/abi/census/symbols.toml"),
        "[[symbol]]\nname = \"foo\"\nring = 0\nkind = \"func\"\n\
         standard = [\"posix\"]\nheader = \"unistd.h\"\n",
    )
    .unwrap();
    std::fs::write(
        path.join("libc/ravenlibc/include/unistd.h"),
        "int foo(void);\n",
    )
    .unwrap();
    write_test(&path, "tests/posix/check-headers.sh", "#!/bin/sh\nexit 0\n");
    write_owner(
        &path,
        "P1",
        "[[row]]\nid = \"interface.foo\"\nstatus = \"present\"\n\
         evidence = \"test:tests/posix/check-headers.sh\"\n\
         notes = \"foo export is covered by the header probe\"\n",
    );
    crate::seed::seed_at(&path, &posix(&path)).unwrap();
    let mut profile = crate::model::load_profile(&posix(&path).join("profile.toml")).unwrap();
    let meta = profile
        .meta_mut("interface.foo")
        .expect("interface.foo row");
    assert_eq!(meta.status, Status::Present);
    assert_eq!(meta.evidence, "test:tests/posix/check-headers.sh");
    assert_eq!(meta.notes, "foo export is covered by the header probe");
    cleanup(&path);
}

#[test]
fn owner_batch_files_are_accepted() {
    let path = root("batch");
    std::fs::write(
        posix(&path).join("evidence/P4-text.toml"),
        "schema = 1\nstandard = \"posix-2024\"\n\n[[row]]\nid = \"util.sort\"\n\
         status = \"waived\"\nnotes = \"batch file smoke test\"\n",
    )
    .unwrap();
    assert!(Overlay::load_at(&posix(&path)).is_ok());
    cleanup(&path);
}
