// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::SliceVa;

fn analyse_ok(fmt: &str) -> Analysis {
    analyse::<u8>(fmt.as_bytes()).expect("valid positional format")
}

#[test]
fn plain_format_has_no_table() {
    let a = analyse_ok("%d %s %f");
    assert!(a.table.is_none());
}

#[test]
fn positional_conversions_build_the_table() {
    let a = analyse_ok("%1$s %2$d");
    let t = a.table.expect("table");
    assert_eq!(t.tablemax, 2);
    assert_eq!(t.types.get(1), Some(&TypeId::PChar));
    assert_eq!(t.types.get(2), Some(&TypeId::Int));
}

#[test]
fn star_positions_are_allowed() {
    let a = analyse_ok("%2$*1$d");
    let t = a.table.expect("table");
    assert_eq!(t.tablemax, 2);
    assert_eq!(t.types.get(1), Some(&TypeId::Int));
    assert_eq!(t.types.get(2), Some(&TypeId::Int));
}

#[test]
fn a_gap_is_invalid() {
    assert!(analyse::<u8>(b"%2$d").is_err());
}

#[test]
fn mixing_is_invalid_in_both_orders() {
    assert!(analyse::<u8>(b"%1$d %d").is_err());
    assert!(analyse::<u8>(b"%d %1$d").is_err());
    assert!(analyse::<u8>(b"%1$d %*d").is_err());
    assert!(analyse::<u8>(b"%*1$d %d").is_err());
}

#[test]
fn a_type_conflict_is_invalid() {
    assert!(analyse::<u8>(b"%1$d %1$f").is_err());
    assert!(analyse::<u8>(b"%1$s %1$d").is_err());
}

#[test]
fn more_than_nl_argmax_is_invalid() {
    let fmt = "%65$d";
    assert!(analyse::<u8>(fmt.as_bytes()).is_err());
}

#[test]
fn length_modifiers_pick_the_type_ids() {
    let a = analyse_ok("%1$hhd %2$hd %3$ld %4$lld %5$jd %6$zd %7$td %8$Lf");
    let t = a.table.expect("table");
    assert_eq!(t.types.get(1), Some(&TypeId::Int));
    assert_eq!(t.types.get(2), Some(&TypeId::Int));
    assert_eq!(t.types.get(3), Some(&TypeId::Long));
    assert_eq!(t.types.get(4), Some(&TypeId::LLong));
    assert_eq!(t.types.get(5), Some(&TypeId::IntMaxT));
    assert_eq!(t.types.get(6), Some(&TypeId::SSizeT));
    assert_eq!(t.types.get(7), Some(&TypeId::PtrDiffT));
    assert_eq!(t.types.get(8), Some(&TypeId::LongDouble));
}

#[test]
fn n_types_are_pointers() {
    let a = analyse_ok("%1$hhn %2$hn %3$n %4$ln %5$lln %6$jn %7$zn %8$tn");
    let t = a.table.expect("table");
    assert_eq!(t.types.get(1), Some(&TypeId::PSChar));
    assert_eq!(t.types.get(2), Some(&TypeId::PShort));
    assert_eq!(t.types.get(3), Some(&TypeId::PInt));
    assert_eq!(t.types.get(4), Some(&TypeId::PLong));
    assert_eq!(t.types.get(5), Some(&TypeId::PLLong));
    assert_eq!(t.types.get(6), Some(&TypeId::PIntMaxT));
    assert_eq!(t.types.get(7), Some(&TypeId::PSSizeT));
    assert_eq!(t.types.get(8), Some(&TypeId::PPtrDiffT));
}

#[test]
fn fetch_reads_each_class_once_in_order() {
    let a = analyse_ok("%1$d %2$f %3$s %4$Lf %5$lc");
    let t = a.table.expect("table");
    let mut va = SliceVa::new();
    let s = va.cstr("x");
    va.int(42).dbl_bits(0x3ff8_0000_0000_0000).ptr(s);
    va.ld_bits(1).int(7);
    let vals = fetch(&t, &mut va).expect("fetch");
    assert_eq!(vals.0.get(1).map(|v| v.a), Some(42));
    assert_eq!(vals.1.get(1), Some(&ArgTag::Int));
    assert_eq!(vals.1.get(2), Some(&ArgTag::F64));
    assert_eq!(vals.1.get(3), Some(&ArgTag::Ptr));
    assert_eq!(vals.1.get(4), Some(&ArgTag::Ld));
    assert_eq!(vals.0.get(4).map(|v| v.ld()), Some(1));
    assert_eq!(vals.1.get(5), Some(&ArgTag::WInt));
    assert_eq!(vals.0.get(5).map(|v| v.a), Some(7));
    assert_eq!(va.pos, 5);
}

#[test]
fn fetch_rejects_a_gap() {
    let mut t = TypeTable::new();
    t.tablemax = 2;
    t.types[2] = TypeId::Int;
    let mut va = SliceVa::new();
    assert!(fetch(&t, &mut va).is_err());
}
