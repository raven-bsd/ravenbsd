// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Service grant policy.
//!
//! Grants are capability names in the service manifest. The `root` capability is reserved for
//! `pcid` (the PCI enumerator) and `platform-root` for `devd` (the Pi 4 platform launcher); every
//! other service gets at most the `initfs` archive. A profile allows one of the device grants by
//! staging a manifest that declares it: the policy below names the only service that may hold
//! each capability, so a manifest cannot invent a grant for another service. The policy is pure
//! so it can be tested on the host.
//!
//! The two capabilities are deliberately not identical. `root` keeps the full
//! `ADMIN|DUP|TRANSFER` device capability that the PCI enumerator has always held.
//! `platform-root` is the narrower child of that capability: `devd` needs `ADMIN` to create the
//! MMIO, DMA and IRQ handles selected from the device tree, but it never needs to duplicate or
//! transfer the root device capability itself. Drivers get only the individual handles `devd`
//! created for them.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_abi::types::{Handle, Rights};

/// One capability a service may receive before its first thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantSpec {
    /// The boot initfs memory object.
    Initfs,
    /// The PCI root device capability (`ADMIN|DUP|TRANSFER`), reserved for `pcid`.
    Root,
    /// The Pi 4 platform root device capability (`ADMIN` only), reserved for `devd`.
    ///
    /// `devd` uses it to create the MMIO/DMA/IRQ handles declared by the device tree. It is
    /// narrower than [`Self::Root`] because `devd` never duplicates or transfers the root
    /// capability; each driver receives only the handles created for that driver.
    PlatformRoot,
}

impl GrantSpec {
    /// Handle to transfer from PID 1.
    pub const fn handle(self) -> Handle {
        match self {
            Self::Initfs => Handle::INITFS,
            Self::Root | Self::PlatformRoot => Handle::ROOT_DEVICE,
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
            Self::PlatformRoot => Rights::ADMIN,
        }
    }
}

/// Translate a manifest grant list into transfer specifications.
///
/// `root` is accepted only for the `pcid` service and `platform-root` only for the `devd`
/// service; unknown names and a device-grant request from any other service are errors. The
/// caller rejects the service before spawning. The service name comes from the profile-managed
/// manifest, not from the process, so a service cannot rename itself into a grant.
pub fn parse_grants(service: &str, grants: &[String]) -> Result<Vec<GrantSpec>, String> {
    let mut result = Vec::new();
    for grant in grants {
        match grant.as_str() {
            "initfs" => result.push(GrantSpec::Initfs),
            "root" if service == "pcid" => result.push(GrantSpec::Root),
            "root" => {
                return Err(format!("root grant reserved for pcid, not `{service}`"));
            }
            "platform-root" if service == "devd" => result.push(GrantSpec::PlatformRoot),
            "platform-root" => {
                return Err(format!(
                    "platform-root grant reserved for devd, not `{service}`"
                ));
            }
            other => return Err(format!("unknown grant `{other}`")),
        }
    }
    if result.contains(&GrantSpec::PlatformRoot) && result.len() != 1 {
        return Err(String::from(
            "platform-root must be the only grant on its service entry",
        ));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "grants_ut.rs"]
mod tests;
