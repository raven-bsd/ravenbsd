// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for [`crate::auxv`].

#![allow(clippy::indexing_slicing)]

use super::*;

struct Stack {
    words: host::vec::Vec<usize>,
}

impl Stack {
    fn new(argc: usize, env_count: usize, auxv: &[(usize, usize)]) -> Self {
        let mut words = host::vec::Vec::new();
        words.push(argc);
        for index in 0..argc {
            words.push(0x1000 + index);
        }
        words.push(0);
        for index in 0..env_count {
            words.push(0x2000 + index);
        }
        words.push(0);
        for (key, value) in auxv {
            words.push(*key);
            words.push(*value);
        }
        words.push(AT_NULL);
        words.push(0);
        Self { words }
    }

    fn parse(&self) -> Option<AuxVec> {
        let mut argc = 99usize;
        let mut argv = core::ptr::null();
        let mut envp = core::ptr::null();
        // SAFETY: the builder produced a well-formed C8 stack.
        let vector =
            unsafe { AuxVec::from_stack(self.words.as_ptr(), &mut argc, &mut argv, &mut envp) };
        if vector.is_some() {
            assert_eq!(argc, self.words[0]);
        }
        vector
    }
}

#[test]
fn well_formed_vector_parses() {
    let stack = Stack::new(2, 1, &[(AT_PAGESZ, 4096), (AT_PHDR, 0x40_0000)]);
    let vector = stack.parse().unwrap();
    assert_eq!(vector.len(), 2);
    assert_eq!(vector.pagesz(), Some(4096));
    assert_eq!(vector.phdr(), Some(0x40_0000));
}

#[test]
fn argc_zero_is_accepted() {
    let stack = Stack::new(0, 0, &[(AT_ENTRY, 0x1234)]);
    let vector = stack.parse().unwrap();
    assert_eq!(vector.entry(), Some(0x1234));
}

#[test]
fn truncated_vector_stops_at_the_bound() {
    let mut words = host::vec::Vec::new();
    words.push(0usize);
    words.push(0usize);
    words.push(0usize);
    for index in 0..MAX_ENTRIES {
        words.push(index + 1);
        words.push(index);
    }
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: the words form the prefix of a C8 stack with a truncated auxv.
    let vector =
        unsafe { AuxVec::from_stack(words.as_ptr(), &mut argc, &mut argv, &mut envp) }.unwrap();
    assert_eq!(vector.len(), MAX_ENTRIES);
    assert_eq!(vector.get(1), Some(0));
    assert_eq!(vector.get(MAX_ENTRIES), Some(MAX_ENTRIES - 1));
    assert_eq!(vector.get(MAX_ENTRIES + 1), None);
}

#[test]
fn unknown_keys_are_ignored_by_getters() {
    let stack = Stack::new(0, 0, &[(0xdead, 7), (AT_PAGESZ, 4096)]);
    let vector = stack.parse().unwrap();
    assert_eq!(vector.get(0xdead), Some(7));
    assert_eq!(vector.pagesz(), Some(4096));
    assert_eq!(vector.hwcap(), None);
}

#[test]
fn duplicate_keys_first_wins() {
    let stack = Stack::new(0, 0, &[(AT_PAGESZ, 8192), (AT_PAGESZ, 4096)]);
    let vector = stack.parse().unwrap();
    assert_eq!(vector.pagesz(), Some(8192));
}

#[test]
fn absent_at_secure_is_zero() {
    let stack = Stack::new(0, 0, &[(AT_PAGESZ, 4096)]);
    assert_eq!(stack.parse().unwrap().secure(), 0);
}

#[test]
fn present_at_secure_is_still_zero_in_v1() {
    let stack = Stack::new(0, 0, &[(AT_SECURE, 1)]);
    let vector = stack.parse().unwrap();
    assert_eq!(vector.get(AT_SECURE), Some(1));
    assert_eq!(vector.secure(), 0, "H-7 fixes AT_SECURE at 0 in v1");
}

#[test]
fn null_and_misaligned_stack_return_none() {
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: a null pointer is explicitly rejected without a dereference.
    assert!(
        unsafe { AuxVec::from_stack(core::ptr::null(), &mut argc, &mut argv, &mut envp) }.is_none()
    );

    let stack = Stack::new(0, 0, &[]);
    let misaligned = (stack.words.as_ptr() as *const u8).wrapping_add(1) as *const usize;
    // SAFETY: the pointer is deliberately misaligned; the parser must reject it.
    assert!(unsafe { AuxVec::from_stack(misaligned, &mut argc, &mut argv, &mut envp) }.is_none());
}

#[test]
fn argc_above_the_bound_is_rejected() {
    let mut words = host::vec![MAX_ARGC + 1];
    words.push(0);
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: `argc` is out of bounds and is rejected before any further read.
    assert!(
        unsafe { AuxVec::from_stack(words.as_ptr(), &mut argc, &mut argv, &mut envp) }.is_none()
    );
}

#[test]
fn missing_argv_terminator_is_rejected() {
    let mut words = host::vec![1usize, 0x1000, 123];
    words.push(0);
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: the stack is malformed; the parser must reject it.
    assert!(
        unsafe { AuxVec::from_stack(words.as_ptr(), &mut argc, &mut argv, &mut envp) }.is_none()
    );
}

#[test]
fn too_many_environment_entries_are_rejected() {
    let mut words = host::vec![0usize, 0usize];
    for index in 0..MAX_ENV {
        words.push(0x2000 + index);
    }
    let mut argc = 0usize;
    let mut argv = core::ptr::null();
    let mut envp = core::ptr::null();
    // SAFETY: the environment array has no terminator inside the bound.
    assert!(
        unsafe { AuxVec::from_stack(words.as_ptr(), &mut argc, &mut argv, &mut envp) }.is_none()
    );
}

#[test]
fn typed_getters_report_every_required_key() {
    let stack = Stack::new(
        0,
        0,
        &[
            (AT_PHDR, 1),
            (AT_PHENT, 2),
            (AT_PHNUM, 3),
            (AT_BASE, 4),
            (AT_ENTRY, 5),
            (AT_RANDOM, 6),
            (AT_HWCAP, 7),
            (AT_HWCAP2, 8),
            (AT_CLKTCK, 9),
            (AT_EXECFN, 10),
            (AT_RAVEN_ABI, 11),
            (AT_RAVEN_FDMAP, 12),
            (AT_RAVEN_PROCINFO, 13),
            (AT_RAVEN_PROCD, 14),
        ],
    );
    let vector = stack.parse().unwrap();
    assert_eq!(vector.phdr(), Some(1));
    assert_eq!(vector.phent(), Some(2));
    assert_eq!(vector.phnum(), Some(3));
    assert_eq!(vector.base(), Some(4));
    assert_eq!(vector.entry(), Some(5));
    assert_eq!(vector.random(), Some(6));
    assert_eq!(vector.hwcap(), Some(7));
    assert_eq!(vector.hwcap2(), Some(8));
    assert_eq!(vector.clktck(), Some(9));
    assert_eq!(vector.execfn(), Some(10));
    assert_eq!(vector.raven_abi(), Some(11));
    assert_eq!(vector.raven_fdmap(), Some(12));
    assert_eq!(vector.raven_procinfo(), Some(13));
    assert_eq!(vector.raven_procd(), Some(14));
}

#[test]
fn abi_check_accepts_match_and_absence_and_rejects_mismatch() {
    let matching = Stack::new(0, 0, &[(AT_RAVEN_ABI, raven_abi::ABI_VERSION as usize)]);
    assert_eq!(matching.parse().unwrap().check_abi(), Ok(()));

    let absent = Stack::new(0, 0, &[]);
    assert_eq!(absent.parse().unwrap().check_abi(), Ok(()));

    let mismatch = Stack::new(0, 0, &[(AT_RAVEN_ABI, 99)]);
    assert_eq!(
        mismatch.parse().unwrap().check_abi(),
        Err(AbiMismatch {
            expected: raven_abi::ABI_VERSION,
            found: 99,
        })
    );
}

#[test]
fn empty_vector_is_empty() {
    let stack = Stack::new(0, 0, &[]);
    let vector = stack.parse().unwrap();
    assert!(vector.is_empty());
    assert_eq!(vector.len(), 0);
    assert_eq!(vector.get(AT_PAGESZ), None);
}
