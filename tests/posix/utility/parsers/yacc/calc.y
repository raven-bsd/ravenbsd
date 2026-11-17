/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * P42 yacc fixture: precedence, associativity, unary minus and error recovery.
 */
%{
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
line: expr '\n' { printf("%d\n", $1); }
    | error '\n' { yyerrok; }
    ;
expr: NUMBER
    | expr '+' expr { $$ = $1 + $3; }
    | expr '-' expr { $$ = $1 - $3; }
    | expr '*' expr { $$ = $1 * $3; }
    | expr '/' expr { $$ = $1 / $3; }
    | '-' expr %prec UMINUS { $$ = -$2; }
    | '(' expr ')' { $$ = $2; }
    ;
%%
void yyerror(const char *message) {
    fprintf(stderr, "%s\n", message);
}

int main(void) {
    return yyparse();
}
