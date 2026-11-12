// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Service grant policy.
//!
//! Grants are capability names in the service manifest. The `root` capability is reserved for
//! `pcid`, the only server allowed to create device capabilities; every other service gets at
//! most the `initfs` archive. The policy is pure so it can be tested on the host.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::types::{Handle, Rights};

/// One capability a service may receive before its first thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantSpec {
    /// The boot initfs memory object.
    Initfs,
    /// The root device capability (`ADMIN|DUP|TRANSFER`), reserved for `pcid`.
    Root,
}

impl GrantSpec {
    /// Handle to transfer from PID 1.
    pub const fn handle(self) -> Handle {
        match self {
            Self::Initfs => Handle::INITFS,
            Self::Root => Handle::ROOT_DEVICE,
        }
    }

    /// Rights to request in the transfer.
    pub const fn rights(self) -> Rights {
        match self {
            Self::Initfs => Rights::READ
                .union(Rights::MAP)
                .union(Rights::EXEC)
                .union(Rights::TRANSFER),
            Self::Root => Rights::ADMIN.union(Rights::DUP).union(Rights::TRANSFER),
        }
    }
}

/// Translate a manifest grant list into transfer specifications.
///
/// `root` is accepted only for the `pcid` service; unknown names and a non-pcid `root` request
/// are errors. The caller rejects the service before spawning.
pub fn parse_grants(service: &str, grants: &[String]) -> Result<Vec<GrantSpec>, String> {
    let mut result = Vec::new();
    for grant in grants {
        match grant.as_str() {
            "initfs" => result.push(GrantSpec::Initfs),
            "root" if service == "pcid" => result.push(GrantSpec::Root),
            "root" => {
                return Err(format!(
                    "root grant reserved for pcid, not `{}`",
                    service.to_string()
                ));
            }
            other => return Err(format!("unknown grant `{other}`")),
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "grants_ut.rs"]
mod tests;
