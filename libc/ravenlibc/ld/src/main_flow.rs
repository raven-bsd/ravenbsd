// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The start-up flow (the host-testable part of `_rtld_main`).
//!
//! The real entry glue parses the stack and auxv; this module takes a [`StartCtx`] and executes
//! the steps that are the same on the host and the target.

use raven_libc_elf::{Arch, ObjId, ObjView};

use crate::loadobj::{LoadError, LoadedObject, Loader, MAX_DEPS, Name};
use crate::mapper::{Mapped, Platform};

/// The start-up inputs.
#[derive(Clone, Copy, Debug)]
pub struct StartCtx<'a> {
    /// The main program path.
    pub main_path: &'a [u8],
    /// The requested main name.
    pub main_name: &'a [u8],
    /// Additional search directories.
    pub library_path: &'a [&'a [u8]],
    /// Direct-execution arguments after the loader name; empty for the normal path.
    pub args: &'a [&'a [u8]],
    /// `LD_PRELOAD` names.
    pub preload: &'a [&'a [u8]],
    /// `LD_DEBUG` value.
    pub debug: &'a [u8],
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
    loader.debug = parse_ld_debug(ctx.debug, ctx.trust);
    let mut main_path = ctx.main_path;
    let mut main_name = ctx.main_name;
    let mut trace = false;
    if !ctx.args.is_empty() {
        let parsed = parse_direct_exec(ctx.args, ctx.trust)?;
        trace = parsed.list;
        if let Some(program) = parsed.program {
            main_path = program;
            main_name = program;
        }
        if let Some(paths) = parsed.library_path {
            for dir in paths.split(|byte| *byte == b':') {
                if !dir.is_empty() {
                    let _ = loader.add_library_path(dir);
                }
            }
        }
        if let Some(preload) = parsed.preload {
            let _ = loader.add_preload(preload);
        }
    }
    // STEP 4: the loader's own digest and RELRO are performed by the entry glue/C61.
    // STEP 5: direct execution has already selected the main path above.
    if ctx.trust {
        for dir in ctx.library_path {
            let _ = loader.add_library_path(dir);
        }
        for preload in ctx.preload {
            let _ = loader.add_preload(preload);
        }
    }
    // STEP 6: digest the main program.
    let main = match loader.load_object(main_path, main_name) {
        Ok(id) => id,
        Err(LoadError::NotDynamic | LoadError::Digest(raven_libc_elf::DigestError::NotDynamic))
            if trace =>
        {
            loader.platform.debug_write(b"not a dynamic executable\n");
            return Err(LoadError::NotDynamic);
        }
        Err(error) => return Err(error),
    };
    loader.debug_write(raven_libc_elf::debug::DebugWords::FILES, b"load main\n");
    // STEP 7: preload the trusted objects before the main dependency closure.
    loader.load_preload_objects()?;
    // STEP 8: the breadth-first `DT_NEEDED` closure; keep the preload roots in the scope.
    loader.scope.main = main;
    loader.load_needed_objects(main)?;
    // STEP 9: version verification is C52's lookup/version path.
    // STEP 10: `LD_TRACE_LOADED_OBJECTS`: print and return without init.
    if trace {
        trace_loaded(loader);
        return Ok(StartResult {
            entry: 0,
            exit_proc: 0,
        });
    }
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

/// The load bias of an already-mapped PIE, derived from its `PT_PHDR` record.
///
/// `phdr` is the absolute address the kernel placed in `AT_PHDR`; the program header's
/// link-time `p_vaddr` is compared with it. `None` means the table has no `PT_PHDR`.
#[must_use]
pub fn main_bias(mem: &dyn raven_libc_elf::MemView, phdr: u64, phnum: u16) -> Option<u64> {
    let headers = raven_libc_elf::elf::read_phdrs(mem, phdr, phnum).ok()?;
    let entry = headers
        .iter()
        .find(|header| header.p_type == raven_libc_elf::consts::PT_PHDR)?;
    phdr.checked_sub(entry.p_vaddr)
}

/// Register a pre-mapped main program (the normal interpreter case) in the object table.
///
/// The kernel or spawner already mapped the executable, so this only digests the view and
/// records the absolute program headers; the entry point comes from `AT_ENTRY`.
///
/// # Errors
///
/// A missing `PT_PHDR`, a malformed dynamic section or a full object table.
pub fn register_main<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier>(
    loader: &mut Loader<P, M, A>,
    phdr: u64,
    phnum: u16,
    name: &[u8],
) -> Result<ObjId, LoadError> {
    let id = register_object(loader, phdr, phnum, name)?;
    loader.scope.main = id;
    Ok(id)
}

/// Register a pre-mapped library (K139 libc) without making it the main program.
///
/// The spawner maps the library from the initfs zero-copy and passes its `PT_PHDR`; this records
/// the same `LoadedObject` shape as a `DT_NEEDED` load, so `by_name` satisfies the dependency
/// without a file-server copy.
pub fn register_premapped<
    P: Platform,
    M: raven_libc_elf::MemView,
    A: raven_libc_elf::RelocApplier,
>(
    loader: &mut Loader<P, M, A>,
    phdr: u64,
    phnum: u16,
    name: &[u8],
) -> Result<ObjId, LoadError> {
    register_object(loader, phdr, phnum, name)
}

fn register_object<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier>(
    loader: &mut Loader<P, M, A>,
    phdr: u64,
    phnum: u16,
    name: &[u8],
) -> Result<ObjId, LoadError> {
    let bias = main_bias(&loader.mem, phdr, phnum)
        .ok_or(LoadError::Elf(raven_libc_elf::ElfError::BadEntry))?;
    let link_phdr = phdr
        .checked_sub(bias)
        .ok_or(LoadError::Elf(raven_libc_elf::ElfError::BadEntry))?;
    let view = ObjView::digest(&loader.mem, link_phdr, phnum, bias).map_err(LoadError::Digest)?;
    if view.gnu_stack_exec {
        return Err(LoadError::ExecutableStack);
    }
    let end = view
        .segments
        .iter()
        .map(|segment| segment.vaddr.saturating_add(segment.memsz))
        .max()
        .unwrap_or(bias);
    let mut object = LoadedObject {
        rpath: view.rpath,
        runpath: view.runpath,
        view,
        mapped: Mapped {
            base: bias,
            span: end.saturating_sub(bias),
            bias,
        },
        names: [Name::empty(); crate::loadobj::MAX_NAMES],
        name_count: 0,
        deps: [ObjId(u32::MAX); MAX_DEPS],
        dep_count: 0,
        refcount: 0,
        nodelete: false,
        tls_offset: 0,
        tls_size: 0,
        tls_initialized: false,
        relocated: false,
        ifunc_done: false,
        fini_done: false,
        dl_open: false,
    };
    // `dl_iterate_phdr` reports the main program with an empty `dlpi_name`; the path is an
    // alias used by diagnostics.
    object.add_name(b"");
    object.add_name(name);
    let id = loader.insert(object).ok_or(LoadError::TooManyObjects)?;
    Ok(id)
}

/// Print the loaded object list in the `ldd` shape through the platform debug console.
fn trace_loaded<P: Platform, M: raven_libc_elf::MemView, A: raven_libc_elf::RelocApplier>(
    loader: &mut Loader<P, M, A>,
) {
    let ids = loader.iterate_ids();
    for id in ids.into_iter().flatten() {
        let Some(object) = loader.object(id) else {
            continue;
        };
        let name = object
            .names
            .iter()
            .take(object.name_count)
            .next()
            .map(|name| name.as_bytes())
            .unwrap_or(b"");
        let path = object
            .names
            .iter()
            .take(object.name_count)
            .nth(1)
            .map(|name| name.as_bytes())
            .unwrap_or(name);
        let mut line = [0u8; 512];
        if let Some(len) = write_trace_line(name, path, object.mapped.base, &mut line)
            && let Some(bytes) = line.get(..len)
        {
            loader.platform.debug_write(bytes);
        }
    }
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
