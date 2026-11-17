// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `make` core.
//!
//! The parser and planner are `no_std` + `alloc`; the host binary supplies a shell runner and a
//! host filesystem, the target binary supplies the RavenBSD spawn/filesystem equivalents. The
//! supported surface is deliberately bounded: explicit/pattern/suffix rules, variables, includes,
//! `.PHONY`, the four automatic variables, `-f -n -s -k -j1`, and command failure propagation.
//! GNU make functions and parallel jobs (`-jN`, N > 1) are documented unsupported errors.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Largest accepted Makefile, included file or recipe line.
pub const MAX_TEXT: usize = 1024 * 1024;
/// Maximum include nesting.
pub const MAX_INCLUDE_DEPTH: usize = 8;
/// Maximum variable-expansion recursion.
pub const MAX_EXPANSION_DEPTH: usize = 32;

/// Command-line options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// `-f FILE`; `None` uses `makefile`, then `Makefile`.
    pub file: Option<String>,
    /// `-n`: print recipes without running them.
    pub dry_run: bool,
    /// `-s`: do not echo recipes.
    pub silent: bool,
    /// `-k`: continue with other goals after a failure.
    pub keep_going: bool,
}

/// One parsed rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Target names; a pattern rule contains `%`.
    pub targets: Vec<String>,
    /// Prerequisite names, after variable expansion.
    pub prereqs: Vec<String>,
    /// Recipe lines, without the leading tab.
    pub recipe: Vec<String>,
}

/// A parsed Makefile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Makefile {
    /// Variable values in definition order.
    pub vars: BTreeMap<String, String>,
    /// Parsed rules in file order.
    pub rules: Vec<Rule>,
    /// `.PHONY` names.
    pub phony: BTreeSet<String>,
    /// The first ordinary target, used when no goal is named.
    pub default_goal: Option<String>,
}

/// File operations the planner needs.
pub trait FileSystem {
    /// Whether `path` exists.
    fn exists(&mut self, path: &str) -> bool;
    /// Last modification time in seconds, or `None` when absent.
    fn mtime(&mut self, path: &str) -> Option<i64>;
    /// Read a text file.
    fn read(&mut self, path: &str) -> Result<String, String>;
}

/// Command execution.
pub trait Runner {
    /// Run one expanded recipe line.
    fn run(&mut self, line: &str) -> Result<(), String>;
}

/// Parse the command line (without `argv[0]`).
pub fn parse_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(Config, Vec<String>), String> {
    let mut config = Config::default();
    let mut goals = Vec::new();
    let mut args = args.into_iter();
    let mut options = true;
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
            continue;
        }
        if options && arg == "-n" {
            config.dry_run = true;
        } else if options && arg == "-s" {
            config.silent = true;
        } else if options && arg == "-k" {
            config.keep_going = true;
        } else if options && arg == "-j1" {
        } else if options && arg == "-j" {
            let value = args.next().ok_or_else(|| "-j needs a value".to_string())?;
            if value != "1" {
                return Err(format!("parallel jobs are not supported: -j{value}"));
            }
        } else if options && arg.starts_with("-j") {
            let value = &arg[2..];
            if value.is_empty() || value == "1" {
            } else {
                return Err(format!("parallel jobs are not supported: {arg}"));
            }
        } else if options && arg == "-f" {
            config.file = Some(args.next().ok_or_else(|| "-f needs a file".to_string())?);
        } else if options && arg.starts_with("-f") {
            let value = &arg[2..];
            if value.is_empty() {
                return Err("-f needs a file".to_string());
            }
            config.file = Some(value.to_string());
        } else if options && arg.starts_with('-') {
            return Err(format!("unsupported option {arg}"));
        } else {
            goals.push(arg);
        }
    }
    Ok((config, goals))
}

/// Load and parse the selected Makefile, resolving `include` recursively.
pub fn load<F: FileSystem>(fs: &mut F, config: &Config) -> Result<Makefile, String> {
    let path = match &config.file {
        Some(path) => path.clone(),
        None if fs.exists("makefile") => "makefile".to_string(),
        None if fs.exists("Makefile") => "Makefile".to_string(),
        None => return Err("no makefile found (tried makefile, Makefile)".to_string()),
    };
    let mut makefile = Makefile::default();
    let mut stack = Vec::new();
    parse_file(fs, &mut makefile, &path, 0, &mut stack)?;
    if makefile.default_goal.is_none() {
        return Err("no targets specified and no default goal".to_string());
    }
    Ok(makefile)
}

fn parse_file<F: FileSystem>(
    fs: &mut F,
    makefile: &mut Makefile,
    path: &str,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    if depth >= MAX_INCLUDE_DEPTH {
        return Err(format!("include depth exceeded at {path}"));
    }
    if stack.iter().any(|entry| entry == path) {
        return Err(format!("recursive include {path}"));
    }
    let text = fs.read(path).map_err(|error| format!("{path}: {error}"))?;
    if text.len() > MAX_TEXT {
        return Err(format!("{path}: file too large"));
    }
    stack.push(path.to_string());
    let result = parse_text(fs, makefile, &text, depth, stack);
    stack.pop();
    result
}

fn parse_text<F: FileSystem>(
    fs: &mut F,
    makefile: &mut Makefile,
    text: &str,
    depth: usize,
    stack: &mut Vec<String>,
) -> Result<(), String> {
    let mut rule: Option<usize> = None;
    let mut logical = String::new();
    let mut lines = Vec::new();
    for raw in text.lines() {
        if let Some(prefix) = raw.strip_suffix('\\') {
            logical.push_str(prefix);
            logical.push(' ');
            continue;
        }
        logical.push_str(raw);
        lines.push(core::mem::take(&mut logical));
    }
    if !logical.is_empty() {
        lines.push(logical);
    }

    for raw in lines {
        if let Some(body) = raw.strip_prefix('\t') {
            let index = rule.ok_or_else(|| "recipe without a rule".to_string())?;
            if body.len() > MAX_TEXT {
                return Err("recipe line too large".to_string());
            }
            if let Some(rule) = makefile.rules.get_mut(index) {
                rule.recipe.push(body.to_string());
            }
            continue;
        }
        rule = None;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("include") {
            let file = expand(rest.trim(), &makefile.vars, 0);
            if file.is_empty() {
                return Err("include needs a file".to_string());
            }
            parse_file(fs, makefile, &file, depth + 1, stack)?;
            continue;
        }
        if let Some(rest) = line.strip_prefix("-include") {
            let file = expand(rest.trim(), &makefile.vars, 0);
            if !file.is_empty() && fs.exists(&file) {
                parse_file(fs, makefile, &file, depth + 1, stack)?;
            }
            continue;
        }
        if line.starts_with(".PHONY") {
            if let Some((_, rest)) = line.split_once(':') {
                for name in rest.split_whitespace() {
                    makefile.phony.insert(expand(name, &makefile.vars, 0));
                }
            }
            continue;
        }
        if line.starts_with(".SUFFIXES") || line.starts_with(".DEFAULT") {
            continue;
        }
        if let Some((name, value, kind)) = assignment(line) {
            match kind {
                Assign::Recursive => {
                    makefile.vars.insert(name, value);
                }
                Assign::Immediate => {
                    let value = expand(&value, &makefile.vars, 0);
                    makefile.vars.insert(name, value);
                }
                Assign::IfAbsent => {
                    makefile.vars.entry(name).or_insert(value);
                }
                Assign::Append => {
                    let entry = makefile.vars.entry(name).or_default();
                    if !entry.is_empty() {
                        entry.push(' ');
                    }
                    entry.push_str(&value);
                }
            }
            continue;
        }
        let Some((targets, prereqs)) = line.split_once(':') else {
            return Err(format!("invalid line: {line}"));
        };
        if targets.trim().is_empty() {
            return Err(format!("rule without a target: {line}"));
        }
        let targets: Vec<String> = targets
            .split_whitespace()
            .map(|value| expand(value, &makefile.vars, 0))
            .collect();
        let prereqs: Vec<String> = prereqs
            .split_whitespace()
            .map(|value| expand(value, &makefile.vars, 0))
            .collect();
        if let Some(suffix) = suffix_rule(&targets) {
            makefile.rules.push(suffix);
            rule = Some(makefile.rules.len() - 1);
            continue;
        }
        for target in &targets {
            if !target.starts_with('.') && !target.contains('%') && makefile.default_goal.is_none()
            {
                makefile.default_goal = Some(target.clone());
            }
        }
        makefile.rules.push(Rule {
            targets,
            prereqs,
            recipe: Vec::new(),
        });
        rule = Some(makefile.rules.len() - 1);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assign {
    Recursive,
    Immediate,
    IfAbsent,
    Append,
}

fn assignment(line: &str) -> Option<(String, String, Assign)> {
    let mut operator = None;
    for (index, _) in line.char_indices() {
        let rest = line.get(index..)?;
        if rest.starts_with(":=") {
            operator = Some((index, 2, Assign::Immediate));
            break;
        }
        if rest.starts_with("?=") {
            operator = Some((index, 2, Assign::IfAbsent));
            break;
        }
        if rest.starts_with("+=") {
            operator = Some((index, 2, Assign::Append));
            break;
        }
        if rest.starts_with('=') {
            operator = Some((index, 1, Assign::Recursive));
            break;
        }
        if rest.starts_with(':') {
            return None;
        }
    }
    let (index, width, kind) = operator?;
    let name = line.get(..index)?.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let value = line.get(index + width..)?.trim().to_string();
    Some((name.to_string(), value, kind))
}

fn suffix_rule(targets: &[String]) -> Option<Rule> {
    let target = targets.first()?;
    if !target.starts_with('.') || target.contains('%') {
        return None;
    }
    let rest = target.strip_prefix('.')?;
    let (source, destination) = rest.split_once('.')?;
    if source.is_empty() || destination.is_empty() {
        return None;
    }
    Some(Rule {
        targets: vec![format!("%.{destination}")],
        prereqs: vec![format!("%.{source}")],
        recipe: Vec::new(),
    })
}

fn expand(value: &str, vars: &BTreeMap<String, String>, depth: usize) -> String {
    expand_auto(value, vars, None, depth)
}

struct Automatic<'a> {
    target: &'a str,
    first: &'a str,
    all: &'a [String],
    newer: &'a [String],
}

fn expand_auto(
    value: &str,
    vars: &BTreeMap<String, String>,
    auto: Option<&Automatic<'_>>,
    depth: usize,
) -> String {
    if depth >= MAX_EXPANSION_DEPTH {
        return String::new();
    }
    let mut output = String::new();
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let Some(offset) = value.get(index..).and_then(|rest| rest.find('$')) else {
            output.push_str(value.get(index..).unwrap_or(""));
            break;
        };
        let at = index + offset;
        output.push_str(value.get(index..at).unwrap_or(""));
        let Some(next) = value.as_bytes().get(at + 1).copied() else {
            output.push('$');
            break;
        };
        match next {
            b'$' => {
                output.push('$');
                index = at + 2;
            }
            b'@' | b'<' | b'^' | b'?' => {
                if let Some(auto) = auto {
                    match next {
                        b'@' => output.push_str(auto.target),
                        b'<' => output.push_str(auto.first),
                        b'^' => {
                            let mut seen = BTreeSet::new();
                            let mut first = true;
                            for item in auto.all {
                                if seen.insert(item.clone()) {
                                    if !first {
                                        output.push(' ');
                                    }
                                    first = false;
                                    output.push_str(item);
                                }
                            }
                        }
                        b'?' => {
                            for (position, item) in auto.newer.iter().enumerate() {
                                if position > 0 {
                                    output.push(' ');
                                }
                                output.push_str(item);
                            }
                        }
                        _ => {}
                    }
                }
                index = at + 2;
            }
            b'(' | b'{' => {
                let close = if next == b'(' { b')' } else { b'}' };
                let mut end = at + 2;
                while end < bytes.len() && bytes[end] != close {
                    end += 1;
                }
                let name = value.get(at + 2..end).unwrap_or("").trim();
                if let Some(raw) = vars.get(name) {
                    output.push_str(&expand_auto(raw, vars, auto, depth + 1));
                }
                index = end.saturating_add(1);
            }
            _ => {
                output.push('$');
                index = at + 1;
            }
        }
    }
    output
}

/// Build `goals` (or the default goal) with the given filesystem and runner.
pub fn build<F: FileSystem, R: Runner, O: FnMut(&str)>(
    makefile: &Makefile,
    goals: &[String],
    config: &Config,
    fs: &mut F,
    runner: &mut R,
    out: &mut O,
) -> Result<(), String> {
    let goals: Vec<String> = if goals.is_empty() {
        makefile
            .default_goal
            .iter()
            .cloned()
            .collect::<Vec<String>>()
    } else {
        goals.to_vec()
    };
    if goals.is_empty() {
        return Err("no goal".to_string());
    }
    let mut state = BuildState::default();
    let mut failed = false;
    for goal in &goals {
        if let Err(error) = build_target(makefile, goal, config, fs, runner, out, &mut state) {
            if !config.keep_going {
                return Err(error);
            }
            out(&format!("make: {error}"));
            failed = true;
        }
    }
    if failed {
        Err("one or more goals failed".to_string())
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct BuildState {
    building: Vec<String>,
    done: BTreeSet<String>,
}

fn build_target<F: FileSystem, R: Runner, O: FnMut(&str)>(
    makefile: &Makefile,
    target: &str,
    config: &Config,
    fs: &mut F,
    runner: &mut R,
    out: &mut O,
    state: &mut BuildState,
) -> Result<(), String> {
    if state.done.contains(target) {
        return Ok(());
    }
    if state.building.iter().any(|entry| entry == target) {
        return Err(format!("dependency cycle at {target}"));
    }
    let Some((rule, prereqs)) = find_rule(makefile, target) else {
        if fs.exists(target) {
            state.done.insert(target.to_string());
            return Ok(());
        }
        return Err(format!("no rule to make target {target}"));
    };
    state.building.push(target.to_string());
    let mut newer = Vec::new();
    for prereq in &prereqs {
        build_target(makefile, prereq, config, fs, runner, out, state)?;
        if fs
            .mtime(prereq)
            .zip(fs.mtime(target))
            .is_some_and(|(prereq_time, target_time)| prereq_time > target_time)
            || (fs.mtime(target).is_none() && fs.exists(prereq))
        {
            newer.push(prereq.clone());
        }
    }
    let phony = makefile.phony.contains(target);
    let needs = phony
        || fs.mtime(target).is_none()
        || prereqs.iter().any(|prereq| {
            fs.mtime(prereq)
                .zip(fs.mtime(target))
                .is_some_and(|(prereq_time, target_time)| prereq_time > target_time)
        });
    state.building.pop();
    state.done.insert(target.to_string());
    if !needs {
        return Ok(());
    }
    let first = prereqs.first().map(String::as_str).unwrap_or("");
    let auto = Automatic {
        target,
        first,
        all: &prereqs,
        newer: &newer,
    };
    for line in &rule.recipe {
        let mut command = expand_auto(line, &makefile.vars, Some(&auto), 0);
        let mut silent = config.silent;
        let mut ignore_error = false;
        loop {
            if let Some(rest) = command.strip_prefix('@') {
                silent = true;
                command = rest.trim_start().to_string();
            } else if let Some(rest) = command.strip_prefix('-') {
                ignore_error = true;
                command = rest.trim_start().to_string();
            } else {
                break;
            }
        }
        if config.dry_run {
            out(&command);
            continue;
        }
        if !silent {
            out(&command);
        }
        if let Err(error) = runner.run(&command) {
            if ignore_error {
                continue;
            }
            return Err(format!("recipe for {target} failed: {error}"));
        }
    }
    Ok(())
}

fn find_rule<'a>(makefile: &'a Makefile, target: &str) -> Option<(&'a Rule, Vec<String>)> {
    if let Some(rule) = makefile
        .rules
        .iter()
        .find(|rule| rule.targets.iter().any(|name| name == target))
    {
        return Some((rule, rule.prereqs.clone()));
    }
    for rule in &makefile.rules {
        for pattern in &rule.targets {
            if let Some(stem) = match_pattern(pattern, target) {
                let prereqs = rule
                    .prereqs
                    .iter()
                    .map(|prereq| prereq.replace('%', &stem))
                    .collect();
                return Some((rule, prereqs));
            }
        }
    }
    None
}

fn match_pattern(pattern: &str, target: &str) -> Option<String> {
    let (prefix, suffix) = pattern.split_once('%')?;
    let rest = target.strip_prefix(prefix)?;
    let stem = rest.strip_suffix(suffix)?;
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

/// Run `make` end-to-end (load, then build).
pub fn run<F: FileSystem, R: Runner, O: FnMut(&str)>(
    config: &Config,
    goals: &[String],
    fs: &mut F,
    runner: &mut R,
    out: &mut O,
) -> Result<(), String> {
    let makefile = load(fs, config)?;
    build(&makefile, goals, config, fs, runner, out)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
