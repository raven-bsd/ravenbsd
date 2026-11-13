// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn min_alignment_is_sixteen() {
    assert_eq!(MIN_TLS_ALIGN, 16);
    assert!(MIN_TLS_ALIGN.is_power_of_two());
}

#[test]
fn max_alignment_is_a_page() {
    assert_eq!(MAX_TLS_ALIGN, 4096);
    assert!(MAX_TLS_ALIGN.is_power_of_two());
}

#[test]
fn headroom_is_two_words() {
    assert_eq!(DTV_HEADROOM, 2 * core::mem::size_of::<usize>());
}

#[test]
fn target_abi_matches_cfg() {
    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(TARGET_ABI, Abi::VariantII { dtp_offset: 0 });
        assert!(!core::hint::black_box(TLS_ABOVE_TP));
        assert_eq!(GAP_ABOVE_TP, 0);
        assert_eq!(DTP_OFFSET, 0);
    }
    #[cfg(target_arch = "aarch64")]
    {
        assert_eq!(
            TARGET_ABI,
            Abi::VariantI {
                gap_above_tp: 16,
                dtp_offset: 0
            }
        );
        assert!(TLS_ABOVE_TP);
        assert_eq!(GAP_ABOVE_TP, 16);
        assert_eq!(DTP_OFFSET, 0);
    }
    #[cfg(target_arch = "riscv64")]
    {
        assert_eq!(
            TARGET_ABI,
            Abi::VariantI {
                gap_above_tp: 0,
                dtp_offset: 0x800
            }
        );
        assert!(TLS_ABOVE_TP);
        assert_eq!(GAP_ABOVE_TP, 0);
        assert_eq!(DTP_OFFSET, 0x800);
    }
}

#[test]
fn abi_gap_and_dtp_accessors() {
    let two = Abi::VariantII { dtp_offset: 0 };
    let one = Abi::VariantI {
        gap_above_tp: 16,
        dtp_offset: 0x800,
    };
    assert_eq!(two.gap_above_tp(), 0);
    assert_eq!(two.dtp_offset(), 0);
    assert!(!two.is_above_tp());
    assert_eq!(one.gap_above_tp(), 16);
    assert_eq!(one.dtp_offset(), 0x800);
    assert!(one.is_above_tp());
}

#[test]
fn builtin_tls_is_one_page() {
    assert_eq!(BUILTIN_TLS_SIZE, 4096);
    assert!(core::hint::black_box(BUILTIN_TLS_SIZE) >= MIN_TLS_ALIGN);
}

#[test]
fn module_limit_is_sixteen() {
    assert_eq!(MAX_TLS_MODULES, 16);
}

#[test]
fn tp_offset_is_zero() {
    assert_eq!(TP_OFFSET, 0);
}
