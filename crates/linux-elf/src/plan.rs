// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Load planning: biases, program headers, `brk`, `PT_GNU_STACK` and `PT_TLS`.

use raven_elf::{Elf, Segment};

use crate::LinuxElfError;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const PHDR_SIZE: u64 = 56;
const EHDR_SIZE: usize = 64;
const PT_TLS: u32 = 7;
const PT_GNU_STACK: u32 = 0x6474_e551;
const PF_X: u32 = 1;

/// Inputs that are not properties of the ELF files themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Preferred load bias for an `ET_DYN` main executable; must be page aligned.
    pub main_pie_base: u64,
    /// Load bias for the `ET_DYN` interpreter; must be page aligned.
    pub interp_base: u64,
    /// Page size used for segment rounding.
    pub page: u64,
    /// Real user id reported through `AT_UID`.
    pub uid: u32,
    /// Effective user id reported through `AT_EUID`.
    pub euid: u32,
    /// Real group id reported through `AT_GID`.
    pub gid: u32,
    /// Effective group id reported through `AT_EGID`.
    pub egid: u32,
    /// Whether the process is secure (`AT_SECURE`).
    pub secure: bool,
    /// `AT_HWCAP` value.
    pub hwcap: u64,
    /// `AT_PLATFORM` string, for example `"x86_64"`.
    pub platform: &'static str,
    /// `AT_CLKTCK` value.
    pub clktck: u64,
    /// Optional vDSO address for `AT_SYSINFO_EHDR`.
    pub sysinfo_ehdr: Option<u64>,
    /// Sixteen random bytes copied into the stack for `AT_RANDOM`.
    pub random: [u8; 16],
}

/// A page-aligned summary of one ELF image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadPlanSummary {
    /// Load bias applied to every address in this image.
    pub bias: u64,
    /// Lowest page-aligned segment address after applying `bias`.
    pub start: u64,
    /// Page-aligned end of the highest segment after applying `bias`.
    pub end: u64,
    /// Entry point after applying `bias`.
    pub entry: u64,
}

/// The `PT_TLS` template of the main image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsInfo {
    /// Biased virtual address of the TLS template.
    pub vaddr: u64,
    /// File offset of the template bytes.
    pub offset: u64,
    /// Number of initialized bytes.
    pub filesz: u64,
    /// Total thread-local storage size.
    pub memsz: u64,
    /// Required alignment.
    pub align: u64,
}

/// Everything the personality needs to build the initial Linux process image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImagePlan<'a> {
    /// Main executable summary.
    pub main: LoadPlanSummary,
    /// Interpreter path and summary, when the main executable is dynamic.
    pub interp: Option<(&'a str, LoadPlanSummary)>,
    /// Bias applied to the main executable.
    pub main_bias: u64,
    /// Bias applied to the interpreter; zero when there is none.
    pub interp_bias: u64,
    /// Interpreter entry when present, otherwise the main entry.
    pub entry: u64,
    /// Address of the main program headers (`AT_PHDR`).
    pub phdr_addr: u64,
    /// Program-header entry size (`AT_PHENT`).
    pub phent: u64,
    /// Number of program headers (`AT_PHNUM`).
    pub phnum: u64,
    /// End of the highest main segment, page aligned (`brk` start).
    pub brk_start: u64,
    /// Whether `PT_GNU_STACK` requests an executable stack.
    pub exec_stack: bool,
    /// Main-image TLS template, if `PT_TLS` is present.
    pub tls: Option<TlsInfo>,
}

/// Compute the load plan for a main executable and an optional interpreter.
pub fn plan_image<'a>(
    main: &'a Elf<'a>,
    interp: Option<&'a Elf<'a>>,
    config: &Config,
) -> Result<ImagePlan<'a>, LinuxElfError> {
    if config.page == 0 || !config.page.is_power_of_two() {
        return Err(LinuxElfError::InvalidConfig);
    }

    let main_bias = match e_type(main)? {
        ET_EXEC => 0,
        ET_DYN => {
            if !aligned(config.main_pie_base, config.page) {
                return Err(LinuxElfError::InvalidConfig);
            }
            config.main_pie_base
        }
        _ => return Err(LinuxElfError::BadProgramHeaders),
    };
    let main_summary = summarize(main, main_bias, config.page)?;
    let phdr_addr = main
        .phdr_vaddr()
        .ok_or(LinuxElfError::BadProgramHeaders)?
        .checked_add(main_bias)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let extras = extras(main, main_bias)?;

    let interp_summary = match (main.interp(), interp) {
        (Some(path), Some(interpreter)) => {
            if e_type(interpreter)? != ET_DYN {
                return Err(LinuxElfError::InterpreterNotDynamic);
            }
            if !aligned(config.interp_base, config.page) {
                return Err(LinuxElfError::InvalidConfig);
            }
            Some((
                path,
                summarize(interpreter, config.interp_base, config.page)?,
            ))
        }
        (Some(_), None) => return Err(LinuxElfError::MissingInterpreter),
        (None, None) => None,
        (None, Some(_)) => return Err(LinuxElfError::UnexpectedInterpreter),
    };

    let (interp_bias, entry) = match &interp_summary {
        Some((_, summary)) => (config.interp_base, summary.entry),
        None => (0, main_summary.entry),
    };

    Ok(ImagePlan {
        main: main_summary,
        interp: interp_summary,
        main_bias,
        interp_bias,
        entry,
        phdr_addr,
        phent: PHDR_SIZE,
        phnum: u64::try_from(main.phnum()).map_err(|_| LinuxElfError::AddressOverflow)?,
        brk_start: main_summary.end,
        exec_stack: extras.exec_stack,
        tls: extras.tls,
    })
}

struct Extras {
    exec_stack: bool,
    tls: Option<TlsInfo>,
}

fn summarize(elf: &Elf<'_>, bias: u64, page: u64) -> Result<LoadPlanSummary, LinuxElfError> {
    let mut start: Option<u64> = None;
    let mut end: Option<u64> = None;
    for segment in elf.segments() {
        let raw_end = segment
            .vaddr
            .checked_add(segment.memsz)
            .ok_or(LinuxElfError::AddressOverflow)?;
        let segment_start = align_down(segment.vaddr, page)?;
        let segment_end = align_up(raw_end, page)?;
        let biased_start = segment_start
            .checked_add(bias)
            .ok_or(LinuxElfError::AddressOverflow)?;
        let biased_end = segment_end
            .checked_add(bias)
            .ok_or(LinuxElfError::AddressOverflow)?;
        start = Some(start.map_or(biased_start, |current| current.min(biased_start)));
        end = Some(end.map_or(biased_end, |current| current.max(biased_end)));
    }
    let entry = elf
        .entry()
        .checked_add(bias)
        .ok_or(LinuxElfError::AddressOverflow)?;
    match (start, end) {
        (Some(start), Some(end)) => Ok(LoadPlanSummary {
            bias,
            start,
            end,
            entry,
        }),
        _ => Err(LinuxElfError::BadProgramHeaders),
    }
}

fn extras(elf: &Elf<'_>, bias: u64) -> Result<Extras, LinuxElfError> {
    let table = program_headers(elf)?;
    let mut exec_stack = false;
    let mut tls = None;

    for index in 0..elf.phnum() {
        let base = index
            .checked_mul(PHDR_SIZE as usize)
            .ok_or(LinuxElfError::AddressOverflow)?;
        let p_type = read_u32(table, base).ok_or(LinuxElfError::BadProgramHeaders)?;
        match p_type {
            PT_GNU_STACK => {
                let flags = read_u32(table, base + 4).ok_or(LinuxElfError::BadProgramHeaders)?;
                exec_stack = flags & PF_X != 0;
            }
            PT_TLS => {
                let offset = read_u64(table, base + 8).ok_or(LinuxElfError::BadProgramHeaders)?;
                let vaddr = read_u64(table, base + 16).ok_or(LinuxElfError::BadProgramHeaders)?;
                let filesz = read_u64(table, base + 32).ok_or(LinuxElfError::BadProgramHeaders)?;
                let memsz = read_u64(table, base + 40).ok_or(LinuxElfError::BadProgramHeaders)?;
                let align = read_u64(table, base + 48).ok_or(LinuxElfError::BadProgramHeaders)?;
                if filesz > memsz {
                    return Err(LinuxElfError::BadProgramHeaders);
                }
                tls = Some(TlsInfo {
                    vaddr: vaddr
                        .checked_add(bias)
                        .ok_or(LinuxElfError::AddressOverflow)?,
                    offset,
                    filesz,
                    memsz,
                    align,
                });
            }
            _ => {}
        }
    }

    Ok(Extras { exec_stack, tls })
}

fn e_type(elf: &Elf<'_>) -> Result<u16, LinuxElfError> {
    let header = header(elf)?;
    read_u16(header, 16).ok_or(LinuxElfError::BadProgramHeaders)
}

fn header<'a>(elf: &Elf<'a>) -> Result<&'a [u8], LinuxElfError> {
    let probe = Segment {
        vaddr: 0,
        memsz: EHDR_SIZE as u64,
        filesz: EHDR_SIZE as u64,
        offset: 0,
        prot: 0,
    };
    let bytes = elf.segment_data(&probe);
    if bytes.len() != EHDR_SIZE {
        return Err(LinuxElfError::BadProgramHeaders);
    }
    Ok(bytes)
}

fn program_headers<'a>(elf: &Elf<'a>) -> Result<&'a [u8], LinuxElfError> {
    let header = header(elf)?;
    let e_phoff = read_u64(header, 32).ok_or(LinuxElfError::BadProgramHeaders)?;
    let e_phentsize = read_u16(header, 54).ok_or(LinuxElfError::BadProgramHeaders)?;
    if u64::from(e_phentsize) != PHDR_SIZE {
        return Err(LinuxElfError::BadProgramHeaders);
    }
    let count = u64::try_from(elf.phnum()).map_err(|_| LinuxElfError::AddressOverflow)?;
    let length = count
        .checked_mul(PHDR_SIZE)
        .ok_or(LinuxElfError::AddressOverflow)?;
    let expected = usize::try_from(length).map_err(|_| LinuxElfError::AddressOverflow)?;
    let probe = Segment {
        vaddr: 0,
        memsz: length,
        filesz: length,
        offset: e_phoff,
        prot: 0,
    };
    let bytes = elf.segment_data(&probe);
    if bytes.len() != expected {
        return Err(LinuxElfError::BadProgramHeaders);
    }
    Ok(bytes)
}

fn aligned(value: u64, page: u64) -> bool {
    value.is_multiple_of(page)
}

fn align_down(value: u64, page: u64) -> Result<u64, LinuxElfError> {
    value
        .checked_sub(value % page)
        .ok_or(LinuxElfError::AddressOverflow)
}

fn align_up(value: u64, page: u64) -> Result<u64, LinuxElfError> {
    value
        .checked_add(page - 1)
        .map(|value| value & !(page - 1))
        .ok_or(LinuxElfError::AddressOverflow)
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(data.get(offset..end)?.try_into().ok()?))
}
