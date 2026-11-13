// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// Version tables are built from small fixed indices.

//! Version matching: the eight combinations, index masking, requirements and locations.

use crate::testbuild as elfbuild;

use alloc::vec::Vec;

use super::*;
use crate::errors::{LookupError, VersionError};
use crate::lookup::{SymLook, VerRef, lookup_obj};
use crate::mem::MemSet;
use crate::object::ObjView;
use crate::scope::{ObjId, ObjTable};
use elfbuild::{Builder, SymbolSpec, VersionDef, VersionNeed};

fn hash(name: &[u8]) -> u32 {
    crate::hash::gnu_hash(name)
}

fn versioned_object(
    version: u16,
    hidden: bool,
    version_name: &[u8],
    index: u16,
) -> (ObjView, Vec<u8>) {
    let image = Builder::new()
        .symbol(SymbolSpec {
            name: b"f".to_vec(),
            info: (1 << 4) | 2,
            other: 0,
            shndx: 1,
            value: 0x100,
            size: 16,
            version,
            hidden,
        })
        .version_def(VersionDef {
            name: version_name.to_vec(),
            index,
            flags: 0,
        })
        .soname(b"libv.so.1")
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    (view, image.bytes)
}

fn unversioned_object() -> (ObjView, Vec<u8>) {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .soname(b"libu.so.1")
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    (view, image.bytes)
}

fn second_bias() -> u64 {
    0x100_0000
}

fn version_request(name: &[u8]) -> SymLook<'_> {
    SymLook::new(b"f").with_version(VerRef {
        name,
        hash: hash(name),
        weak: false,
        file: None,
    })
}

#[test]
fn an_unversioned_definition_accepts_a_versioned_reference() {
    let (view, bytes) = unversioned_object();
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let hit = lookup_obj(ObjId(0), &view, &memory, &version_request(b"A"))
        .unwrap()
        .unwrap();
    assert_eq!(hit.sym.typ(), 2);
}

#[test]
fn a_matching_version_is_accepted() {
    let (view, bytes) = versioned_object(2, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let hit = lookup_obj(ObjId(0), &view, &memory, &version_request(b"A"))
        .unwrap()
        .unwrap();
    assert_eq!(hit.sym.value, 0x100);
}

#[test]
fn a_hidden_matching_version_is_accepted() {
    let (view, bytes) = versioned_object(2, true, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &version_request(b"A"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_different_version_is_rejected() {
    let (view, bytes) = versioned_object(2, false, b"B", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &version_request(b"A"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_unversioned_reference_takes_the_default_version() {
    let (view, bytes) = versioned_object(2, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let request = SymLook::new(b"f");
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &request)
            .unwrap()
            .is_some()
    );
}

#[test]
fn an_unversioned_reference_does_not_take_a_hidden_version() {
    let (view, bytes) = versioned_object(2, true, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let request = SymLook::new(b"f");
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &request)
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_unversioned_reference_accepts_a_global_definition() {
    let (view, bytes) = versioned_object(1, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let request = SymLook::new(b"f");
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &request)
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_versioned_reference_accepts_a_global_definition() {
    let (view, bytes) = versioned_object(1, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    assert!(
        lookup_obj(ObjId(0), &view, &memory, &version_request(b"A"))
            .unwrap()
            .is_some()
    );
}

#[test]
fn the_version_index_masks_the_hidden_bit() {
    let (view, bytes) = versioned_object(0x8002, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let entry = fetch_ventry(&view, &memory, 1).unwrap().unwrap();
    assert_eq!(entry.index, 2);
    assert!(entry.hidden);
    assert_eq!(entry.hash, hash(b"A"));
    assert_eq!(
        view.name_at(&memory, entry.name_off).unwrap(),
        b"A".to_vec()
    );
}

#[test]
fn index_zero_and_one_have_no_entry() {
    let (view, bytes) = versioned_object(1, false, b"A", 2);
    let memory = crate::mem::SliceMem::new(0, &bytes);
    assert!(fetch_ventry(&view, &memory, 1).unwrap().is_none());
    let mut another = view.clone();
    another.versym = None;
    assert!(fetch_ventry(&another, &memory, 1).unwrap().is_none());
}

struct Table {
    views: Vec<Option<ObjView>>,
}

impl ObjTable for Table {
    fn view(&self, id: ObjId) -> Option<&ObjView> {
        self.views.get(id.0 as usize).and_then(|view| view.as_ref())
    }
    fn deps(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }
    fn dldags(&self, _id: ObjId) -> &[ObjId] {
        &[]
    }
    fn deepbind(&self, _id: ObjId) -> bool {
        false
    }
}

fn requirement(flags: u16) -> (ObjView, Vec<u8>) {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .version_need(VersionNeed {
            file: b"libdep.so.1".to_vec(),
            versions: vec![(b"A".to_vec(), 2, flags)],
        })
        .soname(b"libuser.so.1")
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    (view, image.bytes)
}

fn dependency() -> (ObjView, Vec<u8>) {
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"dep", 0x100))
        .version_def(VersionDef {
            name: b"A".to_vec(),
            index: 2,
            flags: 0,
        })
        .soname(b"libdep.so.1")
        .build();
    let view = ObjView::digest(
        &image.memory(second_bias()),
        image.phdr_vaddr,
        image.phnum,
        second_bias(),
    )
    .unwrap();
    (view, image.bytes)
}

fn verify(user: &(ObjView, Vec<u8>), dep: &(ObjView, Vec<u8>)) -> Result<(), VersionError> {
    let mut table = Table {
        views: vec![Some(user.0.clone()), Some(dep.0.clone())],
    };
    let mut set = MemSet::new();
    set.push(crate::mem::SliceMem::new(0, &user.1));
    set.push(crate::mem::SliceMem::new(second_bias(), &dep.1));
    let locate = |name: &[u8]| {
        if name == b"libdep.so.1" {
            Some(ObjId(1))
        } else {
            None
        }
    };
    let _ = &mut table;
    verify_object_versions(&user.0, &set, &locate, &table)
}

#[test]
fn a_satisfied_requirement_is_ok() {
    let user = requirement(0);
    let dep = dependency();
    assert_eq!(verify(&user, &dep), Ok(()));
}

#[test]
fn a_missing_strong_requirement_is_unsatisfied() {
    let user = requirement(0);
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"dep", 0x100))
        .version_def(VersionDef {
            name: b"B".to_vec(),
            index: 2,
            flags: 0,
        })
        .soname(b"libdep.so.1")
        .build();
    let dep = (
        ObjView::digest(
            &image.memory(second_bias()),
            image.phdr_vaddr,
            image.phnum,
            second_bias(),
        )
        .unwrap(),
        image.bytes,
    );
    assert!(matches!(
        verify(&user, &dep),
        Err(VersionError::Unsatisfied { .. })
    ));
}

#[test]
fn a_missing_weak_requirement_is_ok() {
    let user = requirement(crate::consts::VER_FLG_WEAK);
    let image = Builder::new()
        .symbol(elfbuild::global_func(b"dep", 0x100))
        .version_def(VersionDef {
            name: b"B".to_vec(),
            index: 2,
            flags: 0,
        })
        .soname(b"libdep.so.1")
        .build();
    let dep = (
        ObjView::digest(
            &image.memory(second_bias()),
            image.phdr_vaddr,
            image.phnum,
            second_bias(),
        )
        .unwrap(),
        image.bytes,
    );
    assert_eq!(verify(&user, &dep), Ok(()));
}

#[test]
fn a_version_hash_mismatch_with_the_same_name_is_unsatisfied() {
    // The requirement's hash is computed from its name by the builder, so use a dependency
    // whose VERDEF hash is corrupted to differ.
    let user = requirement(crate::consts::VER_FLG_WEAK);
    let dep = dependency();
    let mut bytes = dep.1.clone();
    let view = &dep.0;
    // Find the verdef record and flip one hash bit.
    let at = usize::try_from(view.verdef.unwrap().0 - second_bias()).unwrap();
    bytes[at + 8] ^= 0x01;
    let corrupted = (dep.0.clone(), bytes);
    assert_eq!(verify(&user, &corrupted), Ok(()));
    let user = requirement(0);
    assert!(matches!(
        verify(&user, &corrupted),
        Err(VersionError::Unsatisfied { .. })
    ));
}

#[test]
fn locate_dependency_finds_a_soname_and_an_alias() {
    let (view, bytes) = dependency();
    let memory = crate::mem::SliceMem::new(second_bias(), &bytes);
    let mut table = Table {
        views: vec![Some(view.clone())],
    };
    let names = |_: ObjId| vec![b"libdep.so.1".to_vec()];
    assert_eq!(
        locate_dependency(&view, &memory, b"libdep.so.1", &table, &names),
        Some(ObjId(0))
    );
    let names_alias = |_: ObjId| vec![b"other.so.1".to_vec(), b"libdep.so.1".to_vec()];
    assert_eq!(
        locate_dependency(&view, &memory, b"libdep.so.1", &table, &names_alias),
        Some(ObjId(0))
    );
    let names_none = |_: ObjId| vec![b"other.so.1".to_vec()];
    assert_eq!(
        locate_dependency(&view, &memory, b"libdep.so.1", &table, &names_none),
        None
    );
    let _ = &mut table;
}

#[test]
fn a_version_requirement_for_an_unknown_dependency_is_an_error() {
    let user = requirement(0);
    let mut table = Table {
        views: vec![Some(user.0.clone())],
    };
    let memory = crate::mem::SliceMem::new(0, &user.1);
    let locate = |_: &[u8]| None;
    let result = verify_object_versions(&user.0, &memory, &locate, &table);
    assert_eq!(result, Err(VersionError::MissingDependency));
    let _ = &mut table;
}

#[test]
fn an_object_without_requirements_is_ok() {
    let (view, bytes) = unversioned_object();
    let table = Table {
        views: vec![Some(view.clone())],
    };
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let locate = |_: &[u8]| None;
    assert_eq!(
        verify_object_versions(&view, &memory, &locate, &table),
        Ok(())
    );
}

#[test]
fn a_bad_version_index_is_reported() {
    let (mut view, bytes) = versioned_object(2, false, b"A", 2);
    // Point DT_VERSYM at a word with an index far above the tables.
    let at = usize::try_from(view.versym.unwrap()).unwrap();
    let mut bytes = bytes;
    bytes[at + 2..at + 4].copy_from_slice(&0x00ffu16.to_le_bytes());
    let set = crate::mem::SliceMem::new(0, &bytes);
    let result = lookup_obj(ObjId(0), &view, &set, &SymLook::new(b"f"));
    assert!(matches!(result, Err(LookupError::BadVersion)));
    let _ = &mut view;
}

#[test]
fn an_unsupported_verneed_version_is_bad_table() {
    let user = requirement(0);
    let mut bytes = user.1.clone();
    let at = usize::try_from(user.0.verneed.unwrap().0).unwrap();
    bytes[at..at + 2].copy_from_slice(&2u16.to_le_bytes());
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let table = Table {
        views: vec![Some(user.0.clone())],
    };
    let locate = |_: &[u8]| Some(ObjId(1));
    assert_eq!(
        verify_object_versions(&user.0, &memory, &locate, &table),
        Err(VersionError::BadTable)
    );
}

#[test]
fn a_verneed_entry_pointing_outside_the_image_is_bad_table() {
    let user = requirement(0);
    let dep = dependency();
    let mut bytes = user.1.clone();
    let at = usize::try_from(user.0.verneed.unwrap().0).unwrap();
    bytes[at + 12..at + 16].copy_from_slice(&0xffff_ff00u32.to_le_bytes());
    let memory = crate::mem::SliceMem::new(0, &bytes);
    let table = Table {
        views: vec![Some(user.0.clone()), Some(dep.0.clone())],
    };
    let locate = |_: &[u8]| Some(ObjId(1));
    assert_eq!(
        verify_object_versions(&user.0, &memory, &locate, &table),
        Err(VersionError::BadTable)
    );
}
