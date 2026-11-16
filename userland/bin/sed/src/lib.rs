// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure POSIX `sed` subset: addresses, s, y, p, d, q, =, a, i and c.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_libc_regex::consts::REG_BASIC;
use raven_libc_regex::locale::ByteLocale;
use raven_libc_regex::{RegMatch, Regex, regcomp, regexec};

/// Why a script or input was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SedError {
    /// Invalid command or missing delimiter.
    Usage,
    /// Invalid regular expression.
    Regex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Address {
    Line(usize),
    Last,
    Regex(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Subst {
    regex: String,
    replacement: String,
    global: bool,
    print: bool,
    nth: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Print,
    Delete,
    Quit,
    Number,
    Subst(Subst),
    Translate { source: String, target: String },
    Append(String),
    Insert(String),
    Change(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Command {
    first: Option<Address>,
    second: Option<Address>,
    negate: bool,
    op: Op,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Program {
    commands: Vec<Command>,
}

/// Run one sed script over `input`; `quiet` is `-n`.
pub fn run(script: &str, input: &str, quiet: bool) -> Result<String, SedError> {
    let program = parse(script)?;
    let lines: Vec<&str> = if input.is_empty() {
        Vec::new()
    } else {
        input.lines().collect()
    };
    let total = lines.len();
    let mut ranges = vec![false; program.commands.len()];
    let mut output = String::new();
    for (index, line) in lines.iter().enumerate() {
        let number = index.saturating_add(1);
        let mut pattern = line.to_string();
        let mut deleted = false;
        let mut quit = false;
        let mut after = Vec::new();
        for (command_index, command) in program.commands.iter().enumerate() {
            if !selected(command, number, total, &pattern, &mut ranges[command_index])? {
                continue;
            }
            match &command.op {
                Op::Print => push(&mut output, &pattern),
                Op::Delete => {
                    deleted = true;
                    break;
                }
                Op::Quit => {
                    quit = true;
                    break;
                }
                Op::Number => output.push_str(&format!("{number}\n")),
                Op::Subst(subst) => {
                    let (next, changed) = substitute(&pattern, subst)?;
                    pattern = next;
                    if changed && subst.print {
                        push(&mut output, &pattern);
                    }
                }
                Op::Translate { source, target } => {
                    pattern = translate(&pattern, source, target)?;
                }
                Op::Append(text) => after.push(text.clone()),
                Op::Insert(text) => push(&mut output, text),
                Op::Change(text) => {
                    push(&mut output, text);
                    deleted = true;
                    break;
                }
            }
        }
        if !deleted && !quiet {
            push(&mut output, &pattern);
        }
        for text in after {
            push(&mut output, &text);
        }
        if quit {
            break;
        }
    }
    Ok(output)
}

fn push(output: &mut String, line: &str) {
    output.push_str(line);
    output.push('\n');
}

fn selected(
    command: &Command,
    number: usize,
    total: usize,
    pattern: &str,
    active: &mut bool,
) -> Result<bool, SedError> {
    let first = match &command.first {
        None => true,
        Some(address) => address_matches(address, number, total, pattern)?,
    };
    let result = match &command.second {
        None => first,
        Some(second) => {
            let second_match = address_matches(second, number, total, pattern)?;
            if !*active {
                if first {
                    *active = true;
                    if second_match {
                        *active = false;
                    }
                    true
                } else {
                    false
                }
            } else {
                if second_match {
                    *active = false;
                }
                true
            }
        }
    };
    Ok(if command.negate { !result } else { result })
}

fn address_matches(
    address: &Address,
    number: usize,
    total: usize,
    pattern: &str,
) -> Result<bool, SedError> {
    match address {
        Address::Line(line) => Ok(*line == number),
        Address::Last => Ok(number == total),
        Address::Regex(expression) => {
            let regex = compile(expression)?;
            regex_matches(&regex, pattern.as_bytes())
        }
    }
}

fn compile(expression: &str) -> Result<Regex, SedError> {
    regcomp(expression.as_bytes(), REG_BASIC, &ByteLocale).map_err(|_| SedError::Regex)
}

fn regex_matches(regex: &Regex, text: &[u8]) -> Result<bool, SedError> {
    match regexec(regex, text, &mut [], 0, &ByteLocale) {
        Ok(()) => Ok(true),
        Err(error) if error.0 == raven_libc_regex::consts::REG_NOMATCH => Ok(false),
        Err(_) => Err(SedError::Regex),
    }
}

fn substitute(pattern: &str, subst: &Subst) -> Result<(String, bool), SedError> {
    let regex = compile(&subst.regex)?;
    let subject = pattern.as_bytes();
    let mut slots = vec![RegMatch::default(); regex.nsub().saturating_add(1)];
    let target = subst
        .nth
        .unwrap_or(if subst.global { usize::MAX } else { 1 });
    if target == 0 {
        return Err(SedError::Usage);
    }
    let mut result = Vec::new();
    let mut cursor = 0usize;
    let mut copied = 0usize;
    let mut occurrence = 0usize;
    let mut changed = false;
    while cursor <= subject.len() {
        if regexec(&regex, &subject[cursor..], &mut slots, 0, &ByteLocale).is_err() {
            break;
        }
        let start = cursor.saturating_add(slots[0].rm_so.max(0) as usize);
        let end = cursor.saturating_add(slots[0].rm_eo.max(0) as usize);
        if end < start || end > subject.len() {
            break;
        }
        occurrence = occurrence.saturating_add(1);
        result.extend_from_slice(subject.get(copied..start).unwrap_or(&[]));
        let replace = occurrence == target || (subst.nth.is_none() && subst.global);
        if replace {
            result.extend(expand(&subst.replacement, &subject[cursor..], &slots));
            changed = true;
        } else {
            result.extend_from_slice(subject.get(start..end).unwrap_or(&[]));
        }
        copied = end;
        if occurrence >= target {
            break;
        }
        if end == start {
            if end >= subject.len() {
                break;
            }
            let step = next_boundary(subject, end);
            result.extend_from_slice(subject.get(end..step).unwrap_or(&[]));
            copied = step;
            cursor = step;
        } else {
            cursor = end;
        }
    }
    result.extend_from_slice(subject.get(copied..).unwrap_or(&[]));
    Ok((String::from_utf8_lossy(&result).into_owned(), changed))
}

fn next_boundary(subject: &[u8], index: usize) -> usize {
    let mut next = index.saturating_add(1);
    while next < subject.len() && (subject.get(next).copied().unwrap_or(0) & 0xc0) == 0x80 {
        next = next.saturating_add(1);
    }
    next
}

fn expand(replacement: &str, subject: &[u8], slots: &[RegMatch]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chars = replacement.chars();
    while let Some(character) = chars.next() {
        if character == '&' {
            append_group(&mut output, subject, slots, 0);
        } else if character == '\\' {
            if let Some(next) = chars.next() {
                if let Some(group) = next.to_digit(10) {
                    append_group(&mut output, subject, slots, group as usize);
                } else {
                    let mut buffer = [0u8; 4];
                    output.extend_from_slice(next.encode_utf8(&mut buffer).as_bytes());
                }
            }
        } else {
            let mut buffer = [0u8; 4];
            output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
    }
    output
}

fn append_group(output: &mut Vec<u8>, subject: &[u8], slots: &[RegMatch], group: usize) {
    let Some(slot) = slots.get(group) else {
        return;
    };
    if slot.rm_so < 0 || slot.rm_eo < slot.rm_so {
        return;
    }
    let start = slot.rm_so as usize;
    let end = slot.rm_eo as usize;
    output.extend_from_slice(subject.get(start..end).unwrap_or(&[]));
}

fn translate(pattern: &str, source: &str, target: &str) -> Result<String, SedError> {
    let source: Vec<char> = source.chars().collect();
    let target: Vec<char> = target.chars().collect();
    if source.len() != target.len() {
        return Err(SedError::Usage);
    }
    let mut output = String::new();
    for character in pattern.chars() {
        if let Some(index) = source.iter().position(|candidate| *candidate == character) {
            if let Some(replacement) = target.get(index) {
                output.push(*replacement);
            }
        } else {
            output.push(character);
        }
    }
    Ok(output)
}

fn parse(script: &str) -> Result<Program, SedError> {
    let chars: Vec<char> = script.chars().collect();
    let mut position = 0usize;
    let mut commands = Vec::new();
    while position < chars.len() {
        while let Some(character) = chars.get(position) {
            if *character == ';' || *character == '\n' || character.is_whitespace() {
                position = position.saturating_add(1);
            } else {
                break;
            }
        }
        if position >= chars.len() {
            break;
        }
        let first = parse_address(&chars, &mut position)?;
        let mut second = None;
        if chars.get(position) == Some(&',') {
            position = position.saturating_add(1);
            second = parse_address(&chars, &mut position)?;
        }
        let mut negate = false;
        if chars.get(position) == Some(&'!') {
            negate = true;
            position = position.saturating_add(1);
        }
        while chars
            .get(position)
            .is_some_and(|character| character.is_whitespace())
        {
            position = position.saturating_add(1);
        }
        let command = *chars.get(position).ok_or(SedError::Usage)?;
        position = position.saturating_add(1);
        let op = match command {
            's' => Op::Subst(parse_subst(&chars, &mut position)?),
            'y' => {
                let (source, target) = parse_translate(&chars, &mut position)?;
                Op::Translate { source, target }
            }
            'a' | 'i' | 'c' => {
                let text = parse_text(&chars, &mut position);
                match command {
                    'a' => Op::Append(text),
                    'i' => Op::Insert(text),
                    _ => Op::Change(text),
                }
            }
            'p' => Op::Print,
            'd' => Op::Delete,
            'q' => Op::Quit,
            '=' => Op::Number,
            _ => return Err(SedError::Usage),
        };
        commands.push(Command {
            first,
            second,
            negate,
            op,
        });
    }
    Ok(Program { commands })
}

fn parse_address(chars: &[char], position: &mut usize) -> Result<Option<Address>, SedError> {
    let Some(character) = chars.get(*position).copied() else {
        return Ok(None);
    };
    if character == '$' {
        *position = position.saturating_add(1);
        return Ok(Some(Address::Last));
    }
    if character.is_ascii_digit() {
        let mut number = 0usize;
        while let Some(digit) = chars
            .get(*position)
            .and_then(|character| character.to_digit(10))
        {
            number = number.saturating_mul(10).saturating_add(digit as usize);
            *position = position.saturating_add(1);
        }
        if number == 0 {
            return Err(SedError::Usage);
        }
        return Ok(Some(Address::Line(number)));
    }
    if character == '/' {
        *position = position.saturating_add(1);
        let expression = take_delimited(chars, position, '/')?;
        return Ok(Some(Address::Regex(expression)));
    }
    Ok(None)
}

fn parse_subst(chars: &[char], position: &mut usize) -> Result<Subst, SedError> {
    let delimiter = *chars.get(*position).ok_or(SedError::Usage)?;
    if delimiter == '\\' || delimiter == '\n' {
        return Err(SedError::Usage);
    }
    *position = position.saturating_add(1);
    let regex = take_delimited(chars, position, delimiter)?;
    let replacement = take_delimited(chars, position, delimiter)?;
    let mut global = false;
    let mut print = false;
    let mut nth = None;
    while let Some(character) = chars.get(*position).copied() {
        match character {
            'g' => {
                global = true;
                *position = position.saturating_add(1);
            }
            'p' => {
                print = true;
                *position = position.saturating_add(1);
            }
            '0'..='9' => {
                let mut number = 0usize;
                while let Some(digit) = chars
                    .get(*position)
                    .and_then(|character| character.to_digit(10))
                {
                    number = number.saturating_mul(10).saturating_add(digit as usize);
                    *position = position.saturating_add(1);
                }
                nth = Some(number);
            }
            ';' | '\n' | ' ' => break,
            _ => return Err(SedError::Usage),
        }
    }
    Ok(Subst {
        regex,
        replacement,
        global,
        print,
        nth,
    })
}

fn parse_translate(chars: &[char], position: &mut usize) -> Result<(String, String), SedError> {
    let delimiter = *chars.get(*position).ok_or(SedError::Usage)?;
    if delimiter == '\\' || delimiter == '\n' {
        return Err(SedError::Usage);
    }
    *position = position.saturating_add(1);
    let source = take_delimited(chars, position, delimiter)?;
    let target = take_delimited(chars, position, delimiter)?;
    if source.chars().count() != target.chars().count() {
        return Err(SedError::Usage);
    }
    Ok((source, target))
}

fn parse_text(chars: &[char], position: &mut usize) -> String {
    if chars.get(*position) == Some(&'\\') {
        *position = position.saturating_add(1);
        if chars.get(*position) == Some(&'\n') {
            *position = position.saturating_add(1);
        }
    } else if chars.get(*position) == Some(&' ') {
        *position = position.saturating_add(1);
    }
    let mut text = String::new();
    while let Some(character) = chars.get(*position).copied() {
        *position = position.saturating_add(1);
        if character == '\n' {
            break;
        }
        text.push(character);
    }
    text
}

fn take_delimited(
    chars: &[char],
    position: &mut usize,
    delimiter: char,
) -> Result<String, SedError> {
    let mut value = String::new();
    while let Some(character) = chars.get(*position).copied() {
        *position = position.saturating_add(1);
        if character == delimiter {
            return Ok(value);
        }
        if character == '\\' {
            value.push(character);
            if let Some(next) = chars.get(*position).copied() {
                *position = position.saturating_add(1);
                value.push(next);
            }
            continue;
        }
        value.push(character);
    }
    Err(SedError::Usage)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
