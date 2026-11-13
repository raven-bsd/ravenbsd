// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The DAG generator uses plain arithmetic over small values.

//! Init and fini order, including a property test over random DAGs.

use crate::testbuild as elfbuild;

use alloc::collections::BTreeMap;

use super::*;
use crate::object::ObjView;
use crate::scope::ObjId;

fn diamond() -> BTreeMap<u32, Vec<u32>> {
    let mut map = BTreeMap::new();
    map.insert(0, vec![1, 2]);
    map.insert(1, vec![3]);
    map.insert(2, vec![3]);
    map.insert(3, vec![]);
    map
}

fn order(roots: &[u32], map: &BTreeMap<u32, Vec<u32>>) -> Vec<u32> {
    let roots: Vec<ObjId> = roots.iter().copied().map(ObjId).collect();
    let lookup = |id: ObjId| {
        map.get(&id.0)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(ObjId)
            .collect()
    };
    init_order(&roots, &lookup)
        .into_iter()
        .map(|id| id.0)
        .collect()
}

#[test]
fn a_diamond_puts_the_shared_dependency_first() {
    let result = order(&[0], &diamond());
    assert_eq!(result, vec![3, 1, 2, 0]);
}

#[test]
fn a_three_cycle_is_broken_deterministically() {
    let mut map = BTreeMap::new();
    map.insert(0, vec![1]);
    map.insert(1, vec![2]);
    map.insert(2, vec![0]);
    assert_eq!(order(&[0], &map), vec![2, 1, 0]);
}

#[test]
fn each_object_appears_once() {
    let map = diamond();
    let result = order(&[0, 1, 2], &map);
    let mut sorted = result.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), result.len());
}

#[test]
fn dependencies_precede_their_dependents() {
    let map = diamond();
    let result = order(&[0], &map);
    for (dependent, dependencies) in &map {
        if let Some(position) = result.iter().position(|id| id == dependent) {
            for dependency in dependencies {
                if let Some(dep_position) = result.iter().position(|id| id == dependency) {
                    assert!(dep_position < position, "{dependency} before {dependent}");
                }
            }
        }
    }
}

#[test]
fn fini_order_is_the_exact_reverse() {
    let init = [ObjId(3), ObjId(1), ObjId(2), ObjId(0)];
    assert_eq!(
        fini_order(&init),
        vec![ObjId(0), ObjId(2), ObjId(1), ObjId(3)]
    );
}

#[test]
fn the_same_roots_give_the_same_order_twice() {
    let map = diamond();
    assert_eq!(order(&[0], &map), order(&[0], &map));
}

struct XorShift(u32);

impl XorShift {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

#[test]
fn two_thousand_random_dags_obey_the_contract() {
    for seed in 1..=2000u32 {
        let mut random = XorShift(seed.wrapping_mul(2_654_435_761));
        let nodes = usize::try_from(seed % 12 + 1).unwrap();
        let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for node in 0..nodes {
            map.insert(node as u32, Vec::new());
        }
        for node in 0..nodes {
            for candidate in 0..nodes {
                if node != candidate && random.next() % 100 < 18 {
                    map.get_mut(&(node as u32)).unwrap().push(candidate as u32);
                }
            }
        }
        let roots = vec![ObjId(0)];
        let lookup = |id: ObjId| {
            map.get(&id.0)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(ObjId)
                .collect()
        };
        let first = init_order(&roots, &lookup);
        let second = init_order(&roots, &lookup);
        assert_eq!(first, second, "seed {seed} is not deterministic");
        let mut unique = first.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), first.len(), "seed {seed} repeated an object");
        let reachable = reachable_from(&map, 0);
        assert_eq!(first.len(), reachable.len(), "seed {seed} missed an object");
        // Every edge whose source is reachable must have its target earlier, unless it is a
        // back edge (the target appears after the source only in a cycle).
        let position = |id: ObjId| first.iter().position(|entry| *entry == id);
        for (node, dependencies) in &map {
            let Some(source) = position(ObjId(*node)) else {
                continue;
            };
            for dependency in dependencies {
                if let Some(target) = position(ObjId(*dependency))
                    && target > source
                {
                    // A back edge: the dependency must reach the node.
                    assert!(
                        reaches(&map, *dependency, *node),
                        "seed {seed}: {dependency} after {node} without a cycle"
                    );
                }
            }
        }
        assert_eq!(fini_order(&first), {
            let mut reversed = first.clone();
            reversed.reverse();
            reversed
        });
    }
}

fn reachable_from(map: &BTreeMap<u32, Vec<u32>>, root: u32) -> Vec<u32> {
    let mut reached = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if reached.contains(&node) {
            continue;
        }
        reached.push(node);
        if let Some(next) = map.get(&node) {
            stack.extend_from_slice(next);
        }
    }
    reached
}

fn reaches(map: &BTreeMap<u32, Vec<u32>>, from: u32, to: u32) -> bool {
    let mut stack = vec![from];
    let mut seen = Vec::new();
    while let Some(node) = stack.pop() {
        if node == to {
            return true;
        }
        if seen.contains(&node) {
            continue;
        }
        seen.push(node);
        if let Some(next) = map.get(&node) {
            stack.extend_from_slice(next);
        }
    }
    false
}

#[test]
fn init_array_entries_skip_the_placeholders() {
    let image = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .init_array(&[0, 1, 0x1000, u64::MAX, 0x2000])
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    let entries = init_array_entries(&view, &image.memory(0), InitKind::Init).unwrap();
    assert_eq!(entries, vec![0x1000, 0x2000]);
}

#[test]
fn an_absent_array_is_empty() {
    let image = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .build();
    let view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    for kind in [InitKind::Init, InitKind::Fini, InitKind::Preinit] {
        assert!(
            init_array_entries(&view, &image.memory(0), kind)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn an_unreadable_array_is_an_error() {
    let image = elfbuild::Builder::new()
        .symbol(elfbuild::global_func(b"f", 0x100))
        .init_array(&[0x1000])
        .build();
    let mut view = ObjView::digest(&image.memory(0), image.phdr_vaddr, image.phnum, 0).unwrap();
    view.init_array.0 = 0x0090_0000;
    assert!(init_array_entries(&view, &image.memory(0), InitKind::Init).is_err());
}
