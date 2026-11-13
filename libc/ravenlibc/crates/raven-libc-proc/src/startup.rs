// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C8 process-start stack parser.

use alloc::vec::Vec;

use crate::auxv::AuxTable;
use crate::consts::{
    AT_CLKTCK, AT_EXECFN, AT_HWCAP, AT_HWCAP2, AT_NULL, AT_PAGESZ, AT_RANDOM, AT_RAVEN_ABI,
    AT_RAVEN_CWD, AT_RAVEN_FDMAP, AT_RAVEN_FDMAP_N, AT_RAVEN_PROCD, AT_RAVEN_PROCINFO,
    AT_RAVEN_TIMEPAGE, AT_SECURE,
};
use crate::limits::{MAX_AUXV_PAIRS, MAX_ENV_WORDS};

/// A source of pointer-sized words from the initial stack.
pub trait WordSource {
    /// Return word `index`, or `None` outside the mapped stack.
    fn word(&self, index: usize) -> Option<usize>;
}

/// A source of NUL-terminated strings in the initial stack.
pub trait StrSource {
    /// Return the bytes of the C string at `addr`, or `None` when unreadable.
    fn cstr(&self, addr: usize) -> Option<&[u8]>;
}

/// The `AT_RAVEN_*` values decoded from the auxiliary vector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RavenKeys {
    /// Procd connection handle.
    pub procd: Option<u32>,
    /// Procinfo page address.
    pub procinfo: Option<usize>,
    /// Fdmap pointer.
    pub fdmap: Option<usize>,
    /// Fdmap entry count.
    pub fdmap_n: usize,
    /// Current-directory string pointer.
    pub cwd: Option<usize>,
    /// ABI version.
    pub abi: Option<u32>,
    /// Time page pointer.
    pub timepage: Option<usize>,
}

/// The parsed process start information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartInfo {
    /// Argument count.
    pub argc: usize,
    /// Argument string pointers.
    pub argv: Vec<usize>,
    /// Environment string pointers.
    pub envp: Vec<usize>,
    /// Auxiliary vector.
    pub auxv: AuxTable,
    /// Page size, default 4096.
    pub page_size: usize,
    /// Clock ticks per second, default 100.
    pub clk_tck: usize,
    /// `AT_SECURE` flag, always false on Raven.
    pub secure: bool,
    /// `AT_RANDOM` pointer.
    pub random: Option<usize>,
    /// `AT_EXECFN` pointer.
    pub execfn: Option<usize>,
    /// `AT_HWCAP`.
    pub hwcap: usize,
    /// `AT_HWCAP2`.
    pub hwcap2: usize,
    /// Raven-private keys.
    pub raven: RavenKeys,
}

/// Why the stack could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// `argv[argc]` is not zero.
    BadArgv,
    /// The environment has no terminating zero word.
    NoEnvTerminator,
    /// The auxiliary vector has no `AT_NULL`.
    NoAtNull,
    /// A word count exceeded its bound.
    TooManyWords,
}

impl StartError {
    /// Stable diagnostic text.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::BadArgv => "invalid argv terminator",
            Self::NoEnvTerminator => "unterminated environment",
            Self::NoAtNull => "unterminated auxiliary vector",
            Self::TooManyWords => "too many startup words",
        }
    }
}

/// Parse the C8 stack from a word source.
pub fn parse_stack(source: &dyn WordSource) -> Result<StartInfo, StartError> {
    let argc = source.word(0).ok_or(StartError::BadArgv)?;
    if argc > MAX_ENV_WORDS {
        return Err(StartError::TooManyWords);
    }
    let terminator = argc.checked_add(1).ok_or(StartError::TooManyWords)?;
    if source.word(terminator) != Some(0) {
        return Err(StartError::BadArgv);
    }
    let mut argv = Vec::with_capacity(argc);
    for index in 1..=argc {
        let word = source.word(index).ok_or(StartError::BadArgv)?;
        argv.push(word);
    }

    let env_start = argc.checked_add(2).ok_or(StartError::TooManyWords)?;
    let mut envp = Vec::new();
    let mut env_end = None;
    let mut index = env_start;
    let mut scanned = 0usize;
    while scanned < MAX_ENV_WORDS {
        let word = source.word(index).ok_or(StartError::NoEnvTerminator)?;
        if word == 0 {
            env_end = Some(index);
            break;
        }
        envp.push(word);
        index = index.checked_add(1).ok_or(StartError::TooManyWords)?;
        scanned = scanned.saturating_add(1);
    }
    let env_end = env_end.ok_or(StartError::NoEnvTerminator)?;

    let mut auxv = AuxTable::new();
    let mut aux_index = env_end.checked_add(1).ok_or(StartError::TooManyWords)?;
    let mut pairs = 0usize;
    let mut saw_null = false;
    while pairs <= MAX_AUXV_PAIRS {
        let key = source.word(aux_index).ok_or(StartError::NoAtNull)?;
        let value_index = aux_index.checked_add(1).ok_or(StartError::TooManyWords)?;
        let value = source.word(value_index).ok_or(StartError::NoAtNull)?;
        if key == AT_NULL {
            saw_null = true;
            break;
        }
        auxv.insert(key, value);
        aux_index = value_index.checked_add(1).ok_or(StartError::TooManyWords)?;
        pairs = pairs.saturating_add(1);
    }
    if !saw_null {
        return Err(StartError::NoAtNull);
    }

    let page_size = auxv.get(AT_PAGESZ).unwrap_or(4096);
    let clk_tck = auxv.get(AT_CLKTCK).unwrap_or(100);
    let secure = auxv.get(AT_SECURE).unwrap_or(0) != 0;
    let random = auxv.get(AT_RANDOM);
    let execfn = auxv.get(AT_EXECFN);
    let hwcap = auxv.get(AT_HWCAP).unwrap_or(0);
    let hwcap2 = auxv.get(AT_HWCAP2).unwrap_or(0);
    let raven = RavenKeys {
        procd: auxv
            .get(AT_RAVEN_PROCD)
            .and_then(|value| u32::try_from(value).ok()),
        procinfo: auxv.get(AT_RAVEN_PROCINFO),
        fdmap: auxv.get(AT_RAVEN_FDMAP),
        fdmap_n: auxv.get(AT_RAVEN_FDMAP_N).unwrap_or(0),
        cwd: auxv.get(AT_RAVEN_CWD),
        abi: auxv
            .get(AT_RAVEN_ABI)
            .and_then(|value| u32::try_from(value).ok()),
        timepage: auxv.get(AT_RAVEN_TIMEPAGE),
    };
    Ok(StartInfo {
        argc,
        argv,
        envp,
        auxv,
        page_size,
        clk_tck,
        secure,
        random,
        execfn,
        hwcap,
        hwcap2,
        raven,
    })
}

/// Offset of the program name after the last `/`.
#[must_use]
pub fn split_progname(argv0: &[u8]) -> usize {
    argv0
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or(0, |index| index.saturating_add(1))
}

/// The program-name source: `argv[0]`, then `AT_EXECFN`, then an empty string.
#[must_use]
pub fn progname_sources(argv0: Option<&[u8]>, execfn: Option<&[u8]>) -> Vec<u8> {
    match (argv0, execfn) {
        (Some(name), _) => name.to_vec(),
        (None, Some(name)) => name.to_vec(),
        (None, None) => Vec::new(),
    }
}

#[cfg(test)]
#[path = "startup_ut.rs"]
mod tests;
