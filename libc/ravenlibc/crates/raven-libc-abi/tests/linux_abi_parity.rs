// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parity with `raven-linux-abi` (T4b) and the crate's own frozen-value contract.
//!
//! The parity half parses the Rust source of `raven-linux-abi` exactly as doc 14 4.7 asks
//! (`pub const NAME: u32 = N;`, taking the `x86_64` submodule for per-arch names) and compares
//! every overlapping name with the generated `ALL` tables. The contract half asserts the
//! doc 14 4.6 values that a user of the crate sees.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../..")).join(relative)
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(repo_path(relative)).unwrap_or_else(|error| {
        panic!("cannot read {relative}: {error}");
    })
}

/// Parse `[[deviation]]` blocks into (name, against) pairs without a TOML dependency.
fn deviations() -> Vec<(String, String)> {
    let text = read("libc/ravenlibc/abi/deviations.toml");
    let mut rows = Vec::new();
    let mut name = None;
    let mut against = None;
    let mut flush = |name: &mut Option<String>, against: &mut Option<String>| {
        if let (Some(name), Some(against)) = (name.take(), against.take()) {
            rows.push((name, against));
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line == "[[deviation]]" {
            flush(&mut name, &mut against);
            continue;
        }
        if let Some(value) = line.strip_prefix("name = \"") {
            name = Some(value.trim_end_matches('"').to_string());
        } else if let Some(value) = line.strip_prefix("against = \"") {
            against = Some(value.trim_end_matches('"').to_string());
        }
    }
    flush(&mut name, &mut against);
    rows
}

fn linux_abi_deviations() -> BTreeMap<String, ()> {
    deviations()
        .into_iter()
        .filter(|(_, against)| against == "linux-abi")
        .map(|(name, _)| (name, ()))
        .collect()
}

/// Parse `pub const NAME: u32 = N;` from `flags.rs`, preferring the `x86_64` submodule.
fn parse_flags() -> BTreeMap<String, i64> {
    let text = read("crates/raven-linux-abi/src/flags.rs");
    let mut modules: Vec<String> = Vec::new();
    let mut seen: BTreeMap<String, (bool, i64)> = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub mod ")
            && let Some(name) = rest.strip_suffix(" {")
        {
            modules.push(name.to_string());
            continue;
        }
        if trimmed == "}" {
            if !modules.is_empty() {
                modules.pop();
            }
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(": u32 = ") else {
            continue;
        };
        let Some(value) = value.strip_suffix(';') else {
            continue;
        };
        let Ok(value) = value.parse::<i64>() else {
            continue;
        };
        let is_x86 = modules.iter().any(|module| module == "x86_64");
        let slot = seen.entry(name.to_string()).or_insert((false, value));
        if is_x86 && !slot.0 {
            *slot = (true, value);
        }
    }
    seen.into_iter()
        .map(|(name, (_, value))| (name, value))
        .collect()
}

/// Parse `pub const NAME: u16 = N;` from `errno.rs`.
fn parse_errno() -> BTreeMap<String, i64> {
    let text = read("crates/raven-linux-abi/src/errno.rs");
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("pub const ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once(": u16 = ") else {
            continue;
        };
        let Some(value) = value.strip_suffix(';') else {
            continue;
        };
        if let Ok(value) = value.parse::<i64>() {
            rows.insert(name.to_string(), value);
        }
    }
    rows
}

#[test]
fn flag_parity_with_raven_linux_abi() {
    let linux = parse_flags();
    let deviations = linux_abi_deviations();
    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    for group in [
        &raven_libc_abi::fcntl::ALL,
        &raven_libc_abi::mman::ALL,
        &raven_libc_abi::signal::ALL,
        &raven_libc_abi::socket::ALL,
        &raven_libc_abi::netinet::ALL,
        &raven_libc_abi::poll::ALL,
        &raven_libc_abi::time::ALL,
        &raven_libc_abi::resource::ALL,
        &raven_libc_abi::misc::ALL,
    ] {
        for (name, value) in *group {
            let Some(their) = linux.get(*name) else {
                continue;
            };
            if deviations.contains_key(*name) {
                continue;
            }
            compared += 1;
            if *value != i128::from(*their) {
                mismatches.push(format!("{name}: ours {value}, linux-abi {their}"));
            }
        }
    }
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    assert!(compared > 300, "only {compared} names compared");
    println!("linux-abi flag parity: compared {compared} constants");
}

#[test]
fn errno_parity_with_raven_linux_abi() {
    let linux = parse_errno();
    assert_eq!(linux.len(), 133, "raven-linux-abi errno table changed");
    let mut compared = 0usize;
    for (name, value) in &linux {
        let found = raven_libc_abi::errno::ALL
            .iter()
            .find(|(candidate, _)| *candidate == name);
        assert!(found.is_some(), "libc errno lacks {name}");
        assert_eq!(
            found.expect("checked").1,
            i128::from(*value),
            "{name} differs"
        );
        compared += 1;
    }
    assert_eq!(compared, 133);
    println!("linux-abi errno parity: compared {compared} constants");
}

#[test]
fn no_linux_abi_deviation_is_stale() {
    let linux = parse_flags();
    let our = [
        &raven_libc_abi::fcntl::ALL,
        &raven_libc_abi::signal::ALL,
        &raven_libc_abi::socket::ALL,
    ];
    for name in linux_abi_deviations().keys() {
        let ours = our
            .iter()
            .flat_map(|group| group.iter())
            .find(|(candidate, _)| candidate == name)
            .unwrap_or_else(|| panic!("deviation {name} has no libc constant"));
        let theirs = linux
            .get(name)
            .unwrap_or_else(|| panic!("deviation {name} has no linux-abi constant"));
        assert_ne!(
            ours.1,
            i128::from(*theirs),
            "stale linux-abi deviation for {name}"
        );
    }
}

#[test]
fn errno_values_are_the_frozen_table() {
    assert_eq!(raven_libc_abi::errno::EPERM, 1);
    assert_eq!(raven_libc_abi::errno::ENOENT, 2);
    assert_eq!(raven_libc_abi::errno::EAGAIN, 11);
    assert_eq!(
        raven_libc_abi::errno::EAGAIN,
        raven_libc_abi::errno::EWOULDBLOCK
    );
    assert_eq!(raven_libc_abi::errno::ENOTSUP, 95);
    assert_eq!(
        raven_libc_abi::errno::ENOTSUP,
        raven_libc_abi::errno::EOPNOTSUPP
    );
    assert_eq!(
        raven_libc_abi::errno::ENOATTR,
        raven_libc_abi::errno::ENODATA
    );
    assert_eq!(raven_libc_abi::errno::EDOOFUS, 1000);
    assert_eq!(raven_libc_abi::errno::EIPSEC, 1013);
    for absent in [41i128, 58] {
        assert!(
            !raven_libc_abi::errno::ALL
                .iter()
                .any(|(_, value)| *value == absent),
            "errno {absent} must be unused"
        );
    }
    assert_eq!(raven_libc_abi::errno::ALL.len(), 149);
}

#[test]
fn errno_classification() {
    assert!(raven_libc_abi::errno::Errno(1).is_linux());
    assert!(raven_libc_abi::errno::Errno(133).is_linux());
    assert!(!raven_libc_abi::errno::Errno(41).is_linux());
    assert!(!raven_libc_abi::errno::Errno(58).is_linux());
    assert!(!raven_libc_abi::errno::Errno(1000).is_linux());
    assert!(raven_libc_abi::errno::Errno(1000).is_bsd_extra());
    assert!(raven_libc_abi::errno::Errno(1013).is_bsd_extra());
    assert!(!raven_libc_abi::errno::Errno(1).is_bsd_extra());
    assert_eq!(raven_libc_abi::errno::Errno(1).name(), Some("EPERM"));
    assert_eq!(raven_libc_abi::errno::Errno(4000).name(), None);
}

#[test]
fn flag_values_are_the_frozen_table() {
    assert_eq!(
        raven_libc_abi::fcntl::O_ACCMODE,
        raven_libc_abi::fcntl::O_PATH | 3
    );
    assert_eq!(raven_libc_abi::fcntl::O_LARGEFILE, 0);
    assert_eq!(raven_libc_abi::fcntl::AT_FDCWD, -100);
    assert_eq!(raven_libc_abi::mman::MAP_HUGE_16KB, 14 << 26);
    assert_eq!(raven_libc_abi::mman::MAP_32BIT, 0x40);
    assert_eq!(raven_libc_abi::resource::RLIM_INFINITY, u64::MAX);
    assert_eq!(raven_libc_abi::socket::SOMAXCONN, 4096);
    assert_eq!(raven_libc_abi::socket::AF_ROUTE, 128);
    assert_eq!(raven_libc_abi::socket::AF_LINK, 129);
    assert_eq!(raven_libc_abi::signal::SIGRTMIN, 35);
    assert_eq!(raven_libc_abi::signal::SIGRTMAX, 64);
    assert_eq!(raven_libc_abi::signal::NSIG, 65);
    assert_eq!(raven_libc_abi::signal::MINSIGSTKSZ, 6144);
    assert_eq!(raven_libc_abi::signal::SIGSTKSZ, 16384);
}

#[test]
fn limits_are_the_frozen_table() {
    assert_eq!(raven_libc_abi::limits::ARG_MAX, 131072);
    assert_eq!(raven_libc_abi::limits::NGROUPS_MAX, 1023);
    assert_eq!(raven_libc_abi::limits::HOST_NAME_MAX, 64);
    assert_eq!(raven_libc_abi::limits::PTHREAD_STACK_MIN, 16384);
    assert_eq!(raven_libc_abi::limits::PTHREAD_KEYS_MAX, 128);
    assert_eq!(raven_libc_abi::limits::NL_ARGMAX, 64);
    assert_eq!(raven_libc_abi::limits::SEM_VALUE_MAX, 0x7fffffff);
    assert_eq!(raven_libc_abi::limits::CLK_TCK, 100);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn page_size_is_x86_64_only() {
    assert_eq!(raven_libc_abi::limits::PAGESIZE, 4096);
    assert_eq!(raven_libc_abi::limits::PAGE_SIZE, 4096);
}

#[test]
fn byte_order_and_socket_types_are_the_frozen_table() {
    assert_eq!(raven_libc_abi::netinet::INADDR_BROADCAST, 0xffff_ffff);
    assert_eq!(raven_libc_abi::netinet::INADDR_LOOPBACK, 0x7f00_0001);
    assert_eq!(raven_libc_abi::netinet::TCP_CORK, 3);
    assert_eq!(raven_libc_abi::netinet::TCP_NOPUSH, 3);
    assert_eq!(raven_libc_abi::poll::POLLIN, 1);
    assert_eq!(raven_libc_abi::poll::POLLRDHUP, 0x2000);
    assert_eq!(raven_libc_abi::time::CLOCKS_PER_SEC, 1_000_000);
    assert_eq!(raven_libc_abi::misc::FD_SETSIZE, 1024);
    assert_eq!(raven_libc_abi::misc::EAI_OVERFLOW, -12);
    assert_eq!(raven_libc_abi::pthread::PTHREAD_MUTEX_RECURSIVE, 1);
}

#[test]
fn char_signedness_matches_the_compiler() {
    assert_eq!(
        raven_libc_abi::types::CHAR_IS_SIGNED,
        core::ffi::c_char::MIN != 0
    );
    assert_eq!(core::mem::size_of::<raven_libc_abi::types::wchar_t>(), 4);
    assert_eq!(core::mem::size_of::<raven_libc_abi::types::time_t>(), 8);
    assert_eq!(core::mem::size_of::<raven_libc_abi::types::off_t>(), 8);
    assert_eq!(core::mem::size_of::<raven_libc_abi::types::pthread_t>(), 8);
}

#[test]
fn stat_layout_and_zero() {
    let stat = raven_libc_abi::layout::find("stat").expect("stat");
    assert_eq!(
        stat.size,
        core::mem::size_of::<raven_libc_abi::structs::Stat>()
    );
    assert_eq!(
        stat.align,
        core::mem::align_of::<raven_libc_abi::structs::Stat>()
    );
    let zero = raven_libc_abi::structs::Stat::ZERO;
    assert_eq!(zero.st_dev, 0);
    assert_eq!(zero.st_ino, 0);
    assert_eq!(zero.st_nlink, 0);
    assert_eq!(zero.st_blocks, 0);
    assert_eq!(zero.__unused, [0; 2]);
    assert_eq!(zero.st_atim, raven_libc_abi::structs::Timespec::ZERO);
    assert_eq!(zero.st_mtim.tv_nsec, 0);
}

#[test]
fn every_record_runtime_layout_matches_the_constant_table() {
    assert_eq!(raven_libc_abi::layout::STRUCTS.len(), 26);
    assert_eq!(core::mem::size_of::<raven_libc_abi::structs::Flock>(), 32);
    assert_eq!(
        core::mem::offset_of!(raven_libc_abi::structs::Flock, l_start),
        8
    );
    assert_eq!(
        core::mem::size_of::<raven_libc_abi::structs::Utsname>(),
        390
    );
    assert_eq!(
        core::mem::size_of::<raven_libc_abi::structs::Sigaction>(),
        152
    );
    assert_eq!(
        core::mem::offset_of!(raven_libc_abi::structs::Sigaction, sa_mask),
        8
    );
    assert_eq!(core::mem::size_of::<raven_libc_abi::structs::Rusage>(), 272);
}

// --- Direct x86_64 Rust-vs-clang comparison -------------------------------------------------

const C_FIXTURE: &str = r#"
#include <stddef.h>
#include <stdarg.h>

typedef struct { long tv_sec; long tv_nsec; } __ab_timespec;
typedef struct { long tv_sec; long tv_usec; } __ab_timeval;
typedef struct { __ab_timespec it_interval; __ab_timespec it_value; } __ab_itimerspec;
typedef struct { void *iov_base; unsigned long iov_len; } __ab_iovec;
typedef struct { unsigned long st_dev; unsigned long st_ino; unsigned int st_mode;
  unsigned int st_nlink; unsigned int st_uid; unsigned int st_gid; unsigned long st_rdev;
  unsigned long __pad1; long st_size; int st_blksize; int __pad2; long st_blocks;
  __ab_timespec st_atim; __ab_timespec st_mtim; __ab_timespec st_ctim; unsigned int __unused[2]; } __ab_stat;
typedef struct { unsigned long d_ino; long d_off; unsigned short d_reclen;
  unsigned char d_type; char d_name[256]; } __ab_dirent;
typedef struct { unsigned short sa_family; char sa_data[14]; } __ab_sockaddr;
typedef struct { unsigned short sin_family; unsigned short sin_port; unsigned int sin_addr;
  unsigned char sin_zero[8]; } __ab_sockaddr_in;
typedef struct { unsigned short sin6_family; unsigned short sin6_port; unsigned int sin6_flowinfo;
  unsigned char sin6_addr[16]; unsigned int sin6_scope_id; } __ab_sockaddr_in6;
typedef struct { unsigned short sun_family; char sun_path[108]; } __ab_sockaddr_un;
typedef struct { void *msg_name; unsigned int msg_namelen; void *msg_iov; int msg_iovlen;
  int __pad1; void *msg_control; unsigned int msg_controllen; int __pad2; int msg_flags; } __ab_msghdr;
typedef struct { unsigned int cmsg_len; int __pad1; int cmsg_level; int cmsg_type; } __ab_cmsghdr;
typedef struct { int ai_flags; int ai_family; int ai_socktype; int ai_protocol;
  unsigned int ai_addrlen; void *ai_addr; void *ai_canonname; void *ai_next; } __ab_addrinfo;
typedef struct { int tm_sec; int tm_min; int tm_hour; int tm_mday; int tm_mon; int tm_year;
  int tm_wday; int tm_yday; int tm_isdst; long tm_gmtoff; void *tm_zone; } __ab_tm;
typedef struct { int fd; short events; short revents; } __ab_pollfd;
typedef struct { unsigned long rlim_cur; unsigned long rlim_max; } __ab_rlimit;
typedef struct { __ab_timeval ru_utime; __ab_timeval ru_stime;
  long ru_maxrss, ru_ixrss, ru_idrss, ru_isrss, ru_minflt, ru_majflt, ru_nswap, ru_inblock,
  ru_oublock, ru_msgsnd, ru_msgrcv, ru_nsignals, ru_nvcsw, ru_nivcsw;
  long __reserved[16]; } __ab_rusage;
typedef struct { char sysname[65]; char nodename[65]; char release[65]; char version[65];
  char machine[65]; char domainname[65]; } __ab_utsname;
typedef union { unsigned long __u; unsigned char __b[128]; } __ab_sigset_t;
typedef struct { void *sa_handler; __ab_sigset_t sa_mask; int sa_flags; void *sa_restorer; } __ab_sigaction;
typedef struct { void *ss_sp; int ss_flags; unsigned long ss_size; } __ab_stack_t;
typedef struct { int si_signo; int si_errno; int si_code; int __pad; unsigned long __si_fields[14]; } __ab_siginfo_t;
typedef struct { unsigned int c_iflag; unsigned int c_oflag; unsigned int c_cflag;
  unsigned int c_lflag; unsigned char c_line; unsigned char c_cc[32];
  unsigned int __c_ispeed; unsigned int __c_ospeed; } __ab_termios;
typedef struct { unsigned long f_bsize, f_frsize, f_blocks, f_bfree, f_bavail, f_files,
  f_ffree, f_favail, f_fsid, f_flag, f_namemax; unsigned int f_type; int __reserved[5]; } __ab_statvfs;
typedef struct { void *pw_name; void *pw_passwd; unsigned int pw_uid; unsigned int pw_gid;
  void *pw_gecos; void *pw_dir; void *pw_shell; } __ab_passwd;
typedef struct { void *gr_name; void *gr_passwd; unsigned int gr_gid; void *gr_mem; } __ab_group;
typedef struct { short l_type; short l_whence; long l_start; long l_len; int l_pid; } __ab_flock;

#define REC(name) \
  const int sz_##name = sizeof(__ab_##name); \
  const int al_##name = _Alignof(__ab_##name);
REC(timespec) REC(timeval) REC(itimerspec) REC(iovec) REC(stat) REC(dirent)
REC(sockaddr) REC(sockaddr_in) REC(sockaddr_in6) REC(sockaddr_un) REC(msghdr)
REC(cmsghdr) REC(addrinfo) REC(tm) REC(pollfd) REC(rlimit) REC(rusage)
REC(utsname) REC(sigaction) REC(stack_t) REC(siginfo_t) REC(termios)
REC(statvfs) REC(passwd) REC(group) REC(flock)

#define OFF(rec, field) const int off_##rec##_##field = offsetof(__ab_##rec, field);
OFF(stat, st_dev) OFF(stat, st_ino) OFF(stat, st_mode) OFF(stat, st_nlink) OFF(stat, st_uid)
OFF(stat, st_gid) OFF(stat, st_rdev) OFF(stat, __pad1) OFF(stat, st_size) OFF(stat, st_blksize)
OFF(stat, __pad2) OFF(stat, st_blocks) OFF(stat, st_atim) OFF(stat, st_mtim) OFF(stat, st_ctim)
OFF(stat, __unused)
OFF(dirent, d_ino) OFF(dirent, d_off) OFF(dirent, d_reclen) OFF(dirent, d_type) OFF(dirent, d_name)
OFF(msghdr, msg_name) OFF(msghdr, msg_namelen) OFF(msghdr, msg_iov) OFF(msghdr, msg_iovlen)
OFF(msghdr, msg_control) OFF(msghdr, msg_controllen) OFF(msghdr, msg_flags)
OFF(sigaction, sa_handler) OFF(sigaction, sa_mask) OFF(sigaction, sa_flags) OFF(sigaction, sa_restorer)
OFF(siginfo_t, si_signo) OFF(siginfo_t, si_errno) OFF(siginfo_t, si_code) OFF(siginfo_t, __si_fields)
OFF(sockaddr_in6, sin6_addr) OFF(sockaddr_in6, sin6_scope_id)
OFF(tm, tm_gmtoff) OFF(tm, tm_zone)
OFF(rusage, __reserved)
OFF(termios, c_cc) OFF(termios, __c_ispeed)
"#;

fn clang_available() -> bool {
    Command::new("clang")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn required() -> bool {
    std::env::var("RL_ABI_REQUIRED").is_ok_and(|value| value == "1")
}

fn clang_measurement() -> Option<BTreeMap<String, i64>> {
    if !clang_available() {
        if required() {
            panic!("clang is required (RL_ABI_REQUIRED=1)");
        }
        eprintln!("SKIP: clang not found");
        return None;
    }
    let directory = std::env::temp_dir().join(format!(
        "raven-libc-abi-clang-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let source = directory.join("layout.c");
    std::fs::write(&source, C_FIXTURE).expect("write probe");
    let resource = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .expect("clang -print-resource-dir");
    let resource = String::from_utf8_lossy(&resource.stdout).trim().to_string();
    let output = Command::new("clang")
        .args([
            "--target=x86_64-unknown-linux-gnu",
            "-nostdinc",
            "-isystem",
            &format!("{resource}/include"),
            "-S",
            "-o",
            "-",
        ])
        .arg(&source)
        .output()
        .expect("clang");
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        output.status.success(),
        "clang failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let mut values = BTreeMap::new();
    let mut wanted: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(label) = trimmed.strip_suffix(':')
            && (label.starts_with("sz_") || label.starts_with("al_") || label.starts_with("off_"))
        {
            wanted = Some(label.to_string());
            continue;
        }
        if let Some(label) = &wanted {
            let mut fields = trimmed.split_whitespace();
            if fields
                .next()
                .is_some_and(|directive| directive.starts_with('.'))
                && let Some(token) = fields.next()
                && let Ok(value) = token.parse::<i64>()
            {
                values.insert(label.clone(), value);
                wanted = None;
            }
        }
    }
    Some(values)
}

#[test]
fn record_layouts_match_clang_directly() {
    let Some(measured) = clang_measurement() else {
        return;
    };
    for record in raven_libc_abi::layout::STRUCTS {
        let size = measured
            .get(&format!("sz_{}", record.name))
            .copied()
            .unwrap_or_else(|| panic!("clang did not measure {}", record.name));
        let align = measured
            .get(&format!("al_{}", record.name))
            .copied()
            .unwrap_or_else(|| panic!("clang did not measure align {}", record.name));
        assert_eq!(
            size as usize, record.size,
            "size of {} differs from clang",
            record.name
        );
        assert_eq!(
            align as usize, record.align,
            "align of {} differs from clang",
            record.name
        );
        for field in record.fields {
            let label = format!("off_{}_{}", record.name, field.name);
            if let Some(offset) = measured.get(&label) {
                assert_eq!(
                    *offset as usize, field.offset,
                    "offset of {}.{} differs from clang",
                    record.name, field.name
                );
            }
        }
    }
}
