// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Assertions for the toolchain ABI comparison.

use super::*;

const PATCH: &str = include_str!("../../../../toolchain/patches/0002-libc-ravenbsd.patch");

fn assert_clean(findings: Vec<String>, label: &str) {
    assert!(
        findings.is_empty(),
        "{label} mismatches:\n{}",
        findings.join("\n")
    );
}

#[test]
fn open_flags_match_the_abi() {
    assert_clean(compare_prefix("O_", ABI_FCNTL, TOOLCHAIN_LIBC), "O_*");
}

#[test]
fn at_flags_match_the_abi() {
    assert_clean(compare_prefix("AT_", ABI_FCNTL, TOOLCHAIN_LIBC), "AT_*");
}

#[test]
fn fcntl_commands_match_the_abi() {
    assert_clean(compare_prefix("F_", ABI_FCNTL, TOOLCHAIN_LIBC), "F_*");
}

#[test]
fn fd_cloexec_matches_the_abi() {
    assert_clean(
        compare_required(&["FD_CLOEXEC"], ABI_FCNTL, TOOLCHAIN_LIBC),
        "FD_CLOEXEC",
    );
}

#[test]
fn errno_values_used_by_std_match_the_abi() {
    const ERRORS: &[&str] = &[
        "E2BIG",
        "EACCES",
        "EAGAIN",
        "EBADF",
        "EBUSY",
        "EEXIST",
        "EFAULT",
        "EFBIG",
        "EINTR",
        "EINVAL",
        "EIO",
        "EISDIR",
        "ELOOP",
        "EMFILE",
        "ENAMETOOLONG",
        "ENFILE",
        "ENOENT",
        "ENOMEM",
        "ENOSPC",
        "ENOTDIR",
        "ENOTEMPTY",
        "ENOTTY",
        "ENXIO",
        "EOPNOTSUPP",
        "EOVERFLOW",
        "EPIPE",
        "EROFS",
        "ESPIPE",
        "EXDEV",
    ];
    assert_clean(compare_required(ERRORS, ABI_ERRNO, TOOLCHAIN_LIBC), "errno");
}

#[test]
fn socket_constants_match_the_abi() {
    const SOCKET: &[&str] = &[
        "AF_UNIX",
        "AF_INET",
        "AF_INET6",
        "SOCK_STREAM",
        "SOCK_DGRAM",
        "SOCK_RAW",
        "SOCK_SEQPACKET",
        "SOL_SOCKET",
        "SO_REUSEADDR",
        "SO_ERROR",
        "SHUT_RD",
        "SHUT_WR",
        "SHUT_RDWR",
    ];
    let mut findings = compare_required(SOCKET, ABI_SOCKET, TOOLCHAIN_LIBC);
    findings.extend(compare_required(
        &[
            "IPPROTO_IP",
            "IPPROTO_TCP",
            "IPPROTO_UDP",
            "IPPROTO_ICMP",
            "IPPROTO_IPV6",
        ],
        ABI_NETINET,
        TOOLCHAIN_LIBC,
    ));
    assert_clean(findings, "socket");
}

#[test]
fn termios_constants_match_the_header() {
    const TERMIOS: &[&str] = &[
        "VINTR",
        "VQUIT",
        "VERASE",
        "VKILL",
        "VEOF",
        "VTIME",
        "VMIN",
        "VSWTC",
        "VSTART",
        "VSTOP",
        "VSUSP",
        "VEOL",
        "VREPRINT",
        "VDISCARD",
        "VWERASE",
        "VLNEXT",
        "VEOL2",
        "IGNBRK",
        "BRKINT",
        "IGNPAR",
        "PARMRK",
        "INPCK",
        "ISTRIP",
        "INLCR",
        "IGNCR",
        "ICRNL",
        "IXON",
        "IXANY",
        "IXOFF",
        "OPOST",
        "ONLCR",
        "OCRNL",
        "ONOCR",
        "ONLRET",
        "CSIZE",
        "CS5",
        "CS6",
        "CS7",
        "CS8",
        "CSTOPB",
        "CREAD",
        "PARENB",
        "PARODD",
        "HUPCL",
        "CLOCAL",
        "ISIG",
        "ICANON",
        "ECHO",
        "ECHOE",
        "ECHOK",
        "ECHONL",
        "NOFLSH",
        "TOSTOP",
        "IEXTEN",
        "ECHOCTL",
        "ECHOPRT",
        "ECHOKE",
        "FLUSHO",
        "PENDIN",
        "EXTPROC",
        "TCSANOW",
        "TCSADRAIN",
        "TCSAFLUSH",
        "TCIFLUSH",
        "TCOFLUSH",
        "TCIOFLUSH",
        "TCOOFF",
        "TCOON",
        "TCIOFF",
        "TCION",
    ];
    assert_clean(compare_termios(TERMIOS), "termios");
}

#[test]
fn scalar_types_match_the_abi() {
    const TYPES: &[(&str, (u32, bool))] = &[
        ("mode_t", (32, false)),
        ("uid_t", (32, false)),
        ("gid_t", (32, false)),
        ("nlink_t", (32, false)),
        ("blksize_t", (32, true)),
        ("off_t", (64, true)),
        ("time_t", (64, true)),
        ("suseconds_t", (64, true)),
    ];
    assert_clean(compare_types(TYPES), "types");
}

#[test]
fn stat_layout_matches_the_abi() {
    const FIELDS: &[&str] = &[
        "u64", "u64", "u32", "u32", "u32", "u32", "u64", "u64", "i64", "i32", "i32", "i64", "i64",
        "i64", "i64", "i64", "i64", "i64", "[u32; 2]",
    ];
    assert_clean(compare_struct("stat", FIELDS), "stat");
}

#[test]
fn statvfs_layout_matches_the_abi() {
    const FIELDS: &[&str] = &[
        "u64", "u64", "u64", "u64", "u64", "u64", "u64", "u64", "u64", "u64", "u64", "u32",
        "[i32; 5]",
    ];
    assert_clean(compare_struct("statvfs", FIELDS), "statvfs");
}

#[test]
fn termios_layout_matches_the_abi() {
    const FIELDS: &[&str] = &["u32", "u32", "u32", "u32", "u8", "[u8; 32]", "u32", "u32"];
    assert_clean(compare_struct("termios", FIELDS), "termios");
}

#[test]
fn c64i_exports_are_in_the_abi_list() {
    const C64I: &[&str] = &[
        "__ctype_get_mb_cur_max",
        "fgetwc",
        "fwprintf",
        "vfwprintf",
        "getline",
        "remove",
        "rewind",
        "stdin",
        "stdout",
        "stderr",
        "arc4random_buf",
        "fgetln",
        "grantpt",
        "posix_openpt",
        "ptsname_r",
        "tcgetattr",
        "tcsetattr",
        "umask",
        "unlockpt",
    ];
    let list = include_str!("../../../../libc/ravenlibc/abi/Symbols.list");
    let names: std::collections::BTreeSet<&str> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    for name in C64I {
        assert!(names.contains(name), "{name} is missing from Symbols.list");
    }
}

const SPAWN_PATCH: &str =
    include_str!("../../../../toolchain/patches/0003-posix-spawn-ravenbsd.patch");

/// Apply the 0003 module hunks to the module produced by 0002, using the same
/// `patch -p1` tool the toolchain build uses. The 0002 body is the pre-0003
/// source; the vendor module is the post-0003 source, so the oracle must apply
/// 0003 before comparing. Only the ravenbsd module section is applied.
fn apply_spawn_patch(module: &str) -> String {
    use std::io::Write as _;
    let marker = "--- a/library/libc-ravenbsd/src/unix/ravenbsd/mod.rs\n";
    let start = SPAWN_PATCH.find(marker).expect("0003 module section");
    let body_start = start + marker.len();
    let body_end = SPAWN_PATCH[body_start..]
        .find("\n--- a/")
        .map_or(SPAWN_PATCH.len(), |offset| body_start + offset);
    let section = &SPAWN_PATCH[start..body_end];
    let dir = std::env::temp_dir().join(format!("raven-s759-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("library/libc-ravenbsd/src/unix/ravenbsd/mod.rs");
    std::fs::create_dir_all(file.parent().expect("parent")).expect("temp tree");
    std::fs::write(&file, module).expect("write module");
    let mut child = std::process::Command::new("patch")
        .args(["-p1", "-d"])
        .arg(&dir)
        .arg("--batch")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("run patch");
    child
        .stdin
        .as_mut()
        .expect("patch stdin")
        .write_all(section.as_bytes())
        .expect("write patch");
    let status = child.wait().expect("patch status");
    assert!(status.success(), "patch 0003 failed: {status}");
    let patched = std::fs::read_to_string(&file).expect("read patched module");
    let _ = std::fs::remove_dir_all(&dir);
    patched
}

#[test]
fn vendored_module_is_the_patched_toolchain_module() {
    let marker = "+++ b/library/libc-ravenbsd/src/unix/ravenbsd/mod.rs\n";
    let start = PATCH.find(marker).expect("module hunk") + marker.len();
    let end = PATCH[start..]
        .find("\ndiff --git ")
        .map_or(PATCH.len(), |offset| start + offset);
    let mut module = String::new();
    for line in PATCH[start..end].lines() {
        if let Some(rest) = line.strip_prefix('+') {
            module.push_str(rest);
            module.push('\n');
        }
    }
    let patched = apply_spawn_patch(&module);
    let module: String = patched.lines().map(|line| format!("{line}\n")).collect();
    let vendor: String = TOOLCHAIN_LIBC
        .lines()
        .skip(3)
        .map(|line| format!("{line}\n"))
        .collect();
    assert_eq!(module, vendor, "toolchain patch and vendored module differ");
}
