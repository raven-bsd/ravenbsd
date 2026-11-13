// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The environment tables are fixed literal lists.

//! `LD_*` parsing and the secure-exec removal list.

use alloc::vec::Vec;

use super::*;

fn parse(envp: &[&[u8]], trust: bool) -> (EnvConfig, Vec<usize>) {
    EnvConfig::parse(envp, trust)
}

#[test]
fn every_kept_variable_is_parsed() {
    let envp: Vec<&[u8]> = vec![
        b"PATH=/bin",
        b"LD_BIND_NOW=1",
        b"LD_PRELOAD=liba.so libb.so:libc.so",
        b"LD_LIBRARY_PATH=/a:/b",
        b"LD_DEBUG=files,reloc",
        b"LD_LIBRARY_PATH_RPATH=yes",
        b"LD_DYNAMIC_WEAK=",
        b"LD_TRACE_LOADED_OBJECTS=1",
        b"LD_TRACE_LOADED_OBJECTS_PROGNAME=prog",
        b"LD_TRACE_LOADED_OBJECTS_FMT1=%o",
        b"LD_TRACE_LOADED_OBJECTS_FMT2=%p",
        b"LD_TRACE_LOADED_OBJECTS_ALL=1",
        b"LD_SHOW_AUXV=1",
        b"LD_STATIC_TLS_EXTRA=0x800",
    ];
    let (config, remove) = parse(&envp, true);
    assert!(remove.is_empty());
    assert!(config.bind_now);
    assert_eq!(
        config.preload,
        vec![
            b"liba.so".to_vec(),
            b"libb.so".to_vec(),
            b"libc.so".to_vec()
        ]
    );
    assert_eq!(config.library_path.as_deref(), Some(&b"/a:/b"[..]));
    assert_eq!(config.debug, DebugWords::parse(b"files,reloc"));
    assert!(config.library_path_rpath);
    assert!(!config.dynamic_weak);
    assert!(config.trace_loaded_objects);
    assert_eq!(config.trace_prog.as_deref(), Some(&b"prog"[..]));
    assert_eq!(config.trace_fmt1.as_deref(), Some(&b"%o"[..]));
    assert_eq!(config.trace_fmt2.as_deref(), Some(&b"%p"[..]));
    assert!(config.trace_all);
    assert!(config.show_auxv);
    assert_eq!(config.static_tls_extra, Some(0x800));
    assert_eq!(config.static_tls_surplus(), 0x800);
}

#[test]
fn preload_splits_on_spaces_and_colons() {
    let envp: Vec<&[u8]> = vec![b"LD_PRELOAD= a.so  b.so:c.so\t d.so "];
    let (config, _) = parse(&envp, true);
    assert_eq!(
        config.preload,
        vec![
            b"a.so".to_vec(),
            b"b.so".to_vec(),
            b"c.so".to_vec(),
            b"d.so".to_vec()
        ]
    );
}

#[test]
fn an_empty_preload_is_empty() {
    let envp: Vec<&[u8]> = vec![b"LD_PRELOAD="];
    let (config, _) = parse(&envp, true);
    assert!(config.preload.is_empty());
}

#[test]
fn library_path_rpath_accepts_y_y_and_one_only() {
    for (value, expected) in [
        (&b"y"[..], true),
        (&b"Y"[..], true),
        (&b"1"[..], true),
        (&b"n"[..], false),
        (&b"yes"[..], true),
        (&b""[..], false),
    ] {
        let entry = [b"LD_LIBRARY_PATH_RPATH=".as_slice(), value].concat();
        let envp: Vec<&[u8]> = vec![&entry];
        let (config, _) = parse(&envp, true);
        assert_eq!(config.library_path_rpath, expected, "{value:?}");
    }
}

#[test]
fn dynamic_weak_defaults_to_true_and_the_variable_makes_it_false() {
    let (config, _) = parse(&[], true);
    assert!(config.dynamic_weak);
    let envp: Vec<&[u8]> = vec![b"LD_DYNAMIC_WEAK=1"];
    let (config, _) = parse(&envp, true);
    assert!(!config.dynamic_weak);
}

#[test]
fn the_static_tls_surplus_has_a_default() {
    let (config, _) = parse(&[], true);
    assert_eq!(config.static_tls_surplus(), DEFAULT_STATIC_TLS_EXTRA);
    assert_eq!(DEFAULT_STATIC_TLS_EXTRA, 1664);
}

#[test]
fn static_tls_extra_parses_hex_octal_and_decimal() {
    for (value, expected) in [
        (&b"0x100"[..], Some(0x100)),
        (&b"0100"[..], Some(64)),
        (&b"100"[..], Some(100)),
        (&b"0"[..], Some(0)),
        (&b""[..], Some(0)),
        (&b"0x"[..], Some(0)),
        (&b"12x"[..], None),
        (&b"0xzz"[..], None),
        (&b"999999999999999999999999"[..], None),
    ] {
        let entry = [b"LD_STATIC_TLS_EXTRA=".as_slice(), value].concat();
        let envp: Vec<&[u8]> = vec![&entry];
        let (config, _) = parse(&envp, true);
        assert_eq!(config.static_tls_extra, expected, "{value:?}");
    }
}

#[test]
fn secure_mode_returns_the_exact_indices() {
    let envp: Vec<&[u8]> = vec![
        b"LD_PRELOAD=libevil.so",
        b"LD_BIND_NOW=1",
        b"LD_LIBRARY_PATH=/tmp",
        b"LD_DEBUG=all",
        b"LD_LIBRARY_PATH_RPATH=1",
        b"LD_STATIC_TLS_EXTRA=0x1000",
        b"LD_DYNAMIC_WEAK=1",
    ];
    let (config, remove) = parse(&envp, false);
    assert_eq!(remove, vec![0, 2, 3, 4, 5, 6]);
    assert!(config.preload.is_empty());
    assert!(config.library_path.is_none());
    assert!(config.debug.is_empty());
    assert!(!config.library_path_rpath);
    assert_eq!(config.static_tls_extra, None);
    assert!(config.dynamic_weak);
    // LD_BIND_NOW is harmless and kept.
    assert!(config.bind_now);
}

#[test]
fn dropped_variables_are_ignored() {
    let envp: Vec<&[u8]> = vec![
        b"LD_LIBMAP=libc.so.1 /tmp/libc.so.1",
        b"LD_LIBRARY_PATH_FDS=3",
        b"LD_PRELOAD_FDS=4",
        b"LD_ELF_HINTS_PATH=/tmp/hints",
        b"LD_UTRACE=1",
        b"LD_LOADFLTR=1",
        b"LD_BIND_NOT=1",
        b"LD_NO_DL_ITERATE_PHDR_AFTER_FORK=1",
        b"LD_DUMP_REL_PRE=1",
    ];
    let (config, remove) = parse(&envp, true);
    assert!(remove.is_empty());
    assert_eq!(config, EnvConfig::default());
}

#[test]
fn duplicate_variables_take_the_first_value() {
    let envp: Vec<&[u8]> = vec![b"LD_LIBRARY_PATH=/first", b"LD_LIBRARY_PATH=/second"];
    let (config, _) = parse(&envp, true);
    assert_eq!(config.library_path.as_deref(), Some(&b"/first"[..]));
}

#[test]
fn a_malformed_entry_is_ignored() {
    let envp: Vec<&[u8]> = vec![b"LD_LIBRARY_PATH", b"JUSTAWORD", b"=LD_DEBUG=all"];
    let (config, remove) = parse(&envp, true);
    assert_eq!(config, EnvConfig::default());
    assert!(remove.is_empty());
}

#[test]
fn a_non_ld_variable_is_ignored() {
    let envp: Vec<&[u8]> = vec![b"LDX_PRELOAD=evil", b"PATH=/bin", b"XLD_DEBUG=1"];
    let (config, _) = parse(&envp, true);
    assert!(config.preload.is_empty());
    assert!(config.debug.is_empty());
}

#[test]
fn a_non_utf8_name_is_ignored() {
    let envp: Vec<&[u8]> = vec![&[b'L', b'D', b'_', 0xff, b'=', b'1']];
    let (config, remove) = parse(&envp, true);
    assert_eq!(config, EnvConfig::default());
    assert!(remove.is_empty());
}

#[test]
fn an_unknown_ld_variable_is_ignored() {
    let envp: Vec<&[u8]> = vec![b"LD_MADE_UP=1"];
    let (config, remove) = parse(&envp, true);
    assert_eq!(config, EnvConfig::default());
    assert!(remove.is_empty());
}

#[test]
fn secure_mode_keeps_trace_and_show_auxv_out_of_the_removal_list() {
    // TRACE_LOADED_OBJECTS and its companions are not marked unsecure in the donor table.
    let envp: Vec<&[u8]> = vec![
        b"LD_TRACE_LOADED_OBJECTS=1",
        b"LD_TRACE_LOADED_OBJECTS_ALL=1",
    ];
    let (config, remove) = parse(&envp, false);
    assert!(remove.is_empty());
    assert!(config.trace_loaded_objects);
    assert!(config.trace_all);
}

#[test]
fn an_empty_environment_gives_the_defaults() {
    let (config, remove) = parse(&[], true);
    assert_eq!(config, EnvConfig::default());
    assert!(remove.is_empty());
    assert!(config.dynamic_weak);
}

#[test]
fn parse_integer_is_strict() {
    assert_eq!(parse_integer(b"0x1f"), Some(31));
    assert_eq!(parse_integer(b"0777"), Some(511));
    assert_eq!(parse_integer(b"+1"), None);
    assert_eq!(parse_integer(b"-1"), None);
    assert_eq!(parse_integer(b"1 "), None);
}
