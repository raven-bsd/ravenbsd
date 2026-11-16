// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host unit tests for the bounded m4 core.

use super::*;

#[derive(Default)]
struct Files {
    files: BTreeMap<String, String>,
}

impl FileProvider for Files {
    fn read_file(&mut self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
}

fn run_text(input: &str) -> String {
    let mut files = Files::default();
    run(input, &Options::default(), &mut files).expect("the script should expand")
}

#[test]
fn define_and_expand() {
    assert_eq!(run_text("define(`hello', `world')dnl\nhello\n"), "world\n");
    assert_eq!(
        run_text("define(`nested', `hello')dnl\nnested\n"),
        "hello\n"
    );
}

#[test]
fn arguments_and_substitutions() {
    assert_eq!(
        run_text("define(`pair', `$1:$2:$#:$*:$@')dnl\npair(`a',`b')\n"),
        "a:b:2:a,b:a,b\n"
    );
    assert_eq!(
        run_text("define(`show', `[$1][$2]')dnl\nshow(  a  ,  b )\n"),
        "[a  ][b ]\n"
    );
    assert_eq!(
        run_text("define(`first', `[$1][$2]')dnl\nfirst(`x')\n"),
        "[x][]\n"
    );
}

#[test]
fn pushdef_popdef_undefine_and_defn() {
    assert_eq!(
        run_text("define(`x',`one')pushdef(`x',`two')dnl\nx popdef(`x')x\n"),
        "two one\n"
    );
    assert_eq!(run_text("define(`a',`A')dnl\ndefn(`a')\n"), "A\n");
    assert_eq!(
        run_text("define(`a',`A')define(`b',defn(`a'))dnl\nb\n"),
        "A\n"
    );
    assert_eq!(
        run_text("define(`a',`A')undefine(`a')dnl\nifdef(`a',`yes',`no')\n"),
        "no\n"
    );
}

#[test]
fn dnl_and_comments() {
    assert_eq!(run_text("left dnl\nright\n"), "left right\n");
    assert_eq!(
        run_text("define(`x',`dnl')dnl\nx ignored\nnext\n"),
        "next\n"
    );
    assert_eq!(
        run_text("define(`x',`X')dnl\n# x is literal\ny x\n"),
        "# x is literal\ny X\n"
    );
}

#[test]
fn quoting_defers_expansion() {
    assert_eq!(run_text("define(`x',`X')dnl\n`x'\n"), "x\n");
    assert_eq!(run_text("`hello world'\n"), "hello world\n");
    assert_eq!(run_text("define(`q',`Q')dnl\n`a `q' b'\n"), "a `q' b\n");
    assert_eq!(
        run("`unterminated", &Options::default(), &mut Files::default()),
        Err(M4Error::Usage)
    );
}

#[test]
fn changequote_and_changecom() {
    assert_eq!(
        run_text("changequote(`[',`]')dnl\ndefine([y],[Y])dnl\ny\n"),
        "Y\n"
    );
    assert_eq!(
        run_text("changequote(`[',`]')dnl\ndefine([y],[Y])dnl\nchangequote\ny\n"),
        "\nY\n"
    );
    assert_eq!(
        run_text("changecom(`/*',`*/')dnl\ndefine(`x',`X')dnl\n/* x */x\n"),
        "/* x */X\n"
    );
    assert_eq!(
        run_text("define(`x',`X')dnl\nchangecom()dnl\n# x\n"),
        "# X\n"
    );
    assert_eq!(
        run_text("define(`x',`X')dnl\nchangecom\ndnl\n# x\n"),
        "\n# X\n"
    );
}

#[test]
fn conditionals() {
    assert_eq!(
        run_text("define(`a',`A')dnl\nifdef(`a',`yes',`no') ifdef(`b',`yes',`no')\n"),
        "yes no\n"
    );
    assert_eq!(
        run_text(
            "ifelse(`a',`a',`same',`diff'):ifelse(`a',`b',`same',`diff'):ifelse(`a',`b',`same')\n"
        ),
        "same:diff:\n"
    );
    assert_eq!(
        run_text("ifelse(`a',`b',`one',`c',`c',`two',`three')\n"),
        "two\n"
    );
    assert_eq!(run_text("ifdef(`x',`define(`q',`Q')',`')dnl\nq\n"), "q\n");
    assert_eq!(
        run_text("ifdef(`define',`define(`q',`Q')',`')dnl\nq\n"),
        "Q\n"
    );
}

#[test]
fn arithmetic() {
    assert_eq!(
        run_text("incr(41) decr(41) eval(2+3*4) eval((2+3)*4) eval(2**10) eval(17%5)\n"),
        "42 40 14 20 1024 2\n"
    );
    assert_eq!(
        run_text("eval(2*3**2) eval(0x10) eval(0b101)\n"),
        "18 16 5\n"
    );
    assert_eq!(
        run_text("eval(3<5) eval(3==4) eval(1&&0) eval(1||0)\n"),
        "1 0 0 1\n"
    );
    assert_eq!(
        run("eval(1/0)", &Options::default(), &mut Files::default()),
        Err(M4Error::Usage)
    );
    assert_eq!(
        run("eval(1+)", &Options::default(), &mut Files::default()),
        Err(M4Error::Usage)
    );
}

#[test]
fn eval_parentheses() {
    assert_eq!(evaluate("(2)"), Ok(2));
    assert_eq!(evaluate("(2+3)*4"), Ok(20));
    assert_eq!(run_text("eval((2+3)*4)\n"), "20\n");
}

#[test]
fn string_builtins() {
    assert_eq!(
        run_text("substr(`abcdef',2,3) substr(`abcdef',2) substr(`abcdef',0,2)\n"),
        "cde cdef ab\n"
    );
    assert_eq!(
        run_text("index(`abcabc',`ca') index(`abc',`zz') index(`abc',`')\n"),
        "2 -1 0\n"
    );
    assert_eq!(
        run_text("translit(`abcabc',`abc',`xy') translit(`abc',`abc',)\n"),
        "xyxy \n"
    );
    assert_eq!(run_text("substr(`h\u{e9}llo',2,3)\n"), "llo\n");
}

#[test]
fn diversions() {
    assert_eq!(
        run_text("a divert(1)b divert(2)c divert(0)d undivert(1)e undivert f\n"),
        "a d b e c  f\n"
    );
    assert_eq!(
        run_text("divert(2)two divert(1)one divert(0)undivert\n"),
        "one two \n"
    );
    assert_eq!(
        run_text("divert(1)hidden divert(0)undivert()undivert(1)\n"),
        "hidden \n"
    );
    assert_eq!(
        run_text("divert(1)one divert(2)two divert(0)undivert(2,1)\n"),
        "two one \n"
    );
    assert_eq!(run_text("a divert(-1)hidden divert(0)b\n"), "a b\n");
    assert_eq!(run_text("divert(1)one divert(0)zero\n"), "zero\none ");
}

#[test]
fn include_uses_search_path() {
    let mut files = Files::default();
    files.files.insert(
        "inc.m4".to_string(),
        "define(`inc',`INCLUDED')inc\n".to_string(),
    );
    let output = run("include(`inc.m4')dnl\n", &Options::default(), &mut files)
        .expect("the include should expand");
    assert_eq!(output, "INCLUDED\n");

    let mut files = Files::default();
    files
        .files
        .insert("/inc/lib.m4".to_string(), "LIB".to_string());
    let options = Options {
        actions: Vec::new(),
        include_dirs: alloc::vec!["/inc".to_string()],
    };
    let output =
        run("include(`lib.m4')\n", &options, &mut files).expect("the include should expand");
    assert_eq!(output, "LIB\n");
}

#[test]
fn missing_include_is_an_error() {
    let mut files = Files::default();
    assert_eq!(
        run("include(`missing.m4')", &Options::default(), &mut files),
        Err(M4Error::Include("missing.m4".to_string()))
    );
}

#[test]
fn unsupported_shell_builtins_are_explicit() {
    let mut files = Files::default();
    assert_eq!(
        run("syscmd(`true')", &Options::default(), &mut files),
        Err(M4Error::Unsupported("syscmd"))
    );
    assert_eq!(
        run("esyscmd(`true')", &Options::default(), &mut files),
        Err(M4Error::Unsupported("esyscmd"))
    );
}

#[test]
fn recursion_is_bounded() {
    let mut files = Files::default();
    assert_eq!(
        run("define(`loop',`loop')loop", &Options::default(), &mut files),
        Err(M4Error::Recursion)
    );
}

#[test]
fn command_line_definitions_and_undefinitions() {
    let invocation = parse_args([
        "-Dfoo=bar",
        "-Ubaz",
        "-I",
        "inc",
        "-Dqux",
        "--",
        "-file",
        "plain",
    ])
    .expect("the command line should parse");
    assert_eq!(
        invocation.options.actions,
        alloc::vec![
            InitAction::Define("foo".to_string(), "bar".to_string()),
            InitAction::Undefine("baz".to_string()),
            InitAction::Define("qux".to_string(), String::new()),
        ]
    );
    assert_eq!(
        invocation.options.include_dirs,
        alloc::vec!["inc".to_string()]
    );
    assert_eq!(
        invocation.operands,
        alloc::vec!["-file".to_string(), "plain".to_string()]
    );

    assert_eq!(parse_args(["-D"]), Err(M4Error::Usage));
    assert_eq!(parse_args(["-Q"]), Err(M4Error::Usage));
    assert_eq!(parse_args(["-D1bad=x"]), Err(M4Error::Usage));
}

#[test]
fn options_apply_in_order() {
    let options = Options {
        actions: alloc::vec![
            InitAction::Define("x".to_string(), "X".to_string()),
            InitAction::Undefine("x".to_string()),
        ],
        include_dirs: Vec::new(),
    };
    let mut files = Files::default();
    assert_eq!(run("x", &options, &mut files), Ok("x".to_string()));

    let options = Options {
        actions: alloc::vec![
            InitAction::Undefine("x".to_string()),
            InitAction::Define("x".to_string(), "X".to_string()),
        ],
        include_dirs: Vec::new(),
    };
    assert_eq!(run("x", &options, &mut files), Ok("X".to_string()));
}

#[test]
fn call_without_closing_parenthesis_is_an_error() {
    let mut files = Files::default();
    assert_eq!(
        run("define(`x',`X')x(", &Options::default(), &mut files),
        Err(M4Error::Usage)
    );
}
