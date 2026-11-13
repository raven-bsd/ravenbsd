// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The test table is built from small indices and fixed biases.

//! Scope order: preload, main DAG, global list, DAGs, rtld, weak preference.

use crate::testbuild as elfbuild;

use alloc::vec::Vec;

use super::*;
use crate::errors::LookupError;
use crate::mem::MemSet;
use crate::object::ObjView;
use elfbuild::{Builder, SymbolSpec};

struct Object {
    base: u64,
    view: ObjView,
    bytes: Vec<u8>,
}

fn object(index: u32, symbols: &[(&'static [u8], u64, u8)]) -> Object {
    let base = 0x1000_0000u64 * u64::from(index + 1);
    let mut builder = Builder::new();
    for (name, value, other) in symbols {
        builder = builder.symbol(SymbolSpec {
            name: name.to_vec(),
            info: (1 << 4) | 2,
            other: *other,
            shndx: 1,
            value: *value,
            size: 16,
            version: 1,
            hidden: false,
        });
    }
    let image = builder.build();
    let view = ObjView::digest(&image.memory(base), image.phdr_vaddr, image.phnum, base).unwrap();
    Object {
        base,
        view,
        bytes: image.bytes,
    }
}

fn symbol(name: &[u8]) -> SymbolSpec {
    SymbolSpec {
        name: name.to_vec(),
        info: (1 << 4) | 2,
        other: 0,
        shndx: 1,
        value: 0x100,
        size: 16,
        version: 1,
        hidden: false,
    }
}

struct Table {
    views: Vec<Option<ObjView>>,
    deps: Vec<Vec<ObjId>>,
    dldags: Vec<Vec<ObjId>>,
    deep: Vec<bool>,
}

impl Table {
    fn new(count: usize) -> Self {
        Self {
            views: (0..count).map(|_| None).collect(),
            deps: vec![Vec::new(); count],
            dldags: vec![Vec::new(); count],
            deep: vec![false; count],
        }
    }

    fn put(&mut self, id: ObjId, view: ObjView) {
        self.views[id.0 as usize] = Some(view);
    }
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

    fn dldags(&self, id: ObjId) -> &[ObjId] {
        self.dldags
            .get(id.0 as usize)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn deepbind(&self, id: ObjId) -> bool {
        self.deep.get(id.0 as usize).copied().unwrap_or(false)
    }
}

fn memory(objects: &[Object]) -> MemSet<'_> {
    let mut set = MemSet::new();
    for object in objects {
        set.push(crate::mem::SliceMem::new(object.base, &object.bytes));
    }
    set
}

fn request(name: &[u8]) -> SymLook<'_> {
    SymLook::new(name)
}

#[test]
fn preload_is_searched_before_the_main_dag() {
    let preload = object(0, &[(b"x", 0x111, 0)]);
    let main = object(1, &[(b"x", 0x222, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), preload.view.clone());
    table.put(ObjId(1), main.view.clone());
    let scope = Scope {
        main: ObjId(1),
        preload: vec![ObjId(0)],
        global: Vec::new(),
        rtld: None,
    };
    let objects = [preload, main];
    let mixed = memory(&objects);
    let hit = symlook_default(&table, &mixed, &scope, &request(b"x"), ObjId(1), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn the_main_dag_is_breadth_first() {
    let main = object(0, &[]);
    let dep = object(1, &[(b"y", 0x100, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), dep.view.clone());
    table.deps[0] = vec![ObjId(1)];
    let objects = [main, dep];
    let memory = memory(&objects);
    let scope = Scope::new(ObjId(0));
    let hit = symlook_default(&table, &memory, &scope, &request(b"y"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(1));
}

#[test]
fn a_global_dlopen_object_is_searched_after_the_main_dag() {
    let main = object(0, &[(b"z", 0x10, 0)]);
    let global = object(1, &[(b"z", 0x20, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), global.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let objects = [main, global];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"z"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn a_symbolic_reference_uses_its_own_object_first() {
    let mut main = object(0, &[(b"x", 0x111, 0)]);
    let other = object(1, &[(b"x", 0x222, 0)]);
    // Mark object 0 symbolic by rebuilding it.
    let base = main.base;
    let image = Builder::new().symbol(symbol(b"x")).symbolic().build();
    main.view = ObjView::digest(&image.memory(base), image.phdr_vaddr, image.phnum, base).unwrap();
    main.bytes = image.bytes;
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), other.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let objects = [main, other];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"x"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn a_protected_symbol_is_found_in_its_own_object() {
    let main = object(0, &[(b"p", 0x111, 3)]);
    let other = object(1, &[(b"p", 0x222, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), other.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let objects = [main, other];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"p"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn without_symbolic_the_own_object_does_not_win() {
    let main = object(0, &[(b"x", 0x111, 0)]);
    let other = object(1, &[(b"x", 0x222, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), other.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let objects = [main, other];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"x"), ObjId(0), true)
        .unwrap()
        .unwrap();
    // The main DAG comes before the global list, and it contains object 0.
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn deepbind_moves_the_global_list_to_the_end() {
    let mut table = Table::new(2);
    let main = object(0, &[(b"x", 0x111, 0)]);
    let global = object(1, &[(b"x", 0x222, 0)]);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), global.view.clone());
    table.deep[0] = true;
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    let objects = [main, global];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"x"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn a_dlopen_dag_of_the_referencing_object_is_searched() {
    let main = object(0, &[]);
    let dag = object(1, &[]);
    let definition = object(2, &[(b"d", 0x100, 0)]);
    let mut table = Table::new(3);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), dag.view.clone());
    table.put(ObjId(2), definition.view.clone());
    table.deps[1] = vec![ObjId(2)];
    table.dldags[0] = vec![ObjId(1)];
    let scope = Scope::new(ObjId(0));
    let objects = [main, dag, definition];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"d"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(2));
}

#[test]
fn the_loader_itself_is_the_last_resort() {
    let main = object(0, &[]);
    let rtld = object(1, &[(b"dlopen", 0x100, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), rtld.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.rtld = Some(ObjId(1));
    let objects = [main, rtld];
    let memory = memory(&objects);
    let hit = symlook_default(&table, &memory, &scope, &request(b"dlopen"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(1));
}

#[test]
fn a_weak_definition_keeps_searching_only_with_dynamic_weak() {
    let main = object(0, &[]);
    let weak = object(1, &[]);
    let strong = object(2, &[(b"w", 0x100, 0)]);
    let mut table = Table::new(3);
    // Object 1 defines w weakly, object 2 strongly.
    let base = weak.base;
    let image = Builder::new()
        .symbol(SymbolSpec {
            name: b"w".to_vec(),
            info: (2 << 4) | 2,
            other: 0,
            shndx: 1,
            value: 0x50,
            size: 16,
            version: 1,
            hidden: false,
        })
        .build();
    let weak = Object {
        base,
        view: ObjView::digest(&image.memory(base), image.phdr_vaddr, image.phnum, base).unwrap(),
        bytes: image.bytes,
    };
    table.put(ObjId(0), main.view.clone());
    table.put(ObjId(1), weak.view.clone());
    table.put(ObjId(2), strong.view.clone());
    let mut scope = Scope::new(ObjId(0));
    scope.global.push(ObjId(1));
    scope.global.push(ObjId(2));
    let objects = [main, weak, strong];
    let memory = memory(&objects);
    let strong_hit = symlook_default(&table, &memory, &scope, &request(b"w"), ObjId(0), true)
        .unwrap()
        .unwrap();
    assert_eq!(strong_hit.obj, ObjId(2));
    let weak_hit = symlook_default(&table, &memory, &scope, &request(b"w"), ObjId(0), false)
        .unwrap()
        .unwrap();
    assert_eq!(weak_hit.obj, ObjId(1));
}

#[test]
fn an_object_listed_twice_is_searched_once() {
    let main = object(0, &[(b"x", 0x100, 0)]);
    let mut table = Table::new(1);
    table.put(ObjId(0), main.view.clone());
    let mut done = Vec::new();
    let objects = [main];
    let memory = memory(&objects);
    let list = [ObjId(0), ObjId(0), ObjId(0)];
    let hit = symlook_list(&table, &memory, &list, &request(b"x"), &mut done)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
    assert_eq!(done, vec![ObjId(0)]);
}

#[test]
fn dag_members_are_unique_and_breadth_first() {
    let mut table = Table::new(4);
    table.deps[0] = vec![ObjId(1), ObjId(2)];
    table.deps[1] = vec![ObjId(3)];
    table.deps[2] = vec![ObjId(3)];
    let members = dag_members(&table, ObjId(0));
    assert_eq!(members, vec![ObjId(0), ObjId(1), ObjId(2), ObjId(3)]);
}

#[test]
fn symlook_needed_searches_the_root_and_its_dependencies() {
    let root = object(0, &[(b"r", 0x100, 0)]);
    let dep = object(1, &[]);
    let mut table = Table::new(2);
    table.put(ObjId(0), root.view.clone());
    table.put(ObjId(1), dep.view.clone());
    table.deps[0] = vec![ObjId(1)];
    let objects = [root, dep];
    let memory = memory(&objects);
    let mut done = Vec::new();
    let hit = symlook_needed(&table, &memory, ObjId(0), &request(b"r"), &mut done)
        .unwrap()
        .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn an_unknown_object_is_skipped() {
    let main = object(0, &[(b"x", 0x100, 0)]);
    let mut table = Table::new(2);
    table.put(ObjId(0), main.view.clone());
    let objects = [main];
    let memory = memory(&objects);
    let mut done = Vec::new();
    let hit = symlook_list(
        &table,
        &memory,
        &[ObjId(1), ObjId(0)],
        &request(b"x"),
        &mut done,
    )
    .unwrap()
    .unwrap();
    assert_eq!(hit.obj, ObjId(0));
}

#[test]
fn a_missing_symbol_returns_none() {
    let main = object(0, &[]);
    let mut table = Table::new(1);
    table.put(ObjId(0), main.view.clone());
    let scope = Scope::new(ObjId(0));
    let objects = [main];
    let memory = memory(&objects);
    let hit: Option<SymHit> =
        symlook_default(&table, &memory, &scope, &request(b"absent"), ObjId(0), true).unwrap();
    assert!(hit.is_none());
}

#[test]
fn a_lookup_error_propagates() {
    // An object whose symbol table address is outside the memory view.
    let main = object(0, &[(b"x", 0x100, 0)]);
    let mut view = main.view.clone();
    view.symtab = 0x0090_0000;
    let mut table = Table::new(1);
    table.put(ObjId(0), view);
    let objects = [main];
    let memory = memory(&objects);
    let result = symlook_list(
        &table,
        &memory,
        &[ObjId(0)],
        &request(b"x"),
        &mut Vec::new(),
    );
    assert!(matches!(result, Err(LookupError::BadSymbol)));
}
