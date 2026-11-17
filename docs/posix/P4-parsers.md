<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# P42 POSIX lex and yacc

Status: P42 implementation. `lex` translates the POSIX lex file structure to C scanners that use
the host `regex.h` engine; `yacc` builds LALR(1) tables (canonical LR(1) item sets merged by core)
and emits a C `yyparse` with precedence/associativity and `error`-token recovery. The tools are
first-party Rust; generated C is built only by the host test runner.

## Evidence

```sh
sh tests/posix/utility/run-parsers.sh
cargo nextest run -p lex -p yacc
cargo xtask posix-check --profile posix --arch x86_64
make check-fast
```

OBSERVED (host, x86_64): `run-parsers.sh` prints `2 pass, 0 fail, 0 defect`; the lex fixture
produces identifiers, numbers and quoted characters through an inclusive start condition and a
comment-excluding exclusive condition; the yacc fixture computes `2+3*4=14`, `(2+3)*4=20`,
`-2*3=-6`, `2-3-4=-5`, `8/2*2=8` and recovers from `2+` to parse the next line `3+4` as `7`.

## lex matrix

| Feature | Status | Notes |
|---|---|---|
| Definitions, `%{ ... %}`, `%%` sections | present | definitions are expanded with `{NAME}` |
| `%s` inclusive and `%x` exclusive start conditions | present | `<SC>`/`<SC1,SC2>` prefixes and `<*>` |
| `yylex`, `yytext`, `yyleng`, `BEGIN`, `ECHO`, `yywrap` | present | generated C globals/macros |
| `.`, classes, ranges, POSIX classes, `* + ?`, `{m,n}`, `|`, `(...)` | present | translated to POSIX ERE |
| `^` and `$` | present | checked as line context at the match boundary |
| quoted strings and `\n`/`\t`/`\\` escapes | present | translated to ERE literals |
| longest match and rule order | present | all active rules are tried; longest wins, first wins ties |
| trailing context (`r/s`) | unsupported | generator error |
| `<<EOF>>` | unsupported | generator error |
| `%option`, `REJECT`, `yyless`, `unput`, `input` | unsupported | generator error or documented absence |

The generated scanner reads `yyin` into memory and copies each match into a NUL-terminated
`yytext`; a NUL byte in the input is not supported.

## yacc matrix

| Feature | Status | Notes |
|---|---|---|
| `%token` names, explicit numbers, `%start` | present | named tokens start at 258 |
| `%left`/`%right`/`%nonassoc`, `%prec` | present | shift/reduce conflicts resolved by precedence; nonassoc becomes an error entry |
| grammar rules, alternatives, empty productions, actions | present | `$$` and `$n` use the int `YYSTYPE` |
| LALR(1) tables | present | canonical LR(1) states merged by LR(0) core |
| `yyparse`, `yylval`, `yynerrs`, `-d` header | present | generated C and `y.tab.h` |
| `error` token and panic-mode recovery | present | `yyerrok`, `yyclearin`, `YYERROR` |
| `-v` conflict report | present | prints rules and conflict counts |
| `%union`, `%type`, `$<field>`, mid-rule actions | unsupported | generator error |
| `%destructor`, `%locations`, `%expect`, `%define` | unsupported | generator error |

`YYSTYPE` is `int`; a grammar can override it in the prologue, but typed semantic-value
translation is outside this bounded core. The generated parser uses a 10000-entry state/value
stack and reports `internal goto` if a table entry is missing.

## Unsupported constructs

* lex: trailing context, `<<EOF>>`, GNU `%option`, `REJECT`, `yyless`, `unput`, `input`, NUL
  bytes, non-ASCII character classes.
* yacc: `%union`, `%type`, `$<field>` references, mid-rule actions, `%destructor`, `%locations`,
  `%expect`, `%define`, non-integer semantic values.

## UNVERIFIED

No Pi 4/Pi 5 board run; both generators and their tests are host tools.
