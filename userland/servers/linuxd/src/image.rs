// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Plan a Linux image for `linuxd` with `raven-elf` and `linux-elf`.
//!
//! This module does not load anything: it validates the ELF, computes the load biases, the entry
//! point, the program-header address for `AT_PHDR`, the `brk` start and the initial stack, and
//! turns the main image and its optional interpreter into page-sized mapping steps.
//! [`crate::loader`] performs the mappings.
//!
//! A `ET_DYN` main executable is parsed with a zero-based address window because its raw segment
//! addresses start at zero; `linux-elf` applies `Config::main_pie_base` to every address and the
//! loader maps the raw steps at that bias. The same applies to the `ET_DYN` interpreter at
//! `Config::interp_base`. `AT_BASE`/`AT_ENTRY` and the initial PC come from `linux-elf`'s plan.

use alloc::vec::Vec;

use linux_elf::{ImagePlan, LinuxElfError, StackImage, build_stack, plan_image as plan_linux};
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, plan_load};

pub use linux_elf::Config;

/// Preferred load bias for an `ET_DYN` main executable. It stays above the low user window and
/// leaves room for the `brk` arena, which is mapped at the main image's end.
pub const MAIN_PIE_BASE: u64 = 0x0000_1000_0000_0000;

/// Preferred load bias for the `ET_DYN` interpreter. It sits above the main image and below the
/// `mmap` arena (`0x3000_0000_0000`); a musl loader is well under 64 MiB.
pub const INTERP_BASE: u64 = 0x0000_1800_0000_0000;

/// Guest path of the Linux root tree staged by the B32 recipe. A `PT_INTERP` path is absolute
/// inside the Linux root (`/lib/ld-musl-x86_64.so.1`), so the server tries the initfs path as-is
/// and then this root prefix.
pub const LINUX_ROOT: &str = "/compat/linux";

const ET_DYN: u16 = 3;

/// Largest Linux initial stack, matching `linux-elf`'s own bound.
pub const MAX_STACK: u64 = 1 << 20;

/// Why an image could not be planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// The ELF parser rejected the file.
    Elf(ElfError),
    /// The Linux image rules rejected the file.
    Linux(LinuxElfError),
    /// A biased mapping would leave the user address window.
    Range,
}

/// A validated Linux image and its page-level load plan.
#[derive(Clone, Debug)]
pub struct ImageSpec {
    /// Load bias for every main-image step address.
    pub bias: u64,
    /// Load bias for every interpreter step address; zero when there is no interpreter.
    pub interp_bias: u64,
    /// First instruction: the interpreter entry when present, otherwise the main entry.
    pub entry: u64,
    /// `AT_PHDR`: address of the main program-header table in the child.
    pub phdr: u64,
    /// `AT_PHENT`.
    pub phent: u64,
    /// `AT_PHNUM`.
    pub phnum: u64,
    /// `brk` start: the first page past the highest main segment.
    pub brk_start: u64,
    /// Page-sized main-image mappings, in increasing raw address order.
    pub steps: Vec<MapStep>,
    /// Page-sized interpreter mappings, in increasing raw address order.
    pub interp_steps: Vec<MapStep>,
    /// The initial stack image; `base` is the address of `bytes[0]`.
    pub stack: StackImage,
}

/// Return the `PT_INTERP` path of `image`, if it has one.
///
/// This parses the ELF only to read the path; [`plan_image`] parses it again with the plan's
/// validation. The caller uses the path to read the interpreter bytes from the initfs before it
/// calls [`plan_image`].
pub fn interp_path(image: &[u8]) -> Result<Option<&str>, ImageError> {
    let e_type = e_type(image)?;
    let range = if e_type == ET_DYN {
        0..USER_END
    } else {
        USER_MIN..USER_END
    };
    let elf = Elf::parse(image, Machine::X86_64, range).map_err(ImageError::Elf)?;
    Ok(elf.interp())
}

/// Validate `image` and its `interp` bytes and build the plan.
///
/// `argv`/`envp`/`execfn` follow the Linux exec convention; `stack_top` is the address the stack is
/// grown down from. `interp` must be the bytes named by the main image's `PT_INTERP`; `None` is
/// accepted only for a static main.
pub fn plan_image(
    image: &[u8],
    interp: Option<&[u8]>,
    config: &Config,
    argv: &[&[u8]],
    envp: &[&[u8]],
    execfn: &[u8],
    stack_top: u64,
) -> Result<ImageSpec, ImageError> {
    let e_type = e_type(image)?;
    let range = if e_type == ET_DYN {
        0..USER_END
    } else {
        USER_MIN..USER_END
    };
    let elf = Elf::parse(image, Machine::X86_64, range).map_err(ImageError::Elf)?;
    let interp_elf = match interp {
        // An interpreter is always ET_DYN; its raw addresses start at zero.
        Some(bytes) => {
            Some(Elf::parse(bytes, Machine::X86_64, 0..USER_END).map_err(ImageError::Elf)?)
        }
        None => None,
    };
    let plan: ImagePlan<'_> =
        plan_linux(&elf, interp_elf.as_ref(), config).map_err(ImageError::Linux)?;
    let steps = collect_steps(&elf, config.page)?;
    check_steps(&steps, plan.main_bias)?;
    let interp_steps = match &interp_elf {
        Some(interp_elf) => {
            let steps = collect_steps(interp_elf, config.page)?;
            check_steps(&steps, plan.interp_bias)?;
            steps
        }
        None => Vec::new(),
    };
    let stack =
        build_stack(stack_top, argv, envp, execfn, &plan, config).map_err(ImageError::Linux)?;
    Ok(ImageSpec {
        bias: plan.main_bias,
        interp_bias: plan.interp_bias,
        entry: plan.entry,
        phdr: plan.phdr_addr,
        phent: plan.phent,
        phnum: plan.phnum,
        brk_start: plan.brk_start,
        steps,
        interp_steps,
        stack,
    })
}

fn e_type(image: &[u8]) -> Result<u16, ImageError> {
    let bytes = image
        .get(16..18)
        .ok_or(ImageError::Elf(ElfError::TooShort))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn collect_steps(elf: &Elf<'_>, page: u64) -> Result<Vec<MapStep>, ImageError> {
    let mut steps: Vec<MapStep> = plan_load(elf, page, LoadMode::Copy)
        .map_err(ImageError::Elf)?
        .collect();
    steps.sort_by_key(|step| step.vaddr);
    Ok(steps)
}

/// Every biased step must lie inside the user window; the kernel would reject it later, but the
/// plan should fail before the process is created.
fn check_steps(steps: &[MapStep], bias: u64) -> Result<(), ImageError> {
    for step in steps {
        let start = step.vaddr.checked_add(bias).ok_or(ImageError::Range)?;
        let end = start.checked_add(step.len).ok_or(ImageError::Range)?;
        if start < USER_MIN || end > USER_END {
            return Err(ImageError::Range);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "image_ut.rs"]
mod tests;
