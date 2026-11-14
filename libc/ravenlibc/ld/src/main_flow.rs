// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The start-up flow (the host-testable part of `_rtld_main`).
//!
//! The real entry glue parses the stack and auxv; this module takes a [`StartCtx`] and executes
//! the steps that are the same on the host and the target.

use raven_libc_elf::{Arch, ObjId, Scope};

use crate::loadobj::{LoadError, Loader, Name};
use crate::mapper::Platform;

/// The start-up inputs.
#[derive(Clone, Copy, Debug)]
pub struct StartCtx<'a> {
    /// The main program path.
    pub main_path: &'a [u8],
    /// The requested main name.
    pub main_name: &'a [u8],
    /// Additional search directories.
    pub library_path: &'a [&'a [u8]],
    /// The architecture.
    pub arch: Arch,
    /// The trusted flag.
    pub trust: bool,
}

/// The result of start-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartResult {
    /// The program entry address.
    pub entry: usize,
    /// The exit function address (zero when none).
    pub exit_proc: usize,
}

/// Run the host-testable start-up steps.
pub fn rtld_main<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier>(
    loader: &mut Loader<P, M, A>,
    ctx: StartCtx<'_>,
) -> Result<StartResult, LoadError> {
    // STEP 1 is the assembly stub (`selfreloc` then `_rtld_main`); the host entry has already
    // run it.
    // STEP 2: the start-up auxv is represented by `StartCtx`; required keys are a C73 item.
    loader.arch = ctx.arch;
    // STEP 3: secure-exec trust and the environment copy. The caller has already parsed env.
    loader.trust = ctx.trust;
    // STEP 4: the loader's own digest and RELRO are performed by the entry glue/C61.
    // STEP 5: direct execution; `StartCtx` names the main path in both modes.
    // STEP 6: digest the main program.
    let main = loader.load_object(ctx.main_path, ctx.main_name)?;
    // STEP 7: preload (only when trusted).
    if ctx.trust {
        for dir in ctx.library_path {
            let _ = loader.add_library_path(dir);
        }
    }
    // STEP 8: the breadth-first `DT_NEEDED` closure.
    loader.scope = Scope::new(main);
    loader.load_needed_objects(main)?;
    // STEP 9: version verification is C52's lookup/version path.
    // STEP 10: `LD_TRACE_LOADED_OBJECTS` is a C64/target tracing item.
    // STEP 11: static TLS offsets are assigned before relocation by the C57 layout.
    loader.assign_tls_offsets()?;
    // STEP 12: non-PLT, RELR and PLT relocations.
    loader.relocate_all()?;
    // STEP 13: the IFUNC pass (inside `relocate_all`, after every non-IFUNC phase).
    // STEP 14: the initial TLS block is a C73/PG-1 item.
    // STEP 15: program variables and `RtldTable` installation are target glue.
    // STEP 16: init order and the init arrays are exposed by `initial_order`.
    // STEP 17: RELRO is enforced at the end of `relocate_all`, after the IFUNC pass.
    let entry = loader
        .object(main)
        .map(|object| object.view.bias)
        .unwrap_or(0);
    Ok(StartResult {
        entry: usize::try_from(entry).unwrap_or(0),
        exit_proc: 0,
    })
}

/// Parse one `LD_LIBRARY_PATH` value into a search list (no allocation).
pub fn split_paths<'a>(value: &'a [u8], out: &'a mut [Name]) -> usize {
    let mut count = 0usize;
    for part in value.split(|byte| *byte == b':') {
        if count >= out.len() {
            break;
        }
        if let Some(name) = Name::new(part) {
            out[count] = name;
            count = count.saturating_add(1);
        }
    }
    count
}

/// The parsed direct-execution command line (`ld-elf.so.1 prog args`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectExec<'a> {
    /// `--list` (the `ldd` mode).
    pub list: bool,
    /// `--library-path <p>`.
    pub library_path: Option<&'a [u8]>,
    /// `--preload <so>`.
    pub preload: Option<&'a [u8]>,
    /// The program path.
    pub program: Option<&'a [u8]>,
    /// The program's first argument index; `0` when none.
    pub program_index: usize,
}

/// Parse the direct-execution command line without allocating.
///
/// # Errors
///
/// `Tainted` when secure-exec refuses direct execution, or `BadDirectArgs` for a missing
/// option value or program.
pub fn parse_direct_exec<'a>(
    args: &'a [&'a [u8]],
    trust: bool,
) -> Result<DirectExec<'a>, LoadError> {
    if !trust {
        return Err(LoadError::Tainted);
    }
    let mut parsed = DirectExec::default();
    let mut index = 0usize;
    let mut after_separator = false;
    while index < args.len() {
        let argument = args[index];
        if !after_separator && argument == b"--" {
            after_separator = true;
            index = index.saturating_add(1);
            continue;
        }
        if !after_separator && argument == b"--list" {
            parsed.list = true;
            index = index.saturating_add(1);
            continue;
        }
        if !after_separator && argument == b"--library-path" {
            index = index.saturating_add(1);
            parsed.library_path = Some(args.get(index).copied().ok_or(LoadError::BadDirectArgs)?);
            index = index.saturating_add(1);
            continue;
        }
        if !after_separator && argument == b"--preload" {
            index = index.saturating_add(1);
            parsed.preload = Some(args.get(index).copied().ok_or(LoadError::BadDirectArgs)?);
            index = index.saturating_add(1);
            continue;
        }
        if !after_separator && argument.first() == Some(&b'-') {
            return Err(LoadError::BadDirectArgs);
        }
        parsed.program = Some(argument);
        parsed.program_index = index;
        break;
    }
    if parsed.program.is_none() {
        return Err(LoadError::BadDirectArgs);
    }
    Ok(parsed)
}

/// Write one `ldd` trace line: `\t<soname> => <path> (0x<base>)`.
///
/// Returns the line length, or `None` when `out` is too small.
pub fn write_trace_line(soname: &[u8], path: &[u8], base: u64, out: &mut [u8]) -> Option<usize> {
    let mut writer = SliceWriter { bytes: out, len: 0 };
    writer.push(b"\t")?;
    writer.push(soname)?;
    writer.push(b" => ")?;
    writer.push(path)?;
    writer.push(b" (0x")?;
    writer.push_hex(base)?;
    writer.push(b")\n")?;
    Some(writer.len)
}

/// Write the `ldd` not-found line: `\t<name> => not found`.
pub fn write_trace_not_found(soname: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut writer = SliceWriter { bytes: out, len: 0 };
    writer.push(b"\t")?;
    writer.push(soname)?;
    writer.push(b" => not found\n")?;
    Some(writer.len)
}

struct SliceWriter<'a> {
    bytes: &'a mut [u8],
    len: usize,
}

impl SliceWriter<'_> {
    fn push(&mut self, text: &[u8]) -> Option<()> {
        let end = self.len.checked_add(text.len())?;
        let target = self.bytes.get_mut(self.len..end)?;
        target.copy_from_slice(text);
        self.len = end;
        Some(())
    }

    fn push_hex(&mut self, mut value: u64) -> Option<()> {
        let mut digits = [0u8; 16];
        let mut at = digits.len();
        loop {
            at = at.checked_sub(1)?;
            let nibble = u8::try_from(value & 0xf).ok()?;
            digits[at] = if nibble < 10 {
                b'0'.wrapping_add(nibble)
            } else {
                b'a'.wrapping_add(nibble.wrapping_sub(10))
            };
            value >>= 4;
            if value == 0 {
                break;
            }
        }
        self.push(digits.get(at..)?)
    }
}

/// Parse `LD_DEBUG`; secure-exec ignores it.
#[must_use]
pub fn parse_ld_debug(value: &[u8], trust: bool) -> raven_libc_elf::debug::DebugWords {
    if trust {
        raven_libc_elf::debug::DebugWords::parse(value)
    } else {
        raven_libc_elf::debug::DebugWords::default()
    }
}

/// The main object id after start-up.
#[must_use]
pub fn main_id() -> ObjId {
    ObjId(0)
}

#[cfg(test)]
#[path = "main_flow_ut.rs"]
mod tests;
