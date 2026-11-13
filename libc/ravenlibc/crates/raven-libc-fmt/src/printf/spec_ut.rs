// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn flag_bits_do_not_overlap() {
    let all = [
        ALT, LADJUST, LONGDBL, LONGINT, LLONGINT, SHORTINT, ZEROPAD, FPT, GROUPING, SIZET,
        PTRDIFFT, INTMAXT, CHARINT, FASTINT,
    ];
    for (i, &a) in all.iter().enumerate() {
        for &b in all.iter().skip(i + 1) {
            assert_eq!(a & b, 0, "flags {a:#x} and {b:#x} overlap");
        }
    }
    assert_eq!(INTMAX_SIZE, INTMAXT | SIZET | PTRDIFFT | LLONGINT);
    assert_eq!(DEFPREC, 6);
    assert_eq!(INT_BUF, 64);
    assert_eq!(STATIC_ARG_TBL_SIZE, 8);
}

#[test]
fn type_table_rejects_a_conflict() {
    let mut table = TypeTable::new();
    assert!(table.add(TypeId::Int).is_ok());
    table.nextarg = 1;
    assert_eq!(
        table.add(TypeId::Double),
        Err(crate::types::FmtErr::Invalid)
    );
}

#[test]
fn type_table_accepts_a_repeated_same_type() {
    let mut table = TypeTable::new();
    assert!(table.add(TypeId::Double).is_ok());
    assert_eq!(table.nextarg, 2);
    assert_eq!(table.tablemax, 1);
}

#[test]
fn type_table_stops_at_nl_argmax() {
    let mut table = TypeTable::new();
    table.nextarg = NL_ARGMAX as u32;
    assert!(table.add(TypeId::Int).is_ok());
    assert_eq!(table.add(TypeId::Int), Err(crate::types::FmtErr::Invalid));
}

#[test]
fn arg_classes_match_the_ids() {
    assert_eq!(TypeId::Int.arg(), ArgClass::Integer);
    assert_eq!(TypeId::Double.arg(), ArgClass::Double);
    assert_eq!(TypeId::LongDouble.arg(), ArgClass::LongDouble);
    assert_eq!(TypeId::PInt.arg(), ArgClass::Pointer);
    assert_eq!(TypeId::WInt.arg(), ArgClass::WInt);
    assert_eq!(TypeId::Unused.arg(), ArgClass::Unused);
}
