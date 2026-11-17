// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the lex specification parser and pattern translator.

use super::*;

fn sample() -> &'static str {
    "%{
#include \"y.tab.h\"
%}
DIGIT [0-9]
%x COMMENT
%s WORD
%%
[ \\t\\n]+      ;
\"/*\"          BEGIN(COMMENT);
<COMMENT>\"*/\" BEGIN(INITIAL);
<COMMENT>.      ;
{DIGIT}+        { yylval = atoi(yytext); return NUMBER; }
\"+\"            { return '+'; }
[[:alpha:]_][[:alnum:]_]* { return ID; }
%%
int main(void) { return yylex(); }
"
}

#[test]
fn a_spec_is_split_into_three_sections() {
    let spec = Spec::parse(sample()).expect("parses");
    assert!(spec.prologue.contains("y.tab.h"));
    assert!(spec.epilogue.contains("int main"));
    assert_eq!(
        spec.definitions,
        vec![("DIGIT".to_string(), "[0-9]".to_string())]
    );
    assert_eq!(spec.conditions.len(), 2);
    assert!(spec.conditions[0].exclusive);
    assert!(!spec.conditions[1].exclusive);
    assert_eq!(spec.rules.len(), 7);
}

#[test]
fn a_rule_captures_conditions_and_action() {
    let spec = Spec::parse(sample()).expect("parses");
    let rule = &spec.rules[4];
    assert_eq!(rule.pattern, "{DIGIT}+");
    assert_eq!(rule.action, "{ yylval = atoi(yytext); return NUMBER; }");
    assert_eq!(spec.rules[2].conditions, vec!["COMMENT".to_string()]);
}

#[test]
fn a_definition_expands() {
    let spec = Spec::parse(sample()).expect("parses");
    let pattern = translate_pattern(&spec.rules[4].pattern, &spec.definitions).expect("translates");
    assert_eq!(pattern.ere, "([0-9])+");
}

#[test]
fn literals_and_classes_translate() {
    let pattern = translate_pattern("[[:alpha:]_][[:alnum:]_]*", &[]).expect("translates");
    assert_eq!(pattern.ere, "[[:alpha:]_][[:alnum:]_]*");
    let literal = translate_pattern("\"+\"", &[]).expect("translates");
    assert_eq!(literal.ere, "\\+");
}

#[test]
fn anchors_are_separate_flags() {
    let pattern = translate_pattern("^foo$", &[]).expect("translates");
    assert!(pattern.begin);
    assert!(pattern.end);
    assert_eq!(pattern.ere, "foo");
}

#[test]
fn repetitions_are_preserved() {
    let pattern = translate_pattern("[0-9]{2,3}", &[]).expect("translates");
    assert_eq!(pattern.ere, "[0-9]{2,3}");
}

#[test]
fn escapes_become_literals() {
    let pattern = translate_pattern("\\+\\-", &[]).expect("translates");
    assert_eq!(pattern.ere, "\\+-");
}

#[test]
fn quoted_strings_keep_spaces() {
    let pattern = translate_pattern("\"a b\"", &[]).expect("translates");
    assert_eq!(pattern.ere, "a b");
}

#[test]
fn compile_emits_a_regex_scanner() {
    let spec = Spec::parse(sample()).expect("parses");
    let c = compile_spec(&spec).expect("compiles");
    assert!(c.contains("#include <regex.h>"));
    assert!(c.contains("regcomp(&yy_regex[index], yy_pattern[index], REG_EXTENDED | REG_NEWLINE)"));
    assert!(c.contains("REG_NOTBOL"));
    assert!(c.contains("BEGIN(s) (yy_start = (s))"));
    assert!(c.contains("yylval = atoi(yytext); return NUMBER;"));
    assert!(c.contains("int yywrap(void) { return 1; }"));
}

#[test]
fn a_missing_separator_is_an_error() {
    assert!(Spec::parse("rule").is_err());
}

#[test]
fn an_undefined_definition_is_an_error() {
    let error = translate_pattern("{NOPE}", &[]).unwrap_err();
    assert!(error.message.contains("undefined"));
}

#[test]
fn trailing_context_is_rejected() {
    let error = translate_pattern("a/b", &[]).unwrap_err();
    assert!(error.message.contains("trailing context"));
}

#[test]
fn an_unterminated_class_is_an_error() {
    let error = translate_pattern("[abc", &[]).unwrap_err();
    assert!(error.message.contains("character class"));
}

#[test]
fn an_unknown_condition_is_rejected() {
    let spec = Spec::parse("%%\n<WHAT>x ;\n%%\n").unwrap_err();
    assert!(spec.message.contains("unknown") || spec.message.contains("condition"));
}
