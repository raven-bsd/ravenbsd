// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The monotonic pledge reduction rule.

use crate::errors::{EPERM, Errno};
use crate::promise::{Promise, PromiseSet, parse_promises};

const CLEANUP_MASK: u64 = (1u64 << Promise::Rpath.index())
    | (1u64 << Promise::Wpath.index())
    | (1u64 << Promise::Cpath.index())
    | (1u64 << Promise::Dpath.index())
    | (1u64 << Promise::Exec.index())
    | (1u64 << Promise::Unix.index())
    | (1u64 << Promise::Unveil.index());

/// The stored promise sets; `None` means the process is not pledged yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PledgeState {
    /// Active promises.
    pub promises: Option<PromiseSet>,
    /// Promises installed in the next image.
    pub exec_promises: Option<PromiseSet>,
}

/// The side effects of one `pledge` call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PledgeOutcome {
    /// The caller must clear the unveil table.
    pub unveil_cleanup: bool,
}

impl PledgeState {
    /// A fresh, unpledged state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            promises: None,
            exec_promises: None,
        }
    }

    /// Apply one `pledge` call.
    pub fn apply(
        &mut self,
        promises: Option<&[u8]>,
        exec: Option<&[u8]>,
    ) -> Result<PledgeOutcome, Errno> {
        let parsed_promises = match promises {
            Some(text) => Some(parse_promises(text)?),
            None => None,
        };
        let parsed_exec = match exec {
            Some(text) => Some(parse_promises(text)?),
            None => None,
        };
        if let Some(new) = parsed_promises {
            let error_mode = self
                .promises
                .is_some_and(|old| old.contains(Promise::Error));
            self.promises = Some(reduce(self.promises, new, error_mode)?);
        }
        if let Some(new) = parsed_exec {
            self.exec_promises = Some(reduce(self.exec_promises, new, false)?);
        }
        let unveil_cleanup = self.promises.is_some_and(|set| set.0 & CLEANUP_MASK == 0);
        Ok(PledgeOutcome { unveil_cleanup })
    }
}

fn reduce(old: Option<PromiseSet>, new: PromiseSet, error_mode: bool) -> Result<PromiseSet, Errno> {
    let Some(old) = old else {
        return Ok(new);
    };
    if new.union(old).0 != old.0 {
        if error_mode {
            Ok(new.intersection(old))
        } else {
            Err(EPERM)
        }
    } else {
        Ok(new)
    }
}

#[cfg(test)]
#[path = "reduce_ut.rs"]
mod tests;
