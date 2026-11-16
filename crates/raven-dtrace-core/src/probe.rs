// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Probe descriptors.
//!
//! A probe is identified by the `<provider, module, function, name>` tuple and a stable numeric
//! identifier. The provider fires the identifier; the registry maps it back to the descriptor.
//! The current provider set is static: a later ticket adds dynamic USDT probes.

/// One probe descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// Stable numeric identifier used by the probe site.
    pub id: u32,
    /// Provider name (`syscall`, ...).
    pub provider: &'static str,
    /// Module name, or an empty string for an unanchored probe.
    pub module: &'static str,
    /// Function name, or an empty string for an unanchored probe.
    pub function: &'static str,
    /// Probe name within the provider.
    pub name: &'static str,
}

/// The syscall-entry probe identifier.
pub const SYSCALL_ENTRY_ID: u32 = 1;

/// The static probe table.
pub const PROBES: &[Probe] = &[Probe {
    id: SYSCALL_ENTRY_ID,
    provider: "syscall",
    module: "kernel",
    function: "dispatch",
    name: "entry",
}];

/// Look a probe up by identifier.
#[must_use]
pub fn probe(id: u32) -> Option<&'static Probe> {
    PROBES.iter().find(|probe| probe.id == id)
}

/// Look a probe up by provider and name.
#[allow(dead_code)] // exercised by the host tests; the kernel enables probes by identifier
#[must_use]
pub fn probe_by_name(provider: &str, name: &str) -> Option<&'static Probe> {
    PROBES
        .iter()
        .find(|probe| probe.provider == provider && probe.name == name)
}

/// User-visible probe record (`dtrace_probe_list`).
///
/// The fixed-size strings keep the layout architecture-independent; unused bytes are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeInfo {
    /// Stable probe identifier.
    pub id: u32,
    /// Provider name, NUL-padded.
    pub provider: [u8; 16],
    /// Probe name, NUL-padded.
    pub name: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<ProbeInfo>() == 52);

/// Fill a user record from a descriptor.
pub fn fill_info(probe: &Probe, info: &mut ProbeInfo) {
    info.id = probe.id;
    info.provider = [0; 16];
    info.name = [0; 32];
    copy_str(&mut info.provider, probe.provider);
    copy_str(&mut info.name, probe.name);
}

/// Copy a NUL-terminated prefix of `text` into `destination`.
fn copy_str(destination: &mut [u8], text: &str) {
    let length = text.len().min(destination.len().saturating_sub(1));
    if let (Some(target), Some(source)) =
        (destination.get_mut(..length), text.as_bytes().get(..length))
    {
        target.copy_from_slice(source);
    }
}

#[cfg(test)]
#[path = "probe_ut.rs"]
mod tests;
