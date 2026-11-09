// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The initial Linux x86-64 stack image.

use alloc::vec;
use alloc::vec::Vec;

use crate::{Auxv, Config, ImagePlan, LinuxElfError, build_auxv};

const MAX_STACK: u64 = 1 << 20;
const STACK_ALIGN: u64 = 16;

/// A stack image ready to copy into a new process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackImage {
    /// Bytes of the image; `bytes[0]` is at [`StackImage::base`].
    pub bytes: Vec<u8>,
    /// Address of `bytes[0]`.
    pub base: u64,
    /// Initial stack pointer, pointing at `argc`; 16-byte aligned.
    pub sp: u64,
}

/// Build the initial stack below `stack_top`.
///
/// Strings are placed at the top in the order execfn, envp, argv, platform and the 16 random
/// bytes. The dynamic block (`argc`, argv pointers, envp pointers and auxv) is placed at the
/// bottom; padding between the two leaves `sp` 16-byte aligned.
pub fn build_stack(
    stack_top: u64,
    argv: &[&[u8]],
    envp: &[&[u8]],
    execfn: &[u8],
    plan: &ImagePlan<'_>,
    config: &Config,
) -> Result<StackImage, LinuxElfError> {
    check_nul(execfn)?;
    for argument in argv {
        check_nul(argument)?;
    }
    for variable in envp {
        check_nul(variable)?;
    }
    if config.platform.as_bytes().contains(&0) {
        return Err(LinuxElfError::EmbeddedNul);
    }

    let mut strings = Vec::new();
    let random_offset = u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?;
    strings.extend_from_slice(&config.random);

    let platform_offset = u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?;
    strings.extend_from_slice(config.platform.as_bytes());
    strings.push(0);

    let mut argv_offsets = Vec::with_capacity(argv.len());
    for argument in argv {
        argv_offsets.push(u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?);
        strings.extend_from_slice(argument);
        strings.push(0);
    }

    let mut envp_offsets = Vec::with_capacity(envp.len());
    for variable in envp {
        envp_offsets.push(u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?);
        strings.extend_from_slice(variable);
        strings.push(0);
    }

    let execfn_offset = u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?;
    strings.extend_from_slice(execfn);
    strings.push(0);

    let strings_len = u64::try_from(strings.len()).map_err(|_| LinuxElfError::TooLarge)?;
    let strings_base = stack_top
        .checked_sub(strings_len)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let random_addr = strings_base
        .checked_add(random_offset)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let platform_addr = strings_base
        .checked_add(platform_offset)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let execfn_addr = strings_base
        .checked_add(execfn_offset)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let auxv = build_auxv(plan, config, execfn_addr, platform_addr, random_addr);

    let argc = u64::try_from(argv.len()).map_err(|_| LinuxElfError::TooLarge)?;
    let envc = u64::try_from(envp.len()).map_err(|_| LinuxElfError::TooLarge)?;
    let dynamic_len = dynamic_length(argc, envc, &auxv)?;
    let total = dynamic_len
        .checked_add(strings_len)
        .ok_or(LinuxElfError::AddressOverflow)?;
    if total > MAX_STACK {
        return Err(LinuxElfError::TooLarge);
    }

    let sp = align16(
        stack_top
            .checked_sub(total)
            .ok_or(LinuxElfError::AddressOverflow)?,
    )?;
    let size = stack_top
        .checked_sub(sp)
        .ok_or(LinuxElfError::AddressOverflow)?;
    if size > MAX_STACK {
        return Err(LinuxElfError::TooLarge);
    }
    let size = usize::try_from(size).map_err(|_| LinuxElfError::TooLarge)?;
    let mut bytes = vec![0u8; size];

    let mut position = 0usize;
    put_u64(&mut bytes, position, argc)?;
    position += 8;
    for offset in argv_offsets {
        let pointer = strings_base
            .checked_add(offset)
            .ok_or(LinuxElfError::AddressOverflow)?;
        put_u64(&mut bytes, position, pointer)?;
        position += 8;
    }
    put_u64(&mut bytes, position, 0)?;
    position += 8;
    for offset in envp_offsets {
        let pointer = strings_base
            .checked_add(offset)
            .ok_or(LinuxElfError::AddressOverflow)?;
        put_u64(&mut bytes, position, pointer)?;
        position += 8;
    }
    put_u64(&mut bytes, position, 0)?;
    position += 8;
    for (key, value) in &auxv.0 {
        put_u64(&mut bytes, position, *key)?;
        position += 8;
        put_u64(&mut bytes, position, *value)?;
        position += 8;
    }

    let string_start = usize::try_from(
        strings_base
            .checked_sub(sp)
            .ok_or(LinuxElfError::AddressOverflow)?,
    )
    .map_err(|_| LinuxElfError::TooLarge)?;
    let string_end = string_start
        .checked_add(strings.len())
        .ok_or(LinuxElfError::AddressOverflow)?;
    let destination = bytes
        .get_mut(string_start..string_end)
        .ok_or(LinuxElfError::AddressOverflow)?;
    destination.copy_from_slice(&strings);

    Ok(StackImage {
        bytes,
        base: sp,
        sp,
    })
}

fn dynamic_length(argc: u64, envc: u64, auxv: &Auxv) -> Result<u64, LinuxElfError> {
    let mut length = 8u64;
    length = length
        .checked_add(argc.checked_mul(8).ok_or(LinuxElfError::AddressOverflow)?)
        .ok_or(LinuxElfError::AddressOverflow)?;
    length = length
        .checked_add(8)
        .ok_or(LinuxElfError::AddressOverflow)?;
    length = length
        .checked_add(envc.checked_mul(8).ok_or(LinuxElfError::AddressOverflow)?)
        .ok_or(LinuxElfError::AddressOverflow)?;
    length = length
        .checked_add(8)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let aux_len = u64::try_from(auxv.0.len()).map_err(|_| LinuxElfError::TooLarge)?;
    length
        .checked_add(
            aux_len
                .checked_mul(16)
                .ok_or(LinuxElfError::AddressOverflow)?,
        )
        .ok_or(LinuxElfError::AddressOverflow)
}

fn check_nul(bytes: &[u8]) -> Result<(), LinuxElfError> {
    if bytes.contains(&0) {
        Err(LinuxElfError::EmbeddedNul)
    } else {
        Ok(())
    }
}

fn align16(value: u64) -> Result<u64, LinuxElfError> {
    value
        .checked_sub(value % STACK_ALIGN)
        .ok_or(LinuxElfError::AddressOverflow)
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) -> Result<(), LinuxElfError> {
    let end = offset
        .checked_add(8)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let slot = bytes
        .get_mut(offset..end)
        .ok_or(LinuxElfError::AddressOverflow)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
