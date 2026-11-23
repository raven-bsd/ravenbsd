// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the process command-name helpers.

use super::*;
use alloc::vec;

#[test]
fn truncation_is_byte_bounded_and_keeps_utf8() {
    let text = "a".repeat(MAX_COMMAND + 20);
    assert_eq!(truncate_command(&text).len(), MAX_COMMAND);

    let wide = "é".repeat(MAX_COMMAND);
    let truncated = truncate_command(&wide);
    assert!(truncated.len() <= MAX_COMMAND);
    assert!(truncated.is_char_boundary(truncated.len()));
}

#[test]
fn argv_zero_wins_over_the_path() {
    let argv = vec![b"shell".to_vec(), b"-c".to_vec()];
    assert_eq!(command_from_argv(&argv, b"/bin/shell"), "shell");
}

#[test]
fn an_empty_argv_falls_back_to_the_path() {
    assert_eq!(command_from_argv(&[], b"/sbin/init"), "/sbin/init");
}

#[test]
fn invalid_utf8_falls_back_to_the_path() {
    let argv = vec![vec![0xff, 0xfe]];
    assert_eq!(command_from_argv(&argv, b"/bin/bad"), "/bin/bad");
}

#[test]
fn c8_start_pins_stack_layout_and_register() {
    let stack_base = 0x0000_7fff_0000_0000u64;
    let mut buffer = vec![0u8; USER_STACK_SIZE as usize];
    let path = b"/init";
    let argv: [&[u8]; 2] = [b"/init", b"arg"];
    let envp: [&[u8]; 1] = [b"K=V"];
    let ids = ProcessIds {
        ppid: 0,
        pgid: 1,
        sid: 1,
        uid: 10,
        euid: 11,
        gid: 12,
        egid: 13,
        suid: 10,
        sgid: 12,
        groups: [0; MAX_GROUPS],
        group_count: 0,
    };
    let aux = StackAuxv {
        phdr: 0x40_0000,
        base: 0x50_0000,
        entry: 0x40_1000,
        phnum: 7,
    };
    let sp0 = build_stack_image(&mut buffer, stack_base, path, &argv, &envp, &ids, aux)
        .expect("C8 stack image");
    assert_eq!(sp0 % 16, 0, "sp0 is 16-byte aligned");
    assert!(sp0 >= stack_base);
    assert!(sp0 < stack_base.saturating_add(USER_STACK_SIZE));

    let offset = usize::try_from(sp0 - stack_base).expect("stack offset");
    let word = |index: usize| -> u64 {
        let start = offset + index * 8;
        u64::from_le_bytes(buffer[start..start + 8].try_into().expect("word"))
    };
    let string = |address: u64| -> &[u8] {
        let start = usize::try_from(address - stack_base).expect("string offset");
        let end = start
            + buffer[start..]
                .iter()
                .position(|byte| *byte == 0)
                .expect("NUL");
        &buffer[start..end]
    };

    assert_eq!(word(0), 2, "argc");
    assert_eq!(word(3), 0, "argv NULL");
    assert_eq!(word(5), 0, "envp NULL");
    assert_eq!(string(word(1)), b"/init");
    assert_eq!(string(word(2)), b"arg");
    assert_eq!(string(word(4)), b"K=V");

    let auxv = 6usize;
    let pairs = [
        (3u64, aux.phdr),
        (4, 56),
        (5, aux.phnum),
        (6, PAGE),
        (7, aux.base),
        (8, 0),
        (9, aux.entry),
        (11, u64::from(ids.uid)),
        (12, u64::from(ids.euid)),
        (13, u64::from(ids.gid)),
        (14, u64::from(ids.egid)),
        (15, word(auxv + 2 * 11 + 1)),
        (16, 0),
        (17, 100),
        (23, 0),
        (25, word(auxv + 2 * 15 + 1)),
        (26, 0),
        (31, word(auxv + 2 * 17 + 1)),
        (51, 8192),
    ];
    for (index, (key, value)) in pairs.iter().enumerate() {
        assert_eq!(word(auxv + 2 * index), *key, "auxv key {index}");
        assert_eq!(word(auxv + 2 * index + 1), *value, "auxv value {index}");
    }
    assert_eq!(word(auxv + 38), 0, "auxv terminator key");
    assert_eq!(word(auxv + 39), 0, "auxv terminator value");
    assert_eq!(string(word(auxv + 2 * 11 + 1)), platform_name().as_bytes());
    assert_eq!(string(word(auxv + 2 * 17 + 1)), path);
    let random = word(auxv + 2 * 15 + 1);
    assert!(
        random == 0 || (random >= stack_base && random < stack_base + USER_STACK_SIZE),
        "AT_RANDOM is zero or inside the stack"
    );

    let (stack_top, arg) = user_start_args(sp0);
    assert_eq!(stack_top, sp0, "C8 stack pointer is rsp/sp");
    assert_eq!(arg, 0, "C8 argument register is zero");
}
