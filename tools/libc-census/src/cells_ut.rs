// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the donor cell scan.

use std::fs;
use std::path::Path;

use super::*;
use crate::model::Kind;
use crate::testutil::TempDir;

fn universe(names: &[(&str, Kind)]) -> Universe {
    Universe::build(
        names
            .iter()
            .map(|(name, kind)| ((*name).to_string(), *kind)),
    )
}

fn scan_with(resources: &Path, names: &[(&str, Kind)], prefilter: bool) -> DonorScan {
    let options = ScanOptions {
        prefilter,
        jobs: 1,
        donors: vec!["glibc".to_string()],
    };
    scan_donors(resources, &universe(names), &options)
        .unwrap()
        .remove("glibc")
        .unwrap()
}

#[test]
fn definition_form_matches_and_rejects() {
    assert!(form_matches(Form::Definition, "int foo(void)", "foo"));
    assert!(form_matches(
        Form::Definition,
        "static int error(void)",
        "error"
    ));
    assert!(form_matches(
        Form::Definition,
        "extern int foo (int);",
        "foo"
    ));
    assert!(!form_matches(Form::Definition, "int foo;", "foo"));
    assert!(!form_matches(Form::Definition, "int foobar(void)", "foo"));
    assert!(!form_matches(
        Form::Definition,
        "  argp_parse (0);",
        "argp_parse"
    ));
    assert!(form_matches(
        Form::Definition,
        "int dprintf(int fd) __printflike(2, 3);",
        "dprintf"
    ));
}

#[test]
fn define_form_matches_and_rejects() {
    assert!(form_matches(Form::Define, "#define foo 1", "foo"));
    assert!(form_matches(Form::Define, "#  define foo(x) (x)", "foo"));
    assert!(!form_matches(Form::Define, "#define foobar 1", "foo"));
    assert!(!form_matches(Form::Define, "int foo;", "foo"));
}

#[test]
fn variable_form_matches_and_rejects() {
    assert!(form_matches(Form::Variable, "char **environ;", "environ"));
    assert!(form_matches(Form::Variable, "int errno = 0;", "errno"));
    assert!(form_matches(Form::Variable, "char *tzname[2];", "tzname"));
    assert!(!form_matches(Form::Variable, "environ;", "environ"));
}

#[test]
fn ssp_real_and_alias_forms_match() {
    assert!(form_matches(
        Form::SspReal,
        "return __ssp_real(strlcpy) (a);",
        "strlcpy"
    ));
    assert!(!form_matches(
        Form::SspReal,
        "__ssp_real(other) (a);",
        "strlcpy"
    ));
    assert!(form_matches(
        Form::Alias,
        "weak_alias (__real_name, public_name)",
        "public_name"
    ));
    assert!(!form_matches(
        Form::Alias,
        "weak_alias (__real_name, public_name)",
        "missing_name"
    ));
}

#[test]
fn source_files_count_as_d_and_headers_as_h() {
    let temp = TempDir::new("cells-dh");
    temp.write("glibc/foo.c", "int foo(void) { return 0; }\n");
    temp.write("glibc/foo.h", "extern int foo(void);\n");
    temp.write("glibc/foo.hpp", "extern int foo(void);\n");
    temp.write("glibc/foo.S", "int foo(void) { return 0; }\n");
    let scan = scan_with(temp.path(), &[("foo", Kind::Func)], true);
    assert_eq!(scan.cells["foo"], Cell { d: 2, h: 2 });
    assert_eq!(scan.files, 4);
}

#[test]
fn test_paths_and_hidden_directories_are_skipped() {
    let temp = TempDir::new("cells-exclude");
    temp.write("glibc/tests/foo.c", "int foo(void) {}\n");
    temp.write("glibc/contest/foo.c", "int foo(void) {}\n");
    temp.write("glibc/regress/foo.c", "int foo(void) {}\n");
    temp.write("glibc/testsuite/foo.c", "int foo(void) {}\n");
    temp.write("glibc/.hidden/foo.c", "int foo(void) {}\n");
    temp.write("glibc/real/foo.c", "int foo(void) {}\n");
    let scan = scan_with(temp.path(), &[("foo", Kind::Func)], true);
    assert_eq!(scan.cells["foo"], Cell { d: 1, h: 0 });
    assert_eq!(scan.files, 1);
}

#[test]
fn binary_files_are_skipped_and_counted() {
    let temp = TempDir::new("cells-binary");
    temp.write_bytes("glibc/binary.c", b"int foo(void) {}\0\n");
    temp.write("glibc/text.c", "int foo(void) {}\n");
    let scan = scan_with(temp.path(), &[("foo", Kind::Func)], true);
    assert_eq!(scan.cells["foo"], Cell { d: 1, h: 0 });
    assert_eq!(scan.skipped_binary, 1);
    assert_eq!(scan.skipped_large, 0);
}

#[test]
fn files_over_sixteen_mib_are_skipped_and_counted() {
    let temp = TempDir::new("cells-large");
    temp.write("glibc/small.c", "int foo(void) {}\n");
    let path = temp.path().join("glibc/large.c");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    let scan = scan_with(temp.path(), &[("foo", Kind::Func)], true);
    assert_eq!(scan.cells["foo"], Cell { d: 1, h: 0 });
    assert_eq!(scan.skipped_large, 1);
}

#[test]
fn prefilter_and_no_prefilter_are_byte_identical() {
    let temp = TempDir::new("cells-prefilter");
    temp.write(
        "glibc/one.c",
        "int alpha(void) {}\n#define beta(x) (x)\nweak_alias (__real, gamma)\n",
    );
    temp.write("glibc/two.h", "extern int alpha(void);\nchar **delta;\n");
    temp.write("glibc/three.c", "static int omega(void) { return 0; }\n");
    let names = [
        ("alpha", Kind::Func),
        ("beta", Kind::Func),
        ("gamma", Kind::Func),
        ("delta", Kind::Data),
        ("omega", Kind::Func),
        ("not_here", Kind::Func),
    ];
    let fast = scan_with(temp.path(), &names, true);
    let slow = scan_with(temp.path(), &names, false);
    assert_eq!(fast.cells, slow.cells);
    assert_eq!(fast.files, slow.files);
    assert_eq!(fast.cells["omega"], Cell { d: 1, h: 0 });
}

#[test]
fn double_underscore_definition_is_found_through_the_prefilter() {
    let temp = TempDir::new("cells-underscore");
    temp.write("glibc/one.c", "void __strlcpy(char *dst) {}\n");
    let scan = scan_with(temp.path(), &[("strlcpy", Kind::Func)], true);
    assert_eq!(scan.cells["strlcpy"], Cell { d: 1, h: 0 });
    let scan = scan_with(temp.path(), &[("__strlcpy", Kind::Func)], true);
    assert_eq!(scan.cells["__strlcpy"], Cell { d: 1, h: 0 });
}

#[test]
fn a_data_name_uses_the_variable_regex() {
    let temp = TempDir::new("cells-data");
    temp.write("glibc/data.c", "char **environ;\nint environ_fn(void) {}\n");
    let scan = scan_with(temp.path(), &[("environ", Kind::Data)], true);
    assert_eq!(scan.cells["environ"], Cell { d: 1, h: 0 });
}

#[test]
fn a_name_that_only_appears_inside_words_does_not_match() {
    let temp = TempDir::new("cells-word");
    temp.write(
        "glibc/one.c",
        "int strlcpy_like(void) {}\n/* strlcpy */\nint other = strlcpy_length;\n",
    );
    let scan = scan_with(temp.path(), &[("strlcpy", Kind::Func)], true);
    assert!(scan.cells.is_empty());
}

#[test]
fn missing_roots_are_reported_and_do_not_fail() {
    let temp = TempDir::new("cells-roots");
    let options = ScanOptions {
        prefilter: true,
        jobs: 1,
        donors: vec!["freebsd".to_string()],
    };
    let scans = scan_donors(temp.path(), &universe(&[("foo", Kind::Func)]), &options).unwrap();
    let scan = &scans["freebsd"];
    assert_eq!(scan.missing_roots.len(), donor_roots("freebsd").len());
    assert!(scan.cells.is_empty());
}

#[test]
fn cell_rows_compute_n6_families_and_libcs() {
    let temp = TempDir::new("cells-rows");
    temp.write("glibc/a.c", "int foo(void) {}\n");
    temp.write("musl/src/b.c", "int foo(void) {}\n");
    temp.write("mlibc/options/c.cpp", "int foo(void) {}\n");
    let names = [("foo", Kind::Func)];
    let options = ScanOptions {
        prefilter: true,
        jobs: 2,
        donors: vec!["glibc".to_string(), "musl".to_string(), "mlibc".to_string()],
    };
    let scans = scan_donors(temp.path(), &universe(&names), &options).unwrap();
    let rows = cell_rows(&universe(&names), &scans, &options.donors);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].n6, 2);
    assert_eq!(rows[0].libcs, vec!["glibc", "mlibc", "musl"]);
    assert_eq!(rows[0].families, vec!["linux"]);
    assert_eq!(rows[0].cells["glibc"].d, 1);
    assert_eq!(rows[0].cells["musl"].d, 1);
}
