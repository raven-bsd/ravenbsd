// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `strace` output parsing.
//!
//! The parser only needs aggregate data: syscall name, call count, error count
//! and a few normalised argument patterns. It never trusts trace content and
//! never panics on malformed input.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Aggregate statistics for one syscall.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyscallStat {
    /// Number of calls seen.
    pub count: u64,
    /// Calls whose return value started with `-1`.
    pub errors: u64,
    /// Normalised argument patterns, for example `flags -> O_RDONLY|O_CLOEXEC`.
    pub patterns: BTreeMap<String, BTreeMap<String, u64>>,
}

/// Parsed trace.
#[derive(Debug, Clone, Default)]
pub struct TraceSummary {
    /// Per-syscall aggregate, sorted by syscall name.
    pub syscalls: BTreeMap<String, SyscallStat>,
    /// Lines that looked like neither a syscall, a signal, an exit nor a summary.
    pub unparsed_lines: u64,
}

#[derive(Debug, Clone)]
struct Pending {
    name: String,
    args: String,
}

/// Parse one `strace -f -qq -s 64` trace.
pub fn parse_trace(text: &str) -> TraceSummary {
    let mut summary = TraceSummary::default();
    let mut pending: HashMap<u64, Pending> = HashMap::new();

    for raw_line in text.lines() {
        let (pid, line) = strip_pid(raw_line);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if is_noise(line) {
            continue;
        }
        if let Some(rest) = resumed_rest(line) {
            let Some(Pending { name, args }) = pending.remove(&pid) else {
                summary.unparsed_lines += 1;
                continue;
            };
            match split_resumed(rest) {
                Some((tail_args, result)) => {
                    let args = merge_args(&args, tail_args);
                    record(&mut summary, &name, &args, &result);
                }
                None => summary.unparsed_lines += 1,
            }
            continue;
        }
        if let Some((name, args)) = unfinished_call(line) {
            pending.insert(
                pid,
                Pending {
                    name: name.to_string(),
                    args: args.to_string(),
                },
            );
            continue;
        }
        if let Some((name, args, result)) = parse_call(line) {
            record(&mut summary, name, &args, &result);
            continue;
        }
        summary.unparsed_lines += 1;
    }

    for pending in pending.into_values() {
        record(&mut summary, &pending.name, &pending.args, "");
    }
    summary
}

fn strip_pid(line: &str) -> (u64, &str) {
    static PREFIX: OnceLock<Regex> = OnceLock::new();
    let regex = PREFIX
        .get_or_init(|| Regex::new(r"^(?:\[pid\s+(\d+)\]\s+|(\d+)\s+)").expect("pid prefix regex"));
    if let Some(captures) = regex.captures(line) {
        let pid = captures
            .get(1)
            .or_else(|| captures.get(2))
            .and_then(|value| value.as_str().parse().ok())
            .unwrap_or(0);
        let end = captures.get(0).map_or(0, |value| value.end());
        (pid, &line[end..])
    } else {
        (0, line)
    }
}

fn resumed_rest(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("<... ")?;
    let (_, rest) = rest.split_once(" resumed>")?;
    Some(rest)
}

fn unfinished_call(line: &str) -> Option<(&str, &str)> {
    let marker = line.find("<unfinished ...>")?;
    let before = line[..marker].trim_end();
    let open = before.find('(')?;
    let name = before[..open].trim();
    if !valid_name(name) {
        return None;
    }
    Some((name, before[open + 1..].trim()))
}

fn parse_call(line: &str) -> Option<(&str, String, String)> {
    let open = line.find('(')?;
    let name = line[..open].trim();
    if !valid_name(name) {
        return None;
    }
    let separator = result_separator(line)?;
    let marker = separator.start();
    if marker <= open {
        return None;
    }
    Some((
        name,
        line[open + 1..marker].to_string(),
        line[separator.end()..].trim().to_string(),
    ))
}

fn split_resumed(rest: &str) -> Option<(&str, String)> {
    let separator = result_separator(rest)?;
    let args = rest[..separator.start()].trim();
    let result = rest[separator.end()..].trim().to_string();
    Some((args, result))
}

fn result_separator(line: &str) -> Option<regex::Match<'_>> {
    static SEPARATOR: OnceLock<Regex> = OnceLock::new();
    let regex = SEPARATOR.get_or_init(|| Regex::new(r"\)\s+=\s+").expect("result separator regex"));
    regex.find_iter(line).last()
}

fn merge_args(before: &str, tail: &str) -> String {
    let tail = tail.trim();
    if tail.is_empty() || tail == ")" {
        return before.to_string();
    }
    let tail = tail.strip_prefix(", ").unwrap_or(tail);
    let tail = tail.strip_suffix(')').unwrap_or(tail);
    if before.is_empty() {
        tail.to_string()
    } else {
        format!("{before}, {tail}")
    }
}

fn valid_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if let Some(hex) = name.strip_prefix("syscall_0x") {
        return !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_noise(line: &str) -> bool {
    if line.starts_with("+++ ") || line.starts_with("--- ") {
        return true;
    }
    if line.starts_with("% time") || line.starts_with("------") {
        return true;
    }
    if line.contains("calls    errors syscall") {
        return true;
    }
    if line.ends_with(" total") {
        return true;
    }
    static SUMMARY: OnceLock<Regex> = OnceLock::new();
    let regex = SUMMARY.get_or_init(|| {
        Regex::new(r"^\s*\d+\.\d+\s+\d+\.\d+\s+\d+\s+\d+\s+(\d+\s+)?\S+$")
            .expect("strace -c summary regex")
    });
    regex.is_match(line)
}

fn record(summary: &mut TraceSummary, name: &str, args: &str, result: &str) {
    let stat = summary.syscalls.entry(name.to_string()).or_default();
    stat.count += 1;
    if result.trim_start().starts_with("-1") {
        stat.errors += 1;
    }
    apply_patterns(name, args, stat);
}

/// Split a syscall argument list on top-level commas, respecting quotes and
/// nested `()[]{}` groups. The returned items keep their quotes, which is fine
/// for pattern extraction.
pub fn split_args(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut depth = 0i32;
    for character in text.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if let Some(delimiter) = quote {
            current.push(character);
            if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => {
                quote = Some(character);
                current.push(character);
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(character);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(character);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            other => current.push(other),
        }
    }
    if !current.trim().is_empty() || !text.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn apply_patterns(name: &str, args: &str, stat: &mut SyscallStat) {
    let args = split_args(args);
    match name {
        "openat" => pattern_flags(stat, "flags", args.get(2), "O_"),
        "mmap" => {
            pattern_flags(stat, "prot", args.get(2), "PROT_");
            pattern_flags(stat, "flags", args.get(3), "MAP_");
        }
        "clone" => pattern_clone_flags(stat, args.get(1)),
        "clone3" => pattern_clone_flags(stat, args.first()),
        "socket" => {
            if let (Some(domain), Some(kind)) = (args.first(), args.get(1)) {
                pattern_add(stat, "domain_type", &format!("{domain}|{kind}"));
            }
        }
        "fcntl" => pattern_named(stat, "cmd", args.get(1), ""),
        "ioctl" => pattern_named(stat, "request", args.get(1), ""),
        "prctl" => pattern_named(stat, "option", args.first(), ""),
        "futex" => pattern_named(stat, "op", args.get(1), ""),
        "epoll_ctl" => pattern_named(stat, "op", args.get(1), ""),
        "rt_sigaction" => pattern_named(stat, "signal", args.first(), ""),
        "statx" => pattern_flags(stat, "mask", args.get(3), "STATX_"),
        "getrandom" => pattern_flags(stat, "flags", args.get(2), "GRND_"),
        _ => {}
    }
}

fn pattern_named(stat: &mut SyscallStat, kind: &str, arg: Option<&String>, prefix: &str) {
    let Some(arg) = arg else {
        return;
    };
    let value = arg.trim();
    let value = value.strip_prefix(prefix).unwrap_or(value);
    let value = value.split([')', '}', ',']).next().unwrap_or(value).trim();
    if !value.is_empty() {
        pattern_add(stat, kind, value);
    }
}

fn pattern_clone_flags(stat: &mut SyscallStat, arg: Option<&String>) {
    let Some(arg) = arg else {
        return;
    };
    let value = arg
        .find("flags=")
        .map_or(arg.as_str(), |index| &arg[index + 6..]);
    let value = value.split([')', '}', ',']).next().unwrap_or(value).trim();
    let mut flags: Vec<&str> = value
        .split('|')
        .map(str::trim)
        .filter(|flag| flag.starts_with("CLONE_"))
        .collect();
    flags.sort_unstable();
    flags.dedup();
    if !flags.is_empty() {
        pattern_add(stat, "flags", &flags.join("|"));
    }
}

fn pattern_flags(stat: &mut SyscallStat, kind: &str, arg: Option<&String>, prefix: &str) {
    let Some(arg) = arg else {
        return;
    };
    let mut flags: Vec<&str> = arg
        .split('|')
        .map(str::trim)
        .filter(|value| value.starts_with(prefix))
        .collect();
    flags.sort_unstable();
    flags.dedup();
    let value = if flags.is_empty() {
        arg.trim().to_string()
    } else {
        flags.join("|")
    };
    if !value.is_empty() {
        pattern_add(stat, kind, &value);
    }
}

fn pattern_add(stat: &mut SyscallStat, kind: &str, value: &str) {
    let value = truncate(value, 160);
    *stat
        .patterns
        .entry(kind.to_string())
        .or_default()
        .entry(value)
        .or_insert(0) += 1;
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let mut result: String = value.chars().take(max).collect();
        result.push('…');
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_call_and_error() {
        let trace =
            "1 write(1, \"hi\\n\", 3) = 3\n1 openat(AT_FDCWD, \"/nope\", O_RDONLY) = -1 ENOENT\n";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["write"].count, 1);
        assert_eq!(summary.syscalls["write"].errors, 0);
        assert_eq!(summary.syscalls["openat"].count, 1);
        assert_eq!(summary.syscalls["openat"].errors, 1);
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn brackets_and_bare_pid_prefixes_are_stripped() {
        let trace = "[pid  123] read(0, \"x\", 1) = 1\n456 close(3) = 0\n";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["read"].count, 1);
        assert_eq!(summary.syscalls["close"].count, 1);
    }

    #[test]
    fn unfinished_and_resumed_pair_counts_once() {
        let trace = "\
1 mprotect(0x1000, 4096, PROT_READ <unfinished ...>
1 <... mprotect resumed>) = 0
";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["mprotect"].count, 1);
        assert_eq!(summary.syscalls["mprotect"].errors, 0);
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn interleaved_pids_finish_their_own_calls() {
        let trace = "\
1 mmap(NULL, 4096, PROT_READ <unfinished ...>
2 mmap(NULL, 8192, PROT_WRITE <unfinished ...>
1 <... mmap resumed>) = 0x1000
2 <... mmap resumed>) = 0x2000
";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["mmap"].count, 2);
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn signals_and_exits_are_ignored() {
        let trace = "\
1 read(0, \"\", 1) = 0
--- SIGCHLD {si_signo=SIGCHLD} ---
+++ exited with 0 +++
+++ killed by SIGKILL +++
";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["read"].count, 1);
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn strace_summary_is_ignored() {
        let text = include_str!("../tests/fixtures/summary-excerpt.strace");
        let summary = parse_trace(text);
        assert_eq!(summary.syscalls["openat"].count, 1);
        assert_eq!(summary.syscalls["execve"].count, 1);
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn empty_trace_is_empty() {
        let summary = parse_trace("");
        assert!(summary.syscalls.is_empty());
        assert_eq!(summary.unparsed_lines, 0);
    }

    #[test]
    fn binary_junk_is_counted_not_fatal() {
        let text = "not a trace\n\u{fffd}\u{fffd}\n1 weird( = 0\n";
        let summary = parse_trace(text);
        assert_eq!(summary.unparsed_lines, 3);
    }

    #[test]
    fn very_long_argument_does_not_hang() {
        let long = "x".repeat(1_048_576);
        let trace = format!("1 write(1, \"{long}\", 1) = 1\n");
        let summary = parse_trace(&trace);
        assert_eq!(summary.syscalls["write"].count, 1);
    }

    #[test]
    fn escaped_quotes_and_parentheses_are_one_argument() {
        let trace = "1 openat(AT_FDCWD, \"/tmp/we)ird, \\\"name\\\"\", O_WRONLY|O_CREAT|O_CLOEXEC, 0666) = 3\n";
        let summary = parse_trace(trace);
        assert_eq!(
            summary.syscalls["openat"].patterns["flags"]["O_CLOEXEC|O_CREAT|O_WRONLY"],
            1
        );
    }

    #[test]
    fn split_args_handles_quotes_and_nesting() {
        let args = split_args("AT_FDCWD, \"/a,b(c)\", O_RDONLY, {flags=1, x=2}, [1, 2]");
        assert_eq!(args.len(), 5);
        assert_eq!(args[1], "\"/a,b(c)\"");
        assert_eq!(args[3], "{flags=1, x=2}");
    }

    #[test]
    fn mmap_patterns_are_normalised() {
        let trace =
            "1 mmap(NULL, 4096, PROT_WRITE|PROT_READ, MAP_ANONYMOUS|MAP_PRIVATE, -1, 0) = 0x1\n";
        let summary = parse_trace(trace);
        let stat = &summary.syscalls["mmap"];
        assert_eq!(stat.patterns["prot"]["PROT_READ|PROT_WRITE"], 1);
        assert_eq!(stat.patterns["flags"]["MAP_ANONYMOUS|MAP_PRIVATE"], 1);
    }

    #[test]
    fn socket_fcntl_ioctl_prctl_patterns() {
        let trace = "\
1 socket(AF_INET, SOCK_STREAM|SOCK_CLOEXEC, IPPROTO_IP) = 4
1 fcntl(4, F_GETFL) = 2
1 ioctl(1, TCGETS, {}) = 0
1 prctl(PR_SET_NAME, \"x\") = 0
";
        let summary = parse_trace(trace);
        assert_eq!(
            summary.syscalls["socket"].patterns["domain_type"]["AF_INET|SOCK_STREAM|SOCK_CLOEXEC"],
            1
        );
        assert_eq!(summary.syscalls["fcntl"].patterns["cmd"]["F_GETFL"], 1);
        assert_eq!(summary.syscalls["ioctl"].patterns["request"]["TCGETS"], 1);
        assert_eq!(
            summary.syscalls["prctl"].patterns["option"]["PR_SET_NAME"],
            1
        );
    }

    #[test]
    fn futex_epoll_signal_statx_getrandom_patterns() {
        let trace = "\
1 futex(0x1, FUTEX_WAIT_BITSET_PRIVATE|FUTEX_CLOCK_REALTIME, 2) = -1 EAGAIN
1 epoll_ctl(3, EPOLL_CTL_ADD, 4, 0x1) = 0
1 rt_sigaction(SIGPIPE, NULL, NULL, 8) = 0
1 statx(AT_FDCWD, \"/x\", 0, STATX_SIZE|STATX_MODE, 0x1) = 0
1 getrandom(0x1, 8, GRND_NONBLOCK) = 8
";
        let summary = parse_trace(trace);
        assert_eq!(
            summary.syscalls["futex"].patterns["op"]["FUTEX_WAIT_BITSET_PRIVATE|FUTEX_CLOCK_REALTIME"],
            1
        );
        assert_eq!(
            summary.syscalls["epoll_ctl"].patterns["op"]["EPOLL_CTL_ADD"],
            1
        );
        assert_eq!(
            summary.syscalls["rt_sigaction"].patterns["signal"]["SIGPIPE"],
            1
        );
        assert_eq!(
            summary.syscalls["statx"].patterns["mask"]["STATX_MODE|STATX_SIZE"],
            1
        );
        assert_eq!(
            summary.syscalls["getrandom"].patterns["flags"]["GRND_NONBLOCK"],
            1
        );
    }

    #[test]
    fn clone_patterns_are_extracted() {
        let trace = "1 clone(child_stack=0x1, flags=CLONE_VM|CLONE_THREAD|CLONE_SETTLS, parent_tid=[2]) = 2\n";
        let summary = parse_trace(trace);
        assert_eq!(
            summary.syscalls["clone"].patterns["flags"]["CLONE_SETTLS|CLONE_THREAD|CLONE_VM"],
            1
        );
    }

    #[test]
    fn clone3_patterns_are_extracted() {
        let trace = "1 clone3({flags=CLONE_VM|CLONE_THREAD, exit_signal=0}, 88) = 2\n";
        let summary = parse_trace(trace);
        assert_eq!(
            summary.syscalls["clone3"].patterns["flags"]["CLONE_THREAD|CLONE_VM"],
            1
        );
    }

    #[test]
    fn restart_results_are_not_errors() {
        let trace = "1 read(0, \"\", 4096) = ? ERESTARTSYS (To be restarted)\n";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["read"].count, 1);
        assert_eq!(summary.syscalls["read"].errors, 0);
    }

    #[test]
    fn unknown_syscall_name_still_counts() {
        let trace = "1 syscall_0x1b3(0x1, 0x2) = -1 ENOSYS\n";
        let summary = parse_trace(trace);
        assert_eq!(summary.syscalls["syscall_0x1b3"].count, 1);
        assert_eq!(summary.syscalls["syscall_0x1b3"].errors, 1);
    }

    #[test]
    fn synthetic_fixtures_parse() {
        for fixture in [
            include_str!("../tests/fixtures/hello-musl.strace"),
            include_str!("../tests/fixtures/threads-excerpt.strace"),
            include_str!("../tests/fixtures/tricky.strace"),
        ] {
            let summary = parse_trace(fixture);
            assert!(!summary.syscalls.is_empty());
        }
    }

    #[test]
    fn real_fixtures_parse_without_many_unparsed_lines() {
        for fixture in [
            include_str!("../tests/fixtures/real-static.strace"),
            include_str!("../tests/fixtures/real-dynamic.strace"),
            include_str!("../tests/fixtures/real-service.strace"),
            include_str!("../tests/fixtures/real-language.strace"),
            include_str!("../tests/fixtures/real-toolchain.strace"),
            include_str!("../tests/fixtures/real-shell.strace"),
            include_str!("../tests/fixtures/real-net.strace"),
        ] {
            let summary = parse_trace(fixture);
            assert!(!summary.syscalls.is_empty());
            assert!(
                summary.unparsed_lines <= 5,
                "unparsed {} lines",
                summary.unparsed_lines
            );
        }
    }
}
