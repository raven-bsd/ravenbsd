// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Column-preserving source scanner for `libc-lint`.
//!
//! Comments and string/char literal contents are blanked in `code`; comments
//! are kept in `comment` so the unsafe check can look for `SAFETY:`.

/// One scanned line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    /// Code with comments and literal contents blanked.
    pub code: String,
    /// Code with comments blanked but string contents kept.
    pub code_str: String,
    /// Comment text on this line.
    pub comment: String,
}

#[derive(Default)]
struct State {
    block_depth: usize,
    raw_hashes: Option<usize>,
    in_string: bool,
}

/// Scan a complete file.
#[must_use]
pub fn scan(text: &str) -> Vec<Line> {
    let mut state = State::default();
    let mut out = Vec::new();
    for line in text.split_inclusive('\n') {
        out.push(scan_line(line, &mut state));
    }
    if out.is_empty() && !text.is_empty() {
        out.push(scan_line(text, &mut state));
    }
    out
}

fn blank(bytes: &mut [u8]) {
    for byte in bytes {
        *byte = b' ';
    }
}

fn scan_line(line: &str, state: &mut State) -> Line {
    let bytes = line.as_bytes();
    let mut code = bytes.to_vec();
    let mut code_str = bytes.to_vec();
    let mut comment = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if let Some(hashes) = state.raw_hashes {
            let closing = b"\"";
            let mut found = None;
            let mut j = i;
            while j < bytes.len() {
                if &bytes[j..] == closing {
                    let mut k = j + 1;
                    let mut count = 0;
                    while count < hashes && k < bytes.len() && bytes[k] == b'#' {
                        count += 1;
                        k += 1;
                    }
                    if count == hashes {
                        found = Some(k);
                        break;
                    }
                }
                j += 1;
            }
            if let Some(end) = found {
                blank(&mut code[i..end]);
                i = end;
                state.raw_hashes = None;
            } else {
                blank(&mut code[i..]);
                blank(&mut code_str[i..]);
                return Line {
                    code: String::from_utf8_lossy(&code).into_owned(),
                    code_str: String::from_utf8_lossy(&code_str).into_owned(),
                    comment,
                };
            }
            continue;
        }
        if state.in_string {
            let mut j = i;
            let mut closed = false;
            while j < bytes.len() {
                match bytes[j] {
                    b'\\' => j += 2,
                    b'"' => {
                        closed = true;
                        j += 1;
                        break;
                    }
                    _ => j += 1,
                }
            }
            blank(&mut code[i..j.min(bytes.len())]);
            if closed {
                state.in_string = false;
            }
            i = j;
            continue;
        }
        if state.block_depth > 0 {
            if bytes.get(i) == Some(&b'/') && bytes.get(i + 1) == Some(&b'*') {
                state.block_depth += 1;
                blank(&mut code[i..i + 2]);
                blank(&mut code_str[i..i + 2]);
                i += 2;
                continue;
            }
            if bytes.get(i) == Some(&b'*') && bytes.get(i + 1) == Some(&b'/') {
                state.block_depth -= 1;
                blank(&mut code[i..i + 2]);
                blank(&mut code_str[i..i + 2]);
                i += 2;
                continue;
            }
            comment.push(bytes[i] as char);
            blank(&mut code[i..i + 1]);
            blank(&mut code_str[i..i + 1]);
            i += 1;
            continue;
        }
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                if let Some(text) = line.get(i + 2..) {
                    comment.push_str(text.trim_end_matches(['\n', '\r']));
                }
                blank(&mut code[i..]);
                blank(&mut code_str[i..]);
                break;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                state.block_depth = 1;
                blank(&mut code[i..i + 2]);
                blank(&mut code_str[i..i + 2]);
                i += 2;
                continue;
            }
            b'r' if bytes.get(i + 1) == Some(&b'"') => {
                state.raw_hashes = Some(0);
                blank(&mut code[i..i + 2]);
                i += 2;
                continue;
            }
            b'r' if bytes.get(i + 1) == Some(&b'#') => {
                let mut j = i + 1;
                let mut hashes = 0usize;
                while bytes.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if bytes.get(j) == Some(&b'"') {
                    state.raw_hashes = Some(hashes);
                    blank(&mut code[i..j + 1]);
                    i = j + 1;
                    continue;
                }
            }
            b'"' => {
                state.in_string = true;
                blank(&mut code[i..i + 1]);
                i += 1;
                continue;
            }
            b'\'' => {
                if let Some(end) = char_literal_end(bytes, i) {
                    blank(&mut code[i..end]);
                    i = end;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    Line {
        code: String::from_utf8_lossy(&code).into_owned(),
        code_str: String::from_utf8_lossy(&code_str).into_owned(),
        comment,
    }
}

fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 2;
    } else if bytes.get(i).is_some_and(|byte| *byte != b'\'') {
        i += 1;
    } else {
        return None;
    }
    if bytes.get(i) == Some(&b'\'') {
        Some(i + 1)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "libc_lint_scan_ut.rs"]
mod tests;
