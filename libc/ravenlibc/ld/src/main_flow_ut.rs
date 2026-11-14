// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn split_paths_keeps_the_order() {
    let mut out = [Name::empty(); 8];
    let count = split_paths(b"/lib:/usr/lib::/x", &mut out);
    assert_eq!(count, 4);
    assert_eq!(out[0].as_bytes(), b"/lib");
    assert_eq!(out[1].as_bytes(), b"/usr/lib");
    assert_eq!(out[2].as_bytes(), b"");
    assert_eq!(out[3].as_bytes(), b"/x");
}

#[test]
fn split_paths_respects_the_array_bound() {
    let mut out = [Name::empty(); 1];
    assert_eq!(split_paths(b"a:b:c", &mut out), 1);
}

#[test]
fn the_main_id_is_zero() {
    assert_eq!(main_id(), ObjId(0));
}

#[test]
fn ld_debug_words_parse_and_are_ignored_when_untrusted() {
    let words = parse_ld_debug(
        b"files,reloc,bind,symbols,versions,tls,init,libs,statistics,all",
        true,
    );
    for bit in [
        raven_libc_elf::debug::DebugWords::FILES,
        raven_libc_elf::debug::DebugWords::RELOC,
        raven_libc_elf::debug::DebugWords::BIND,
        raven_libc_elf::debug::DebugWords::SYMBOLS,
        raven_libc_elf::debug::DebugWords::VERSIONS,
        raven_libc_elf::debug::DebugWords::TLS,
        raven_libc_elf::debug::DebugWords::INIT,
        raven_libc_elf::debug::DebugWords::LIBS,
        raven_libc_elf::debug::DebugWords::STATISTICS,
        raven_libc_elf::debug::DebugWords::ALL,
    ] {
        assert!(words.contains(bit));
    }
    assert!(parse_ld_debug(b"files", false).is_empty());
}

#[test]
fn direct_exec_parses_the_supported_flags() {
    let args: [&[u8]; 7] = [
        b"--list",
        b"--library-path",
        b"/lib:/usr/lib",
        b"--preload",
        b"libx.so",
        b"--",
        b"/bin/prog",
    ];
    let parsed = parse_direct_exec(&args, true).expect("parse");
    assert!(parsed.list);
    assert_eq!(parsed.library_path, Some(b"/lib:/usr/lib".as_slice()));
    assert_eq!(parsed.preload, Some(b"libx.so".as_slice()));
    assert_eq!(parsed.program, Some(b"/bin/prog".as_slice()));
    assert_eq!(parsed.program_index, 6);
}

#[test]
fn direct_exec_is_refused_when_untrusted() {
    let args: [&[u8]; 1] = [b"/bin/prog"];
    assert_eq!(parse_direct_exec(&args, false), Err(LoadError::Tainted));
}

#[test]
fn direct_exec_rejects_a_missing_program_or_option_value() {
    assert_eq!(parse_direct_exec(&[], true), Err(LoadError::BadDirectArgs));
    let args: [&[u8]; 1] = [b"--library-path"];
    assert_eq!(
        parse_direct_exec(&args, true),
        Err(LoadError::BadDirectArgs)
    );
}

#[test]
fn the_ldd_trace_line_is_golden() {
    let mut out = [0u8; 128];
    let len = write_trace_line(
        b"libdeep.so",
        b"/tmp/libdeep.so",
        0x7f00_0000_1000,
        &mut out,
    )
    .expect("line");
    assert_eq!(
        out.get(..len).expect("bytes"),
        b"\tlibdeep.so => /tmp/libdeep.so (0x7f0000001000)\n"
    );
    let len = write_trace_not_found(b"libmissing.so", &mut out).expect("line");
    assert_eq!(
        out.get(..len).expect("bytes"),
        b"\tlibmissing.so => not found\n"
    );
}

#[test]
fn the_seventeen_step_markers_appear_in_order() {
    const SOURCE: &str = include_str!("main_flow.rs");
    let mut previous = 0usize;
    for step in 1..=17 {
        let marker = format!("// STEP {step}");
        let Some(position) = SOURCE.find(&marker) else {
            panic!("missing {marker}");
        };
        assert!(position > previous, "{marker} is out of order");
        previous = position;
    }
}

#[test]
fn modules_reachable_before_selfreloc_have_no_pointer_statics() {
    for (name, source) in [
        ("main_flow.rs", include_str!("main_flow.rs")),
        ("loadobj.rs", include_str!("loadobj.rs")),
        ("mapper.rs", include_str!("mapper.rs")),
        ("relocate.rs", include_str!("relocate.rs")),
        ("initfini.rs", include_str!("initfini.rs")),
        ("dlfcn.rs", include_str!("dlfcn.rs")),
        ("rdebug.rs", include_str!("rdebug.rs")),
        ("unload.rs", include_str!("unload.rs")),
        ("lock.rs", include_str!("lock.rs")),
        ("tls.rs", include_str!("tls.rs")),
        ("arch/mod.rs", include_str!("arch/mod.rs")),
        ("errbuf.rs", include_str!("errbuf.rs")),
        ("table.rs", include_str!("table.rs")),
        ("xalloc.rs", include_str!("xalloc.rs")),
    ] {
        assert!(
            !source
                .lines()
                .any(|line| line.trim_start().starts_with("static ")),
            "{name} declares a static before selfreloc returns"
        );
    }
}
