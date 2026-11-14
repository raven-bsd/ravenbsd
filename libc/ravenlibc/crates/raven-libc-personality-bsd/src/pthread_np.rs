// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The BSD `pthread_np.h` names (`B09`).
//!
//! `raven-libc-thread` (C57) is not on the C66 branch, so this ticket ships the alias table
//! only: the shell cannot alias a base symbol that does not exist. When C57 merges, the shell
//! adds the aliases and the table becomes live without a logic change.

/// `(BSD name, Linux-form base)`, in `02` B09 order.
///
/// `"-"` marks a name with no Linux-form base (a predicate the thread crate supplies);
/// `"ENOSYS"` marks a documented unsupported call (`PG-48`).
pub const ALIASES: &[(&str, &str)] = &[
    ("pthread_set_name_np", "pthread_setname_np"),
    ("pthread_get_name_np", "pthread_getname_np"),
    ("pthread_attr_get_np", "pthread_getattr_np"),
    ("pthread_main_np", "-"),
    ("pthread_suspend_np", "ENOSYS"),
    ("pthread_resume_np", "ENOSYS"),
];

/// The base of one BSD spelling.
#[must_use]
pub fn base(name: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, base)| *base)
}

#[cfg(test)]
#[path = "pthread_np_ut.rs"]
mod tests;
