// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SPDX header detection, licence-expression parsing, and header insertion.

use std::collections::BTreeSet;
use std::path::Path;

/// The licence every first-party file must declare.
pub(crate) const PROJECT_LICENSE: &str = "BSD-3-Clause";

/// The exact copyright line required in first-party files.
pub(crate) const COPYRIGHT_LINE: &str = "Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors";

const SPDX_TAG: &str = "SPDX-License-Identifier:";
const MAX_ALTERNATIVES: usize = 256;

/// Extensions checked everywhere in the tree.
const CORE_EXTENSIONS: &[&str] = &["rs", "S", "s", "ld", "sh", "toml", "md", "yml"];

/// Roots whose non-Rust sources are checked as well.
const ADDON_ROOTS: &[&str] = &["addons/", "tools/", "tests/"];

/// Extensions checked only under [`ADDON_ROOTS`]: add-on C, Python, Go and YAML sources.
///
/// `.json` is deliberately absent: the format has no comment syntax, so a header cannot be
/// carried there and generated result files could never comply.
const ADDON_EXTENSIONS: &[&str] = &["c", "h", "py", "go", "yaml"];

/// Comment syntax used when inserting a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommentStyle {
    /// `// ...` for Rust and assembly.
    LineSlash,
    /// `# ...` for shell, TOML, YAML, and Makefiles.
    LineHash,
    /// `/* ... */` for linker scripts.
    BlockC,
    /// `<!-- ... -->` for Markdown.
    Html,
}

impl CommentStyle {
    fn wrap(self, text: &str) -> String {
        match self {
            Self::LineSlash => format!("// {text}"),
            Self::LineHash => format!("# {text}"),
            Self::BlockC => format!("/* {text} */"),
            Self::Html => format!("<!-- {text} -->"),
        }
    }
}

/// Whether the file type is subject to the SPDX header rule.
///
/// Core source and documentation types are checked everywhere. Add-on sources (C, Python, Go,
/// YAML) are checked only under the add-on roots, where hand-written non-Rust code lives.
pub(crate) fn is_checked_file(path: &str) -> bool {
    if is_makefile(path) {
        return true;
    }
    let Some(extension) = Path::new(path).extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    CORE_EXTENSIONS.contains(&extension)
        || (is_addon_source(path) && ADDON_EXTENSIONS.contains(&extension))
}

fn is_addon_source(path: &str) -> bool {
    ADDON_ROOTS.iter().any(|root| path.starts_with(root))
}

/// Comment style for a checked file type.
pub(crate) fn comment_style(path: &str) -> Option<CommentStyle> {
    if is_makefile(path) {
        return Some(CommentStyle::LineHash);
    }
    match Path::new(path).extension().and_then(|ext| ext.to_str())? {
        "rs" | "S" | "s" | "go" => Some(CommentStyle::LineSlash),
        "sh" | "toml" | "yml" | "yaml" | "py" => Some(CommentStyle::LineHash),
        "ld" | "c" | "h" => Some(CommentStyle::BlockC),
        "md" => Some(CommentStyle::Html),
        _ => None,
    }
}

fn is_makefile(path: &str) -> bool {
    Path::new(path).file_name().and_then(|name| name.to_str()) == Some("Makefile")
}

/// Find an SPDX expression in the first `max_lines` lines.
pub(crate) fn find_spdx(content: &str, max_lines: usize) -> Option<String> {
    content.lines().take(max_lines).find_map(spdx_on_line)
}

fn spdx_on_line(line: &str) -> Option<String> {
    let (_, rest) = line.split_once(SPDX_TAG)?;
    let expression = rest
        .trim()
        .trim_end_matches("*/")
        .trim_end_matches("-->")
        .trim();
    if expression.is_empty() {
        None
    } else {
        Some(expression.to_string())
    }
}

/// Whether the exact project copyright line appears in the first `max_lines` lines.
pub(crate) fn has_copyright(content: &str, max_lines: usize) -> bool {
    content
        .lines()
        .take(max_lines)
        .any(|line| line.contains(COPYRIGHT_LINE))
}

/// Whether an expression is exactly the project licence.
pub(crate) fn is_project_license(expression: &str) -> bool {
    matches!(
        parse_expression(expression),
        Ok(Expr::Term(term)) if term.eq_ignore_ascii_case(PROJECT_LICENSE)
    )
}

/// Insert the standard header into `content`, or return `None` when the file type has no
/// comment syntax or a header is already present.
pub(crate) fn fixed_content(path: &str, content: &str) -> Option<String> {
    if find_spdx(content, 5).is_some() {
        return None;
    }
    let style = comment_style(path)?;
    let header = format!(
        "{}\n{}\n",
        style.wrap(&format!("SPDX-License-Identifier: {PROJECT_LICENSE}")),
        style.wrap(COPYRIGHT_LINE)
    );

    if content.starts_with("#!") && !content.starts_with("#![") {
        let (first, tail) = match content.find('\n') {
            Some(index) => (&content[..=index], &content[index + 1..]),
            None => (content, ""),
        };
        let mut updated = String::with_capacity(content.len() + header.len() + 1);
        updated.push_str(first);
        if !first.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(&header);
        if !tail.is_empty() && !tail.starts_with('\n') {
            updated.push('\n');
        }
        updated.push_str(tail);
        return Some(updated);
    }

    let mut updated = String::with_capacity(content.len() + header.len() + 1);
    updated.push_str(&header);
    if !content.is_empty() && !content.starts_with('\n') {
        updated.push('\n');
    }
    updated.push_str(content);
    Some(updated)
}

/// A parsed SPDX licence expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expr {
    /// A single licence identifier.
    Term(String),
    /// A licence with an exception (`WITH`).
    With(String, String),
    /// Conjunction.
    And(Vec<Expr>),
    /// Disjunction.
    Or(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    LParen,
    RParen,
}

/// Parse an SPDX licence expression.
pub(crate) fn parse_expression(text: &str) -> Result<Expr, String> {
    Parser {
        tokens: tokenize(text)?,
        position: 0,
    }
    .parse()
}

/// Split an expression into OR alternatives of required licence terms.
pub(crate) fn alternatives(expression: &Expr) -> Result<Vec<BTreeSet<String>>, String> {
    let mut result = match expression {
        Expr::Term(term) => vec![BTreeSet::from([term.to_ascii_lowercase()])],
        Expr::With(base, exception) => vec![BTreeSet::from([format!(
            "{} with {}",
            base.to_ascii_lowercase(),
            exception.to_ascii_lowercase()
        )])],
        Expr::Or(parts) => {
            let mut all = Vec::new();
            for part in parts {
                all.extend(alternatives(part)?);
                if all.len() > MAX_ALTERNATIVES {
                    return Err("licence expression is too complex".to_string());
                }
            }
            all
        }
        Expr::And(parts) => {
            let mut all: Vec<BTreeSet<String>> = vec![BTreeSet::new()];
            for part in parts {
                let part_alternatives = alternatives(part)?;
                let mut combined = Vec::new();
                for existing in &all {
                    for alternative in &part_alternatives {
                        let mut union = existing.clone();
                        union.extend(alternative.iter().cloned());
                        combined.push(union);
                        if combined.len() > MAX_ALTERNATIVES {
                            return Err("licence expression is too complex".to_string());
                        }
                    }
                }
                all = combined;
            }
            all
        }
    };
    result.sort();
    result.dedup();
    Ok(result)
}

/// Whether a file's expression is covered by an import entry's expression.
///
/// The entry licence must grant every term the file licence requires; a file may pick
/// any alternative of a dual-licensed entry. `Apache-2.0 WITH LLVM-exception` is
/// accepted where the entry says `Apache-2.0`.
pub(crate) fn expressions_compatible(file: &str, entry: &str) -> bool {
    let Ok(file_expression) = parse_expression(file) else {
        return false;
    };
    let Ok(entry_expression) = parse_expression(entry) else {
        return false;
    };
    let (Ok(file_alternatives), Ok(entry_alternatives)) = (
        alternatives(&file_expression),
        alternatives(&entry_expression),
    ) else {
        return false;
    };
    file_alternatives.iter().all(|file_alternative| {
        entry_alternatives
            .iter()
            .any(|entry_alternative| terms_compatible(entry_alternative, file_alternative))
    })
}

/// The entry and file term sets must agree; a `WITH` exception may be added to a term on
/// either side but an exception present in the entry cannot be dropped by the file.
fn terms_compatible(entry: &BTreeSet<String>, file: &BTreeSet<String>) -> bool {
    entry.iter().all(|entry_term| {
        file.iter()
            .any(|file_term| same_term(file_term, entry_term))
    }) && file.iter().all(|file_term| {
        entry
            .iter()
            .any(|entry_term| same_term(file_term, entry_term))
    })
}

fn same_term(left: &str, right: &str) -> bool {
    left == right || is_exception_of(left, right)
}

fn is_exception_of(extended: &str, base: &str) -> bool {
    extended
        .strip_prefix(base)
        .is_some_and(|rest| rest.starts_with(" with "))
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn parse(mut self) -> Result<Expr, String> {
        let expression = self.parse_or()?;
        if self.position != self.tokens.len() {
            return Err("unexpected trailing tokens in licence expression".to_string());
        }
        Ok(expression)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.parse_and()?];
        while self.eat_word("OR") {
            parts.push(self.parse_and()?);
        }
        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Expr::Or(parts))
        }
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.parse_with()?];
        while self.eat_word("AND") {
            parts.push(self.parse_with()?);
        }
        if parts.len() == 1 {
            Ok(parts.remove(0))
        } else {
            Ok(Expr::And(parts))
        }
    }

    fn parse_with(&mut self) -> Result<Expr, String> {
        let base = self.parse_primary()?;
        if !self.eat_word("WITH") {
            return Ok(base);
        }
        let exception = self.expect_word()?;
        match base {
            Expr::Term(license) => Ok(Expr::With(license, exception)),
            _ => Err("WITH must follow a single licence identifier".to_string()),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Word(word)) => {
                if word.eq_ignore_ascii_case("AND")
                    || word.eq_ignore_ascii_case("OR")
                    || word.eq_ignore_ascii_case("WITH")
                {
                    Err(format!("unexpected operator `{word}`"))
                } else {
                    Ok(Expr::Term(word))
                }
            }
            Some(Token::LParen) => {
                let inner = self.parse_or()?;
                match self.next() {
                    Some(Token::RParen) => Ok(inner),
                    _ => Err("missing `)` in licence expression".to_string()),
                }
            }
            _ => Err("expected a licence identifier".to_string()),
        }
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position)?.clone();
        self.position += 1;
        Some(token)
    }

    fn eat_word(&mut self, expected: &str) -> bool {
        if let Some(Token::Word(word)) = self.tokens.get(self.position)
            && word.eq_ignore_ascii_case(expected)
        {
            self.position += 1;
            return true;
        }
        false
    }

    fn expect_word(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Token::Word(word)) => Ok(word),
            _ => Err("expected a licence exception identifier".to_string()),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        match character {
            '(' => {
                flush(&mut current, &mut tokens);
                tokens.push(Token::LParen);
            }
            ')' => {
                flush(&mut current, &mut tokens);
                tokens.push(Token::RParen);
            }
            value if value.is_whitespace() => flush(&mut current, &mut tokens),
            value
                if value.is_ascii_alphanumeric()
                    || matches!(value, '.' | '-' | '+' | ':' | '_') =>
            {
                current.push(value);
            }
            value => return Err(format!("invalid character `{value}` in licence expression")),
        }
    }
    flush(&mut current, &mut tokens);
    if tokens.is_empty() {
        return Err("empty licence expression".to_string());
    }
    Ok(tokens)
}

fn flush(current: &mut String, tokens: &mut Vec<Token>) {
    if !current.is_empty() {
        tokens.push(Token::Word(std::mem::take(current)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_extensions_are_recognised() {
        for path in [
            "a.rs", "a.S", "a.s", "a.ld", "a.sh", "a.toml", "a.md", "a.yml", "Makefile",
        ] {
            assert!(is_checked_file(path), "{path} should be checked");
        }
        for path in ["a.txt", ".gitignore", "a.c", "a.yaml", ".provenance-ignore"] {
            assert!(!is_checked_file(path), "{path} should not be checked");
        }
    }

    #[test]
    fn addon_extensions_are_checked_only_under_the_addon_roots() {
        for path in [
            "addons/foo/bar.c",
            "addons/foo/bar.h",
            "tools/ufs-oracle/audit.py",
            "tools/linux-corpus/programs/hello.go",
            "tests/bench/load.yaml",
            "tests/fixtures/foo/build.sh",
        ] {
            assert!(is_checked_file(path), "{path} should be checked");
        }
        // The same extensions outside the add-on roots are not part of the contract.
        for path in ["bar.c", "src/bar.h", "crates/foo/bar.py", "docs/bar.yaml"] {
            assert!(!is_checked_file(path), "{path} should not be checked");
        }
        // JSON has no comment syntax, so a header cannot be required there.
        for path in [
            "tests/net/results/syn.json",
            "tools/bench-host/results/x.json",
        ] {
            assert!(!is_checked_file(path), "{path} should not be checked");
        }
    }

    #[test]
    fn addon_comment_styles_follow_the_language() {
        assert_eq!(
            comment_style("addons/foo/bar.c"),
            Some(CommentStyle::BlockC)
        );
        assert_eq!(
            comment_style("addons/foo/bar.h"),
            Some(CommentStyle::BlockC)
        );
        assert_eq!(comment_style("tools/x.py"), Some(CommentStyle::LineHash));
        assert_eq!(comment_style("tools/x.go"), Some(CommentStyle::LineSlash));
        assert_eq!(comment_style("tests/x.yaml"), Some(CommentStyle::LineHash));
    }

    #[test]
    fn spdx_within_five_lines_is_found() {
        let content = "line\n// SPDX-License-Identifier: MIT\n";
        assert_eq!(find_spdx(content, 5).as_deref(), Some("MIT"));
    }

    #[test]
    fn spdx_after_five_lines_is_not_found() {
        let content = "1\n2\n3\n4\n5\n// SPDX-License-Identifier: MIT\n";
        assert_eq!(find_spdx(content, 5), None);
    }

    #[test]
    fn block_and_html_closers_are_stripped() {
        assert_eq!(
            find_spdx("/* SPDX-License-Identifier: Apache-2.0 */", 5).as_deref(),
            Some("Apache-2.0")
        );
        assert_eq!(
            find_spdx("<!-- SPDX-License-Identifier: MIT -->", 5).as_deref(),
            Some("MIT")
        );
    }

    #[test]
    fn copyright_line_is_detected() {
        assert!(has_copyright(&format!("// {COPYRIGHT_LINE}"), 5));
        assert!(!has_copyright("// Copyright 2020 Someone Else", 5));
    }

    #[test]
    fn project_license_matches_exactly() {
        assert!(is_project_license("BSD-3-Clause"));
        assert!(is_project_license("bsd-3-clause"));
        assert!(!is_project_license("MIT"));
        assert!(!is_project_license("BSD-3-Clause OR MIT"));
        assert!(!is_project_license("BSD-3-Clause AND MIT"));
    }

    #[test]
    fn expression_parsing_handles_precedence_and_parentheses() {
        let expression = parse_expression("MIT AND (Apache-2.0 OR BSD-3-Clause)").unwrap();
        let alternatives = alternatives(&expression).unwrap();
        assert_eq!(alternatives.len(), 2);
    }

    #[test]
    fn invalid_expressions_are_rejected() {
        for expression in [
            "",
            "MIT AND",
            "(MIT",
            "MIT)",
            "MIT WITH",
            "MIT & Apache-2.0",
        ] {
            assert!(
                parse_expression(expression).is_err(),
                "{expression} should be invalid"
            );
        }
    }

    #[test]
    fn dual_license_entry_accepts_one_alternative() {
        assert!(expressions_compatible("MIT", "MIT OR Apache-2.0"));
        assert!(expressions_compatible("Apache-2.0", "MIT OR Apache-2.0"));
    }

    #[test]
    fn file_cannot_require_more_than_the_entry_grants() {
        assert!(!expressions_compatible(
            "MIT AND Apache-2.0",
            "MIT OR Apache-2.0"
        ));
        assert!(expressions_compatible(
            "MIT AND Apache-2.0",
            "MIT AND Apache-2.0"
        ));
    }

    #[test]
    fn with_exception_is_compatible_with_base_license() {
        assert!(expressions_compatible(
            "Apache-2.0 WITH LLVM-exception",
            "Apache-2.0"
        ));
        assert!(!expressions_compatible(
            "Apache-2.0",
            "Apache-2.0 WITH LLVM-exception"
        ));
    }

    #[test]
    fn fix_uses_slash_comments_for_rust() {
        let updated = fixed_content("src/lib.rs", "fn main() {}\n").unwrap();
        assert!(updated.starts_with(&format!("// SPDX-License-Identifier: {PROJECT_LICENSE}\n")));
        assert!(updated.contains(&format!("// {COPYRIGHT_LINE}\n")));
    }

    #[test]
    fn fix_uses_hash_comments_for_shell_and_toml() {
        for path in ["script.sh", "Cargo.toml", "Makefile", "ci.yml"] {
            let updated = fixed_content(path, "body\n").unwrap();
            assert!(
                updated.starts_with(&format!("# SPDX-License-Identifier: {PROJECT_LICENSE}\n"))
            );
        }
    }

    #[test]
    fn fix_uses_block_comments_for_linker_scripts() {
        let updated = fixed_content("kernel/linker/x86_64.ld", "SECTIONS {}\n").unwrap();
        assert!(updated.starts_with(&format!(
            "/* SPDX-License-Identifier: {PROJECT_LICENSE} */\n"
        )));
    }

    #[test]
    fn fix_treats_addon_sources_by_language() {
        for path in ["addons/foo/bar.c", "addons/foo/bar.h"] {
            let updated = fixed_content(path, "int x;\n").unwrap();
            assert!(updated.starts_with(&format!(
                "/* SPDX-License-Identifier: {PROJECT_LICENSE} */\n"
            )));
        }
        let updated = fixed_content("tools/x.py", "#!/usr/bin/env python3\nprint(1)\n").unwrap();
        assert!(updated.starts_with("#!/usr/bin/env python3\n# SPDX-License-Identifier:"));
        let updated = fixed_content("tools/x.go", "package main\n").unwrap();
        assert!(updated.starts_with(&format!("// SPDX-License-Identifier: {PROJECT_LICENSE}\n")));
        let updated = fixed_content("tests/x.yaml", "key: value\n").unwrap();
        assert!(updated.starts_with(&format!("# SPDX-License-Identifier: {PROJECT_LICENSE}\n")));
    }

    #[test]
    fn fix_uses_html_comments_for_markdown() {
        let updated = fixed_content("docs/design.md", "# Design\n").unwrap();
        assert!(updated.starts_with(&format!(
            "<!-- SPDX-License-Identifier: {PROJECT_LICENSE} -->\n"
        )));
    }

    #[test]
    fn fix_keeps_shebang_first() {
        let updated = fixed_content("script.sh", "#!/bin/sh\necho hi\n").unwrap();
        assert!(updated.starts_with("#!/bin/sh\n"));
        assert!(updated.contains(&format!("# SPDX-License-Identifier: {PROJECT_LICENSE}\n")));
    }

    #[test]
    fn fix_does_not_treat_inner_attribute_as_shebang() {
        let updated = fixed_content("lib.rs", "#![no_std]\n").unwrap();
        assert!(updated.starts_with(&format!("// SPDX-License-Identifier: {PROJECT_LICENSE}\n")));
        assert!(updated.ends_with("#![no_std]\n"));
    }

    #[test]
    fn fix_is_idempotent() {
        let once = fixed_content("a.rs", "fn main() {}\n").unwrap();
        assert_eq!(fixed_content("a.rs", &once), None);
    }

    #[test]
    fn fix_skips_files_with_existing_spdx() {
        let content = "// SPDX-License-Identifier: MIT\n// Copyright someone\n";
        assert_eq!(fixed_content("a.rs", content), None);
    }

    #[test]
    fn fix_skips_unknown_file_types() {
        assert_eq!(fixed_content("notes.txt", "hello\n"), None);
    }

    #[test]
    fn too_complex_expressions_are_rejected() {
        let expression = parse_expression(
            "MIT AND MIT AND MIT AND MIT AND MIT AND MIT AND MIT AND MIT AND MIT AND MIT",
        )
        .unwrap();
        assert!(alternatives(&expression).is_ok());
        let wide = "MIT OR ".repeat(300);
        let expression = parse_expression(&format!("{wide}MIT")).unwrap();
        assert!(alternatives(&expression).is_err());
    }
}
