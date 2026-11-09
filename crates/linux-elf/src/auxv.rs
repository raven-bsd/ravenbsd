// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The Linux auxiliary vector.

use alloc::vec::Vec;

use raven_linux_abi::flags::auxv::x86_64::AT_SYSINFO_EHDR;
use raven_linux_abi::flags::auxv::{
    AT_BASE, AT_CLKTCK, AT_EGID, AT_ENTRY, AT_EUID, AT_EXECFN, AT_FLAGS, AT_GID, AT_HWCAP, AT_NULL,
    AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM, AT_PLATFORM, AT_RANDOM, AT_SECURE, AT_UID,
};

use crate::{Config, ImagePlan};

/// Auxiliary vector pairs, terminated by `(AT_NULL, 0)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auxv(
    /// The `(key, value)` pairs in Linux order.
    pub Vec<(u64, u64)>,
);

/// Build the auxiliary vector for a planned image.
///
/// `execfn_addr`, `platform_addr` and `random_addr` are addresses inside the initial stack where
/// [`crate::build_stack`] placed the corresponding bytes.
pub fn build_auxv(
    plan: &ImagePlan<'_>,
    config: &Config,
    execfn_addr: u64,
    platform_addr: u64,
    random_addr: u64,
) -> Auxv {
    let mut pairs = Vec::with_capacity(19);
    pairs.push((u64::from(AT_PHDR), plan.phdr_addr));
    pairs.push((u64::from(AT_PHENT), plan.phent));
    pairs.push((u64::from(AT_PHNUM), plan.phnum));
    pairs.push((u64::from(AT_PAGESZ), config.page));
    pairs.push((u64::from(AT_BASE), plan.interp_bias));
    pairs.push((u64::from(AT_FLAGS), 0));
    pairs.push((u64::from(AT_ENTRY), plan.main.entry));
    pairs.push((u64::from(AT_UID), u64::from(config.uid)));
    pairs.push((u64::from(AT_EUID), u64::from(config.euid)));
    pairs.push((u64::from(AT_GID), u64::from(config.gid)));
    pairs.push((u64::from(AT_EGID), u64::from(config.egid)));
    pairs.push((u64::from(AT_PLATFORM), platform_addr));
    pairs.push((u64::from(AT_HWCAP), config.hwcap));
    pairs.push((u64::from(AT_CLKTCK), config.clktck));
    pairs.push((u64::from(AT_SECURE), if config.secure { 1 } else { 0 }));
    pairs.push((u64::from(AT_RANDOM), random_addr));
    pairs.push((u64::from(AT_EXECFN), execfn_addr));
    if let Some(ehdr) = config.sysinfo_ehdr {
        pairs.push((u64::from(AT_SYSINFO_EHDR), ehdr));
    }
    pairs.push((u64::from(AT_NULL), 0));
    Auxv(pairs)
}
