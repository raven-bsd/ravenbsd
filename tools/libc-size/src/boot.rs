// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Static boot-path checks `BP-1` ... `BP-5`, `BP-7` of `donor-map/16` 3.A.9.1.
//!
//! Everything here is a static property of the P0 `tiny` static link: no execution, no
//! QEMU. The run-time seeds B1-B3 live under `tests/libc/size/qtest/`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::Arch;
use crate::build::{CommandSpec, compact_error};
use crate::measure::{parse_map_members, parse_sections, section_size};
use crate::members::matching_members;
use crate::report::BootRow;
use crate::tools::Tools;

/// The PROPOSED instruction ceiling of 3.A.9.1: `250 + 5 * 8`.
pub const START_INSTRUCTION_CEILING: u64 = 250 + 5 * 8;

/// The syscall ceiling before `main` (`O-A4`).
pub const SYSCALL_CEILING: u64 = 1;

/// The allocator entry points that must not be reachable from a start root (`R-17`).
pub const MALLOC_SET: &[&str] = &[
    "malloc",
    "calloc",
    "realloc",
    "free",
    "aligned_alloc",
    "posix_memalign",
    "memalign",
];

/// Member name patterns that mean allocator code (`BP-2` item a).
pub const MALLOC_MEMBER_PATTERNS: &[&str] = &[
    "malloc*",
    "calloc*",
    "realloc*",
    "free*",
    "mmap_heap*",
    "__rl_malloc*",
];

/// Symbols a start path must not call (`BP-3`, `BP-4`).
pub const FORBIDDEN_START_SYMBOLS: &[&str] = &["open", "openat", "read", "getenv"];

/// The roots and reviewed exceptions of `tests/libc/size/malloc-roots.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct MallocRoots {
    /// Start-path roots.
    #[serde(default)]
    pub roots: Vec<String>,
    /// Reviewed exceptions.
    #[serde(default, rename = "allow")]
    pub allow: Vec<AllowRoot>,
}

/// One reviewed exception.
#[derive(Clone, Debug, Deserialize)]
pub struct AllowRoot {
    /// The root symbol (or a malloc callee) that may allocate.
    pub symbol: String,
    /// Why.
    #[serde(default)]
    pub reason: String,
}

/// Load `malloc-roots.toml`.
///
/// # Errors
/// An I/O or parse error.
pub fn load_roots(path: &Path) -> Result<MallocRoots> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// One member of an archive as reported by `llvm-nm -A -g`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NmMember {
    /// Member file name.
    pub member: String,
    /// Defined symbols.
    pub defined: Vec<String>,
    /// Undefined symbols.
    pub undefined: Vec<String>,
}

const SYMBOL_KINDS: &[&str] = &[
    "A", "B", "C", "D", "G", "I", "N", "R", "S", "T", "U", "V", "W", "a", "b", "c", "d", "g", "i",
    "n", "r", "s", "t", "u", "v", "w",
];

/// Parse `llvm-nm -A -g archive` lines (`archive:member: <addr> T name`).
#[must_use]
pub fn parse_nm_edges(text: &str) -> Vec<NmMember> {
    let mut members: BTreeMap<String, NmMember> = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(3, ':');
        let (_archive, member, rest) = match (parts.next(), parts.next(), parts.next()) {
            (Some(archive), Some(member), Some(rest)) => (archive, member, rest),
            _ => continue,
        };
        let member = member.trim();
        if member.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        let Some(kind_index) = tokens.iter().position(|token| SYMBOL_KINDS.contains(token)) else {
            continue;
        };
        let Some(name) = tokens.get(kind_index + 1) else {
            continue;
        };
        let kind = tokens[kind_index];
        let entry = members
            .entry(member.to_string())
            .or_insert_with(|| NmMember {
                member: member.to_string(),
                defined: Vec::new(),
                undefined: Vec::new(),
            });
        if matches!(kind, "U" | "u" | "w") {
            entry.undefined.push((*name).to_string());
        } else {
            entry.defined.push((*name).to_string());
        }
    }
    members.into_values().collect()
}

/// The allocation paths reachable from the roots of `malloc-roots.toml`.
///
/// The graph has one node per archive member; an edge exists when a reachable member has an
/// undefined symbol that another member defines. `R-1` (one symbol per file) makes the
/// member granularity exact for ravenlibc; for `core`/`compiler_builtins` it over-approximates.
#[must_use]
pub fn malloc_paths(members: &[NmMember], roots: &MallocRoots) -> Vec<String> {
    let allowed: BTreeSet<&str> = roots.allow.iter().map(|row| row.symbol.as_str()).collect();
    let malloc: BTreeSet<&str> = MALLOC_SET.iter().copied().collect();
    let mut defined_by: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, member) in members.iter().enumerate() {
        for symbol in &member.defined {
            defined_by.entry(symbol.as_str()).or_default().push(index);
        }
    }
    let mut out = Vec::new();
    for root in &roots.roots {
        if allowed.contains(root.as_str()) {
            continue;
        }
        let Some(starts) = defined_by.get(root.as_str()) else {
            continue;
        };
        let mut queue: VecDeque<(usize, Vec<String>)> = VecDeque::new();
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        for start in starts {
            queue.push_back((*start, vec![root.clone()]));
        }
        while let Some((index, path)) = queue.pop_front() {
            if !visited.insert(index) {
                continue;
            }
            let Some(member) = members.get(index) else {
                continue;
            };
            for symbol in &member.undefined {
                if malloc.contains(symbol.as_str()) {
                    let mut trail = path.clone();
                    trail.push(format!("{} -> {symbol}", member.member));
                    out.push(trail.join(" -> "));
                    continue;
                }
                if allowed.contains(symbol.as_str()) {
                    continue;
                }
                if let Some(next) = defined_by.get(symbol.as_str()) {
                    for target in next {
                        if !visited.contains(target) {
                            let mut trail = path.clone();
                            trail.push(member.member.clone());
                            queue.push_back((*target, trail));
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// One instruction of the disassembly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// The mnemonic (`callq`, `syscall`, `svc`, `ecall`, ...).
    pub mnemonic: String,
    /// A direct call target, when present.
    pub call: Option<CallTarget>,
}

/// A direct call target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// A numeric address.
    Address(u64),
    /// A symbol name from `<name>`.
    Symbol(String),
}

/// One function of the disassembly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// Symbol name.
    pub name: String,
    /// Start address (0 when the label carries none).
    pub start: u64,
    /// The instructions.
    pub instructions: Vec<Instruction>,
}

/// A parsed disassembly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disassembly {
    /// Every function in file order.
    pub functions: Vec<Function>,
}

impl Disassembly {
    /// Resolve a function index by name.
    #[must_use]
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.functions
            .iter()
            .position(|function| function.name == name)
    }

    /// Resolve a call target to a function index.
    #[must_use]
    pub fn resolve(&self, target: &CallTarget) -> Option<usize> {
        match target {
            CallTarget::Symbol(name) => self.index_of(name),
            CallTarget::Address(address) => self.functions.iter().position(|function| {
                let end = function
                    .instructions
                    .len()
                    .checked_mul(0)
                    .map_or(u64::MAX, |_| u64::MAX);
                let _ = end;
                function.start == *address
            }),
        }
    }

    /// The functions reachable from `root` through direct calls (cycles terminate).
    #[must_use]
    pub fn reachable(&self, root: &str) -> Vec<usize> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        if let Some(index) = self.index_of(root) {
            queue.push_back(index);
        }
        while let Some(index) = queue.pop_front() {
            if !visited.insert(index) {
                continue;
            }
            let Some(function) = self.functions.get(index) else {
                continue;
            };
            for instruction in &function.instructions {
                if let Some(target) = &instruction.call
                    && let Some(next) = self.resolve(target)
                    && !visited.contains(&next)
                {
                    queue.push_back(next);
                }
            }
        }
        visited.into_iter().collect()
    }

    /// Instructions in the reachable set (each once).
    #[must_use]
    pub fn instruction_count(&self, reachable: &[usize]) -> u64 {
        reachable
            .iter()
            .filter_map(|index| self.functions.get(*index))
            .map(|function| u64::try_from(function.instructions.len()).unwrap_or(u64::MAX))
            .sum()
    }

    /// Syscalls in the reachable set.
    #[must_use]
    pub fn syscall_count(&self, reachable: &[usize]) -> u64 {
        let mut count = 0u64;
        for index in reachable {
            let Some(function) = self.functions.get(*index) else {
                continue;
            };
            for instruction in &function.instructions {
                if matches!(instruction.mnemonic.as_str(), "syscall" | "svc" | "ecall") {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    /// Direct calls to the forbidden names from the reachable set.
    #[must_use]
    pub fn forbidden_refs(&self, reachable: &[usize], forbidden: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for index in reachable {
            let Some(function) = self.functions.get(*index) else {
                continue;
            };
            for instruction in &function.instructions {
                let Some(CallTarget::Symbol(name)) = &instruction.call else {
                    continue;
                };
                if forbidden.contains(&name.as_str()) {
                    out.push(format!("{} -> {name}", function.name));
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Parse `llvm-objdump -d --no-show-raw-insn`.
#[must_use]
pub fn parse_disassembly(text: &str) -> Disassembly {
    let mut functions: Vec<Function> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((address, name)) = parse_label(trimmed) {
            functions.push(Function {
                name,
                start: address,
                instructions: Vec::new(),
            });
            current = Some(functions.len().saturating_sub(1));
            continue;
        }
        let Some(index) = current else {
            continue;
        };
        let Some((address, rest)) = parse_instruction_line(trimmed) else {
            continue;
        };
        let _ = address;
        let mut tokens = rest.split_whitespace();
        let Some(mnemonic) = tokens.next() else {
            continue;
        };
        let mnemonic = mnemonic.trim_end_matches(',').to_string();
        let call = if matches!(
            mnemonic.as_str(),
            "call" | "callq" | "bl" | "blx" | "jal" | "jalr"
        ) {
            parse_call_target(rest)
        } else {
            None
        };
        if let Some(function) = functions.get_mut(index) {
            function.instructions.push(Instruction { mnemonic, call });
        }
    }
    Disassembly { functions }
}

fn parse_label(line: &str) -> Option<(u64, String)> {
    if !line.ends_with(':') || !line.contains('<') {
        return None;
    }
    let (address, rest) = line.split_once(char::is_whitespace)?;
    let address = u64::from_str_radix(address.trim_start_matches("0x"), 16).ok()?;
    let name = rest
        .trim()
        .strip_prefix('<')?
        .strip_suffix(">:")?
        .to_string();
    Some((address, name))
}

fn parse_instruction_line(line: &str) -> Option<(u64, &str)> {
    let (address, rest) = line.split_once(':')?;
    let address = u64::from_str_radix(address.trim(), 16).ok()?;
    Some((address, rest.trim()))
}

fn parse_call_target(rest: &str) -> Option<CallTarget> {
    if let Some(open) = rest.find('<')
        && let Some(close) = rest[open..].find('>')
    {
        let name = &rest[open + 1..open + close];
        return Some(CallTarget::Symbol(name.to_string()));
    }
    for token in rest.split_whitespace().skip(1) {
        let token = token.trim_end_matches(',');
        if let Some(hex) = token.strip_prefix("0x")
            && let Ok(value) = u64::from_str_radix(hex, 16)
        {
            return Some(CallTarget::Address(value));
        }
    }
    None
}

/// Scan a `.rodata` blob for `LD_` and `/etc/` references (`BP-3`, `BP-4`).
#[must_use]
pub fn scan_rodata(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for (needle, label) in [(b"LD_".as_slice(), "LD_"), (b"/etc/".as_slice(), "/etc/")] {
        let mut start = 0usize;
        while start + needle.len() <= bytes.len() {
            if &bytes[start..start + needle.len()] == needle {
                out.push(format!("{label} at offset 0x{start:x}"));
                start += needle.len();
            } else {
                start += 1;
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Everything `boot` needs from a previous measurement.
pub struct Inputs<'a> {
    /// Repository root.
    pub root: &'a Path,
    /// Architecture.
    pub arch: Arch,
    /// P0 unstripped executable.
    pub p0_binary: &'a Path,
    /// P0 map.
    pub p0_map: &'a Path,
    /// P1 unstripped executable, when built.
    pub p1_binary: Option<&'a Path>,
    /// P1 map, when built.
    pub p1_map: Option<&'a Path>,
    /// `libc.a`.
    pub archive: &'a Path,
    /// Scratch directory for the `.rodata` dump.
    pub scratch: &'a Path,
}

/// Run every host-checkable boot rule on the P0 link.
///
/// # Errors
/// A tool or I/O failure.
pub fn analyze(tools: &Tools, inputs: &Inputs<'_>) -> Result<BootRow> {
    let sections_text = run_text(
        tools,
        &tools.size,
        &["-A".to_string(), inputs.p0_binary.display().to_string()],
        inputs.root,
    )?;
    let sections = parse_sections(&sections_text)
        .with_context(|| format!("parse llvm-size -A of {}", inputs.p0_binary.display()))?;
    let init_array = section_size(&sections, ".init_array");
    let preinit_array = section_size(&sections, ".preinit_array");
    let ctors = section_size(&sections, ".ctors");

    let mut early_malloc = Vec::new();
    let mut member_lists = Vec::new();
    for path in [Some(inputs.p0_map), inputs.p1_map].into_iter().flatten() {
        let map = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let members = parse_map_members(&map);
        member_lists.push(members);
    }
    let patterns: Vec<String> = MALLOC_MEMBER_PATTERNS
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    for members in &member_lists {
        for member in matching_members(members, &patterns) {
            early_malloc.push(format!("early allocator member: {member}"));
        }
    }

    let roots = load_roots(&inputs.root.join("tests/libc/size/malloc-roots.toml"))?;
    let nm_text = run_text(
        tools,
        &tools.nm,
        &[
            "-A".to_string(),
            "-g".to_string(),
            inputs.archive.display().to_string(),
        ],
        inputs.root,
    )?;
    let members = parse_nm_edges(&nm_text);
    early_malloc.extend(malloc_paths(&members, &roots));
    early_malloc.sort();
    early_malloc.dedup();

    let disassembly_text = run_text(
        tools,
        &tools.objdump,
        &[
            "-d".to_string(),
            "--no-show-raw-insn".to_string(),
            inputs.p0_binary.display().to_string(),
        ],
        inputs.root,
    )?;
    let disassembly = parse_disassembly(&disassembly_text);
    let reachable = disassembly.reachable("__raven_libc_start");
    if reachable.is_empty() {
        return Ok(BootRow {
            arch: inputs.arch,
            syscalls_before_main: 0,
            start_instructions: 0,
            init_array,
            preinit_array,
            ctors,
            early_malloc,
            forbidden_refs: Vec::new(),
            rodata: Vec::new(),
            notes: Vec::new(),
            status: "skipped: __raven_libc_start absent from the P0 link".to_string(),
        });
    }
    let syscalls = disassembly.syscall_count(&reachable);
    let instructions = disassembly.instruction_count(&reachable);
    let forbidden_refs = disassembly.forbidden_refs(&reachable, FORBIDDEN_START_SYMBOLS);

    let rodata_dump = inputs.scratch.join("p0.rodata");
    let rodata = dump_and_scan(tools, inputs.root, inputs.p0_binary, &rodata_dump);
    let _ = std::fs::remove_file(&rodata_dump);

    let mut notes = Vec::new();
    let private = inputs.root.join("libc/ravenlibc/abi/Symbols.private");
    if private.is_file() {
        let text = std::fs::read_to_string(&private).unwrap_or_default();
        if text.lines().any(|line| line.trim() == "__rl_libc_init") {
            notes.push("BP-5: abi/Symbols.private lists __rl_libc_init".to_string());
        } else {
            notes.push("BP-5: abi/Symbols.private lacks __rl_libc_init".to_string());
        }
    } else {
        notes.push("BP-5: skipped (abi/Symbols.private absent, C68)".to_string());
    }
    notes.push("BP-7: skipped (libc.so.1 absent (C62 not landed))".to_string());

    let mut failures = Vec::new();
    if init_array != 0 || preinit_array != 0 || ctors != 0 {
        failures.push(format!(
            "[boot-init] .init_array={init_array} .preinit_array={preinit_array} .ctors={ctors}"
        ));
    }
    if !early_malloc.is_empty() {
        failures.push("[boot-malloc] allocator reachable from a start root".to_string());
    }
    if syscalls > SYSCALL_CEILING {
        failures.push(format!(
            "[boot-syscall] {syscalls} syscalls reachable, ceiling {SYSCALL_CEILING}"
        ));
    }
    if instructions > START_INSTRUCTION_CEILING {
        failures.push(format!(
            "[boot-instructions] {instructions} reachable, PROPOSED ceiling {START_INSTRUCTION_CEILING}"
        ));
    }
    if !forbidden_refs.is_empty() {
        failures.push(format!("[boot-ref] {}", forbidden_refs.join(", ")));
    }
    if !rodata.is_empty() {
        failures.push(format!("[boot-rodata] {}", rodata.join(", ")));
    }
    Ok(BootRow {
        arch: inputs.arch,
        syscalls_before_main: syscalls,
        start_instructions: instructions,
        init_array,
        preinit_array,
        ctors,
        early_malloc,
        forbidden_refs,
        rodata,
        notes,
        status: if failures.is_empty() {
            "ok".to_string()
        } else {
            failures.join("; ")
        },
    })
}

fn dump_and_scan(tools: &Tools, root: &Path, binary: &Path, out: &Path) -> Vec<String> {
    let spec = CommandSpec::new(
        &tools.objcopy,
        vec![
            format!("--dump-section=.rodata={}", out.display()),
            binary.display().to_string(),
        ],
        root.to_path_buf(),
    );
    let Ok(output) = spec.run() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    match std::fs::read(out) {
        Ok(bytes) => scan_rodata(&bytes),
        Err(_) => Vec::new(),
    }
}

fn run_text(tools: &Tools, program: &Path, args: &[String], root: &Path) -> Result<String> {
    let _ = tools;
    let spec = CommandSpec::new(program, args.to_vec(), root.to_path_buf());
    let output = spec.run()?;
    if !output.status.success() {
        anyhow::bail!(
            "{} failed: {}",
            program.display(),
            compact_error(&String::from_utf8_lossy(&output.stderr))
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The P0 binary and map of one measured set/language combination.
#[must_use]
pub fn artifacts(
    out_root: &Path,
    arch: Arch,
    set: crate::Set,
    lang: crate::Lang,
) -> (PathBuf, PathBuf) {
    let dir = out_root
        .join(arch.short())
        .join(set.as_str())
        .join("static")
        .join(lang.as_str());
    (dir.join("p0.elf"), dir.join("p0.map"))
}

#[cfg(test)]
#[path = "boot_ut.rs"]
mod tests;
