// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Plan a static Linux image for `linuxd` with `raven-elf` and `linux-elf`.
//!
//! This module does not load anything: it validates the ELF, computes the load bias, the entry
//! point, the program-header address for `AT_PHDR`, the `brk` start and the initial stack, and
//! turns the segments into page-sized mapping steps. [`crate::loader`] performs the mappings.

use alloc::vec::Vec;

use linux_elf::{ImagePlan, LinuxElfError, StackImage, build_stack, plan_image as plan_linux};
use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, plan_load};

pub use linux_elf::Config;

/// Largest Linux initial stack, matching `linux-elf`'s own bound.
pub const MAX_STACK: u64 = 1 << 20;

/// Why an image could not be planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// The ELF parser rejected the file.
    Elf(ElfError),
    /// The Linux image rules rejected the file.
    Linux(LinuxElfError),
}

/// A validated static Linux image and its page-level load plan.
#[derive(Clone, Debug)]
pub struct ImageSpec {
    /// Load bias for every step address.
    pub bias: u64,
    /// First instruction.
    pub entry: u64,
    /// `AT_PHDR`: address of the program-header table in the child.
    pub phdr: u64,
    /// `AT_PHENT`.
    pub phent: u64,
    /// `AT_PHNUM`.
    pub phnum: u64,
    /// `brk` start: the first page past the highest segment.
    pub brk_start: u64,
    /// Page-sized mappings, in increasing address order.
    pub steps: Vec<MapStep>,
    /// The initial stack image; `base` is the address of `bytes[0]`.
    pub stack: StackImage,
}

/// Validate `image` and build the plan.
///
/// `argv`/`envp`/`execfn` follow the Linux exec convention; `stack_top` is the address the stack is
/// grown down from.
pub fn plan_image(
    image: &[u8],
    config: &Config,
    argv: &[&[u8]],
    envp: &[&[u8]],
    execfn: &[u8],
    stack_top: u64,
) -> Result<ImageSpec, ImageError> {
    let elf = Elf::parse(
        image,
        Machine::X86_64,
        raven_abi::layout::x86_64::USER_MIN..raven_abi::layout::x86_64::USER_END,
    )
    .map_err(ImageError::Elf)?;
    let plan: ImagePlan<'_> = plan_linux(&elf, None, config).map_err(ImageError::Linux)?;
    let steps = plan_load(&elf, config.page, LoadMode::Copy).map_err(ImageError::Elf)?;
    let stack =
        build_stack(stack_top, argv, envp, execfn, &plan, config).map_err(ImageError::Linux)?;
    let mut ordered: Vec<MapStep> = steps.collect();
    ordered.sort_by_key(|step| step.vaddr);
    Ok(ImageSpec {
        bias: plan.main_bias,
        entry: plan.entry,
        phdr: plan.phdr_addr,
        phent: plan.phent,
        phnum: plan.phnum,
        brk_start: plan.brk_start,
        steps: ordered,
        stack,
    })
}

#[cfg(test)]
#[path = "image_ut.rs"]
mod tests;
