// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Root pivot for the vfsd mount table.

use raven_abi::errno::Errno;
use raven_vfs_core::MountTable;

/// Replace the root mount with `scheme`, preserving every non-root mount.
///
/// `MountTable` deliberately rejects a duplicate mount point and `/` cannot be unmounted while
/// another mount exists, so a live root pivot builds a replacement table from the existing
/// non-root entries and installs the new root. The old open files keep their targets; new path
/// resolution uses the replacement.
#[allow(dead_code)] // the mount handler is target-only; the host build omits it
pub(crate) fn pivot_root(table: &MountTable, scheme: &str) -> Result<MountTable, Errno> {
    let mut replacement = MountTable::new();
    for mount in table.mounts() {
        if mount.path != "/" {
            replacement.mount(&mount.path, &mount.scheme)?;
        }
    }
    replacement.mount("/", scheme)?;
    Ok(replacement)
}

#[cfg(test)]
#[path = "pivot_ut.rs"]
mod tests;
