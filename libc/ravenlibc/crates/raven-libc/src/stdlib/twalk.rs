// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `twalk(3)`.
use core::ffi::{c_int, c_void};

use raven_libc_stdlib::search::Visit;

use super::search_support::{RawNodes, root_from};

/// Walk the tree at `root` in the donor's order.
///
/// # Safety
/// `root` is a tree root or NULL; `action` is a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn twalk(
    root: *const c_void,
    action: Option<extern "C" fn(*const c_void, c_int, c_int)>,
) {
    let Some(func) = action else {
        return;
    };
    let store = RawNodes;
    let mut callback = |node: *mut super::search_support::TreeNode, visit: Visit, depth: usize| {
        let value = match visit {
            Visit::Preorder => 0,
            Visit::Postorder => 1,
            Visit::Endorder => 2,
            Visit::Leaf => 3,
        };
        func(
            node.cast::<c_void>(),
            value,
            c_int::try_from(depth).unwrap_or(c_int::MAX),
        );
    };
    raven_libc_stdlib::search::twalk(&store, root_from(root.cast_mut()), &mut callback);
}
