// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The table below is a fixed two-object fixture.

//! The `dl_iterate_phdr` model and its iteration order.

use crate::testbuild as elfbuild;

use alloc::vec::Vec;

use super::*;
use crate::object::ObjView;
use crate::scope::{ObjId, ObjTable, Scope};

struct Table {
    views: Vec<Option<ObjView>>,
    deps: Vec<Vec<ObjId>>,
}

impl ObjTable for Table {
    fn view(&self, id: ObjId) -> Option<&ObjView> {
        self.views.get(id.0 as usize).and_then(|view| view.as_ref())
    }
    fn deps(&self, id: ObjId) -> &[ObjId] {
        self.deps
            .get(id.0 as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn dldags(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }
    fn deepbind(&self, _id: ObjId) -> bool {
        false
    }
}

fn fixture() -> Table {
    let main = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"main", 0x100))
        .build();
    let dep = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"dep", 0x100))
        .build();
    let main_view = ObjView::digest(&main.memory(0), main.phdr_vaddr, main.phnum, 0).unwrap();
    let dep_view = ObjView::digest(
        &dep.memory(0x100_0000),
        dep.phdr_vaddr,
        dep.phnum,
        0x100_0000,
    )
    .unwrap();
    Table {
        views: vec![Some(main_view), Some(dep_view)],
        deps: vec![vec![ObjId(1)], Vec::new()],
    }
}

#[test]
fn the_record_has_eight_fields() {
    assert_eq!(PhdrInfo::field_count(), 8);
}

#[test]
fn a_record_keeps_its_inputs() {
    let record = PhdrInfo::new(0x1000, b"/lib/libz.so.1".to_vec(), (0x2000, 7), 3, 1, 4);
    assert_eq!(record.addr, 0x1000);
    assert_eq!(record.name, b"/lib/libz.so.1");
    assert_eq!(record.phdr, (0x2000, 7));
    assert_eq!(record.adds, 3);
    assert_eq!(record.subs, 1);
    assert_eq!(record.tls_modid, 4);
}

#[test]
fn the_iteration_starts_with_the_main_program() {
    let table = fixture();
    let scope = Scope::new(ObjId(0));
    let records = phdr_info_list(&table, &scope, 2, 0, &|_| 0);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].addr, 0);
    assert_eq!(records[1].addr, 0x100_0000);
}

#[test]
fn each_object_appears_once_even_when_reachable_twice() {
    let mut table = fixture();
    table.deps[0] = vec![ObjId(1), ObjId(1)];
    let scope = Scope::new(ObjId(0));
    let records = phdr_info_list(&table, &scope, 0, 0, &|_| 0);
    assert_eq!(records.len(), 2);
}

#[test]
fn the_generation_counters_and_tls_id_are_filled() {
    let table = fixture();
    let scope = Scope::new(ObjId(0));
    let records = phdr_info_list(&table, &scope, 7, 2, &|id| u64::from(id.0) + 10);
    assert_eq!(records[0].adds, 7);
    assert_eq!(records[0].subs, 2);
    assert_eq!(records[0].tls_modid, 10);
    assert_eq!(records[1].tls_modid, 11);
}

#[test]
fn a_global_scope_root_is_appended_after_the_main_dag() {
    let table = fixture();
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let records = phdr_info_list(&table, &scope, 0, 0, &|_| 0);
    assert_eq!(records.len(), 2);
}

#[test]
fn an_unknown_id_is_skipped() {
    let mut table = fixture();
    table.views.push(None);
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(2));
    let records = phdr_info_list(&table, &scope, 0, 0, &|_| 0);
    assert_eq!(records.len(), 2);
}
