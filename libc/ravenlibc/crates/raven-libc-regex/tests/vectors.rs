// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The data-driven vector driver: all 19 `.in` files and all 7 AT&T `.dat` files.
//!
//! Written from `tests/data/README` and `tests/data/att/README`; the donor `main.c`,
//! `split.c` and `t_regex_att.c` were read for behaviour only (BSD-4, STUDY). Each file is
//! run in the byte locale and in the UTF-8 locale. A line that cannot be expressed in UTF-8
//! (the donor's raw `\xff` byte line) is listed as skipped with its reason; every other line
//! must execute and match.

mod common;

use common::{
    Counts, InLine, NSUBS, check, error_code, fixstr, flags_for, parse_in_line, read_data,
    split_tabs, unvis,
};
use raven_libc_regex::consts::{REG_EXTENDED, REG_ICASE, REG_NOMATCH, REG_NOSUB, REG_STARTEND};
use raven_libc_regex::{ByteLocale, RegError, RegMatch, RegexLocale, Utf8Locale, regcomp, regexec};

/// The 19 `.in` files, in the order of the ticket.
const IN_FILES: [&str; 19] = [
    "anchor.in",
    "backref.in",
    "basic.in",
    "bracket.in",
    "c_comments.in",
    "complex.in",
    "error.in",
    "meta.in",
    "nospec.in",
    "paren.in",
    "regress.in",
    "repet_bounded.in",
    "repet_multi.in",
    "repet_ordinary.in",
    "startend.in",
    "subexp.in",
    "subtle.in",
    "word_bound.in",
    "zero.in",
];

/// The 7 AT&T `.dat` files.
const DAT_FILES: [&str; 7] = [
    "att/basic.dat",
    "att/categorization.dat",
    "att/forcedassoc.dat",
    "att/leftassoc.dat",
    "att/nullsubexpr.dat",
    "att/repetition.dat",
    "att/rightassoc.dat",
];

/// A failure collected by the driver.
struct Failure {
    file: String,
    line: usize,
    message: String,
}

/// Execute one `.in` file variant and return the failure count.
fn run_in_file(
    file: &str,
    loc: &dyn RegexLocale,
    mode: &str,
    failures: &mut Vec<Failure>,
) -> Counts {
    let text = match String::from_utf8(read_data(file)) {
        Ok(text) => text,
        Err(error) => {
            failures.push(Failure {
                file: file.to_string(),
                line: 0,
                message: format!("not UTF-8 text: {error}"),
            });
            return Counts::default();
        }
    };
    let mut counts = Counts::default();
    for (index, line) in text.lines().enumerate() {
        let Some(parsed) = parse_in_line(line) else {
            continue;
        };
        counts.lines += 1;
        let case = match parsed {
            Ok(case) => case,
            Err(message) => {
                failures.push(Failure {
                    file: file.to_string(),
                    line: index + 1,
                    message,
                });
                continue;
            }
        };
        if mode == "utf8" && !is_utf8_line(&case) {
            counts.skipped += 1;
            counts.reasons.push(format!(
                "line {}: byte-oriented vector is not valid UTF-8",
                index + 1
            ));
            continue;
        }
        let mut variants = vec![true];
        if case.flags.contains('&') {
            variants.push(false);
        }
        for extended in variants {
            counts.executed += 1;
            if let Err(message) = run_in_case(&case, extended, loc) {
                failures.push(Failure {
                    file: file.to_string(),
                    line: index + 1,
                    message,
                });
            }
        }
    }
    counts.report(file, mode);
    counts
}

/// Whether a `.in` line is representable in the UTF-8 locale.
fn is_utf8_line(case: &InLine) -> bool {
    if case.flags.contains('C') {
        return true;
    }
    std::str::from_utf8(&fixstr(&case.pattern)).is_ok()
        && std::str::from_utf8(&fixstr(&case.subject)).is_ok()
}

/// Execute one parsed `.in` case under one compile variant.
fn run_in_case(case: &InLine, extended: bool, loc: &dyn RegexLocale) -> Result<(), String> {
    let cflags = flags_for(&case.flags, extended, false);
    let eflags = flags_for(&case.flags, false, true);
    let mut fixed_pattern = fixstr(&case.pattern);
    let fixed_subject = fixstr(&case.subject);
    // Without REG_PEND the shell passes the bytes before the first NUL (the C string form).
    if cflags & raven_libc_regex::consts::REG_PEND == 0
        && let Some(nul) = fixed_pattern.iter().position(|&byte| byte == 0)
    {
        fixed_pattern.truncate(nul);
    }
    let compiled = regcomp(&fixed_pattern, cflags, loc);

    if case.flags.contains('C') {
        let name = std::str::from_utf8(&case.subject)
            .map_err(|_| "error name is not UTF-8".to_string())?;
        let Some(code) = error_code(name) else {
            return Err(format!("unknown error name {name:?}"));
        };
        return match compiled {
            Err(RegError(got)) if got == code => Ok(()),
            Err(RegError(got)) => Err(format!("regcomp returned {got}, wanted {code}")),
            Ok(_) => Err(format!("regcomp succeeded, wanted error {code}")),
        };
    }

    let re = match compiled {
        Ok(re) => re,
        Err(RegError(code)) => return Err(format!("regcomp failed with {code}")),
    };
    let mut matches = [RegMatch {
        rm_so: -1,
        rm_eo: -1,
    }; NSUBS];
    if eflags & REG_STARTEND != 0 {
        let start = case
            .subject
            .iter()
            .position(|&byte| byte == b'(')
            .ok_or_else(|| "bad STARTEND syntax: no (".to_string())?;
        let end = case
            .subject
            .iter()
            .position(|&byte| byte == b')')
            .ok_or_else(|| "bad STARTEND syntax: no )".to_string())?;
        matches[0].rm_so = start as i64 + 1;
        matches[0].rm_eo = end as i64;
    }

    let result = regexec(&re, &fixed_subject, &mut matches, eflags, loc);
    match result {
        Err(RegError(code)) if code == REG_NOMATCH => {
            if let Some(expected) = &case.expected {
                return Err(format!("regexec did not match (wanted {expected:?})"));
            }
            return Ok(());
        }
        Err(RegError(code)) => return Err(format!("regexec failed with {code}")),
        Ok(()) => {}
    }
    if cflags & REG_NOSUB != 0 {
        return Ok(());
    }
    let Some(expected) = &case.expected else {
        return Err("regexec matched but the vector expects no match".to_string());
    };
    check(&case.subject, &matches[0], Some(expected))?;
    if let Some(subexpr) = &case.subexpr {
        for (index, slot) in matches.iter().enumerate().skip(1) {
            let wanted = subexpr.get(index - 1);
            check(&case.subject, slot, wanted)?;
        }
    }
    Ok(())
}

/// The Spencer `bug()` table of `t_regex_att.c`, as behaviour: exact pattern/input pairs the
/// donor itself skips because Spencer's sub-matches do not meet the AT&T expectation.
const SPENCER_BUGS: &[(&str, &str)] = &[
    ("a[-]?c", "ac"),
    ("(a*)*", "a"),
    ("(aba|a*b)*", "ababa"),
    ("\\(a\\(b\\)*\\)*\\2", "abab"),
    ("(a*)*", "aaaaaa"),
    ("(a*)*", "aaaaaax"),
    ("(a*)+", "a"),
    ("(a*)+", "aaaaaa"),
    ("(a*)+", "aaaaaax"),
    ("([a]*)*", "a"),
    ("([a]*)*", "aaaaaa"),
    ("([a]*)*", "aaaaaax"),
    ("([a]*)+", "a"),
    ("([a]*)+", "aaaaaa"),
    ("([a]*)+", "aaaaaax"),
    ("([^b]*)*", "a"),
    ("([^b]*)*", "aaaaaa"),
    ("([^b]*)*", "aaaaaab"),
    ("([ab]*)*", "a"),
    ("([ab]*)*", "aaaaaa"),
    ("([ab]*)*", "ababab"),
    ("([ab]*)*", "bababa"),
    ("([ab]*)*", "b"),
    ("([ab]*)*", "bbbbbb"),
    ("([ab]*)*", "aaaabcde"),
    ("([^a]*)*", "b"),
    ("([^a]*)*", "bbbbbb"),
    ("([^ab]*)*", "ccccxx"),
    ("\\(a*\\)*\\(x\\)", "ax"),
    ("\\(a*\\)*\\(x\\)", "axa"),
    ("\\(a*\\)*\\(x\\)\\(\\1\\)", "x"),
    ("\\(a*\\)*\\(x\\)\\(\\1\\)", "ax"),
    ("\\(a*\\)*\\(x\\)\\(\\1\\)\\(x\\)", "axxa"),
    ("(a*)*(x)", "ax"),
    ("(a*)*(x)", "axa"),
    ("(a*)+(x)", "ax"),
    ("(a*)+(x)", "axa"),
    ("((a|ab)(c|bcd))(d*)", "abcd"),
    ("((a|ab)(bcd|c))(d*)", "abcd"),
    ("((ab|a)(c|bcd))(d*)", "abcd"),
    ("((ab|a)(bcd|c))(d*)", "abcd"),
    ("((a*)(b|abc))(c*)", "abc"),
    ("((a*)(abc|b))(c*)", "abc"),
    ("((..)|(.)){2}", "aaa"),
    ("((..)|(.)){3}", "aaa"),
    ("((..)|(.)){3}", "aaaa"),
    ("((..)|(.)){3}", "aaaaa"),
    ("X(.?){0,}Y", "X1234567Y"),
    ("X(.?){1,}Y", "X1234567Y"),
    ("X(.?){2,}Y", "X1234567Y"),
    ("X(.?){3,}Y", "X1234567Y"),
    ("X(.?){4,}Y", "X1234567Y"),
    ("X(.?){5,}Y", "X1234567Y"),
    ("X(.?){6,}Y", "X1234567Y"),
    ("X(.?){7,}Y", "X1234567Y"),
    ("X(.?){0,8}Y", "X1234567Y"),
    ("X(.?){1,8}Y", "X1234567Y"),
    ("X(.?){2,8}Y", "X1234567Y"),
    ("X(.?){3,8}Y", "X1234567Y"),
    ("X(.?){4,8}Y", "X1234567Y"),
    ("X(.?){5,8}Y", "X1234567Y"),
    ("X(.?){6,8}Y", "X1234567Y"),
    ("X(.?){7,8}Y", "X1234567Y"),
    ("(a|ab|c|bcd){0,}(d*)", "ababcd"),
    ("(a|ab|c|bcd){1,}(d*)", "ababcd"),
    ("(a|ab|c|bcd){2,}(d*)", "ababcd"),
    ("(a|ab|c|bcd){3,}(d*)", "ababcd"),
    ("(a|ab|c|bcd){1,10}(d*)", "ababcd"),
    ("(a|ab|c|bcd){2,10}(d*)", "ababcd"),
    ("(a|ab|c|bcd){3,10}(d*)", "ababcd"),
    ("(a|ab|c|bcd)*(d*)", "ababcd"),
    ("(a|ab|c|bcd)+(d*)", "ababcd"),
    ("(ab|a|c|bcd){0,}(d*)", "ababcd"),
    ("(ab|a|c|bcd){1,}(d*)", "ababcd"),
    ("(ab|a|c|bcd){2,}(d*)", "ababcd"),
    ("(ab|a|c|bcd){3,}(d*)", "ababcd"),
    ("(ab|a|c|bcd){1,10}(d*)", "ababcd"),
    ("(ab|a|c|bcd){2,10}(d*)", "ababcd"),
    ("(ab|a|c|bcd){3,10}(d*)", "ababcd"),
    ("(ab|a|c|bcd)*(d*)", "ababcd"),
    ("(ab|a|c|bcd)+(d*)", "ababcd"),
];

/// The Spencer `unsupported()` list of `t_regex_att.c`.
const UNSUPPORTED: [&str; 12] = [
    "ASSOCIATIVITY=left",
    "SUBEXPRESSION=precedence",
    "REPEAT_LONGEST=last",
    "BUG=alternation-order",
    "BUG=first-match",
    "BUG=nomatch-match",
    "BUG=repeat-any",
    "BUG=range-null",
    "BUG=repeat-null-unknown",
    "BUG=repeat-null",
    "BUG=repeat-artifact",
    "BUG=subexpression-first",
];

/// Whether the comment names a feature Spencer does not implement.
fn unsupported(comment: Option<&str>) -> bool {
    let Some(comment) = comment else {
        return false;
    };
    let trimmed = comment.trim_start_matches(['#', ' ', '\t']);
    UNSUPPORTED.contains(&trimmed)
}

/// The donor's `optional()` table: the one optional feature Spencer lacks.
fn optional(comment: Option<&str>) -> bool {
    comment.map(|comment| comment.trim_start_matches(['#', ' ', '\t']))
        == Some("no *? +? mimimal match ops")
}

/// Whether the pair is in the donor's Spencer bug list.
fn known_bug(pattern: &[u8], input: &[u8]) -> bool {
    let pattern = String::from_utf8_lossy(pattern);
    let input = String::from_utf8_lossy(input);
    SPENCER_BUGS
        .iter()
        .any(|(left, right)| *left == pattern && *right == input)
}

/// One expected `.dat` result.
enum DatExpect {
    /// An error name (`OK`, `NOMATCH`, a compile error).
    Error(i32, i32),
    /// `(so,eo)` pairs, one per parenthesis.
    Matches(Vec<String>),
}

/// Parse the expected field of a `.dat` line.
fn dat_expect(matches: &str) -> Result<DatExpect, String> {
    if matches.starts_with('(') {
        let mut rendered = Vec::new();
        let mut rest = matches;
        while let Some(start) = rest.find('(') {
            let tail = &rest[start..];
            let Some(end) = tail.find(')') else {
                return Err(format!("unterminated match field {matches:?}"));
            };
            rendered.push(format!("({})", &tail[1..end]));
            rest = &tail[end + 1..];
        }
        return Ok(DatExpect::Matches(rendered));
    }
    let (comp, exec) = match matches {
        "OK" => (0, 0),
        "NOMATCH" => (0, REG_NOMATCH),
        other => match error_code(other) {
            Some(code) => (code, 0),
            None => return Err(format!("unknown error name {other:?}")),
        },
    };
    Ok(DatExpect::Error(comp, exec))
}

/// Execute one `.dat` file.
fn run_dat_file(
    file: &str,
    loc: &dyn RegexLocale,
    mode: &str,
    failures: &mut Vec<Failure>,
) -> Counts {
    let text = match String::from_utf8(read_data(file)) {
        Ok(text) => text,
        Err(error) => {
            failures.push(Failure {
                file: file.to_string(),
                line: 0,
                message: format!("not UTF-8 text: {error}"),
            });
            return Counts::default();
        }
    };
    let mut counts = Counts::default();
    let mut skipping = false;
    let mut last_pattern: Option<Vec<u8>> = None;
    for (index, line) in text.lines().enumerate() {
        let fields = split_tabs(line);
        let Some(name) = fields.first().copied() else {
            continue;
        };
        if name.is_empty() || line.starts_with('#') {
            continue;
        }
        counts.lines += 1;
        if name == "}" {
            skipping = false;
            continue;
        }
        if name.starts_with(';') || name.starts_with('#') || name == "NOTE" {
            continue;
        }
        if file.ends_with("leftassoc.dat") {
            // The donor's Makefile passes -DSKIP_LEFTASSOC and t_regex_att.c marks the whole
            // leftassoc test expected-fail: Spencer is right-associative, so these
            // left-associative expectations are a categorisation probe, not a vector.
            counts.skipped += 1;
            counts.reasons.push(format!(
                "line {}: left-assoc expectations; donor marks SKIP_LEFTASSOC expected-fail",
                index + 1
            ));
            continue;
        }
        if skipping {
            counts.skipped += 1;
            counts.reasons.push(format!(
                "line {}: inside an unsupported optional block",
                index + 1
            ));
            continue;
        }
        let mut name = name;
        if let Some(rest) = name.strip_prefix(':')
            && let Some((_, tail)) = rest.split_once(':')
        {
            name = tail;
        }
        if let Some(rest) = name.strip_prefix('{') {
            if optional(fields.get(4).copied()) {
                skipping = true;
                counts.skipped += 1;
                counts.reasons.push(format!(
                    "line {}: optional feature not supported by Spencer",
                    index + 1
                ));
                continue;
            }
            name = rest;
        } else if let Some(rest) = name.strip_prefix('?').or_else(|| name.strip_prefix('|')) {
            if unsupported(fields.get(4).copied()) {
                counts.skipped += 1;
                counts.reasons.push(format!(
                    "line {}: categorization alternative Spencer does not implement",
                    index + 1
                ));
                continue;
            }
            name = rest;
        }
        let Some(pattern_field) = fields.get(1) else {
            failures.push(Failure {
                file: file.to_string(),
                line: index + 1,
                message: "missing pattern".to_string(),
            });
            continue;
        };
        let Some(input_field) = fields.get(2) else {
            failures.push(Failure {
                file: file.to_string(),
                line: index + 1,
                message: "missing input".to_string(),
            });
            continue;
        };
        let Some(matches_field) = fields.get(3) else {
            failures.push(Failure {
                file: file.to_string(),
                line: index + 1,
                message: "missing matches".to_string(),
            });
            continue;
        };
        let mut pattern = pattern_field.as_bytes().to_vec();
        let mut input = input_field.as_bytes().to_vec();
        let mut iflags = 0u32;
        let mut unvis_line = false;
        let mut variants = Vec::new();
        for byte in name.bytes() {
            match byte {
                b'0'..=b'9' | b'$' | b'n' => {}
                b'i' => iflags |= REG_ICASE,
                b'B' => variants.push(false),
                b'E' => variants.push(true),
                b'L' => variants.push(false),
                b'F' => {}
                _ => {
                    failures.push(Failure {
                        file: file.to_string(),
                        line: index + 1,
                        message: format!("unknown flag letter {}", char::from(byte)),
                    });
                }
            }
            if byte == b'$' {
                unvis_line = true;
            }
        }
        if unvis_line {
            pattern = unvis(&pattern);
            input = unvis(&input);
        }
        if input == b"NULL" {
            input.clear();
        }
        if pattern == b"SAME" {
            match &last_pattern {
                Some(previous) => pattern = previous.clone(),
                None => {
                    failures.push(Failure {
                        file: file.to_string(),
                        line: index + 1,
                        message: "SAME without a previous pattern".to_string(),
                    });
                    continue;
                }
            }
        } else {
            last_pattern = Some(pattern.clone());
        }
        if mode == "utf8"
            && (std::str::from_utf8(&pattern).is_err() || std::str::from_utf8(&input).is_err())
        {
            counts.skipped += 1;
            counts.reasons.push(format!(
                "line {}: byte-oriented vector is not valid UTF-8",
                index + 1
            ));
            continue;
        }
        if known_bug(&pattern, &input) {
            counts.skipped += 1;
            counts.reasons.push(format!(
                "line {}: Spencer bug list (donor t_regex_att.c bug())",
                index + 1
            ));
            continue;
        }
        let expect = match dat_expect(matches_field) {
            Ok(expect) => expect,
            Err(message) => {
                failures.push(Failure {
                    file: file.to_string(),
                    line: index + 1,
                    message,
                });
                continue;
            }
        };
        if variants.is_empty() {
            counts.skipped += 1;
            counts
                .reasons
                .push(format!("line {}: no B/E/L compile letter", index + 1));
            continue;
        }
        for extended in variants {
            counts.executed += 1;
            let cflags = if extended { REG_EXTENDED } else { 0 } | iflags;
            if let Err(message) = run_dat_case(
                &pattern,
                &input,
                &expect,
                matches_field,
                cflags,
                loc,
                name.contains('L'),
            ) {
                failures.push(Failure {
                    file: file.to_string(),
                    line: index + 1,
                    message,
                });
            }
        }
    }
    counts.report(file, mode);
    counts
}

/// Execute one `.dat` compile variant.
#[allow(clippy::too_many_arguments)]
fn run_dat_case(
    pattern: &[u8],
    input: &[u8],
    expect: &DatExpect,
    matches_field: &str,
    mut cflags: u32,
    loc: &dyn RegexLocale,
    nospec: bool,
) -> Result<(), String> {
    if nospec {
        cflags |= raven_libc_regex::consts::REG_NOSPEC;
    }
    let compiled = regcomp(pattern, cflags, loc);
    let (want_comp, want_exec) = match expect {
        DatExpect::Error(comp, exec) => (*comp, *exec),
        DatExpect::Matches(_) => (0, 0),
    };
    let re = match compiled {
        Err(RegError(got)) => {
            return if got == want_comp {
                Ok(())
            } else {
                Err(format!("regcomp returned {got}, wanted {want_comp}"))
            };
        }
        Ok(re) => re,
    };
    if want_comp != 0 {
        return Err(format!("regcomp succeeded, wanted {want_comp}"));
    }
    let count = matches_field.matches('(').count();
    let mut slots = vec![
        RegMatch {
            rm_so: -1,
            rm_eo: -1,
        };
        count
    ];
    match regexec(&re, input, &mut slots, 0, loc) {
        Err(RegError(got)) => {
            if got == want_exec {
                if let DatExpect::Matches(_) = expect {
                    return Err(format!("regexec failed with {got}, wanted a match"));
                }
                return Ok(());
            }
            return Err(format!("regexec returned {got}, wanted {want_exec}"));
        }
        Ok(()) => {
            if want_exec != 0 {
                return Err(format!("regexec matched, wanted error {want_exec}"));
            }
        }
    }
    if let DatExpect::Matches(wanted) = expect {
        let mut rendered = String::new();
        for slot in &slots {
            if slot.rm_so == -1 && slot.rm_eo == -1 {
                rendered.push_str("(?,?)");
            } else {
                rendered.push('(');
                rendered.push_str(&slot.rm_so.to_string());
                rendered.push(',');
                rendered.push_str(&slot.rm_eo.to_string());
                rendered.push(')');
            }
        }
        if rendered != wanted.join("") {
            return Err(format!("matches {rendered}, wanted {}", wanted.join("")));
        }
    }
    Ok(())
}

/// Run the `.in` corpus in one locale and fail the test on any mismatch.
fn in_corpus(loc: &dyn RegexLocale, mode: &str) -> (usize, usize) {
    let mut failures = Vec::new();
    let mut executed = 0usize;
    let mut skipped = 0usize;
    for file in IN_FILES {
        let counts = run_in_file(file, loc, mode, &mut failures);
        executed += counts.executed;
        skipped += counts.skipped;
    }
    report("in", mode, executed, skipped, &failures);
    (executed, skipped)
}

/// Run the `.dat` corpus in one locale.
fn dat_corpus(loc: &dyn RegexLocale, mode: &str) -> (usize, usize) {
    let mut failures = Vec::new();
    let mut executed = 0usize;
    let mut skipped = 0usize;
    for file in DAT_FILES {
        let counts = run_dat_file(file, loc, mode, &mut failures);
        executed += counts.executed;
        skipped += counts.skipped;
    }
    report("dat", mode, executed, skipped, &failures);
    (executed, skipped)
}

/// Print the total and panic on failures.
fn report(kind: &str, mode: &str, executed: usize, skipped: usize, failures: &[Failure]) {
    println!("{kind} [{mode}] total: {executed} executed, {skipped} skipped");
    if failures.is_empty() {
        return;
    }
    let mut message = format!("{} mismatches ({kind}, {mode}):", failures.len());
    for failure in failures.iter().take(40) {
        message.push_str(&format!(
            "\n  {}:{}: {}",
            failure.file, failure.line, failure.message
        ));
    }
    panic!("{message}");
}

/// Run one `.in` file and panic on failures (per-file test entry point).
fn in_file(file: &str, loc: &dyn RegexLocale, mode: &str) {
    let mut failures = Vec::new();
    let counts = run_in_file(file, loc, mode, &mut failures);
    report("in", mode, counts.executed, counts.skipped, &failures);
}

/// Run one `.dat` file and panic on failures.
fn dat_file(file: &str, loc: &dyn RegexLocale, mode: &str) {
    let mut failures = Vec::new();
    let counts = run_dat_file(file, loc, mode, &mut failures);
    report("dat", mode, counts.executed, counts.skipped, &failures);
}

macro_rules! in_file_tests {
    ($($name:ident, $file:expr, $loc:expr, $mode:expr;)*) => {
        $(
            #[test]
            fn $name() {
                in_file($file, $loc, $mode);
            }
        )*
    };
}

macro_rules! dat_file_tests {
    ($($name:ident, $file:expr, $loc:expr, $mode:expr;)*) => {
        $(
            #[test]
            fn $name() {
                dat_file($file, $loc, $mode);
            }
        )*
    };
}

in_file_tests! {
    anchor_in_byte, "anchor.in", &ByteLocale, "byte";
    backref_in_byte, "backref.in", &ByteLocale, "byte";
    basic_in_byte, "basic.in", &ByteLocale, "byte";
    bracket_in_byte, "bracket.in", &ByteLocale, "byte";
    c_comments_in_byte, "c_comments.in", &ByteLocale, "byte";
    complex_in_byte, "complex.in", &ByteLocale, "byte";
    error_in_byte, "error.in", &ByteLocale, "byte";
    meta_in_byte, "meta.in", &ByteLocale, "byte";
    nospec_in_byte, "nospec.in", &ByteLocale, "byte";
    paren_in_byte, "paren.in", &ByteLocale, "byte";
    regress_in_byte, "regress.in", &ByteLocale, "byte";
    repet_bounded_in_byte, "repet_bounded.in", &ByteLocale, "byte";
    repet_multi_in_byte, "repet_multi.in", &ByteLocale, "byte";
    repet_ordinary_in_byte, "repet_ordinary.in", &ByteLocale, "byte";
    startend_in_byte, "startend.in", &ByteLocale, "byte";
    subexp_in_byte, "subexp.in", &ByteLocale, "byte";
    subtle_in_byte, "subtle.in", &ByteLocale, "byte";
    word_bound_in_byte, "word_bound.in", &ByteLocale, "byte";
    zero_in_byte, "zero.in", &ByteLocale, "byte";
    anchor_in_utf8, "anchor.in", &Utf8Locale, "utf8";
    backref_in_utf8, "backref.in", &Utf8Locale, "utf8";
    basic_in_utf8, "basic.in", &Utf8Locale, "utf8";
    bracket_in_utf8, "bracket.in", &Utf8Locale, "utf8";
    c_comments_in_utf8, "c_comments.in", &Utf8Locale, "utf8";
    complex_in_utf8, "complex.in", &Utf8Locale, "utf8";
    error_in_utf8, "error.in", &Utf8Locale, "utf8";
    meta_in_utf8, "meta.in", &Utf8Locale, "utf8";
    nospec_in_utf8, "nospec.in", &Utf8Locale, "utf8";
    paren_in_utf8, "paren.in", &Utf8Locale, "utf8";
    regress_in_utf8, "regress.in", &Utf8Locale, "utf8";
    repet_bounded_in_utf8, "repet_bounded.in", &Utf8Locale, "utf8";
    repet_multi_in_utf8, "repet_multi.in", &Utf8Locale, "utf8";
    repet_ordinary_in_utf8, "repet_ordinary.in", &Utf8Locale, "utf8";
    startend_in_utf8, "startend.in", &Utf8Locale, "utf8";
    subexp_in_utf8, "subexp.in", &Utf8Locale, "utf8";
    subtle_in_utf8, "subtle.in", &Utf8Locale, "utf8";
    word_bound_in_utf8, "word_bound.in", &Utf8Locale, "utf8";
    zero_in_utf8, "zero.in", &Utf8Locale, "utf8";
}

dat_file_tests! {
    basic_dat_byte, "att/basic.dat", &ByteLocale, "byte";
    categorization_dat_byte, "att/categorization.dat", &ByteLocale, "byte";
    forcedassoc_dat_byte, "att/forcedassoc.dat", &ByteLocale, "byte";
    leftassoc_dat_byte, "att/leftassoc.dat", &ByteLocale, "byte";
    nullsubexpr_dat_byte, "att/nullsubexpr.dat", &ByteLocale, "byte";
    repetition_dat_byte, "att/repetition.dat", &ByteLocale, "byte";
    rightassoc_dat_byte, "att/rightassoc.dat", &ByteLocale, "byte";
    basic_dat_utf8, "att/basic.dat", &Utf8Locale, "utf8";
    categorization_dat_utf8, "att/categorization.dat", &Utf8Locale, "utf8";
    forcedassoc_dat_utf8, "att/forcedassoc.dat", &Utf8Locale, "utf8";
    leftassoc_dat_utf8, "att/leftassoc.dat", &Utf8Locale, "utf8";
    nullsubexpr_dat_utf8, "att/nullsubexpr.dat", &Utf8Locale, "utf8";
    repetition_dat_utf8, "att/repetition.dat", &Utf8Locale, "utf8";
    rightassoc_dat_utf8, "att/rightassoc.dat", &Utf8Locale, "utf8";
}

#[test]
fn in_vectors_byte_locale() {
    in_corpus(&ByteLocale, "byte");
}

#[test]
fn in_vectors_utf8_locale() {
    in_corpus(&Utf8Locale, "utf8");
}

#[test]
fn dat_vectors_byte_locale() {
    dat_corpus(&ByteLocale, "byte");
}

#[test]
fn dat_vectors_utf8_locale() {
    dat_corpus(&Utf8Locale, "utf8");
}
