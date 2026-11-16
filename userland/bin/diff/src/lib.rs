// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure line-difference computation for `diff`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options<'a> {
    /// Use unified output.
    pub unified: bool,
    /// Two input paths.
    pub operands: Vec<&'a str>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown short option.
    UnknownOption(char),
}

/// Parse `diff [-u] file1 file2`.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options<'a>, UsageError> {
    let mut options = Options::default();
    let mut parse_options = true;
    for argument in args {
        if parse_options && argument == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && argument.len() > 1 && argument.starts_with('-') {
            for option in argument.chars().skip(1) {
                match option {
                    'u' => options.unified = true,
                    other => return Err(UsageError::UnknownOption(other)),
                }
            }
            continue;
        }
        options.operands.push(argument);
    }
    Ok(options)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

fn operations(left: &[&str], right: &[&str]) -> Vec<Op> {
    let mut table = vec![vec![0u32; right.len().saturating_add(1)]; left.len().saturating_add(1)];
    for i in (0..left.len()).rev() {
        for j in (0..right.len()).rev() {
            table[i][j] = if left[i] == right[j] {
                table[i + 1][j + 1].saturating_add(1)
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut result = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < left.len() && j < right.len() {
        if left[i] == right[j] {
            result.push(Op::Equal(i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            result.push(Op::Delete(i));
            i += 1;
        } else {
            result.push(Op::Insert(j));
            j += 1;
        }
    }
    while i < left.len() {
        result.push(Op::Delete(i));
        i += 1;
    }
    while j < right.len() {
        result.push(Op::Insert(j));
        j += 1;
    }
    result
}

/// Compare two texts. Returns `(changed, rendered)`.
#[must_use]
pub fn diff(left: &str, right: &str, unified: bool) -> (bool, String) {
    let left_lines: Vec<&str> = left.lines().collect();
    let right_lines: Vec<&str> = right.lines().collect();
    let ops = operations(&left_lines, &right_lines);
    let changed = ops.iter().any(|op| !matches!(op, Op::Equal(..)));
    if !changed {
        return (false, String::new());
    }
    let rendered = if unified {
        unified_output(&left_lines, &right_lines, &ops)
    } else {
        normal_output(&left_lines, &right_lines, &ops)
    };
    (true, rendered)
}

fn range(start: usize, end: usize) -> String {
    if start == end {
        format!("{start}")
    } else {
        format!("{start},{end}")
    }
}

fn normal_output(left: &[&str], right: &[&str], ops: &[Op]) -> String {
    let mut output = String::new();
    let mut a_pos = 0usize;
    let mut b_pos = 0usize;
    let mut index = 0usize;
    while index < ops.len() {
        if let Op::Equal(..) = ops[index] {
            a_pos += 1;
            b_pos += 1;
            index += 1;
            continue;
        }
        let a_start = a_pos.saturating_add(1);
        let b_start = b_pos.saturating_add(1);
        let mut a_end = a_start;
        let mut b_end = b_start;
        let mut deletes = Vec::new();
        let mut inserts = Vec::new();
        while index < ops.len() && !matches!(ops[index], Op::Equal(..)) {
            match ops[index] {
                Op::Delete(line) => {
                    deletes.push(line);
                    a_pos += 1;
                    a_end = a_pos;
                }
                Op::Insert(line) => {
                    inserts.push(line);
                    b_pos += 1;
                    b_end = b_pos;
                }
                Op::Equal(..) => {}
            }
            index += 1;
        }
        if !deletes.is_empty() && !inserts.is_empty() {
            output.push_str(&format!(
                "{}c{}\n",
                range(a_start, a_end),
                range(b_start, b_end)
            ));
            for line in deletes {
                output.push_str(&format!("< {}\n", left.get(line).copied().unwrap_or("")));
            }
            output.push_str("---\n");
            for line in inserts {
                output.push_str(&format!("> {}\n", right.get(line).copied().unwrap_or("")));
            }
        } else if !deletes.is_empty() {
            output.push_str(&format!("{}d{}\n", range(a_start, a_end), b_pos));
            for line in deletes {
                output.push_str(&format!("< {}\n", left.get(line).copied().unwrap_or("")));
            }
        } else {
            output.push_str(&format!("{}a{}\n", a_pos, range(b_start, b_end)));
            for line in inserts {
                output.push_str(&format!("> {}\n", right.get(line).copied().unwrap_or("")));
            }
        }
    }
    output
}

fn unified_output(left: &[&str], right: &[&str], ops: &[Op]) -> String {
    let mut output = String::new();
    output.push_str("--- a\n");
    output.push_str("+++ b\n");
    let left_range = if left.is_empty() {
        "0,0".to_string()
    } else {
        format!("1,{}", left.len())
    };
    let right_range = if right.is_empty() {
        "0,0".to_string()
    } else {
        format!("1,{}", right.len())
    };
    output.push_str(&format!("@@ -{left_range} +{right_range} @@\n"));
    for op in ops {
        match *op {
            Op::Equal(line, _) => {
                output.push_str(&format!(" {}\n", left.get(line).copied().unwrap_or("")));
            }
            Op::Delete(line) => {
                output.push_str(&format!("-{}\n", left.get(line).copied().unwrap_or("")));
            }
            Op::Insert(line) => {
                output.push_str(&format!("+{}\n", right.get(line).copied().unwrap_or("")));
            }
        }
    }
    output
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
