// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the boot-path parsers and rules, using hand-built disassembly and archive
//! graphs (no QEMU, no real link).

use super::*;

const DISASM: &str = "\
/tmp/p0: file format elf64-x86-64

Disassembly of section .text:

0000000000200000 <__raven_libc_start>:
  200000: callq 0x200010 <stage2>
  200005: syscall
  200007: retq

0000000000200010 <stage2>:
  200010: callq 0x200020 <exit>
  200015: retq

0000000000200020 <exit>:
  200020: callq 0x200030 <_exit>
  200025: retq

0000000000200030 <_exit>:
  200030: syscall
  200032: retq

0000000000200040 <unused>:
  200040: syscall
  200042: syscall
  200044: retq
";

#[test]
fn parses_functions_calls_and_syscalls() {
    let disassembly = parse_disassembly(DISASM);
    assert_eq!(disassembly.functions.len(), 5);
    assert_eq!(
        disassembly.functions.first().expect("first").name,
        "__raven_libc_start"
    );
    assert_eq!(
        disassembly
            .functions
            .first()
            .expect("first")
            .instructions
            .first()
            .expect("call")
            .call,
        Some(CallTarget::Symbol("stage2".to_string()))
    );
}

#[test]
fn reachability_counts_only_reachable_syscalls() {
    let disassembly = parse_disassembly(DISASM);
    let reachable = disassembly.reachable("__raven_libc_start");
    assert_eq!(disassembly.syscall_count(&reachable), 2);
    assert_eq!(disassembly.instruction_count(&reachable), 9);
}

#[test]
fn two_reachable_syscalls_fail_the_ceiling() {
    let disassembly = parse_disassembly(DISASM);
    let reachable = disassembly.reachable("__raven_libc_start");
    assert!(disassembly.syscall_count(&reachable) > SYSCALL_CEILING);
}

#[test]
fn unreachable_syscalls_are_not_counted() {
    let disassembly = parse_disassembly(DISASM);
    let reachable = disassembly.reachable("__raven_libc_start");
    assert!(!reachable.contains(&disassembly.index_of("unused").expect("unused")));
}

#[test]
fn a_call_cycle_terminates() {
    let text = "\
0000000000200000 <a>:
  200000: callq 0x200010 <b>
0000000000200010 <b>:
  200010: callq 0x200000 <a>
  200015: syscall
";
    let disassembly = parse_disassembly(text);
    let reachable = disassembly.reachable("a");
    assert_eq!(reachable.len(), 2);
    assert_eq!(disassembly.syscall_count(&reachable), 1);
}

#[test]
fn forbidden_start_references_are_called_out() {
    let text = "\
0000000000200000 <__raven_libc_start>:
  200000: callq 0x200010 <getenv>
  200005: callq 0x200020 <write>
0000000000200010 <getenv>:
  200010: retq
0000000000200020 <write>:
  200020: retq
";
    let disassembly = parse_disassembly(text);
    let reachable = disassembly.reachable("__raven_libc_start");
    let refs = disassembly.forbidden_refs(&reachable, FORBIDDEN_START_SYMBOLS);
    assert_eq!(refs, vec!["__raven_libc_start -> getenv"]);
}

#[test]
fn instruction_ceiling_is_250_plus_five_per_eight_entries() {
    assert_eq!(START_INSTRUCTION_CEILING, 290);
}

const ARCHIVE_NM: &str = "\
libc.a:start.o: 0000000000000000 T __raven_libc_start
libc.a:start.o:                  U exit
libc.a:exit.o: 0000000000000000 T exit
libc.a:exit.o:                  U getenv
libc.a:getenv.o: 0000000000000000 T getenv
libc.a:getenv.o:                  U malloc
libc.a:malloc.o: 0000000000000000 T malloc
libc.a:strlen.o: 0000000000000000 T strlen
";

#[test]
fn archive_edges_parse_defined_and_undefined() {
    let members = parse_nm_edges(ARCHIVE_NM);
    assert_eq!(members.len(), 5);
    let start = members
        .iter()
        .find(|member| member.member == "start.o")
        .expect("start");
    assert_eq!(start.defined, vec!["__raven_libc_start"]);
    assert_eq!(start.undefined, vec!["exit"]);
}

fn roots() -> MallocRoots {
    MallocRoots {
        roots: vec![
            "__raven_libc_start".to_string(),
            "exit".to_string(),
            "getenv".to_string(),
        ],
        allow: Vec::new(),
    }
}

#[test]
fn getenv_reaching_malloc_reports_the_path() {
    let paths = malloc_paths(&parse_nm_edges(ARCHIVE_NM), &roots());
    assert!(paths.iter().any(|path| path.contains("getenv.o -> malloc")));
    assert!(paths.iter().any(|path| path.starts_with("getenv ->")));
}

#[test]
fn an_allowlisted_root_is_not_traversed() {
    let mut roots = roots();
    roots.allow.push(AllowRoot {
        symbol: "getenv".to_string(),
        reason: "documented allocation".to_string(),
    });
    let paths = malloc_paths(&parse_nm_edges(ARCHIVE_NM), &roots);
    assert!(!paths.iter().any(|path| path.starts_with("getenv ->")));
}

#[test]
fn a_clean_graph_has_no_malloc_path() {
    let text = "\
libc.a:start.o: 0000000000000000 T __raven_libc_start
libc.a:start.o:                  U exit
libc.a:exit.o: 0000000000000000 T exit
libc.a:exit.o:                  U _exit
libc.a:_exit.o: 0000000000000000 T _exit
";
    assert!(malloc_paths(&parse_nm_edges(text), &roots()).is_empty());
}

#[test]
fn rodata_scan_finds_ld_and_etc() {
    let bytes = b"hello\0LD_PRELOAD\0/etc/localtime\0world";
    let found = scan_rodata(bytes);
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|value| value.starts_with("LD_")));
    assert!(found.iter().any(|value| value.starts_with("/etc/")));
}

#[test]
fn rodata_scan_is_clean_for_a_plain_blob() {
    assert!(scan_rodata(b"hello world").is_empty());
}

#[test]
fn member_patterns_catch_allocator_members() {
    let members = vec![
        "raven_libc_malloc-1.cgu.o".to_string(),
        "raven_libc_stdio-1.cgu.o".to_string(),
    ];
    let patterns: Vec<String> = MALLOC_MEMBER_PATTERNS
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    let matched = matching_members(&members, &patterns);
    assert_eq!(matched.len(), 1);
    assert!(crate::members::member_matches(&matched[0], &patterns));
}

#[test]
fn malloc_roots_load_from_toml() {
    let dir = crate::testutil::TempDir::new("boot-roots").expect("temp");
    let path = dir
        .write(
            "malloc-roots.toml",
            b"roots = [\"exit\"]\n[[allow]]\nsymbol = \"setenv\"\nreason = \"allocates\"\n",
        )
        .expect("write");
    let roots = load_roots(&path).expect("load");
    assert_eq!(roots.roots, vec!["exit"]);
    assert_eq!(roots.allow.first().expect("allow").symbol, "setenv");
}

#[test]
fn artifacts_path_matches_the_protocol() {
    let (binary, map) = artifacts(
        Path::new("/repo/build/size"),
        Arch::X86_64,
        crate::Set::Tiny,
        crate::Lang::Rust,
    );
    assert_eq!(
        binary,
        PathBuf::from("/repo/build/size/x86_64/tiny/static/rust/p0.elf")
    );
    assert_eq!(
        map,
        PathBuf::from("/repo/build/size/x86_64/tiny/static/rust/p0.map")
    );
}
