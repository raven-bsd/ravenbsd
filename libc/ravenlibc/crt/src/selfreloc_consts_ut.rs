// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: first-party tests of the transpiled self-relocator (no donor text).

use super::*;

#[test]
fn current_architecture_numbers() {
    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(REL_RELATIVE, REL_RELATIVE_X86_64);
        assert_eq!(REL_IRELATIVE, REL_IRELATIVE_X86_64);
        assert_eq!(REL_RELATIVE, 8);
        assert_eq!(REL_IRELATIVE, 37);
    }
    #[cfg(target_arch = "aarch64")]
    {
        assert_eq!(REL_RELATIVE, REL_RELATIVE_AARCH64);
        assert_eq!(REL_IRELATIVE, REL_IRELATIVE_AARCH64);
    }
    #[cfg(target_arch = "riscv64")]
    {
        assert_eq!(REL_RELATIVE, REL_RELATIVE_RISCV64);
        assert_eq!(REL_IRELATIVE, REL_IRELATIVE_RISCV64);
    }
}

#[test]
fn all_three_architectures_have_distinct_constants() {
    assert_eq!(REL_RELATIVE_X86_64, 8);
    assert_eq!(REL_IRELATIVE_X86_64, 37);
    assert_eq!(REL_RELATIVE_AARCH64, 1027);
    assert_eq!(REL_IRELATIVE_AARCH64, 1032);
    assert_eq!(REL_RELATIVE_RISCV64, 3);
    assert_eq!(REL_IRELATIVE_RISCV64, 58);
    assert_ne!(REL_RELATIVE_X86_64, REL_RELATIVE_AARCH64);
    assert_ne!(REL_RELATIVE_AARCH64, REL_RELATIVE_RISCV64);
}

#[test]
fn r_type_keeps_the_low_31_bits() {
    assert_eq!(r_type(0), 0);
    assert_eq!(r_type(8), 8);
    assert_eq!(r_type(0x8000_0008), 8);
    assert_eq!(r_type(usize::MAX), 0x7fff_ffff);
    assert_eq!(r_type(0x0000_0001_0000_0000), 0);
}

#[test]
fn relative_and_irelative_are_not_confused() {
    assert_ne!(REL_RELATIVE, REL_IRELATIVE);
    assert_ne!(r_type(REL_RELATIVE), r_type(REL_IRELATIVE));
}
