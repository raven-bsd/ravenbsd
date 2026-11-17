// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the shared C analyzer.

use alloc::string::ToString;
use alloc::vec::Vec;

use super::*;

const FIXTURE: &str = "\
/* P41 devtools C fixture. */
#define LIMIT 10

struct point {
    int x;
    int y;
};

enum color { RED, GREEN };

int global_count = 0;

static int helper(int value) {
    return value + LIMIT;
}

int add(int left, int right) {
    return left + right;
}

int main(void) {
    int total = add(1, 2);
    total += helper(total);
    global_count = total;
    return total;
}
";

#[test]
fn finds_macros_structs_enums_globals_and_functions() {
    let analysis = analyze(FIXTURE);
    let found = analysis
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.line))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        vec![
            ("LIMIT", SymbolKind::Macro, 2),
            ("point", SymbolKind::Struct, 4),
            ("color", SymbolKind::Enum, 9),
            ("global_count", SymbolKind::Global, 11),
            ("helper", SymbolKind::Function, 13),
            ("add", SymbolKind::Function, 17),
            ("main", SymbolKind::Function, 21),
        ]
    );
}

#[test]
fn finds_direct_function_calls() {
    let analysis = analyze(FIXTURE);
    let calls = analysis
        .functions
        .iter()
        .map(|function| (function.name.as_str(), function.calls.clone()))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].0, "helper");
    assert!(calls[0].1.is_empty());
    assert_eq!(calls[1].0, "add");
    assert!(calls[1].1.is_empty());
    assert_eq!(calls[2].0, "main");
    assert_eq!(calls[2].1, vec!["add".to_string(), "helper".to_string()]);
}

#[test]
fn records_references_with_lines() {
    let analysis = analyze(FIXTURE);
    let limit = analysis
        .references
        .iter()
        .filter(|reference| reference.name == "LIMIT")
        .map(|reference| reference.line)
        .collect::<Vec<_>>();
    assert_eq!(limit, vec![14]);
    let global = analysis
        .references
        .iter()
        .filter(|reference| reference.name == "global_count")
        .map(|reference| reference.line)
        .collect::<Vec<_>>();
    assert_eq!(global, vec![11, 24]);
}

#[test]
fn prototypes_are_not_function_definitions() {
    let analysis =
        analyze("int add(int a, int b);\nint add(int a, int b) {\n    return a + b;\n}\n");
    let functions = analysis
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(functions, vec!["add"]);
    let globals = analysis
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Global)
        .count();
    assert_eq!(globals, 0);
}

#[test]
fn typedef_bodies_do_not_create_globals() {
    let analysis = analyze(
        "typedef struct {\n    int x;\n} point;\nint value;\nint get(void) { return value; }\n",
    );
    let symbols = analysis
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect::<Vec<_>>();
    assert_eq!(
        symbols,
        vec![("value", SymbolKind::Global), ("get", SymbolKind::Function),]
    );
}

#[test]
fn comments_and_strings_are_not_tokens() {
    let analysis = analyze(
        "/* struct fake { int field; }; */\n#define REAL 1\nint value = 0;\nchar *text = \"int nope;\";\n",
    );
    let names = analysis
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["REAL", "value", "text"]);
}

#[test]
fn function_pointer_globals_are_skipped() {
    let analysis = analyze("int (*handler)(int);\nint value;\n");
    let names = analysis
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["value"]);
}
