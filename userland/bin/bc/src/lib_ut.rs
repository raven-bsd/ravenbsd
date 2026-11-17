// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host unit tests for the bounded bc parser and runtime.

use super::*;

fn run_text(input: &str) -> String {
    run(input, &Options::default()).expect("the program should run")
}

#[test]
fn arithmetic_and_scale() {
    assert_eq!(run_text("1/3\n"), "0\n");
    assert_eq!(run_text("scale=5\n1/3\n"), ".33333\n");
    assert_eq!(run_text("1.20*2.0\n"), "2.40\n");
    assert_eq!(run_text("scale=5\nsqrt(2)\n"), "1.41421\n");
    assert_eq!(run_text("2^10\n"), "1024\n");
    assert_eq!(run_text("scale=2.9\nscale\n"), "2\n");
}

#[test]
fn variables_arrays_and_assignments() {
    assert_eq!(run_text("x=3\ny=4\nx*y\n"), "12\n");
    assert_eq!(run_text("a[1]=7\na[1]\na[2]\n"), "7\n0\n");
    assert_eq!(run_text("x=3\nx\n"), "3\n");
    assert_eq!(run_text("x=3\nx+=4\nx\n"), "7\n");
}

#[test]
fn conditionals_and_loops() {
    assert_eq!(run_text("if (1) 2 else 3\n"), "2\n");
    assert_eq!(run_text("if (0) 2 else 3\n"), "3\n");
    assert_eq!(
        run_text("i=0\ns=0\nwhile (i<4) { i=i+1; s=s+i }\ns\n"),
        "10\n"
    );
    assert_eq!(run_text("s=0\nfor (i=0; i<4; i=i+1) s=s+i\ns\n"), "6\n");
}

#[test]
fn logical_and_loop_edges() {
    assert_eq!(run_text("1&&0\n1||0\n!0\n"), "0\n1\n1\n");
    assert_eq!(
        run_text("s=0\nfor (i=0; i<6; i=i+1) { if (i==3) break; s=s+i }\ns\n"),
        "3\n"
    );
    assert_eq!(
        run_text("s=0\nfor (i=0; i<5; i=i+1) { if (i==2) continue; s=s+i }\ns\n"),
        "8\n"
    );
    assert_eq!(run_text("scale=-1\nscale\n"), "0\n");
}

#[test]
fn functions_and_autos() {
    assert_eq!(run_text("define f(x) { return (x*x) }\nf(5)\n"), "25\n");
    assert_eq!(
        run_text("define f(n) { if (n <= 1) return (1); return (n*f(n-1)) }\nf(5)\n"),
        "120\n"
    );
    assert_eq!(
        run_text("define f(x) { auto y; y=x+1; return (y) }\nf(4)\n"),
        "5\n"
    );
}

#[test]
fn print_strings_and_comments() {
    assert_eq!(run_text("print \"x=\", 3, \"\\n\"\n"), "x=3\n");
    assert_eq!(run_text("\"hi\"\n3\n"), "hi3\n");
    assert_eq!(run_text("/* comment */ 1+1\n# line\n2+2\n"), "2\n4\n");
}

#[test]
fn math_library() {
    let options = Options { math_lib: true };
    assert_eq!(run("scale\n", &options).unwrap(), "20\n");
    assert_eq!(run("e(1)\n", &options).unwrap(), "2.71828182845904523536\n");
    assert_eq!(run("l(2)\n", &options).unwrap(), ".69314718055994530941\n");
    assert_eq!(run("s(1)\n", &options).unwrap(), ".84147098480789650665\n");
    assert_eq!(run("c(1)\n", &options).unwrap(), ".54030230586813971740\n");
    assert_eq!(run("a(1)\n", &options).unwrap(), ".78539816339744830961\n");
    assert_eq!(
        run("e(1)\n", &Options::default()),
        Err(BcError::Unsupported(
            "math function `e` requires -l".to_string()
        ))
    );
}

#[test]
fn sessions_keep_state() {
    let mut session = Session::new(&Options::default());
    session.process("x=4\n").unwrap();
    session.process("x+1\n").unwrap();
    assert_eq!(session.finish(), "5\n");
}

#[test]
fn quit_stops_processing() {
    assert_eq!(run_text("quit\n1\n"), "");
}

#[test]
fn errors_are_reported() {
    assert!(matches!(
        run("1/0\n", &Options::default()),
        Err(BcError::Runtime(_))
    ));
    assert!(matches!(
        run("sqrt(-1)\n", &Options::default()),
        Err(BcError::Runtime(_))
    ));
    assert!(matches!(
        run("missing(1)\n", &Options::default()),
        Err(BcError::Runtime(_))
    ));
    assert!(matches!(
        run("obase=16\n", &Options::default()),
        Err(BcError::Unsupported(_))
    ));
    assert!(matches!(
        run("if (1)\n", &Options::default()),
        Err(BcError::Parse(_))
    ));
}
