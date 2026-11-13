// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<search.h>`: the AVL tree (`tsearch` family), the open-addressing hash table
//! (`hsearch_r` family), `insque`/`remque` and `lsearch`/`lfind`.

pub mod avl;
pub mod hsearch;
pub mod lsearch;
pub mod queue;

pub use avl::{NodeStore, TDelete, Visit, tdelete, tdestroy, tfind, tsearch, twalk};
pub use hsearch::{
    Action, Entropy, Entry, HSearchError, HTable, KeyText, SlotId, hcreate, hdestroy,
};
pub use lsearch::{LSearchError, lfind, lsearch};
pub use queue::{ListStore, insque, remque};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
