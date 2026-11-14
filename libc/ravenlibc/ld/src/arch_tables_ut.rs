// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_elf::Arch;
use raven_libc_elf::RelocAction;

#[test]
fn x86_64_relative_is_relative() {
    assert_eq!(x86_64::reloc_action(8), RelocAction::Relative);
}

#[test]
fn x86_64_glob_dat_is_glob_dat() {
    assert_eq!(x86_64::reloc_action(6), RelocAction::GlobDat);
}

#[test]
fn x86_64_jump_slot_is_jump_slot() {
    assert_eq!(x86_64::reloc_action(7), RelocAction::JumpSlot);
}

#[test]
fn x86_64_irelative_is_irelative() {
    assert_eq!(x86_64::reloc_action(37), RelocAction::IRelative);
}

#[test]
fn x86_64_pc32_is_a_pc_relative_action() {
    assert_eq!(x86_64::reloc_action(2), RelocAction::Pc32);
}

#[test]
fn x86_64_dtpmod_is_dtpmod() {
    assert_eq!(x86_64::reloc_action(16), RelocAction::DtpMod64);
}

#[test]
fn x86_64_dtpoff_is_dtpoff() {
    assert_eq!(x86_64::reloc_action(17), RelocAction::DtpOff64);
}

#[test]
fn x86_64_tpoff_is_tpoff() {
    assert_eq!(x86_64::reloc_action(18), RelocAction::TpOff64);
}

#[test]
fn aarch64_abs64_is_abs64() {
    assert_eq!(aarch64::reloc_action(257), RelocAction::Abs64 { sym: true });
}

#[test]
fn aarch64_glob_dat_is_glob_dat() {
    assert_eq!(aarch64::reloc_action(1025), RelocAction::GlobDat);
}

#[test]
fn aarch64_jump_slot_is_jump_slot() {
    assert_eq!(aarch64::reloc_action(1026), RelocAction::JumpSlot);
}

#[test]
fn aarch64_relative_is_relative() {
    assert_eq!(aarch64::reloc_action(1027), RelocAction::Relative);
}

#[test]
fn aarch64_dtpmod_is_number_1028() {
    assert_eq!(aarch64::reloc_action(1028), RelocAction::DtpMod64);
}

#[test]
fn aarch64_dtpoff_is_number_1029() {
    assert_eq!(aarch64::reloc_action(1029), RelocAction::DtpOff64);
}

#[test]
fn aarch64_tpoff_is_number_1030() {
    assert_eq!(aarch64::reloc_action(1030), RelocAction::TpOff64);
}

#[test]
fn aarch64_irelative_is_irelative() {
    assert_eq!(aarch64::reloc_action(1032), RelocAction::IRelative);
}

#[test]
fn riscv_64_is_abs64() {
    assert_eq!(riscv64::reloc_action(2), RelocAction::Abs64 { sym: true });
}

#[test]
fn riscv_relative_is_relative() {
    assert_eq!(riscv64::reloc_action(3), RelocAction::Relative);
}

#[test]
fn riscv_jump_slot_is_jump_slot() {
    assert_eq!(riscv64::reloc_action(5), RelocAction::JumpSlot);
}

#[test]
fn riscv_dtpmod_is_dtpmod() {
    assert_eq!(riscv64::reloc_action(7), RelocAction::DtpMod64);
}

#[test]
fn riscv_dtpoff_is_dtpoff() {
    assert_eq!(riscv64::reloc_action(9), RelocAction::DtpOff64);
}

#[test]
fn riscv_tpoff_is_tpoff() {
    assert_eq!(riscv64::reloc_action(11), RelocAction::TpOff64);
}

#[test]
fn riscv_irelative_is_irelative() {
    assert_eq!(riscv64::reloc_action(58), RelocAction::IRelative);
}

#[test]
fn riscv_tlsdesc_is_rejected() {
    assert!(matches!(
        riscv64::reloc_action(0xdead),
        RelocAction::Reject(_)
    ));
}

#[test]
fn the_target_arch_is_a_supported_arch() {
    assert!(matches!(
        target_arch(),
        Arch::X86_64 | Arch::Aarch64 | Arch::Riscv64
    ));
}
