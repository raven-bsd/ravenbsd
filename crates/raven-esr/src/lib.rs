// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 exception syndrome (`ESR_EL1`), saved program status (`SPSR_EL1`) and exception
//! vector slot decoding.
//!
//! The crate is pure: it only unpacks values captured by the kernel's exception entry code. Field
//! positions come from the ARMv8-A Architecture Reference Manual (`ESR_EL1`, `SPSR_EL1` and the
//! 16-entry exception vector table layout).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// A saved `ESR_EL1` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Esr(u64);

impl Esr {
    /// Wrap a raw `ESR_EL1` value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw register value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Exception class, `ESR_EL1.EC` (bits 31:26).
    pub const fn ec(self) -> u8 {
        ((self.0 >> 26) & 0x3f) as u8
    }

    /// Instruction length bit, `ESR_EL1.IL` (bit 25).
    pub const fn il(self) -> bool {
        (self.0 >> 25) & 1 != 0
    }

    /// Instruction specific syndrome, `ESR_EL1.ISS` (bits 24:0).
    pub const fn iss(self) -> u32 {
        (self.0 & 0x01ff_ffff) as u32
    }
}

impl From<u64> for Esr {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

/// Human-readable name of an exception class.
///
/// The numbers not listed here are reported as `"reserved"`.
pub const fn ec_name(ec: u8) -> &'static str {
    match ec {
        0x00 => "unknown",
        0x01 => "WFI/WFE",
        0x07 => "FP/SIMD access",
        0x0e => "illegal execution state",
        0x15 => "SVC (AArch64)",
        0x20 => "instruction abort lower EL",
        0x21 => "instruction abort same EL",
        0x22 => "PC alignment fault",
        0x24 => "data abort lower EL",
        0x25 => "data abort same EL",
        0x26 => "SP alignment fault",
        0x2f => "SError",
        0x30 => "breakpoint lower EL",
        0x31 => "breakpoint same EL",
        0x32 => "software step lower EL",
        0x33 => "software step same EL",
        0x34 => "watchpoint lower EL",
        0x35 => "watchpoint same EL",
        0x3c => "BRK",
        _ => "reserved",
    }
}

/// Decoded instruction specific syndrome of a data abort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataAbort {
    /// The faulting access was a write (`ESR_EL1.ISS.WnR`).
    pub write: bool,
    /// Data fault status code (`ESR_EL1.ISS.DFSC`).
    pub dfsc: u8,
    /// `FAR_EL1` holds a valid fault address (`ESR_EL1.ISS.FnV` is clear).
    pub far_valid: bool,
}

/// Decode the instruction specific syndrome of a data abort.
pub const fn data_abort(iss: u32) -> DataAbort {
    DataAbort {
        write: (iss >> 6) & 1 != 0,
        dfsc: (iss & 0x3f) as u8,
        far_valid: (iss >> 10) & 1 == 0,
    }
}

/// Human-readable name of a data fault status code.
pub const fn dfsc_name(dfsc: u8) -> &'static str {
    match dfsc {
        0x04 => "translation fault level 0",
        0x05 => "translation fault level 1",
        0x06 => "translation fault level 2",
        0x07 => "translation fault level 3",
        0x09 => "access flag fault level 1",
        0x0a => "access flag fault level 2",
        0x0b => "access flag fault level 3",
        0x0d => "permission fault level 1",
        0x0e => "permission fault level 2",
        0x0f => "permission fault level 3",
        0x10 => "synchronous external abort",
        0x21 => "alignment fault",
        _ => "other",
    }
}

/// A saved `SPSR_EL1` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spsr(u64);

impl Spsr {
    /// Wrap a raw `SPSR_EL1` value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw register value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Exception level the interrupted context ran at (`SPSR_EL1.M[3:2]`).
    pub const fn el(self) -> u8 {
        ((self.0 >> 2) & 0x3) as u8
    }

    /// Whether the interrupted context used `SP_ELx` (`SPSR_EL1.M[0]` set).
    pub const fn uses_sp_elx(self) -> bool {
        self.0 & 1 != 0
    }

    /// The DAIF mask bits as a four-bit value (`D=8, A=4, I=2, F=1`).
    pub const fn daif(self) -> u8 {
        ((self.0 >> 6) & 0xf) as u8
    }

    /// Debug exception mask (`SPSR_EL1.D`).
    pub const fn d(self) -> bool {
        self.daif() & 0b1000 != 0
    }

    /// SError mask (`SPSR_EL1.A`).
    pub const fn a(self) -> bool {
        self.daif() & 0b0100 != 0
    }

    /// IRQ mask (`SPSR_EL1.I`).
    pub const fn i(self) -> bool {
        self.daif() & 0b0010 != 0
    }

    /// FIQ mask (`SPSR_EL1.F`).
    pub const fn f(self) -> bool {
        self.daif() & 0b0001 != 0
    }

    /// The condition flag bits as a four-bit value (`N=8, Z=4, C=2, V=1`).
    pub const fn nzcv(self) -> u8 {
        ((self.0 >> 28) & 0xf) as u8
    }

    /// Negative flag (`SPSR_EL1.N`).
    pub const fn n(self) -> bool {
        self.nzcv() & 0b1000 != 0
    }

    /// Zero flag (`SPSR_EL1.Z`).
    pub const fn z(self) -> bool {
        self.nzcv() & 0b0100 != 0
    }

    /// Carry flag (`SPSR_EL1.C`).
    pub const fn c(self) -> bool {
        self.nzcv() & 0b0010 != 0
    }

    /// Overflow flag (`SPSR_EL1.V`).
    pub const fn v(self) -> bool {
        self.nzcv() & 0b0001 != 0
    }
}

impl From<u64> for Spsr {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

/// Name of one of the 16 exception vector slots, in architectural order.
///
/// `sync-cur-sp0` is slot 0, `serror-lower32` is slot 15. An out-of-range index is `"unknown"`.
pub const fn slot_name(index: usize) -> &'static str {
    match index {
        0 => "sync-cur-sp0",
        1 => "irq-cur-sp0",
        2 => "fiq-cur-sp0",
        3 => "serror-cur-sp0",
        4 => "sync-cur-spx",
        5 => "irq-cur-spx",
        6 => "fiq-cur-spx",
        7 => "serror-cur-spx",
        8 => "sync-lower64",
        9 => "irq-lower64",
        10 => "fiq-lower64",
        11 => "serror-lower64",
        12 => "sync-lower32",
        13 => "irq-lower32",
        14 => "fiq-lower32",
        15 => "serror-lower32",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{DataAbort, Esr, Spsr, data_abort, dfsc_name, ec_name, slot_name};

    #[test]
    fn esr_fields_are_unpacked() {
        // EC 0x3c, IL set, ISS 0x13: the `brk #0x13` syndrome.
        let esr = Esr::new(0xf200_0013);
        assert_eq!(esr.ec(), 0x3c);
        assert!(esr.il());
        assert_eq!(esr.iss(), 0x13);
        assert_eq!(esr.raw(), 0xf200_0013);
    }

    #[test]
    fn esr_il_clear() {
        let esr = Esr::from(0xf000_0000);
        assert_eq!(esr.ec(), 0x3c);
        assert!(!esr.il());
        assert_eq!(esr.iss(), 0);
    }

    #[test]
    fn ec_names_system_classes() {
        assert_eq!(ec_name(0x00), "unknown");
        assert_eq!(ec_name(0x01), "WFI/WFE");
        assert_eq!(ec_name(0x07), "FP/SIMD access");
        assert_eq!(ec_name(0x0e), "illegal execution state");
        assert_eq!(ec_name(0x15), "SVC (AArch64)");
        assert_eq!(ec_name(0x26), "SP alignment fault");
        assert_eq!(ec_name(0x2f), "SError");
    }

    #[test]
    fn ec_names_memory_faults() {
        assert_eq!(ec_name(0x20), "instruction abort lower EL");
        assert_eq!(ec_name(0x21), "instruction abort same EL");
        assert_eq!(ec_name(0x22), "PC alignment fault");
        assert_eq!(ec_name(0x24), "data abort lower EL");
        assert_eq!(ec_name(0x25), "data abort same EL");
    }

    #[test]
    fn ec_names_debug_classes() {
        assert_eq!(ec_name(0x30), "breakpoint lower EL");
        assert_eq!(ec_name(0x31), "breakpoint same EL");
        assert_eq!(ec_name(0x32), "software step lower EL");
        assert_eq!(ec_name(0x33), "software step same EL");
        assert_eq!(ec_name(0x34), "watchpoint lower EL");
        assert_eq!(ec_name(0x35), "watchpoint same EL");
    }

    #[test]
    fn ec_name_brk_is_exact() {
        assert_eq!(ec_name(0x3c), "BRK");
    }

    #[test]
    fn reserved_ec_is_named_reserved() {
        for ec in [
            0x02, 0x03, 0x06, 0x08, 0x10, 0x23, 0x27, 0x2e, 0x36, 0x3b, 0x3d, 0x3f,
        ] {
            assert_eq!(ec_name(ec), "reserved", "EC {ec:#x}");
        }
    }

    #[test]
    fn data_abort_bits_are_decoded() {
        // WnR = 1 (bit 6), FnV = 1 (bit 10), DFSC = permission fault level 3 (0x0f).
        let abort = data_abort((1 << 6) | (1 << 10) | 0x0f);
        assert_eq!(
            abort,
            DataAbort {
                write: true,
                dfsc: 0x0f,
                far_valid: false,
            }
        );
        let abort = data_abort(0x06);
        assert!(!abort.write);
        assert!(abort.far_valid);
        assert_eq!(abort.dfsc, 0x06);
    }

    #[test]
    fn dfsc_translation_faults() {
        assert_eq!(dfsc_name(0x04), "translation fault level 0");
        assert_eq!(dfsc_name(0x05), "translation fault level 1");
        assert_eq!(dfsc_name(0x06), "translation fault level 2");
        assert_eq!(dfsc_name(0x07), "translation fault level 3");
    }

    #[test]
    fn dfsc_access_flag_faults() {
        assert_eq!(dfsc_name(0x09), "access flag fault level 1");
        assert_eq!(dfsc_name(0x0a), "access flag fault level 2");
        assert_eq!(dfsc_name(0x0b), "access flag fault level 3");
    }

    #[test]
    fn dfsc_permission_faults() {
        assert_eq!(dfsc_name(0x0d), "permission fault level 1");
        assert_eq!(dfsc_name(0x0e), "permission fault level 2");
        assert_eq!(dfsc_name(0x0f), "permission fault level 3");
    }

    #[test]
    fn dfsc_external_and_alignment() {
        assert_eq!(dfsc_name(0x10), "synchronous external abort");
        assert_eq!(dfsc_name(0x21), "alignment fault");
    }

    #[test]
    fn dfsc_unknown_is_other() {
        for dfsc in [0x00, 0x01, 0x02, 0x03, 0x08, 0x0c, 0x11, 0x20, 0x22, 0x3f] {
            assert_eq!(dfsc_name(dfsc), "other", "DFSC {dfsc:#x}");
        }
    }

    #[test]
    fn spsr_exception_level() {
        assert_eq!(Spsr::new(0b0100).el(), 1); // EL1t
        assert_eq!(Spsr::new(0b0101).el(), 1); // EL1h
        assert_eq!(Spsr::new(0b0000).el(), 0); // EL0t
        assert_eq!(Spsr::new(0b1111).el(), 3);
    }

    #[test]
    fn spsr_stack_pointer_selection() {
        assert!(Spsr::new(0b0101).uses_sp_elx());
        assert!(!Spsr::new(0b0100).uses_sp_elx());
        assert!(!Spsr::new(0b0000).uses_sp_elx());
    }

    #[test]
    fn spsr_daif_bits() {
        let spsr = Spsr::new(0b1111 << 6);
        assert_eq!(spsr.daif(), 0b1111);
        assert!(spsr.d() && spsr.a() && spsr.i() && spsr.f());
        let clear = Spsr::new(0);
        assert_eq!(clear.daif(), 0);
        assert!(!clear.d() && !clear.a() && !clear.i() && !clear.f());
    }

    #[test]
    fn spsr_nzcv_bits() {
        let spsr = Spsr::new(0b1010 << 28);
        assert_eq!(spsr.nzcv(), 0b1010);
        assert!(spsr.n());
        assert!(!spsr.z());
        assert!(spsr.c());
        assert!(!spsr.v());
        assert_eq!(Spsr::new(0).nzcv(), 0);
    }

    #[test]
    fn every_vector_slot_has_a_name() {
        let expected = [
            "sync-cur-sp0",
            "irq-cur-sp0",
            "fiq-cur-sp0",
            "serror-cur-sp0",
            "sync-cur-spx",
            "irq-cur-spx",
            "fiq-cur-spx",
            "serror-cur-spx",
            "sync-lower64",
            "irq-lower64",
            "fiq-lower64",
            "serror-lower64",
            "sync-lower32",
            "irq-lower32",
            "fiq-lower32",
            "serror-lower32",
        ];
        for (index, name) in expected.iter().enumerate() {
            assert_eq!(slot_name(index), *name, "slot {index}");
        }
    }

    #[test]
    fn slot_index_out_of_range_is_unknown() {
        assert_eq!(slot_name(16), "unknown");
        assert_eq!(slot_name(usize::MAX), "unknown");
    }
}
