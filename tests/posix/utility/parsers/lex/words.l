/* SPDX-License-Identifier: BSD-3-Clause */
/* Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors */
/*
 * P42 lex fixture: identifiers, numbers, quoted characters, a comment start condition and
 * longest-match behaviour.
 */
%{
#include <stdio.h>
%}
%x COMMENT
%%
[ \t\n]+      ;
"/*"          BEGIN(COMMENT);
<COMMENT>"*/" BEGIN(INITIAL);
<COMMENT>.    ;
[[:alpha:]_][[:alnum:]_]*  { printf("ID:%s\n", yytext); }
[0-9]+        { printf("NUM:%s\n", yytext); }
.             { printf("CHAR:%s\n", yytext); }
%%
int main(void) {
    yylex();
    return 0;
}
