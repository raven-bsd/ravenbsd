// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the bounded awk core.

use super::{AwkError, Options, run};

fn run_with(script: &str, input: &str) -> String {
    run(script, input, &Options::default()).unwrap()
}

#[test]
fn fields_records_and_begin_end() {
    let output = run_with(
        "BEGIN { FS=\":\" } { print NR, NF, $1 } END { print \"done\" }",
        "a:b\nc:d\n",
    );
    assert_eq!(output, "1 2 a\n2 2 c\ndone\n");
}

#[test]
fn variables_arithmetic_comparison_and_concat() {
    let output = run_with(
        "BEGIN { x=2; y=3; print x+y*2, x<y, x==y, \"a\" \"b\" }",
        "",
    );
    assert_eq!(output, "8 1 0 ab\n");
}

#[test]
fn if_while_for_and_arrays() {
    let output = run_with(
        "BEGIN { i=0; while (i<2) { print i; i++ } for (j=0; j<2; j++) print j; a[\"k\"]=7; for (k in a) print k, a[k] }",
        "",
    );
    assert_eq!(output, "0\n1\n0\n1\nk 7\n");
}

#[test]
fn builtins_and_printf() {
    let output = run_with(
        "BEGIN { print length(\"abc\"), substr(\"abcdef\",2,3), index(\"abc\",\"b\"), int(2.9); printf \"%s:%d\\n\", \"x\", 4; print sprintf(\"%s-%d\", \"y\", 5) }",
        "",
    );
    assert_eq!(output, "3 bcd 2 2\nx:4\ny-5\n");
}

#[test]
fn split_sub_gsub_and_match() {
    let output = run_with(
        "BEGIN { n=split(\"a:b:c\", p, \":\"); print n, p[1], p[3] } { sub(/a/, \"x\"); gsub(/b/, \"y\"); print; print match(\"abc\", /b/), RSTART, RLENGTH }",
        "ab\n",
    );
    assert_eq!(output, "3 a c\nxy\n2 2 1\n");
}

#[test]
fn pattern_action_and_next() {
    let output = run_with(
        "$1 == \"a\" { print $2; next } { print \"other\" }",
        "a one\nb two\n",
    );
    assert_eq!(output, "one\nother\n");
}

#[test]
fn custom_fs_and_options() {
    let options = Options {
        fs: Some(":".to_string()),
        vars: vec![("name".to_string(), "bo".to_string())],
    };
    let output = run("{ print $2, name }", "x:y\n", &options).unwrap();
    assert_eq!(output, "y bo\n");
}

#[test]
fn unsupported_constructs_are_errors() {
    assert_eq!(
        run("function f() { return 1 }", "", &Options::default()),
        Err(AwkError::Usage)
    );
    assert_eq!(
        run("{ getline }", "x\n", &Options::default()),
        Err(AwkError::Usage)
    );
    assert_eq!(
        run("{ close(\"x\") }", "x\n", &Options::default()),
        Err(AwkError::Usage)
    );
}
