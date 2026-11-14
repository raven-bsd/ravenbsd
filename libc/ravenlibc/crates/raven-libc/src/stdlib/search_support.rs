// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Raw-pointer stores for C41's `<search.h>` engines (C60c).
//!
//! `tsearch` nodes are one `malloc` block whose first word is the caller's key and whose
//! remaining words are the AVL links; returning the block address is exactly the C "pointer to
//! the key field" contract because `key` is the first member. `insque`/`remque` read and write
//! the first two pointer words of the caller's own object.

use core::ffi::{c_int, c_void};

use raven_libc_stdlib::search::{ListStore, NodeStore};

/// The C `insque`/`remque` store: the caller's object starts with `next` then `prev`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RawListStore;

impl ListStore for RawListStore {
    type Node = usize;

    fn next(&self, node: Self::Node) -> Option<Self::Node> {
        // SAFETY: the caller's object has a forward pointer at offset 0.
        let value = unsafe { *(node as *const usize) };
        if value == 0 { None } else { Some(value) }
    }

    fn prev(&self, node: Self::Node) -> Option<Self::Node> {
        // SAFETY: the caller's object has a backward pointer at offset 8.
        let value = unsafe { *((node as *const usize).add(1)) };
        if value == 0 { None } else { Some(value) }
    }

    fn set_next(&mut self, node: Self::Node, next: Option<Self::Node>) {
        // SAFETY: the caller's object has a writable forward pointer.
        unsafe { *(node as *mut usize) = next.unwrap_or(0) };
    }

    fn set_prev(&mut self, node: Self::Node, prev: Option<Self::Node>) {
        // SAFETY: the caller's object has a writable backward pointer.
        unsafe { *((node as *mut usize).add(1)) = prev.unwrap_or(0) };
    }
}

/// One AVL node: the caller's key first, then the two links and the balance.
#[repr(C)]
pub(crate) struct TreeNode {
    /// The caller's opaque key word (the returned C pointer).
    pub key: usize,
    /// Left child.
    pub left: *mut TreeNode,
    /// Right child.
    pub right: *mut TreeNode,
    /// The donor's +1/0/-1 balance.
    pub balance: i8,
    /// Alignment padding.
    pub padding: [u8; 7],
}

/// The `tsearch` arena over `malloc`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RawNodes;

impl NodeStore for RawNodes {
    type Node = *mut TreeNode;

    fn key(&self, node: Self::Node) -> usize {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).key }
    }

    fn set_key(&mut self, node: Self::Node, key: usize) {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).key = key };
    }

    fn left(&self, node: Self::Node) -> Option<Self::Node> {
        // SAFETY: nodes come from `alloc`.
        let child = unsafe { (*node).left };
        if child.is_null() { None } else { Some(child) }
    }

    fn right(&self, node: Self::Node) -> Option<Self::Node> {
        // SAFETY: nodes come from `alloc`.
        let child = unsafe { (*node).right };
        if child.is_null() { None } else { Some(child) }
    }

    fn balance(&self, node: Self::Node) -> i8 {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).balance }
    }

    fn set_left(&mut self, node: Self::Node, child: Option<Self::Node>) {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).left = child.unwrap_or(core::ptr::null_mut()) };
    }

    fn set_right(&mut self, node: Self::Node, child: Option<Self::Node>) {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).right = child.unwrap_or(core::ptr::null_mut()) };
    }

    fn set_balance(&mut self, node: Self::Node, balance: i8) {
        // SAFETY: nodes come from `alloc`.
        unsafe { (*node).balance = balance };
    }

    fn alloc(&mut self, key: usize) -> Option<Self::Node> {
        // SAFETY: the shell allocator returns one node-sized block or NULL.
        let raw =
            unsafe { crate::malloc::abi_alloc(core::mem::size_of::<TreeNode>()) } as *mut TreeNode;
        if raw.is_null() {
            return None;
        }
        // SAFETY: the block is writable and zeroed below.
        unsafe {
            (*raw) = TreeNode {
                key,
                left: core::ptr::null_mut(),
                right: core::ptr::null_mut(),
                balance: 0,
                padding: [0; 7],
            }
        };
        Some(raw)
    }

    fn free(&mut self, node: Self::Node) {
        if node.is_null() {
            return;
        }
        // SAFETY: nodes come from `alloc`.
        unsafe { crate::malloc::free::free(node.cast::<c_void>()) };
    }
}

/// A root word as a node option.
pub(crate) fn root_from(word: *mut c_void) -> Option<*mut TreeNode> {
    if word.is_null() {
        None
    } else {
        Some(word as *mut TreeNode)
    }
}

/// A node option as the C root word.
pub(crate) fn root_word(root: Option<*mut TreeNode>) -> *mut c_void {
    root.map_or(core::ptr::null_mut(), |node| node.cast::<c_void>())
}

/// The C tree comparator as the crate's word comparator.
pub(crate) fn comparator(
    func: Option<extern "C" fn(*const c_void, *const c_void) -> c_int>,
) -> impl FnMut(usize, usize) -> c_int {
    move |a: usize, b: usize| {
        let Some(func) = func else {
            return 0;
        };
        func(a as *const c_void, b as *const c_void)
    }
}
