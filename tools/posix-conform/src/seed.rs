// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Seeding of `docs/posix/profile.toml` and `docs/posix/traceability.toml`.
//!
//! The interface/header rows are derived from the committed ravenlibc census ring-0 names
//! (POSIX/C/XSI tags only). The utility, shell, filesystem, process and tty rows are curated
//! first-party requirement statements. No standard prose is copied.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};

use crate::model::{
    BehaviorRow, HeaderRow, InterfaceRow, Meta, Profile, STANDARD, ShellRow, Status, UtilityRow,
    read_ring0, read_symbols, trace_from_profile,
};

/// POSIX.1-2024 headers. Additions are identifiers, not standard prose.
const STANDARD_HEADERS: &[&str] = &[
    "aio.h",
    "arpa/inet.h",
    "assert.h",
    "complex.h",
    "cpio.h",
    "ctype.h",
    "dirent.h",
    "dlfcn.h",
    "errno.h",
    "fcntl.h",
    "fenv.h",
    "float.h",
    "fmtmsg.h",
    "fnmatch.h",
    "ftw.h",
    "glob.h",
    "grp.h",
    "iconv.h",
    "inttypes.h",
    "iso646.h",
    "langinfo.h",
    "libgen.h",
    "libintl.h",
    "limits.h",
    "locale.h",
    "math.h",
    "monetary.h",
    "mqueue.h",
    "ndbm.h",
    "net/if.h",
    "netdb.h",
    "netinet/in.h",
    "netinet/tcp.h",
    "nl_types.h",
    "poll.h",
    "pthread.h",
    "pwd.h",
    "regex.h",
    "sched.h",
    "search.h",
    "semaphore.h",
    "setjmp.h",
    "signal.h",
    "spawn.h",
    "stdarg.h",
    "stdatomic.h",
    "stdbool.h",
    "stddef.h",
    "stdint.h",
    "stdio.h",
    "stdlib.h",
    "string.h",
    "strings.h",
    "stropts.h",
    "sys/ipc.h",
    "sys/mman.h",
    "sys/msg.h",
    "sys/resource.h",
    "sys/select.h",
    "sys/sem.h",
    "sys/shm.h",
    "sys/socket.h",
    "sys/stat.h",
    "sys/statvfs.h",
    "sys/time.h",
    "sys/times.h",
    "sys/types.h",
    "sys/uio.h",
    "sys/un.h",
    "sys/utsname.h",
    "sys/wait.h",
    "syslog.h",
    "tar.h",
    "termios.h",
    "tgmath.h",
    "threads.h",
    "time.h",
    "trace.h",
    "uchar.h",
    "ulimit.h",
    "unistd.h",
    "utime.h",
    "utmpx.h",
    "wchar.h",
    "wctype.h",
    "wordexp.h",
];

/// Interface names that are language macros rather than `libc.a` symbols.
const MACRO_INTERFACES: &[&str] = &[
    "assert", "offsetof", "va_arg", "va_copy", "va_end", "va_start",
];

/// POSIX Shell and Utilities volume names; optional utilities are included.
const UTILITIES: &[&str] = &[
    "admin",
    "alias",
    "ar",
    "arch",
    "asa",
    "at",
    "awk",
    "b2sum",
    "base32",
    "base64",
    "basename",
    "basenc",
    "batch",
    "bc",
    "bg",
    "break",
    "c17",
    "c99",
    "cal",
    "cat",
    "cd",
    "cflow",
    "chgrp",
    "chmod",
    "chown",
    "cksum",
    "cmp",
    "cod",
    "comm",
    "command",
    "compress",
    "continue",
    "cp",
    "crontab",
    "csplit",
    "ctags",
    "cut",
    "cxref",
    "date",
    "dd",
    "delta",
    "df",
    "diff",
    "dircolors",
    "dirname",
    "dot",
    "du",
    "echo",
    "ed",
    "env",
    "eval",
    "ex",
    "exec",
    "exit",
    "expand",
    "export",
    "expr",
    "factor",
    "false",
    "fc",
    "fg",
    "file",
    "find",
    "fmt",
    "fold",
    "fort77",
    "gencat",
    "get",
    "getconf",
    "getopts",
    "grep",
    "groups",
    "hash",
    "head",
    "hostid",
    "iconv",
    "id",
    "install",
    "ipcrm",
    "ipcs",
    "jobs",
    "join",
    "kill",
    "lex",
    "link",
    "ln",
    "locale",
    "localedef",
    "logger",
    "logname",
    "lp",
    "ls",
    "m4",
    "mail",
    "mailx",
    "make",
    "man",
    "md5sum",
    "mesg",
    "mkdir",
    "mkfifo",
    "mknod",
    "mktemp",
    "more",
    "mv",
    "neqn",
    "newgrp",
    "nice",
    "nl",
    "nm",
    "nohup",
    "nproc",
    "numfmt",
    "od",
    "paste",
    "patch",
    "pathchk",
    "pax",
    "pinky",
    "pr",
    "printenv",
    "printf",
    "prs",
    "ps",
    "ptx",
    "pwd",
    "qalter",
    "qdel",
    "qhold",
    "qmove",
    "qmsg",
    "qrerun",
    "qrls",
    "qselect",
    "qsig",
    "qstat",
    "qsub",
    "read",
    "readlink",
    "readonly",
    "realpath",
    "renice",
    "return",
    "rm",
    "rmdel",
    "rmdir",
    "sact",
    "sccs",
    "sed",
    "seq",
    "set",
    "sh",
    "sha1sum",
    "sha224sum",
    "sha256sum",
    "sha384sum",
    "sha512sum",
    "shift",
    "shred",
    "shuf",
    "sleep",
    "sort",
    "split",
    "stat",
    "stdbuf",
    "strings",
    "strip",
    "stty",
    "sum",
    "sync",
    "tabs",
    "tac",
    "tail",
    "talk",
    "tar",
    "tee",
    "test",
    "time",
    "timeout",
    "times",
    "touch",
    "tput",
    "tr",
    "trap",
    "true",
    "truncate",
    "tsort",
    "tty",
    "type",
    "ulimit",
    "umask",
    "unalias",
    "uname",
    "uncompress",
    "unexpand",
    "unget",
    "uniq",
    "unlink",
    "unset",
    "uptime",
    "users",
    "uucp",
    "uudecode",
    "uuencode",
    "uustat",
    "uux",
    "val",
    "vi",
    "wait",
    "wc",
    "what",
    "who",
    "write",
    "xargs",
    "yacc",
    "yes",
    "zcat",
];

pub(crate) fn posix_hidden_symbols(root: &Path) -> BTreeSet<String> {
    let path = root.join("tests/libc/headers/probes.toml");
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeSet::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return BTreeSet::new();
    };
    let mut symbols = BTreeSet::new();
    let Some(probes) = value.get("probe").and_then(toml::Value::as_array) else {
        return symbols;
    };
    for probe in probes {
        let hidden = probe
            .get("hidden_in")
            .and_then(toml::Value::as_array)
            .is_some_and(|modes| modes.iter().any(|mode| mode.as_str() == Some("posix")));
        if hidden && let Some(name) = probe.get("symbol").and_then(toml::Value::as_str) {
            symbols.insert(name.to_string());
        }
    }
    symbols
}

fn normalize_header(header: &str) -> Option<&'static str> {
    let known = match header {
        "malloc.h" => "stdlib.h",
        "limits.h (optional)" => "limits.h",
        other => other,
    };
    STANDARD_HEADERS
        .iter()
        .copied()
        .find(|candidate| *candidate == known)
}

fn fallback_header(name: &str, kind: &str) -> &'static str {
    if name.starts_with("mq_") {
        return "mqueue.h";
    }
    if name.starts_with("msg") {
        return "sys/msg.h";
    }
    if name.starts_with("sem") {
        return "sys/sem.h";
    }
    if name.starts_with("shm") {
        return "sys/shm.h";
    }
    // `catan`/`catanh` are complex math, not message catalogs: the `cat` prefix must not
    // misclassify them into `<nl_types.h>` (the census assigns them to `m`/`complex.h`).
    if name.starts_with("catan") {
        return "complex.h";
    }
    if name.starts_with("cat") {
        return "nl_types.h";
    }
    if name.starts_with("timer_") {
        return "time.h";
    }
    if name.starts_with("strfmon") {
        return "monetary.h";
    }
    if name.starts_with("tcget") || name.starts_with("tcset") {
        return "termios.h";
    }
    if name.starts_with("wcs") || name == "open_wmemstream" {
        return "wchar.h";
    }
    if name == "sockaddr_un" {
        return "sys/un.h";
    }
    if name == "pselect" {
        return "sys/select.h";
    }
    if name == "psiginfo" {
        return "signal.h";
    }
    if matches!(
        name,
        "readlinkat" | "symlinkat" | "linkat" | "pipe2" | "usleep"
    ) {
        return "unistd.h";
    }
    if name == "posix_fadvise" || name == "posix_fallocate" {
        return "fcntl.h";
    }
    if name == "posix_madvise" {
        return "sys/mman.h";
    }
    if name == "quick_exit" {
        return "stdlib.h";
    }
    if name == "fstatvfs" {
        return "sys/statvfs.h";
    }
    if name.starts_with("pthread_") {
        return "pthread.h";
    }
    if name.starts_with("cabs")
        || name.starts_with("cacos")
        || name.starts_with("carg")
        || name.starts_with("casin")
        || name.starts_with("catan")
        || name.starts_with("ccos")
        || name.starts_with("cexp")
        || name.starts_with("cimag")
        || name.starts_with("clog")
        || name.starts_with("conj")
        || name.starts_with("cpow")
        || name.starts_with("cproj")
        || name.starts_with("creal")
        || name.starts_with("csin")
        || name.starts_with("csqrt")
        || name.starts_with("ctan")
        || name.starts_with("cacosh")
        || name.starts_with("casinh")
        || name.starts_with("catanh")
        || name.starts_with("ccosh")
        || name.starts_with("csinh")
        || name.starts_with("ctanh")
    {
        return "complex.h";
    }
    if name.starts_with("c16rtomb")
        || name.starts_with("c32rtomb")
        || name.starts_with("mbrtoc16")
        || name.starts_with("mbrtoc32")
        || name.starts_with("mbrtoc8")
        || name.starts_with("c8rtomb")
    {
        return "uchar.h";
    }
    if name == "LC_GLOBAL_LOCALE" {
        return "locale.h";
    }
    if name == "NOEXPR" || name == "YESEXPR" {
        return "langinfo.h";
    }
    if name == "at_quick_exit" {
        return "stdlib.h";
    }
    if name == "ftok" {
        return "sys/ipc.h";
    }
    if name.starts_with("fchmodat")
        || name.starts_with("fchownat")
        || name.starts_with("fstatat")
        || name.starts_with("fstatvfs")
        || name.starts_with("lchown")
        || name.starts_with("mkfifoat")
        || name.starts_with("mknodat")
        || name == "lockf"
    {
        return "sys/stat.h";
    }
    if name.starts_with("clock_getcpuclockid") {
        return "time.h";
    }
    if kind == "macro" {
        return "limits.h";
    }
    "math.h"
}

fn meta(id: String, status: Status, evidence: &str, owner: &str, notes: &str, pg: &[&str]) -> Meta {
    Meta {
        id,
        standard: STANDARD.to_string(),
        status,
        evidence: evidence.to_string(),
        owner: owner.to_string(),
        waiver: String::new(),
        notes: notes.to_string(),
        pg: pg.iter().map(|value| (*value).to_string()).collect(),
    }
}

fn header_evidence(root: &Path, header: &str) -> (&'static str, Status) {
    if root.join("libc/ravenlibc/include").join(header).is_file() {
        ("libc/ravenlibc/include", Status::Partial)
    } else {
        ("none", Status::Missing)
    }
}

fn utility_row(root: &Path, name: &str) -> UtilityRow {
    let candidate = if name == "sh" {
        root.join("userland/bin/rsh")
    } else {
        root.join("userland/bin").join(name)
    };
    let (status, evidence) = if candidate.is_dir() {
        (
            Status::Partial,
            candidate
                .strip_prefix(root)
                .unwrap_or(&candidate)
                .display()
                .to_string(),
        )
    } else {
        (Status::Missing, "none".to_string())
    };
    let test = if name == "ps" {
        "tests/qtest/cases/modern-json-tools.toml"
    } else {
        "none"
    };
    UtilityRow {
        meta: meta(
            format!("util.{name}"),
            status,
            if evidence.is_empty() {
                "none"
            } else {
                &evidence
            },
            "P4",
            if name == "sh" {
                "the rescue shell rsh is not a POSIX sh"
            } else {
                "the utility name is not proof of POSIX conformance"
            },
            &[],
        ),
        name: name.to_string(),
        options: "standard option set".to_string(),
        exit_status: "0 success; >0 failure; 2 usage".to_string(),
        locale: "POSIX locale categories and LC_ALL".to_string(),
        test: test.to_string(),
    }
}

fn shell_row(
    name: &str,
    description: &str,
    status: Status,
    evidence: &str,
    pg: &[&str],
) -> ShellRow {
    ShellRow {
        meta: meta(
            format!("shell.{name}"),
            status,
            evidence,
            "P4",
            "shell rows are grammar/behaviour requirements, not utility names",
            pg,
        ),
        name: name.to_string(),
        description: description.to_string(),
    }
}

fn behavior(
    area: &str,
    name: &str,
    description: &str,
    status: Status,
    evidence: &str,
    owner: &str,
    pg: &[&str],
) -> BehaviorRow {
    BehaviorRow {
        meta: meta(
            format!("{area}.{name}"),
            status,
            evidence,
            owner,
            "behaviour row from the POSIX.1-2024 profile",
            pg,
        ),
        name: name.to_string(),
        description: description.to_string(),
    }
}

/// Build the seed profile for `root`.
pub fn seed_profile(root: &Path) -> Result<Profile> {
    let symbols = read_symbols(root)?;
    let ring0 = read_ring0(root)?;
    let posix_hidden = posix_hidden_symbols(root);

    /// Heuristic first pass over a POSIX header: one-line prototypes ending in `;`.
    fn parse_header_names(root: &Path, header: &str) -> (Vec<String>, Vec<String>) {
        let path = root.join("libc/ravenlibc/include").join(header);
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Vec::new(), Vec::new());
        };
        let keywords = [
            "void", "int", "char", "long", "short", "float", "double", "signed", "unsigned",
            "const", "struct", "union", "enum", "return", "sizeof",
        ];
        let mut functions = Vec::new();
        let mut macros = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#')
                || line.starts_with("typedef")
                || line.starts_with("static")
                || !line.ends_with(';')
                || !line.contains('(')
            {
                continue;
            }
            let Some(before) = line.split('(').next() else {
                continue;
            };
            let Some(name) = before.split_whitespace().last() else {
                continue;
            };
            if name.starts_with("__")
                || name.ends_with("_np")
                || keywords.contains(&name)
                || !name
                    .chars()
                    .next()
                    .is_some_and(|value| value.is_ascii_alphabetic() || value == '_')
            {
                continue;
            }
            if name
                .chars()
                .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || value == '_')
            {
                macros.push(name.to_string());
            } else {
                functions.push(name.to_string());
            }
        }
        (functions, macros)
    }
    let mut header_functions: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let mut header_macros: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let mut header_types: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let mut interfaces: BTreeMap<String, (String, Status)> = BTreeMap::new();

    for (name, kind, tags, census_header) in ring0 {
        if !crate::model::is_posix_relevant(&tags) {
            continue;
        }
        let header = census_header
            .as_deref()
            .and_then(normalize_header)
            .unwrap_or_else(|| fallback_header(&name, &kind));
        let normalized = normalize_header(header).unwrap_or("unistd.h");
        let function_like = kind == "func"
            && !name
                .chars()
                .all(|value| value.is_ascii_uppercase() || value.is_ascii_digit() || value == '_');
        match if function_like { "func" } else { "macro" } {
            "func" => {
                header_functions
                    .entry(normalized)
                    .or_default()
                    .insert(name.clone());
                let status = if symbols.contains(&name) {
                    Status::Present
                } else {
                    Status::Missing
                };
                interfaces
                    .entry(name)
                    .or_insert_with(|| (normalized.to_string(), status));
            }
            "data" => {
                header_types.entry(normalized).or_default().insert(name);
            }
            _ => {
                header_macros.entry(normalized).or_default().insert(name);
            }
        }
    }

    for header in STANDARD_HEADERS {
        let (functions, macros) = parse_header_names(root, header);
        for name in functions {
            if posix_hidden.contains(&name) {
                continue;
            }
            header_functions
                .entry(*header)
                .or_default()
                .insert(name.clone());
            let status = if symbols.contains(&name) {
                Status::Present
            } else {
                Status::Missing
            };
            interfaces
                .entry(name)
                .or_insert_with(|| ((*header).to_string(), status));
        }
        for name in macros {
            header_macros.entry(*header).or_default().insert(name);
        }
    }

    let mut header_rows = Vec::new();
    for header in STANDARD_HEADERS {
        let functions: Vec<String> = header_functions
            .get(header)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        let macros: Vec<String> = header_macros
            .get(header)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        let types: Vec<String> = header_types
            .get(header)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();
        let exists = root.join("libc/ravenlibc/include").join(header).is_file();
        let all_exported = functions.iter().all(|name| symbols.contains(name));
        let status = if !exists {
            Status::Missing
        } else if all_exported {
            Status::Present
        } else {
            Status::Partial
        };
        let (evidence, _) = header_evidence(root, header);
        header_rows.push(HeaderRow {
            meta: meta(
                format!("header.{}", header.replace('/', "-")),
                status,
                evidence,
                "P1",
                "header presence is file-level evidence; behavior is P1",
                &[],
            ),
            name: (*header).to_string(),
            required_macros: macros,
            required_types: types,
            required_functions: functions,
        });
    }

    let mut interface_rows = Vec::new();
    for (name, (header, status)) in interfaces {
        let evidence = if status == Status::Present {
            "libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols"
        } else {
            "none"
        };
        let kind = if MACRO_INTERFACES.contains(&name.as_str()) {
            "macro"
        } else {
            "func"
        };
        interface_rows.push(InterfaceRow {
            meta: meta(
                format!("interface.{name}"),
                status,
                evidence,
                "P1",
                "export presence is file-level evidence; behavior is P1",
                &[],
            ),
            name,
            header,
            kind: kind.to_string(),
            required: true,
            signature: "declared by the named header".to_string(),
        });
    }
    interface_rows.sort_by(|left, right| left.name.cmp(&right.name));

    let mut utility_rows: Vec<UtilityRow> = UTILITIES
        .iter()
        .map(|name| utility_row(root, name))
        .collect();
    utility_rows.sort_by(|left, right| left.name.cmp(&right.name));

    let shell_features = vec![
        shell_row(
            "simple-command",
            "run a command with arguments and assignments",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "pipeline",
            "connect commands with pipes",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "list",
            "sequence and asynchronous lists",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "compound-command",
            "brace and subshell groups",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "function",
            "function definition and invocation",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "parameter-expansion",
            "POSIX parameter expansion forms",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "command-substitution",
            "command substitution in words",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "arithmetic-expansion",
            "arithmetic expansion",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "field-splitting",
            "IFS field splitting",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "pathname-expansion",
            "glob expansion",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "quote-removal",
            "quote removal",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "redirection",
            "input/output/append/dup redirection",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "here-document",
            "here-document redirection",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "special-builtin",
            "special built-in utilities and their exit rules",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "regular-builtin",
            "regular built-in utility dispatch",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "set-options",
            "set -e/-u/-x and positional parameters",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "loop",
            "for/while/until loops",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "conditional",
            "if and case conditionals",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "trap",
            "signal traps and EXIT",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row(
            "job-control",
            "background jobs, fg/bg/jobs and terminal control",
            Status::Missing,
            "none",
            &["PG-56"],
        ),
        shell_row(
            "interactive-shell",
            "interactive prompt and command reading",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "exit-status",
            "exit status propagation and 128+signal",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "shell-variable",
            "shell variables and environment import/export",
            Status::Partial,
            "userland/bin/rsh",
            &[],
        ),
        shell_row(
            "getopts",
            "getopts option processing",
            Status::Missing,
            "none",
            &[],
        ),
        shell_row("umask", "umask built-in", Status::Missing, "none", &[]),
    ];

    let filesystem = vec![
        behavior(
            "filesystem",
            "path-resolution",
            "dot, dot-dot, repeated separators and trailing slash",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-33"],
        ),
        behavior(
            "filesystem",
            "symlink",
            "symbolic link creation, following and loops",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-31", "PG-36"],
        ),
        behavior(
            "filesystem",
            "permission",
            "owner/group/other permission bits and access checks",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-23", "PG-33"],
        ),
        behavior(
            "filesystem",
            "umask",
            "creation mask on every create operation",
            Status::Missing,
            "none",
            "P5",
            &["PG-24"],
        ),
        behavior(
            "filesystem",
            "ownership",
            "chown/lchown/fchown and group rules",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-33"],
        ),
        behavior(
            "filesystem",
            "timestamp",
            "atime/mtime/ctime and utimensat UTIME_NOW/OMIT",
            Status::Partial,
            "none",
            "P5",
            &["PG-30"],
        ),
        behavior(
            "filesystem",
            "hard-link",
            "link/unlink and link count",
            Status::Partial,
            "none",
            "P5",
            &["PG-31"],
        ),
        behavior(
            "filesystem",
            "rename",
            "rename across directories, replacement and directory rules",
            Status::Partial,
            "none",
            "P5",
            &["PG-31"],
        ),
        behavior(
            "filesystem",
            "directory",
            "opendir/readdir/closedir and directory offsets",
            Status::Partial,
            "crates/raven-vfs-core",
            "P5",
            &["PG-30"],
        ),
        behavior(
            "filesystem",
            "mkdir-rmdir",
            "mkdir/rmdir permissions and empty-directory rule",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-31"],
        ),
        behavior(
            "filesystem",
            "truncate",
            "truncate/ftruncate and sparse extension",
            Status::Missing,
            "none",
            "P5",
            &["PG-32"],
        ),
        behavior(
            "filesystem",
            "record-lock",
            "fcntl record locks and close-on-exec",
            Status::Missing,
            "none",
            "P5",
            &["PG-32"],
        ),
        behavior(
            "filesystem",
            "stat-family",
            "stat/lstat/fstat/statat and mode/file-type fields",
            Status::Partial,
            "crates/raven-vfs-core",
            "P5",
            &["PG-30"],
        ),
        behavior(
            "filesystem",
            "statvfs",
            "filesystem capacity and block fields",
            Status::Partial,
            "none",
            "P5",
            &["PG-30"],
        ),
        behavior(
            "filesystem",
            "access",
            "access/faccessat and real/effective identity",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-33"],
        ),
        behavior(
            "filesystem",
            "mmap-file",
            "private/shared file mappings and pager hooks",
            Status::Partial,
            "kernel/src/memory/uspace.rs",
            "P5",
            &["PG-36", "PG-13"],
        ),
        behavior(
            "filesystem",
            "sync",
            "fsync/fdatasync and durable write ordering",
            Status::Missing,
            "none",
            "P5",
            &["PG-32"],
        ),
        behavior(
            "filesystem",
            "special-file",
            "mkfifo/mknod and device nodes",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-34", "PG-35"],
        ),
        behavior(
            "filesystem",
            "mount",
            "mount table, mount points and visibility",
            Status::Partial,
            "userland/servers/vfsd",
            "P5",
            &["PG-37"],
        ),
        behavior(
            "filesystem",
            "sparse-file",
            "holes, seek and block accounting",
            Status::Missing,
            "none",
            "P5",
            &["PG-32"],
        ),
    ];

    let process = vec![
        behavior(
            "process",
            "fork",
            "copy-on-write process creation",
            Status::Present,
            "kernel/src/syscall/process_ctl.rs",
            "P2",
            &["PG-45"],
        ),
        behavior(
            "process",
            "exec",
            "image replacement and close-on-exec",
            Status::Missing,
            "none",
            "P2",
            &["PG-44", "PG-46"],
        ),
        behavior(
            "process",
            "wait",
            "wait/waitpid/waitid status and reaping",
            Status::Partial,
            "kernel/src/syscall/process_ctl.rs",
            "P2",
            &["PG-44"],
        ),
        behavior(
            "process",
            "session",
            "setsid and session membership",
            Status::Missing,
            "none",
            "P2",
            &["PG-44", "PG-56"],
        ),
        behavior(
            "process",
            "process-group",
            "setpgid/getpgid and group signalling",
            Status::Missing,
            "none",
            "P2",
            &["PG-44", "PG-56"],
        ),
        behavior(
            "process",
            "credential",
            "uid/gid/groups and setuid/setgid semantics",
            Status::Missing,
            "none",
            "P2",
            &["PG-44"],
        ),
        behavior(
            "process",
            "signal",
            "POSIX signal sets, dispositions and delivery",
            Status::Partial,
            "kernel/src/syscall/thread.rs",
            "P2",
            &["PG-7", "PG-8"],
        ),
        behavior(
            "process",
            "job-control",
            "stop/continue, foreground group and terminal owner",
            Status::Missing,
            "none",
            "P2",
            &["PG-10", "PG-56"],
        ),
        behavior(
            "process",
            "priority",
            "nice/getpriority/setpriority and scheduling classes",
            Status::Missing,
            "none",
            "P2",
            &["PG-11"],
        ),
        behavior(
            "process",
            "resource",
            "getrlimit/setrlimit and resource accounting",
            Status::Missing,
            "none",
            "P2",
            &["PG-18"],
        ),
        behavior(
            "process",
            "process-id",
            "pid/ppid/pgrp/sid identity and reuse rules",
            Status::Missing,
            "none",
            "P2",
            &["PG-44"],
        ),
        behavior(
            "process",
            "terminate",
            "kill/raise and exit-status encoding",
            Status::Partial,
            "kernel/src/syscall/process_ctl.rs",
            "P2",
            &["PG-44"],
        ),
        behavior(
            "process",
            "posix-spawn",
            "posix_spawn file actions and attributes",
            Status::Partial,
            "libc/ravenlibc/crates/raven-libc/src/process/posix_spawn.rs",
            "P2",
            &["PG-49"],
        ),
        behavior(
            "process",
            "system",
            "system() shell invocation and status",
            Status::Missing,
            "none",
            "P2",
            &[],
        ),
        behavior(
            "process",
            "popen",
            "popen/pclose pipe and shell",
            Status::Missing,
            "none",
            "P2",
            &["PG-35"],
        ),
        behavior(
            "process",
            "times",
            "times/tms and CPU accounting",
            Status::Partial,
            "none",
            "P2",
            &["PG-18"],
        ),
        behavior(
            "process",
            "tty-control",
            "tcgetpgrp/tcsetpgrp and controlling terminal",
            Status::Missing,
            "none",
            "P2",
            &["PG-56"],
        ),
    ];

    let tty = vec![
        behavior(
            "tty",
            "termios",
            "termios get/set and speed fields",
            Status::Partial,
            "libc/ravenlibc/include/termios.h",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "canonical",
            "canonical input, line editing and EOF",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "raw",
            "non-canonical/raw input and VMIN/VTIME",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "echo",
            "echo, echoctl and echo processing",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "control-character",
            "VINTR/VQUIT/VSUSP/VERASE/VKILL and signals",
            Status::Missing,
            "none",
            "P3",
            &["PG-54", "PG-56"],
        ),
        behavior(
            "tty",
            "flow-control",
            "IXON/IXOFF and output flow",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "line-discipline",
            "line discipline and input/output queues",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "controlling-terminal",
            "session leader and controlling terminal acquisition",
            Status::Missing,
            "none",
            "P3",
            &["PG-56"],
        ),
        behavior(
            "tty",
            "pty",
            "pty allocation and master/slave pairs",
            Status::Missing,
            "none",
            "P3",
            &["PG-55"],
        ),
        behavior(
            "tty",
            "ttyname",
            "ttyname/isatty identity",
            Status::Partial,
            "libc/ravenlibc/include/unistd.h",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "stty",
            "stty terminal settings utility",
            Status::Missing,
            "none",
            "P3",
            &["PG-54"],
        ),
        behavior(
            "tty",
            "job-control-glue",
            "foreground/background terminal handoff",
            Status::Missing,
            "none",
            "P3",
            &["PG-56"],
        ),
    ];

    let mut profile = Profile {
        schema: 1,
        standard: STANDARD.to_string(),
        platform: crate::model::PlatformStatus::new(),
        header: header_rows,
        interface: interface_rows,
        utility: utility_rows,
        shell_feature: shell_features,
        filesystem,
        process,
        tty,
    };
    profile
        .header
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .interface
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .utility
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .shell_feature
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .filesystem
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .process
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    profile
        .tty
        .sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    Ok(profile)
}

/// Recompute header status from the (possibly overlaid) interface statuses.
///
/// The seed derives header rows before the owner overlays run, so a header whose missing
/// interfaces are later waived or closed by executable evidence would stay `partial`. A header
/// is present when every required function is present or waived in the profile and the header
/// file exists; a header with an explicit overlay claim or a missing file keeps its status.
pub fn reconcile_header_status(profile: &mut Profile, claimed: &BTreeSet<String>) {
    let statuses: BTreeMap<&str, Status> = profile
        .interface
        .iter()
        .map(|row| (row.name.as_str(), row.meta.status))
        .collect();
    for header in &mut profile.header {
        if claimed.contains(&header.meta.id) || header.meta.status == Status::Missing {
            continue;
        }
        let all = header.required_functions.iter().all(|name| {
            matches!(
                statuses.get(name.as_str()).copied(),
                Some(Status::Present | Status::Waived)
            )
        });
        header.meta.status = if all {
            Status::Present
        } else {
            Status::Partial
        };
    }
}

/// Write the seed profile, traceability file and generated gap matrix.
///
/// The owner overlays under the management `posix/evidence/` are merged after the derivation.
/// Unsupported claims are ignored here; the gate reports them as failures.
pub fn seed(root: &Path) -> Result<()> {
    seed_at(root, &crate::mgmt::posix_dir(root)?)
}

pub(crate) fn seed_at(root: &Path, posix: &Path) -> Result<()> {
    let mut profile = seed_profile(root)?;
    let overlay = crate::overlay::Overlay::load_at(posix)?;
    let outcome = overlay.apply(root, &mut profile);
    for claim in &outcome.unsupported {
        eprintln!(
            "posix-conform: overlay row `{}` ignored: {}",
            claim.id, claim.reason
        );
    }
    let claimed: BTreeSet<String> = outcome
        .claims
        .iter()
        .map(|claim| claim.id().to_string())
        .collect();
    reconcile_header_status(&mut profile, &claimed);
    let trace = trace_from_profile(&profile);
    let dir = posix.to_path_buf();
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    std::fs::write(
        dir.join("profile.toml"),
        crate::model::render_profile(&profile)?,
    )
    .context("cannot write profile.toml")?;
    std::fs::write(
        dir.join("traceability.toml"),
        crate::model::render_trace(&trace)?,
    )
    .context("cannot write traceability.toml")?;
    let gaps = crate::render::render_gaps(root, &profile, &trace)?;
    std::fs::write(dir.join("GAPS.md"), gaps).context("cannot write GAPS.md")?;
    Ok(())
}

#[cfg(test)]
#[path = "seed_ut.rs"]
mod tests;
