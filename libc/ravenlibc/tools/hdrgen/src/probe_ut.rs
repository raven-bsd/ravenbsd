// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the probe source generators.

use super::*;
use crate::validate;

fn tables() -> (Tables, Values, std::path::PathBuf) {
    let abi = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."))
        .join("libc/ravenlibc/abi");
    let tables = validate::load(&abi).expect("tables");
    let values = validate::validate(&tables).expect("values");
    (tables, values, abi)
}

#[test]
fn layout_probe_has_assertions_for_every_record() {
    let (tables, _, _) = tables();
    let probe = layout_probe(&tables, "x86_64").expect("probe");
    assert!(probe.contains("_Static_assert(sizeof(struct __ab_stat) == 128"));
    assert!(probe.contains("_Static_assert(offsetof(struct __ab_stat, st_size) == 48"));
    assert!(probe.contains("_Static_assert(_Alignof(struct __ab_sigaction) == 8"));
    assert!(
        probe.contains(
            "typedef struct { _Alignas(16) unsigned char __bytes[4560]; } __ab_ucontext_t;"
        ) || probe.contains(
            "typedef struct { _Alignas(8) unsigned char __bytes[936]; } __ab_ucontext_t;"
        )
    );
}

#[test]
fn layout_probe_orders_nested_types_first() {
    let (tables, _, _) = tables();
    let probe = layout_probe(&tables, "x86_64").expect("probe");
    let timespec = probe.find("struct __ab_timespec {").expect("timespec");
    let itimerspec = probe.find("struct __ab_itimerspec {").expect("itimerspec");
    let sigset = probe.find("__ab_sigset_t;").expect("sigset");
    let sigaction = probe.find("struct __ab_sigaction {").expect("sigaction");
    assert!(timespec < itimerspec);
    assert!(sigset < sigaction);
}

#[test]
fn layout_probe_per_arch_uses_that_archs_sizes() {
    let (tables, _, _) = tables();
    let aarch64 = layout_probe(&tables, "aarch64").expect("aarch64");
    let riscv = layout_probe(&tables, "riscv64").expect("riscv64");
    assert!(aarch64.contains("unsigned char __bytes[4560];"));
    assert!(riscv.contains("unsigned char __bytes[960];"));
    assert!(riscv.contains("unsigned char __bytes[784];"));
}

#[test]
fn musl_probe_includes_every_public_header() {
    let (tables, _, _) = tables();
    let probe = musl_probe_source(&tables);
    for header in [
        "setjmp.h",
        "signal.h",
        "sys/stat.h",
        "pthread.h",
        "sys/epoll.h",
        "sys/file.h",
    ] {
        assert!(probe.contains(&format!("#include <{header}>")), "{header}");
    }
    assert!(probe.contains("#define _GNU_SOURCE"));
    assert!(probe.contains("const int sz_stat = sizeof(struct stat);"));
    assert!(probe.contains("const int sz_stat = sizeof(struct stat);"));
    assert!(probe.contains("const int off_dirent_d_name = offsetof(struct dirent, d_name);"));
    assert!(probe.contains("const int off_tm_tm_gmtoff = offsetof(struct tm, __tm_gmtoff);"));
    assert!(probe.contains("const int sz_nlink_t = sizeof(nlink_t);"));
}

#[test]
fn musl_probe_skips_named_padding_fields() {
    let (tables, _, _) = tables();
    let probe = musl_probe_source(&tables);
    assert!(!probe.contains("off_msghdr___pad1"));
    assert!(!probe.contains("off_siginfo_t___pad"));
}

#[test]
fn constant_probe_includes_the_group_headers() {
    let (tables, _, _) = tables();
    let errno = &tables.groups[0].def;
    let probe = constant_probe_source("errno", errno);
    assert!(probe.contains("#include <errno.h>"));
    assert!(probe.contains("#ifdef EPERM"));
    assert!(probe.contains("const long long m_EPERM = (long long)(EPERM);"));
}

#[test]
fn constant_probe_uses_the_misc_header_set() {
    let (tables, _, _) = tables();
    let misc = tables
        .groups
        .iter()
        .find(|group| group.stem == "misc")
        .expect("misc");
    let probe = constant_probe_source("misc", &misc.def);
    for header in [
        "locale.h",
        "dlfcn.h",
        "sys/wait.h",
        "netdb.h",
        "sys/select.h",
    ] {
        assert!(probe.contains(&format!("#include <{header}>")), "{header}");
    }
}

#[test]
fn int_or_table_helper_reads_the_arch() {
    let value = crate::model::IntOrTable::Table(std::collections::BTreeMap::from([(
        "x86_64".to_string(),
        12,
    )]));
    assert_eq!(int_or_table(&value, "x86_64"), Some(12));
    assert_eq!(int_or_table(&value, "aarch64"), None);
}

#[test]
fn verify_without_a_musl_checkout_is_a_skip() {
    let (tables, values, abi) = tables();
    let empty = std::env::temp_dir().join(format!(
        "raven-libc-hdrgen-empty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    std::fs::create_dir_all(&empty).expect("temp");
    let report = verify(&tables, &values, "all", &abi, &empty, "clang", "sed").expect("skip");
    let _ = std::fs::remove_dir_all(&empty);
    assert!(report.skip.is_some());
    assert!(report.failures.is_empty());
}

#[test]
fn tools_probe_finds_clang_and_sed_when_installed() {
    // Do not assert the environment; this only exercises the discovery code path.
    if let Ok(Some((clang, sed))) = tools("clang", "sed") {
        assert_eq!(clang, "clang");
        assert_eq!(sed, "sed");
    }
}
