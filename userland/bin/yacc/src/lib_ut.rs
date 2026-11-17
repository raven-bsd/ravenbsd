// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the yacc grammar parser, LALR tables and C emitter.

use super::*;

fn calculator() -> &'static str {
    "%{
#include <stdio.h>
void yyerror(const char *);
%}
%token NUMBER
%left '+' '-'
%left '*' '/'
%right UMINUS
%start input
%%
input: | input line ;
line: expr '\\n' { printf(\"%d\\n\", $1); } | error '\\n' { yyerrok; } ;
expr: NUMBER
    | expr '+' expr { $$ = $1 + $3; }
    | expr '-' expr { $$ = $1 - $3; }
    | expr '*' expr { $$ = $1 * $3; }
    | expr '/' expr { $$ = $1 / $3; }
    | '-' expr %prec UMINUS { $$ = -$2; }
    | '(' expr ')' { $$ = $2; }
    ;
%%
void yyerror(const char *message) { fprintf(stderr, \"%s\\n\", message); }
int main(void) { return yyparse(); }
"
}

#[test]
fn a_calculator_grammar_parses() {
    let grammar = Grammar::parse(calculator()).expect("parses");
    assert_eq!(grammar.start.as_deref(), Some("input"));
    assert!(grammar.tokens.iter().any(|token| token.name == "NUMBER"));
    assert!(grammar.tokens.iter().any(|token| token.name == "UMINUS"));
    assert_eq!(grammar.rules.len(), 11);
    assert_eq!(grammar.rules[0].lhs, "input");
    assert!(grammar.rules[0].rhs.is_empty());
}

#[test]
fn precedence_and_start_are_recorded() {
    let grammar = Grammar::parse(calculator()).expect("parses");
    let number = grammar
        .tokens
        .iter()
        .find(|token| token.name == "NUMBER")
        .unwrap();
    assert!(number.precedence.is_none());
    let plus = grammar
        .tokens
        .iter()
        .find(|token| token.name == "'+'")
        .unwrap();
    assert!(plus.precedence.is_some());
    assert_eq!(plus.assoc, Some(Assoc::Left));
}

#[test]
fn the_calculator_has_lalr_tables_without_conflicts() {
    let grammar = Grammar::parse(calculator()).expect("parses");
    let tables = tables(&grammar).expect("tables");
    assert_eq!(tables.shift_reduce, 0);
    assert_eq!(tables.reduce_reduce, 0);
    assert!(tables.action.len() > 10);
    assert!(tables.max_token >= 258);
}

#[test]
fn action_values_are_translated() {
    let action = translate_action("{ $$ = $1 + $3; }", 3, 1).expect("translates");
    assert!(action.contains("yyval = yyvs[yyssp - 3 + 1] + yyvs[yyssp - 3 + 3]"));
    let unary = translate_action("{ $$ = -$2; }", 2, 1).expect("translates");
    assert!(unary.contains("yyval = -yyvs[yyssp - 2 + 2]"));
}

#[test]
fn field_references_are_rejected() {
    let error = translate_action("{ $$ = $<num>1; }", 1, 1).unwrap_err();
    assert!(error.message.contains("field"));
}

#[test]
fn the_emitted_parser_has_the_expected_core() {
    let grammar = Grammar::parse(calculator()).expect("parses");
    let c = compile_grammar(&grammar).expect("compiles");
    assert!(c.contains("int yyparse(void)"));
    assert!(c.contains("yy_action"));
    assert!(c.contains("YYMAXTOKEN"));
    assert!(c.contains("case 1:"));
    assert!(c.contains("yyerrorlab:"));
    assert!(c.contains("yyval = yyvs[yyssp - 3 + 1] + yyvs[yyssp - 3 + 3]"));
}

#[test]
fn the_header_defines_named_tokens() {
    let grammar = Grammar::parse(calculator()).expect("parses");
    let header = header(&grammar).expect("header");
    assert!(header.contains("#define NUMBER 258"));
    assert!(header.contains("extern YYSTYPE yylval"));
}

#[test]
fn union_is_rejected() {
    let error = Grammar::parse("%union { int value; }\n%%\ns: ;\n%%\n").unwrap_err();
    assert!(error.message.contains("unsupported"));
}

#[test]
fn a_mid_rule_action_is_rejected() {
    let text = "%%\ns: { $$ = 1; } 'a' ;\n%%\n";
    let error = Grammar::parse(text).unwrap_err();
    assert!(error.message.contains("mid-rule"));
}

#[test]
fn a_rule_can_have_an_action_before_the_next_alternative() {
    let text = "%%\ns: 'a' { $$ = 1; } | 'b' { $$ = 2; } ;\n%%\n";
    let grammar = Grammar::parse(text).expect("parses");
    assert_eq!(grammar.rules.len(), 2);
}

#[test]
fn a_missing_separator_is_an_error() {
    assert!(Grammar::parse("s: 'a' ;").is_err());
}
