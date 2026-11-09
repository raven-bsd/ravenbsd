// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-exact and property tests for the initial stack.

mod common;

use common::{ElfOptions, STACK_TOP, build_elf, config, decode, parse_elf};
use linux_elf::{LinuxElfError, build_stack, plan_image};

const TOP: u64 = 0x8000_0000;

fn static_plan() -> linux_elf::ImagePlan<'static> {
    // The Vec is leaked so the borrowed Elf lives for the test process; tests are short-lived.
    let data: &'static [u8] = Box::leak(build_elf(&ElfOptions::default()).into_boxed_slice());
    let main: &'static raven_elf::Elf<'static> = Box::leak(Box::new(parse_elf(data)));
    plan_image(main, None, &config()).unwrap()
}

#[test]
fn exact_stack_for_no_args_and_no_environment() {
    let config = config();
    let plan = static_plan();
    let image = build_stack(TOP, &[], &[], b"/bin/init", &plan, &config).unwrap();

    let base = TOP - 352;
    assert_eq!(image.base, base);
    assert_eq!(image.sp, base);
    assert_eq!(image.bytes.len(), 0x160);

    let mut expected = vec![0u8; 0x160];
    put64(&mut expected, 0, 0);
    put64(&mut expected, 8, 0);
    put64(&mut expected, 16, 0);
    let auxv = [
        (3, 0x40_0040),
        (4, 56),
        (5, 3),
        (6, 4096),
        (7, 0),
        (8, 0),
        (9, 0x40_0000),
        (11, 1000),
        (12, 1000),
        (13, 1000),
        (14, 1000),
        (15, 0x7fff_ffef),
        (16, 0x1234),
        (17, 100),
        (23, 0),
        (25, 0x7fff_ffdf),
        (31, 0x7fff_fff6),
        (0, 0),
    ];
    for (index, (key, value)) in auxv.iter().enumerate() {
        let offset = 24 + index * 16;
        put64(&mut expected, offset, *key);
        put64(&mut expected, offset + 8, *value);
    }
    let strings = 0x13f;
    expected[strings..strings + 16].fill(0x11);
    expected[strings + 16..strings + 23].copy_from_slice(b"x86_64\0");
    expected[strings + 23..strings + 33].copy_from_slice(b"/bin/init\0");

    assert_eq!(image.bytes, expected);
}

#[test]
fn exact_stack_for_two_args_and_one_environment() {
    let config = config();
    let plan = static_plan();
    let image = build_stack(
        TOP,
        &[&b"one"[..], &b"two"[..]],
        &[&b"K=V"[..]],
        b"/bin/app",
        &plan,
        &config,
    )
    .unwrap();

    let base = TOP - 384;
    assert_eq!(image.base, base);
    assert_eq!(image.bytes.len(), 0x180);

    let mut expected = vec![0u8; 0x180];
    put64(&mut expected, 0, 2);
    put64(&mut expected, 8, 0x7fff_ffeb);
    put64(&mut expected, 16, 0x7fff_ffef);
    put64(&mut expected, 24, 0);
    put64(&mut expected, 32, 0x7fff_fff3);
    put64(&mut expected, 40, 0);
    let auxv = [
        (3, 0x40_0040),
        (4, 56),
        (5, 3),
        (6, 4096),
        (7, 0),
        (8, 0),
        (9, 0x40_0000),
        (11, 1000),
        (12, 1000),
        (13, 1000),
        (14, 1000),
        (15, 0x7fff_ffe4),
        (16, 0x1234),
        (17, 100),
        (23, 0),
        (25, 0x7fff_ffd4),
        (31, 0x7fff_fff7),
        (0, 0),
    ];
    for (index, (key, value)) in auxv.iter().enumerate() {
        let offset = 48 + index * 16;
        put64(&mut expected, offset, *key);
        put64(&mut expected, offset + 8, *value);
    }
    let strings = 0x154;
    expected[strings..strings + 16].fill(0x11);
    expected[strings + 16..strings + 23].copy_from_slice(b"x86_64\0");
    expected[strings + 23..strings + 27].copy_from_slice(b"one\0");
    expected[strings + 27..strings + 31].copy_from_slice(b"two\0");
    expected[strings + 31..strings + 35].copy_from_slice(b"K=V\0");
    expected[strings + 35..strings + 44].copy_from_slice(b"/bin/app\0");

    assert_eq!(image.bytes, expected);
}

#[test]
fn empty_strings_are_nul_terminated() {
    let config = config();
    let plan = static_plan();
    let image = build_stack(TOP, &[&b""[..], &b""[..]], &[&b""[..]], b"", &plan, &config).unwrap();
    let base = TOP - 368;
    assert_eq!(image.base, base);
    assert_eq!(image.bytes.len(), 0x170);

    let mut expected = vec![0u8; 0x170];
    put64(&mut expected, 0, 2);
    put64(&mut expected, 8, 0x7fff_fffc);
    put64(&mut expected, 16, 0x7fff_fffd);
    put64(&mut expected, 24, 0);
    put64(&mut expected, 32, 0x7fff_fffe);
    put64(&mut expected, 40, 0);
    let auxv = [
        (3, 0x40_0040),
        (4, 56),
        (5, 3),
        (6, 4096),
        (7, 0),
        (8, 0),
        (9, 0x40_0000),
        (11, 1000),
        (12, 1000),
        (13, 1000),
        (14, 1000),
        (15, 0x7fff_fff5),
        (16, 0x1234),
        (17, 100),
        (23, 0),
        (25, 0x7fff_ffe5),
        (31, 0x7fff_ffff),
        (0, 0),
    ];
    for (index, (key, value)) in auxv.iter().enumerate() {
        let offset = 48 + index * 16;
        put64(&mut expected, offset, *key);
        put64(&mut expected, offset + 8, *value);
    }
    expected[0x155..0x165].fill(0x11);
    expected[0x165..0x16c].copy_from_slice(b"x86_64\0");
    assert_eq!(image.bytes, expected);

    let decoded = decode(&image, TOP);
    assert_eq!(decoded.argc, 2);
    assert_eq!(decoded.argv, vec![Vec::<u8>::new(), Vec::new()]);
    assert_eq!(decoded.envp, vec![Vec::<u8>::new()]);
    for pointer in decoded.argv_pointers.iter().chain(&decoded.envp_pointers) {
        assert_eq!(image.bytes[(pointer - image.base) as usize], 0);
    }
}

#[test]
fn alignment_and_roundtrip_hold_for_500_random_argument_sets() {
    let config = config();
    let plan = static_plan();
    let mut seed = 0x1234_5678_9abc_def0u64;

    for _ in 0..500 {
        let argc = (next(&mut seed) % 5) as usize;
        let envc = (next(&mut seed) % 5) as usize;
        let argv_owned: Vec<Vec<u8>> = (0..argc)
            .map(|_| {
                let length = (next(&mut seed) % 20) as usize;
                random_string(&mut seed, length)
            })
            .collect();
        let envp_owned: Vec<Vec<u8>> = (0..envc)
            .map(|_| {
                let length = (next(&mut seed) % 20) as usize;
                random_string(&mut seed, length)
            })
            .collect();
        let argv: Vec<&[u8]> = argv_owned.iter().map(Vec::as_slice).collect();
        let envp: Vec<&[u8]> = envp_owned.iter().map(Vec::as_slice).collect();
        let execfn = random_string(&mut seed, 10);
        let image = build_stack(STACK_TOP, &argv, &envp, &execfn, &plan, &config).unwrap();
        let decoded = decode(&image, STACK_TOP);

        assert_eq!(decoded.argc as usize, argv.len());
        assert_eq!(decoded.argv, argv_owned);
        assert_eq!(decoded.envp, envp_owned);
        let execfn_pointer = common::auxv_value(&decoded.auxv, 31).expect("AT_EXECFN");
        assert!(execfn_pointer >= image.base);
        let offset = (execfn_pointer - image.base) as usize;
        assert_eq!(&image.bytes[offset..offset + execfn.len()], &execfn);
        assert_eq!(image.bytes[offset + execfn.len()], 0);
    }
}

#[test]
fn too_large_stack_is_rejected() {
    let config = config();
    let plan = static_plan();
    let huge = vec![b'a'; 1_048_576];
    assert_eq!(
        build_stack(TOP, &[&huge[..]], &[], b"/bin/x", &plan, &config),
        Err(LinuxElfError::TooLarge)
    );
}

#[test]
fn nul_in_argv_is_rejected() {
    let config = config();
    let plan = static_plan();
    assert_eq!(
        build_stack(TOP, &[&b"a\0b"[..]], &[], b"/bin/x", &plan, &config),
        Err(LinuxElfError::EmbeddedNul)
    );
}

#[test]
fn nul_in_execfn_is_rejected() {
    let config = config();
    let plan = static_plan();
    assert_eq!(
        build_stack(TOP, &[], &[], b"/bin/\0x", &plan, &config),
        Err(LinuxElfError::EmbeddedNul)
    );
}

#[test]
fn stack_near_zero_overflows() {
    let config = config();
    let plan = static_plan();
    assert_eq!(
        build_stack(16, &[], &[], b"/bin/x", &plan, &config),
        Err(LinuxElfError::AddressOverflow)
    );
}

#[test]
fn random_bytes_are_at_at_random() {
    let config = config();
    let plan = static_plan();
    let image = build_stack(TOP, &[], &[], b"/bin/x", &plan, &config).unwrap();
    let decoded = decode(&image, TOP);
    let random = common::auxv_value(&decoded.auxv, 25).expect("AT_RANDOM");
    let offset = (random - image.base) as usize;
    assert_eq!(&image.bytes[offset..offset + 16], &config.random);
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn next(seed: &mut u64) -> u64 {
    let mut value = *seed;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *seed = value;
    value
}

fn random_string(seed: &mut u64, length: usize) -> Vec<u8> {
    (0..length).map(|_| (next(seed) % 255 + 1) as u8).collect()
}
