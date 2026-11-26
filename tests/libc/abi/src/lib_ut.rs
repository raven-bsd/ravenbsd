// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Normative-value tests for the four libc ABI constant groups.
//!
//! Expected values are transcribed from musl `include/{poll,time,sys/stat,sys/time,locale,
//! dlfcn,sys/wait,sys/file,netdb,sched,sys/select,net/if}.h` and the matching POSIX clauses.
//! They are deliberately not read from the TOML: a TOML key with no expected row fails.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

type Expected = &'static [(&'static str, i128)];

const GROUP_PATHS: &[(&str, &str)] = &[
    ("misc", "libc/ravenlibc/abi/misc.toml"),
    ("poll", "libc/ravenlibc/abi/poll.toml"),
    ("stat", "libc/ravenlibc/abi/stat.toml"),
    ("time", "libc/ravenlibc/abi/time.toml"),
];

fn group_path(group: &str) -> &'static str {
    GROUP_PATHS
        .iter()
        .find(|(name, _)| *name == group)
        .map(|(_, path)| *path)
        .unwrap_or_else(|| panic!("{group}: no register path"))
}

const POLL_EXPECTED: Expected = &[
    ("POLLIN", 0x001),
    ("POLLPRI", 0x002),
    ("POLLOUT", 0x004),
    ("POLLERR", 0x008),
    ("POLLHUP", 0x010),
    ("POLLNVAL", 0x020),
    ("POLLRDNORM", 0x040),
    ("POLLRDBAND", 0x080),
    ("POLLWRNORM", 0x100),
    ("POLLWRBAND", 0x200),
    ("POLLMSG", 0x400),
    ("POLLRDHUP", 0x2000),
];

const TIME_EXPECTED: Expected = &[
    ("CLOCKS_PER_SEC", 1_000_000),
    ("TIME_UTC", 1),
    ("CLOCK_REALTIME", 0),
    ("CLOCK_MONOTONIC", 1),
    ("CLOCK_PROCESS_CPUTIME_ID", 2),
    ("CLOCK_THREAD_CPUTIME_ID", 3),
    ("CLOCK_MONOTONIC_RAW", 4),
    ("CLOCK_REALTIME_COARSE", 5),
    ("CLOCK_MONOTONIC_COARSE", 6),
    ("CLOCK_BOOTTIME", 7),
    ("CLOCK_REALTIME_ALARM", 8),
    ("CLOCK_BOOTTIME_ALARM", 9),
    ("CLOCK_SGI_CYCLE", 10),
    ("CLOCK_TAI", 11),
    ("CLOCK_UPTIME", 7),
    ("CLOCK_UPTIME_FAST", 7),
    ("CLOCK_UPTIME_PRECISE", 7),
    ("CLOCK_MONOTONIC_FAST", 1),
    ("CLOCK_MONOTONIC_PRECISE", 1),
    ("CLOCK_REALTIME_FAST", 0),
    ("CLOCK_REALTIME_PRECISE", 0),
    ("TIMER_ABSTIME", 1),
    ("ITIMER_REAL", 0),
    ("ITIMER_VIRTUAL", 1),
    ("ITIMER_PROF", 2),
];

const STAT_EXPECTED: Expected = &[
    ("S_IFMT", 0o170000),
    ("S_IFSOCK", 0o140000),
    ("S_IFLNK", 0o120000),
    ("S_IFREG", 0o100000),
    ("S_IFBLK", 0o060000),
    ("S_IFDIR", 0o040000),
    ("S_IFCHR", 0o020000),
    ("S_IFIFO", 0o010000),
    ("S_ISUID", 0o4000),
    ("S_ISGID", 0o2000),
    ("S_ISVTX", 0o1000),
    ("S_ISTXT", 0o1000),
    ("S_IRWXU", 0o700),
    ("S_IRUSR", 0o400),
    ("S_IWUSR", 0o200),
    ("S_IXUSR", 0o100),
    ("S_IRWXG", 0o070),
    ("S_IRGRP", 0o040),
    ("S_IWGRP", 0o020),
    ("S_IXGRP", 0o010),
    ("S_IRWXO", 0o007),
    ("S_IROTH", 0o004),
    ("S_IWOTH", 0o002),
    ("S_IXOTH", 0o001),
    ("S_IREAD", 0o400),
    ("S_IWRITE", 0o200),
    ("S_IEXEC", 0o100),
    ("DT_UNKNOWN", 0),
    ("DT_FIFO", 1),
    ("DT_CHR", 2),
    ("DT_DIR", 4),
    ("DT_BLK", 6),
    ("DT_REG", 8),
    ("DT_LNK", 10),
    ("DT_SOCK", 12),
    ("DT_WHT", 14),
    ("UTIME_NOW", 0x3fff_ffff),
    ("UTIME_OMIT", 0x3fff_fffe),
];

const MISC_EXPECTED: Expected = &[
    ("LC_CTYPE", 0),
    ("LC_NUMERIC", 1),
    ("LC_TIME", 2),
    ("LC_COLLATE", 3),
    ("LC_MONETARY", 4),
    ("LC_MESSAGES", 5),
    ("LC_ALL", 6),
    ("LC_CTYPE_MASK", 1),
    ("LC_NUMERIC_MASK", 2),
    ("LC_TIME_MASK", 4),
    ("LC_COLLATE_MASK", 8),
    ("LC_MONETARY_MASK", 16),
    ("LC_MESSAGES_MASK", 32),
    ("LC_ALL_MASK", 0x7fff_ffff),
    ("RTLD_LAZY", 1),
    ("RTLD_NOW", 2),
    ("RTLD_NOLOAD", 4),
    ("RTLD_NODELETE", 0x1000),
    ("RTLD_GLOBAL", 0x100),
    ("RTLD_LOCAL", 0),
    ("WNOHANG", 1),
    ("WUNTRACED", 2),
    ("WSTOPPED", 2),
    ("WEXITED", 4),
    ("WCONTINUED", 8),
    ("WNOWAIT", 0x0100_0000),
    ("LOCK_SH", 1),
    ("LOCK_EX", 2),
    ("LOCK_NB", 4),
    ("LOCK_UN", 8),
    ("EAI_BADFLAGS", -1),
    ("EAI_NONAME", -2),
    ("EAI_AGAIN", -3),
    ("EAI_FAIL", -4),
    ("EAI_NODATA", -5),
    ("EAI_FAMILY", -6),
    ("EAI_SOCKTYPE", -7),
    ("EAI_SERVICE", -8),
    ("EAI_MEMORY", -10),
    ("EAI_SYSTEM", -11),
    ("EAI_OVERFLOW", -12),
    ("SCHED_OTHER", 0),
    ("SCHED_FIFO", 1),
    ("SCHED_RR", 2),
    ("SCHED_BATCH", 3),
    ("SCHED_IDLE", 5),
    ("SCHED_DEADLINE", 6),
    ("FD_SETSIZE", 1024),
    ("IF_NAMESIZE", 16),
];

#[derive(Default)]
struct Def {
    name: String,
    value: Option<i128>,
    alias: Option<String>,
    shl: Option<(i128, i128)>,
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join(relative)
}

fn parse_int(raw: &str) -> i128 {
    let compact = raw.trim().replace('_', "");
    let (negative, body) = match compact.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, compact.as_str()),
    };
    let (radix, digits) =
        if let Some(rest) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
            (8, rest)
        } else if body.len() > 1 && body.starts_with('0') {
            (8, &body[1..])
        } else {
            (10, body)
        };
    let magnitude = i128::from_str_radix(digits, radix)
        .unwrap_or_else(|error| panic!("invalid integer {raw:?}: {error}"));
    if negative { -magnitude } else { magnitude }
}

fn split_fields(text: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut depth = 0i32;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            '[' | '(' if !quoted => {
                depth += 1;
                current.push(character);
            }
            ']' | ')' if !quoted => {
                depth -= 1;
                current.push(character);
            }
            ',' if !quoted && depth == 0 => {
                if !current.trim().is_empty() {
                    fields.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        fields.push(current.trim().to_string());
    }
    fields
}

fn parse_string(raw: &str) -> String {
    raw.trim().trim_matches('"').to_string()
}

fn parse_shl(raw: &str) -> (i128, i128) {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let mut parts = inner.split(',').map(parse_int);
    (
        parts
            .next()
            .unwrap_or_else(|| panic!("missing shift base in {raw:?}")),
        parts
            .next()
            .unwrap_or_else(|| panic!("missing shift amount in {raw:?}")),
    )
}

fn parse_group(group: &str) -> Vec<Def> {
    let path = repo_path(group_path(group));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let block = text
        .split_once("consts = [")
        .unwrap_or_else(|| panic!("{group}: no consts block"))
        .1
        .split_once("\n]")
        .unwrap_or_else(|| panic!("{group}: unterminated consts block"))
        .0;
    let mut definitions = Vec::new();
    for line in block.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let inner = line
            .trim_start_matches('{')
            .trim_end_matches(',')
            .trim_end_matches('}');
        let mut definition = Def::default();
        for field in split_fields(inner) {
            let (key, value) = field
                .split_once('=')
                .unwrap_or_else(|| panic!("{group}: malformed field {field:?}"));
            match key.trim() {
                "name" => definition.name = parse_string(value),
                "value" => definition.value = Some(parse_int(value)),
                "alias" => definition.alias = Some(parse_string(value)),
                "shl" => definition.shl = Some(parse_shl(value)),
                _ => {}
            }
        }
        assert!(
            !definition.name.is_empty(),
            "{group}: constant without a name"
        );
        definitions.push(definition);
    }
    definitions
}

fn expected_map(expected: Expected) -> BTreeMap<&'static str, i128> {
    expected.iter().copied().collect()
}

fn parse_header(group: &str) -> BTreeMap<String, String> {
    let path = repo_path(&format!(
        "libc/ravenlibc/abi/generated/include/bits/abi_{group}.h"
    ));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut defines = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut pieces = rest.split_whitespace();
        let Some(name) = pieces.next() else {
            continue;
        };
        let value = rest[name.len()..].trim();
        if name.contains('(') || value.is_empty() {
            continue;
        }
        defines.insert(name.to_string(), value.to_string());
    }
    defines
}

fn parse_c_number(raw: &str) -> Option<i128> {
    let (negative, body) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let (radix, digits) =
        if let Some(rest) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            (16, rest)
        } else if body.len() > 1 && body.starts_with('0') {
            (8, &body[1..])
        } else {
            (10, body)
        };
    if digits.is_empty() {
        return None;
    }
    i128::from_str_radix(digits, radix)
        .ok()
        .map(|value| if negative { -value } else { value })
}

fn resolve_header_value(raw: &str, expected: &BTreeMap<&'static str, i128>) -> i128 {
    let mut value = raw.trim();
    while value.starts_with('(') && value.ends_with(')') {
        value = value[1..value.len() - 1].trim();
    }
    if let Some((left, right)) = value.split_once("<<") {
        return resolve_header_value(left, expected) << resolve_header_value(right, expected);
    }
    let trimmed = value.trim_end_matches(['L', 'U', 'l', 'u']);
    if let Some(number) = parse_c_number(trimmed) {
        return number;
    }
    if let Some(number) = expected.get(trimmed) {
        return *number;
    }
    panic!("header value {raw:?} is neither numeric nor a known constant");
}

fn check_group(group: &str, expected: Expected, rust_all: &[(&str, i128)]) {
    let definitions = parse_group(group);
    let expected = expected_map(expected);
    let mut seen = BTreeSet::new();

    for definition in &definitions {
        let name = definition.name.as_str();
        let expected_value = *expected
            .get(name)
            .unwrap_or_else(|| panic!("{group}: TOML key {name} has no explicit expected row"));
        seen.insert(name.to_string());

        let actual = match (&definition.value, &definition.alias, &definition.shl) {
            (Some(value), None, None) => *value,
            (None, Some(alias), None) => *expected
                .get(alias.as_str())
                .unwrap_or_else(|| panic!("{group}: alias {name} -> {alias} has no expected row")),
            (None, None, Some((base, shift))) => base << shift,
            _ => panic!("{group}: {name} must have exactly one of value/alias/shl"),
        };
        assert_eq!(
            actual, expected_value,
            "{group}: TOML key {name} value {actual} != expected {expected_value}"
        );
        if let Some(alias) = &definition.alias {
            assert_eq!(
                actual,
                *expected
                    .get(alias.as_str())
                    .unwrap_or_else(|| panic!("{group}: alias target {alias} missing")),
                "{group}: alias {name} -> {alias} does not resolve to its expected value"
            );
        }
        if let Some((base, shift)) = definition.shl {
            assert_eq!(
                base << shift,
                expected_value,
                "{group}: shift expression for {name} does not match the expected value"
            );
        }
    }

    for name in expected.keys() {
        assert!(
            seen.contains(*name),
            "{group}: expected row {name} is not present in the TOML"
        );
    }

    let rust: BTreeMap<&str, i128> = rust_all.iter().copied().collect();
    assert_eq!(
        rust.len(),
        expected.len(),
        "{group}: raven-libc-abi ALL has a different key count"
    );
    for (name, value) in &expected {
        let actual = rust
            .get(*name)
            .unwrap_or_else(|| panic!("{group}: raven-libc-abi ALL is missing {name}"));
        assert_eq!(
            actual, value,
            "{group}: raven-libc-abi constant {name} = {actual} != expected {value}"
        );
    }

    let header = parse_header(group);
    for (name, value) in &expected {
        let raw = header
            .get(*name)
            .unwrap_or_else(|| panic!("{group}: generated header is missing {name}"));
        let actual = resolve_header_value(raw, &expected);
        assert_eq!(
            actual, *value,
            "{group}: generated header {name} = {raw} resolves to {actual} != expected {value}"
        );
    }
}

macro_rules! check_direct {
    ($module:ident, $($name:ident => $value:expr),+ $(,)?) => {
        $(
            assert_eq!(
                raven_libc_abi::$module::$name as i128,
                $value as i128,
                "raven-libc-abi {}::{} does not match the normative value",
                stringify!($module),
                stringify!($name),
            );
        )+
    };
}

#[test]
fn poll_group_matches_normative_source() {
    check_direct!(
        poll,
        POLLIN => 0x001,
        POLLPRI => 0x002,
        POLLOUT => 0x004,
        POLLERR => 0x008,
        POLLHUP => 0x010,
        POLLNVAL => 0x020,
        POLLRDNORM => 0x040,
        POLLRDBAND => 0x080,
        POLLWRNORM => 0x100,
        POLLWRBAND => 0x200,
        POLLMSG => 0x400,
        POLLRDHUP => 0x2000,
    );
    check_group("poll", POLL_EXPECTED, raven_libc_abi::poll::ALL);
}

#[test]
fn time_group_matches_normative_source() {
    check_direct!(
        time,
        CLOCKS_PER_SEC => 1_000_000,
        TIME_UTC => 1,
        CLOCK_REALTIME => 0,
        CLOCK_MONOTONIC => 1,
        CLOCK_PROCESS_CPUTIME_ID => 2,
        CLOCK_THREAD_CPUTIME_ID => 3,
        CLOCK_MONOTONIC_RAW => 4,
        CLOCK_REALTIME_COARSE => 5,
        CLOCK_MONOTONIC_COARSE => 6,
        CLOCK_BOOTTIME => 7,
        CLOCK_REALTIME_ALARM => 8,
        CLOCK_BOOTTIME_ALARM => 9,
        CLOCK_SGI_CYCLE => 10,
        CLOCK_TAI => 11,
        CLOCK_UPTIME => 7,
        CLOCK_UPTIME_FAST => 7,
        CLOCK_UPTIME_PRECISE => 7,
        CLOCK_MONOTONIC_FAST => 1,
        CLOCK_MONOTONIC_PRECISE => 1,
        CLOCK_REALTIME_FAST => 0,
        CLOCK_REALTIME_PRECISE => 0,
        TIMER_ABSTIME => 1,
        ITIMER_REAL => 0,
        ITIMER_VIRTUAL => 1,
        ITIMER_PROF => 2,
    );
    check_group("time", TIME_EXPECTED, raven_libc_abi::time::ALL);
}

#[test]
fn stat_group_matches_normative_source() {
    check_direct!(
        stat,
        S_IFMT => 0o170000,
        S_IFSOCK => 0o140000,
        S_IFLNK => 0o120000,
        S_IFREG => 0o100000,
        S_IFBLK => 0o060000,
        S_IFDIR => 0o040000,
        S_IFCHR => 0o020000,
        S_IFIFO => 0o010000,
        S_ISUID => 0o4000,
        S_ISGID => 0o2000,
        S_ISVTX => 0o1000,
        S_ISTXT => 0o1000,
        S_IRWXU => 0o700,
        S_IRUSR => 0o400,
        S_IWUSR => 0o200,
        S_IXUSR => 0o100,
        S_IRWXG => 0o070,
        S_IRGRP => 0o040,
        S_IWGRP => 0o020,
        S_IXGRP => 0o010,
        S_IRWXO => 0o007,
        S_IROTH => 0o004,
        S_IWOTH => 0o002,
        S_IXOTH => 0o001,
        S_IREAD => 0o400,
        S_IWRITE => 0o200,
        S_IEXEC => 0o100,
        DT_UNKNOWN => 0,
        DT_FIFO => 1,
        DT_CHR => 2,
        DT_DIR => 4,
        DT_BLK => 6,
        DT_REG => 8,
        DT_LNK => 10,
        DT_SOCK => 12,
        DT_WHT => 14,
        UTIME_NOW => 0x3fff_ffff,
        UTIME_OMIT => 0x3fff_fffe,
    );
    check_group("stat", STAT_EXPECTED, raven_libc_abi::stat::ALL);
}

#[test]
fn misc_group_matches_normative_source() {
    check_direct!(
        misc,
        LC_CTYPE => 0,
        LC_NUMERIC => 1,
        LC_TIME => 2,
        LC_COLLATE => 3,
        LC_MONETARY => 4,
        LC_MESSAGES => 5,
        LC_ALL => 6,
        LC_CTYPE_MASK => 1,
        LC_NUMERIC_MASK => 2,
        LC_TIME_MASK => 4,
        LC_COLLATE_MASK => 8,
        LC_MONETARY_MASK => 16,
        LC_MESSAGES_MASK => 32,
        LC_ALL_MASK => 0x7fff_ffff,
        RTLD_LAZY => 1,
        RTLD_NOW => 2,
        RTLD_NOLOAD => 4,
        RTLD_NODELETE => 0x1000,
        RTLD_GLOBAL => 0x100,
        RTLD_LOCAL => 0,
        WNOHANG => 1,
        WUNTRACED => 2,
        WSTOPPED => 2,
        WEXITED => 4,
        WCONTINUED => 8,
        WNOWAIT => 0x0100_0000,
        LOCK_SH => 1,
        LOCK_EX => 2,
        LOCK_NB => 4,
        LOCK_UN => 8,
        EAI_BADFLAGS => -1,
        EAI_NONAME => -2,
        EAI_AGAIN => -3,
        EAI_FAIL => -4,
        EAI_NODATA => -5,
        EAI_FAMILY => -6,
        EAI_SOCKTYPE => -7,
        EAI_SERVICE => -8,
        EAI_MEMORY => -10,
        EAI_SYSTEM => -11,
        EAI_OVERFLOW => -12,
        SCHED_OTHER => 0,
        SCHED_FIFO => 1,
        SCHED_RR => 2,
        SCHED_BATCH => 3,
        SCHED_IDLE => 5,
        SCHED_DEADLINE => 6,
        FD_SETSIZE => 1024,
        IF_NAMESIZE => 16,
    );
    check_group("misc", MISC_EXPECTED, raven_libc_abi::misc::ALL);
}
