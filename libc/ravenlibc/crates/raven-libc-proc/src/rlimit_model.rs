// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The local rlimit table and its defaults.

use crate::consts::{RLIM_INFINITY, RLIMIT_NLIMITS, RLIMIT_NOFILE};
use crate::errors::{EINVAL, EPERM, Errno};

/// The 16 resource limits, current and maximum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RlimitTable {
    /// Current soft limits.
    pub cur: [u64; 16],
    /// Hard limits.
    pub max: [u64; 16],
}

impl RlimitTable {
    /// The built-in defaults: `NOFILE 1024/32768`, everything else unlimited.
    #[must_use]
    #[allow(clippy::indexing_slicing)] // the constant indices are below 16
    pub const fn new() -> Self {
        let mut cur = [RLIM_INFINITY; 16];
        let mut max = [RLIM_INFINITY; 16];
        cur[RLIMIT_NOFILE as usize] = 1024;
        max[RLIMIT_NOFILE as usize] = 32_768;
        Self { cur, max }
    }

    /// Read one limit pair.
    pub fn get(&self, resource: u32) -> Result<(u64, u64), Errno> {
        let index = resource_index(resource)?;
        let cur = self.cur.get(index).copied().ok_or(EINVAL)?;
        let max = self.max.get(index).copied().ok_or(EINVAL)?;
        Ok((cur, max))
    }

    /// Set one limit pair.
    pub fn set(
        &mut self,
        resource: u32,
        new_cur: u64,
        new_max: u64,
        is_root: bool,
    ) -> Result<(), Errno> {
        let index = resource_index(resource)?;
        if new_cur > new_max {
            return Err(EINVAL);
        }
        let old_max = self.max.get(index).copied().ok_or(EINVAL)?;
        if !is_root && new_max > old_max {
            return Err(EPERM);
        }
        let cur = self.cur.get_mut(index).ok_or(EINVAL)?;
        *cur = new_cur;
        let max = self.max.get_mut(index).ok_or(EINVAL)?;
        *max = new_max;
        Ok(())
    }

    /// Copy the 16 pairs from a procinfo page.
    pub fn get_from_page(page: &[u8; 4096]) -> Result<Self, Errno> {
        let mut table = Self::new();
        for index in 0..16usize {
            let offset = 200usize
                .checked_add(index.checked_mul(16).ok_or(EINVAL)?)
                .ok_or(EINVAL)?;
            let cur = read_u64(page, offset)?;
            let max = read_u64(page, offset.checked_add(8).ok_or(EINVAL)?)?;
            let slot = table.cur.get_mut(index).ok_or(EINVAL)?;
            *slot = cur;
            let slot = table.max.get_mut(index).ok_or(EINVAL)?;
            *slot = max;
        }
        Ok(table)
    }
}

impl Default for RlimitTable {
    fn default() -> Self {
        Self::new()
    }
}

fn resource_index(resource: u32) -> Result<usize, Errno> {
    if resource >= RLIMIT_NLIMITS {
        return Err(EINVAL);
    }
    usize::try_from(resource).map_err(|_| EINVAL)
}

fn read_u64(page: &[u8], offset: usize) -> Result<u64, Errno> {
    let bytes = page
        .get(offset..offset.checked_add(8).ok_or(EINVAL)?)
        .ok_or(EINVAL)?;
    let array: [u8; 8] = bytes.try_into().map_err(|_| EINVAL)?;
    Ok(u64::from_le_bytes(array))
}

#[cfg(test)]
#[path = "rlimit_model_ut.rs"]
mod tests;
