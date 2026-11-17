// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the tokenizer, the parser and variable expansion.

use alloc::string::ToString;
use alloc::vec::Vec;

use super::*;

fn tokens(line: &str) -> Vec<Token> {
    tokenize(line).expect("tokenizes")
}

fn words(line: &str) -> Vec<String> {
    let list = parse(line).expect("parses");
    list.items
        .iter()
        .flat_map(|item| item.pipeline.commands[0].words.iter().map(Word::render))
        .collect()
}

fn table() -> Table {
    let mut table = Table::new();
    table.set("NAME", "world");
    table.set("PWD", "/tmp");
    table.set("EMPTY", "");
    table
}

fn expand_str(text: &str) -> String {
    expand(&Word::expandable(text), &table(), 3)
}

// --- tokenizer ---------------------------------------------------------------------------------

#[test]
fn plain_words_are_split_on_whitespace() {
    assert_eq!(words("ls -la /tmp"), alloc::vec!["ls", "-la", "/tmp"]);
}

#[test]
fn extra_whitespace_is_ignored() {
    assert_eq!(words("   ls    -l   "), alloc::vec!["ls", "-l"]);
}

#[test]
fn single_quotes_keep_the_text() {
    assert_eq!(words("echo 'a b c'"), alloc::vec!["echo", "a b c"]);
}

#[test]
fn double_quotes_keep_the_text() {
    assert_eq!(words("echo \"a b\""), alloc::vec!["echo", "a b"]);
}

#[test]
fn quotes_can_be_adjacent() {
    assert_eq!(words("echo a\"b\"c"), alloc::vec!["echo", "abc"]);
}

#[test]
fn empty_quotes_make_an_empty_word() {
    let list = parse("echo ''").unwrap();
    assert_eq!(list.items[0].pipeline.commands[0].words.len(), 2);
    assert!(list.items[0].pipeline.commands[0].words[1].is_blank());
}

#[test]
fn a_backslash_escapes_the_next_byte() {
    assert_eq!(words("echo a\\ b"), alloc::vec!["echo", "a b"]);
}

#[test]
fn a_comment_ends_the_line() {
    assert_eq!(words("ls # trailing words"), alloc::vec!["ls"]);
}

#[test]
fn a_comment_inside_a_word_is_text() {
    assert_eq!(words("echo a#b"), alloc::vec!["echo", "a#b"]);
}

#[test]
fn operators_are_recognised_attached_to_words() {
    let parsed = tokens("echo a>b");
    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[1], Token::Word(Word::expandable("a")));
    assert_eq!(
        parsed[2],
        Token::Redir {
            fd: 1,
            kind: RedirKind::Out,
            strip_tabs: false
        }
    );
}

#[test]
fn semicolon_is_an_operator() {
    let parsed = tokens("a;b");
    assert_eq!(parsed.len(), 3);
    assert!(matches!(parsed[1], Token::Semi));
}

#[test]
fn and_and_or_operators_are_recognised() {
    let parsed = tokens("a && b || c");
    assert!(parsed.iter().any(|token| matches!(token, Token::AndIf)));
    assert!(parsed.iter().any(|token| matches!(token, Token::OrIf)));
}

#[test]
fn a_single_ampersand_is_rejected() {
    assert_eq!(tokenize("a & b"), Err(ParseError::UnexpectedOperator("&")));
}

#[test]
fn the_append_operator_is_recognised() {
    let parsed = tokens("a >> b");
    assert!(parsed.iter().any(|token| matches!(
        token,
        Token::Redir {
            fd: 1,
            kind: RedirKind::Append,
            strip_tabs: false
        }
    )));
}

#[test]
fn the_input_operator_is_recognised() {
    let parsed = tokens("a < b");
    assert!(parsed.iter().any(|token| matches!(
        token,
        Token::Redir {
            fd: 0,
            kind: RedirKind::In,
            strip_tabs: false
        }
    )));
}

#[test]
fn an_unterminated_single_quote_is_rejected() {
    assert_eq!(
        tokenize("echo 'abc"),
        Err(ParseError::UnterminatedQuote('\''))
    );
}

#[test]
fn an_unterminated_double_quote_is_rejected() {
    assert_eq!(
        tokenize("echo \"abc"),
        Err(ParseError::UnterminatedQuote('"'))
    );
}

#[test]
fn a_trailing_backslash_is_rejected() {
    assert_eq!(tokenize("echo \\"), Err(ParseError::TrailingBackslash));
}

#[test]
fn escaped_quotes_inside_double_quotes_are_literal() {
    assert_eq!(words("echo \"a\\\"b\""), alloc::vec!["echo", "a\"b"]);
}

#[test]
fn a_backslash_inside_single_quotes_is_literal() {
    assert_eq!(words("echo 'a\\b'"), alloc::vec!["echo", "a\\b"]);
}

// --- parser ------------------------------------------------------------------------------------

#[test]
fn one_command_is_a_list_of_one() {
    let list = parse("ls").unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].connector, Connector::Next);
}

#[test]
fn semicolon_separates_commands() {
    let list = parse("ls; pwd").unwrap();
    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[1].connector, Connector::Next);
}

#[test]
fn a_trailing_semicolon_is_allowed() {
    let list = parse("ls;").unwrap();
    assert_eq!(list.items.len(), 1);
}

#[test]
fn and_if_sets_the_connector() {
    let list = parse("ls && pwd").unwrap();
    assert_eq!(list.items[1].connector, Connector::And);
}

#[test]
fn or_if_sets_the_connector() {
    let list = parse("ls || pwd").unwrap();
    assert_eq!(list.items[1].connector, Connector::Or);
}

#[test]
fn connectors_can_mix() {
    let list = parse("a && b || c ; d").unwrap();
    let connectors: Vec<Connector> = list.items.iter().map(|item| item.connector).collect();
    assert_eq!(
        connectors,
        alloc::vec![
            Connector::Next,
            Connector::And,
            Connector::Or,
            Connector::Next
        ]
    );
}

#[test]
fn a_leading_semicolon_is_a_syntax_error() {
    assert_eq!(parse("; ls"), Err(ParseError::UnexpectedOperator(";")));
}

#[test]
fn two_semicolons_are_a_syntax_error() {
    assert_eq!(
        parse("ls ;; pwd"),
        Err(ParseError::UnexpectedOperator("operator"))
    );
}

#[test]
fn a_pipe_builds_a_pipeline() {
    let list = parse("ls | wc | cat").unwrap();
    assert_eq!(list.items.len(), 1);
    assert_eq!(list.items[0].pipeline.commands.len(), 3);
    assert_eq!(list.items[0].pipeline.commands[1].words[0].render(), "wc");
}

#[test]
fn a_pipeline_keeps_redirections_per_stage() {
    let list = parse("echo a > /tmp/a | cat < /tmp/a").unwrap();
    let stages = &list.items[0].pipeline.commands;
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0].redirs[0].kind, RedirKind::Out);
    assert_eq!(stages[1].redirs[0].kind, RedirKind::In);
}

#[test]
fn stderr_and_dup_redirections_are_captured() {
    let list = parse("echo err 2> /tmp/e 2>&1").unwrap();
    let redirs = &list.items[0].pipeline.commands[0].redirs;
    assert_eq!(redirs[0].fd, 2);
    assert_eq!(redirs[0].kind, RedirKind::Out);
    assert_eq!(redirs[1].fd, 2);
    assert_eq!(redirs[1].kind, RedirKind::Dup);
    assert_eq!(redirs[1].dup, Some(1));
}

#[test]
fn a_here_document_captures_its_body() {
    let list = parse("cat <<EOF\nbody\nEOF\n").unwrap();
    let redir = &list.items[0].pipeline.commands[0].redirs[0];
    assert_eq!(redir.kind, RedirKind::HereDoc);
    assert_eq!(redir.body.as_deref(), Some("body\n"));
}

#[test]
fn a_tab_stripped_here_document_captures_its_body() {
    let list = parse("cat <<-EOF\n\tbody\n\tEOF\n").unwrap();
    let redir = &list.items[0].pipeline.commands[0].redirs[0];
    assert_eq!(redir.body.as_deref(), Some("body\n"));
}

#[test]
fn an_unterminated_here_document_is_incomplete() {
    assert_eq!(
        parse("cat <<EOF\nbody\n"),
        Err(ParseError::IncompleteHereDoc)
    );
}

#[test]
fn a_command_renders_to_parseable_source() {
    let list = parse("echo 'a b' \"$NAME\" > /tmp/out 2>&1").unwrap();
    let rendered = list.items[0].pipeline.commands[0].render();
    let reparsed = parse(&rendered).unwrap();
    assert_eq!(
        reparsed.items[0].pipeline.commands[0].words[1].render(),
        "a b"
    );
    assert_eq!(reparsed.items[0].pipeline.commands[0].redirs.len(), 2);
}

#[test]
fn a_here_document_renders_and_reparses() {
    let list = parse("cat <<EOF\nbody\nEOF\n").unwrap();
    let rendered = list.items[0].pipeline.commands[0].render();
    let reparsed = parse(&rendered).unwrap();
    assert_eq!(
        reparsed.items[0].pipeline.commands[0].redirs[0]
            .body
            .as_deref(),
        Some("body\n")
    );
}

#[test]
fn redirection_targets_are_captured() {
    let list = parse("echo hi > /tmp/a").unwrap();
    let command = &list.items[0].pipeline.commands[0];
    assert_eq!(command.words.len(), 2);
    assert_eq!(command.redirs.len(), 1);
    assert_eq!(command.redirs[0].kind, RedirKind::Out);
    assert_eq!(command.redirs[0].target.render(), "/tmp/a");
}

#[test]
fn append_and_input_redirections_are_captured() {
    let list = parse("echo hi >> /tmp/a < /etc/in").unwrap();
    let redirs = &list.items[0].pipeline.commands[0].redirs;
    assert_eq!(redirs[0].kind, RedirKind::Append);
    assert_eq!(redirs[1].kind, RedirKind::In);
}

#[test]
fn a_redirection_without_a_target_is_rejected() {
    assert_eq!(
        parse("echo hi >"),
        Err(ParseError::MissingRedirectionTarget)
    );
}

#[test]
fn two_redirections_in_a_row_are_rejected() {
    assert_eq!(
        parse("echo > > x"),
        Err(ParseError::MissingRedirectionTarget)
    );
}

#[test]
fn an_empty_line_is_rejected() {
    assert_eq!(parse(""), Err(ParseError::Empty));
    assert_eq!(parse("   "), Err(ParseError::Empty));
    assert_eq!(parse("# only a comment"), Err(ParseError::Empty));
}

#[test]
fn too_many_words_are_rejected() {
    let line = alloc::vec!["a"; MAX_WORDS + 1].join(" ");
    assert_eq!(parse(&line), Err(ParseError::TooManyWords));
}

#[test]
fn too_many_redirections_are_rejected() {
    let mut line = alloc::string::String::from("echo");
    for index in 0..=MAX_REDIRS {
        line.push_str(&alloc::format!(" > /tmp/f{index}"));
    }
    assert_eq!(parse(&line), Err(ParseError::TooManyRedirs));
}

#[test]
fn a_quoted_semicolon_is_text() {
    assert_eq!(words("echo 'a;b'"), alloc::vec!["echo", "a;b"]);
}

#[test]
fn a_quoted_pipe_is_text() {
    assert_eq!(words("echo 'a|b'"), alloc::vec!["echo", "a|b"]);
}

#[test]
fn commands_before_a_connector_keep_their_redirections() {
    let list = parse("echo a > f && echo b").unwrap();
    assert_eq!(list.items[0].pipeline.commands[0].redirs.len(), 1);
    assert!(list.items[1].pipeline.commands[0].redirs.is_empty());
}

// --- expansion ---------------------------------------------------------------------------------

#[test]
fn a_plain_word_is_unchanged() {
    assert_eq!(expand_str("hello"), "hello");
}

#[test]
fn a_variable_expands() {
    assert_eq!(expand_str("$NAME"), "world");
}

#[test]
fn a_braced_variable_expands() {
    assert_eq!(expand_str("${NAME}"), "world");
}

#[test]
fn a_variable_next_to_text_expands() {
    assert_eq!(expand_str("hello-$NAME!"), "hello-world!");
}

#[test]
fn an_unknown_variable_is_empty() {
    assert_eq!(expand_str("[$NOPE]"), "[]");
}

#[test]
fn an_empty_variable_expands_to_nothing() {
    assert_eq!(expand_str("[$EMPTY]"), "[]");
}

#[test]
fn the_status_expands() {
    assert_eq!(expand_str("$?"), "3");
}

#[test]
fn a_lone_dollar_is_literal() {
    assert_eq!(expand_str("a$"), "a$");
    assert_eq!(expand_str("$ x"), "$ x");
}

#[test]
fn a_dollar_digit_is_literal() {
    assert_eq!(expand_str("$1"), "$1");
}

#[test]
fn an_unterminated_brace_stays_literal() {
    assert_eq!(expand_str("${NAME"), "${NAME");
}

#[test]
fn single_quotes_suppress_expansion() {
    let mut word = Word::default();
    word.push_literal("$NAME");
    assert_eq!(expand(&word, &table(), 0), "$NAME");
}

#[test]
fn double_quotes_allow_expansion() {
    let mut word = Word::default();
    word.push_expandable("$NAME");
    assert_eq!(expand(&word, &table(), 0), "world");
}

#[test]
fn a_backslash_escaped_dollar_is_literal() {
    assert_eq!(words("echo \\$NAME"), alloc::vec!["echo", "$NAME"]);
    let list = parse("echo \\$NAME").unwrap();
    assert_eq!(
        expand(&list.items[0].pipeline.commands[0].words[1], &table(), 0),
        "$NAME"
    );
}

#[test]
fn an_escaped_dollar_inside_double_quotes_is_literal() {
    let list = parse("echo \"\\$NAME\"").unwrap();
    assert_eq!(
        expand(&list.items[0].pipeline.commands[0].words[1], &table(), 0),
        "$NAME"
    );
}

#[test]
fn a_table_can_unset_and_overwrite() {
    let mut table = table();
    table.set("NAME", "there");
    assert_eq!(table.get("NAME"), Some("there".to_string()));
    table.unset("NAME");
    assert_eq!(table.get("NAME"), None);
}

#[test]
fn the_table_keeps_insertion_order() {
    let mut table = Table::new();
    table.set("B", "2");
    table.set("A", "1");
    let names: Vec<&str> = table.pairs.iter().map(|pair| pair.0.as_str()).collect();
    assert_eq!(names, alloc::vec!["B", "A"]);
}

#[test]
fn a_redirection_target_expands() {
    let list = parse("echo hi > $PWD/out").unwrap();
    let target = &list.items[0].pipeline.commands[0].redirs[0].target;
    assert_eq!(expand(target, &table(), 0), "/tmp/out");
}

#[test]
fn word_rendering_joins_segments() {
    let mut word = Word::literal("a");
    word.push_expandable("b");
    word.push_literal("c");
    assert_eq!(word.render(), "abc");
    assert_eq!(word.segments.len(), 3);
}

#[test]
fn blank_words_are_detected() {
    assert!(Word::default().is_blank());
    assert!(Word::literal("").is_blank());
    assert!(!Word::literal("x").is_blank());
}

#[test]
fn adjacent_segments_of_the_same_kind_merge() {
    let mut word = Word::default();
    word.push_literal("a");
    word.push_literal("b");
    word.push_expandable("c");
    word.push_expandable("d");
    assert_eq!(word.segments.len(), 2);
    assert_eq!(word.render(), "abcd");
}

#[test]
fn parse_and_expand_a_pipeline_of_commands() {
    let list = parse("cd /tmp && echo $? ; pwd").unwrap();
    assert_eq!(list.items.len(), 3);
    assert_eq!(
        expand(&list.items[1].pipeline.commands[0].words[0], &table(), 0),
        "echo"
    );
    assert_eq!(
        expand(&list.items[1].pipeline.commands[0].words[1], &table(), 7),
        "7"
    );
}

#[test]
fn an_unknown_variable_in_quotes_is_empty() {
    let list = parse("echo \"[$NOPE]\"").unwrap();
    assert_eq!(
        expand(&list.items[0].pipeline.commands[0].words[1], &table(), 0),
        "[]"
    );
}

#[test]
fn the_line_limit_is_enforced() {
    let line = "a".repeat(MAX_LINE + 1);
    assert_eq!(parse(&line), Err(ParseError::TooManyWords));
}

#[test]
fn error_messages_are_never_empty() {
    for error in [
        ParseError::Empty,
        ParseError::UnterminatedQuote('\''),
        ParseError::TrailingBackslash,
        ParseError::MissingRedirectionTarget,
        ParseError::UnexpectedOperator(";"),
        ParseError::TooManyWords,
        ParseError::TooManyRedirs,
        ParseError::TooManyStages,
        ParseError::IncompleteHereDoc,
        ParseError::InvalidRedirection,
    ] {
        assert!(!error.message().is_empty());
    }
}

#[test]
fn a_here_document_without_a_trailing_newline_parses() {
    let list = parse("cat <<EOF\nhere:body\nEOF").expect("parses");
    assert_eq!(
        list.items[0].pipeline.commands[0].redirs[0].body.as_deref(),
        Some("here:body\n")
    );
}
