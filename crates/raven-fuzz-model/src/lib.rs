// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side fuzz targets for the syscall-facing parsers (S20).
//!
//! The targets drive the *real* host-testable code the kernel uses at its security boundaries:
//! `raven_abi` syscall numbering and errno mapping, `raven_usercopy` range validation,
//! `raven_handles` handle table, `raven_elf` parsing and load planning, and `raven_ipc` packet
//! decoding. Each target is a plain function over a byte slice, so the same code runs from the
//! `raven-fuzz` tool, from the committed-corpus replay tests, and from a Miri or libFuzzer driver
//! without changes.
//!
//! The targets assert the invariants the kernel relies on (no panic, no accepted out-of-range
//! slice, no rights widening, no overlapping load plan). An assertion failure is a finding; the
//! report asks for a stable regression test and a follow-up ticket.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, plan_load};
use raven_handles::HandleTable;
use raven_ipc::protocol::{self, Opcode, PACKET_SIZE, SchemePacket};
use raven_usercopy::{UserPtr, UserSlice, validate_range};

/// The user address range the usercopy target checks against (`raven-abi` layout constants).
const USER_START: u64 = 0x1000;
/// Upper bound of the user range; matches the kernel's `USER_END` on x86-64.
const USER_END: u64 = 0x0000_7FFF_FFFF_F000;
/// Largest errno the ABI maps.
const MAX_ERRNO: isize = 4095;

/// One fuzz target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Syscall number lookup and errno mapping.
    Syscall,
    /// User-copy address/length arithmetic.
    Usercopy,
    /// Handle table create/dup/get/close sequences.
    Handles,
    /// ELF header, program headers and load planning.
    Elf,
    /// S11 scheme packet decoding and re-encoding.
    Ipc,
}

impl Target {
    /// Every target, in report order.
    pub const ALL: [Target; 5] = [
        Target::Syscall,
        Target::Usercopy,
        Target::Handles,
        Target::Elf,
        Target::Ipc,
    ];

    /// Stable name used by the tool, the corpus directories and the report.
    pub fn name(self) -> &'static str {
        match self {
            Self::Syscall => "syscall",
            Self::Usercopy => "usercopy",
            Self::Handles => "handles",
            Self::Elf => "elf",
            Self::Ipc => "ipc",
        }
    }

    /// Parse a target name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|target| target.name() == name)
    }
}

/// Run one target over one input. The target panics only when an invariant is violated.
pub fn run(target: Target, input: &[u8]) {
    match target {
        Target::Syscall => syscall(input),
        Target::Usercopy => usercopy(input),
        Target::Handles => handles(input),
        Target::Elf => elf(input),
        Target::Ipc => ipc(input),
    }
}

/// A deterministic seed input for one target.
pub fn seed(target: Target) -> Vec<u8> {
    match target {
        Target::Syscall => {
            let mut bytes = Vec::new();
            for number in [0x01u64, 0x07, 0x32, 0xff, 0x1_0000] {
                bytes.extend_from_slice(&number.to_le_bytes());
            }
            bytes
        }
        Target::Usercopy => {
            let mut bytes = Vec::new();
            for (address, length) in [
                (USER_START, 0x1000u64),
                (USER_END - 8, 8),
                (USER_END, 0),
                (0, 0x1000),
                (u64::MAX, 1),
            ] {
                bytes.extend_from_slice(&address.to_le_bytes());
                bytes.extend_from_slice(&length.to_le_bytes());
            }
            bytes
        }
        Target::Handles => vec![
            0, 0x03, 0, 0x01, 1, 0x03, 2, 0x00, 0x00, 0x00, 0x01, 3, 0x00,
        ],
        Target::Elf => minimal_elf(),
        Target::Ipc => {
            let mut packet = SchemePacket::request(Opcode::Open, 7, 9);
            let _ = packet.set_path(b"/bin/cat");
            let mut bytes = vec![0u8; PACKET_SIZE];
            if protocol::encode(&packet, &mut bytes).is_err() {
                return Vec::new();
            }
            bytes
        }
    }
}

/// A minimal, valid ELF64 image with one 4 KiB `PT_LOAD` segment and its payload.
fn minimal_elf() -> Vec<u8> {
    const HEADER: usize = 64;
    const PHDR: usize = 56;
    const PAGE: usize = 4096;
    let mut image = vec![0u8; PAGE * 2];
    image[..4].copy_from_slice(b"\x7fELF");
    image[4] = 2; // ELFCLASS64
    image[5] = 1; // little endian
    image[6] = 1; // version
    image[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    image[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    image[20..24].copy_from_slice(&1u32.to_le_bytes()); // version
    image[24..32].copy_from_slice(&0x40_0000u64.to_le_bytes()); // entry
    image[32..40].copy_from_slice(&(HEADER as u64).to_le_bytes()); // phoff
    image[52..54].copy_from_slice(&(HEADER as u16).to_le_bytes()); // ehsize
    image[54..56].copy_from_slice(&(PHDR as u16).to_le_bytes()); // phentsize
    image[56..58].copy_from_slice(&1u16.to_le_bytes()); // phnum
    let program = HEADER;
    image[program..program + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    image[program + 4..program + 8].copy_from_slice(&5u32.to_le_bytes()); // R+X
    image[program + 8..program + 16].copy_from_slice(&(PAGE as u64).to_le_bytes()); // offset
    image[program + 16..program + 24].copy_from_slice(&0x40_0000u64.to_le_bytes()); // vaddr
    image[program + 24..program + 32].copy_from_slice(&0x40_0000u64.to_le_bytes()); // paddr
    image[program + 32..program + 40].copy_from_slice(&(PAGE as u64).to_le_bytes()); // filesz
    image[program + 40..program + 48].copy_from_slice(&(PAGE as u64).to_le_bytes()); // memsz
    image[program + 48..program + 56].copy_from_slice(&(PAGE as u64).to_le_bytes()); // align
    image[PAGE..PAGE + 4].copy_from_slice(&0x9090_9090u32.to_le_bytes());
    image
}

// --- targets -----------------------------------------------------------------------------------

/// Syscall numbering and errno mapping.
///
/// Invariants: every number `from_usize` accepts maps back to the same value; scanning the whole
/// 0..=0x40 window never panics; a negative return inside `-MAX_ERRNO..0` maps to an errno whose
/// `to_ret` reproduces it; values outside that window never become errnos.
fn syscall(input: &[u8]) {
    for chunk in input.chunks(8) {
        let mut raw = [0u8; 8];
        raw[..chunk.len()].copy_from_slice(chunk);
        let number = u64::from_le_bytes(raw);
        if let Some(found) = Syscall::from_usize(number as usize) {
            assert_eq!(found as usize, number as usize, "syscall repr mismatch");
        }
    }
    let mut number = 0usize;
    while number <= 0x40 {
        if let Some(found) = Syscall::from_usize(number) {
            assert_eq!(found as usize, number);
        }
        number += 1;
    }
    for chunk in input.chunks(4) {
        let mut raw = [0u8; 4];
        raw[..chunk.len()].copy_from_slice(chunk);
        let value = i32::from_le_bytes(raw) as isize;
        let mapped = Errno::from_ret(value);
        if (-MAX_ERRNO..0).contains(&value) {
            let errno = mapped.expect_err("a negative return must map to an errno");
            assert_eq!(errno.to_ret(), value, "errno round trip");
            assert!(errno.0 != 0, "errno zero is not an error");
        } else if value == 0 {
            assert_eq!(mapped, Ok(0));
        } else if value < -MAX_ERRNO {
            assert!(mapped.is_ok(), "values below -4095 are not errnos");
        }
    }
}

/// User-copy address/length arithmetic.
///
/// Invariants: validation never panics, an accepted range lies inside the range it was checked
/// against, and `validate_range` and `UserSlice::new_in` agree.
fn usercopy(input: &[u8]) {
    for chunk in input.chunks(16) {
        let mut raw = [0u8; 16];
        raw[..chunk.len()].copy_from_slice(chunk);
        let address = u64::from_le_bytes(raw[..8].try_into().unwrap_or([0u8; 8]));
        let length = u64::from_le_bytes(raw[8..].try_into().unwrap_or([0u8; 8]));
        for (start, end) in [
            (USER_START, USER_END),
            (0, USER_END),
            (USER_START, USER_START + 0x1_0000),
        ] {
            let validated = validate_range(address, length, start, end);
            let slice = UserSlice::new_in(address, length, start, end);
            assert_eq!(validated.is_ok(), slice.is_ok(), "two validators disagree");
            if validated.is_ok() {
                assert!(address >= start && address <= end, "range start");
                if let Some(last) = address.checked_add(length) {
                    assert!(last <= end, "range end");
                } else {
                    panic!("accepted a wrapping range");
                }
            }
        }
        let _ = UserSlice::new(address, length);
        let _ = UserPtr::<u64>::new(address);
        let _ = UserPtr::<u8>::new_in(address, USER_START, USER_END);
    }
}

/// Handle table create/dup/get/close sequences.
///
/// Invariants: `len` equals the number of live handles, a dup never widens rights, a closed handle
/// stops resolving, and closing twice or addressing a stale handle is an error rather than a panic.
fn handles(input: &[u8]) {
    let mut table: HandleTable<u32> = HandleTable::new();
    let mut live: Vec<(Handle, Rights)> = Vec::new();
    let mut index = 0;
    while index < input.len() {
        let op = input[index] % 4;
        index += 1;
        match op {
            0 => {
                let rights = Rights(u32::from(*input.get(index).unwrap_or(&0)) & 0x7f);
                index += 1;
                let value = table.len() as u32;
                if let Ok(handle) = table.insert(value, rights) {
                    live.push((handle, rights));
                }
            }
            1 => {
                let handle = read_handle(input, &mut index);
                let needed = Rights(u32::from(*input.get(index).unwrap_or(&0)) & 0x7f);
                index += 1;
                match table.get(handle, needed) {
                    Ok(_) => {
                        let entry = live
                            .iter()
                            .find(|(item, _)| *item == handle)
                            .expect("a live handle must be tracked");
                        assert!(
                            entry.1.intersect(needed) == needed,
                            "get returned a handle without the requested rights"
                        );
                    }
                    Err(error) => assert!(
                        error == Errno::EACCES || error == Errno::EBADF,
                        "unexpected get error {error:?}"
                    ),
                }
            }
            2 => {
                let handle = read_handle(input, &mut index);
                let wanted = Rights(u32::from(*input.get(index).unwrap_or(&0)) & 0x7f);
                index += 1;
                if let Ok(duplicate) = table.dup(handle, wanted) {
                    let source = live
                        .iter()
                        .find(|(item, _)| *item == handle)
                        .expect("dup from a live handle");
                    let granted = table.rights(duplicate).expect("the duplicate is live");
                    assert!(
                        granted.intersect(source.1) == granted,
                        "dup widened the source rights"
                    );
                    live.push((duplicate, granted));
                }
            }
            _ => {
                let handle = read_handle(input, &mut index);
                if table.close(handle).is_ok() {
                    live.retain(|(item, _)| *item != handle);
                    assert!(
                        table.get(handle, Rights::NONE).is_err(),
                        "a closed handle still resolves"
                    );
                }
            }
        }
        assert_eq!(table.len(), live.len(), "live handle bookkeeping");
    }
}

fn read_handle(input: &[u8], index: &mut usize) -> Handle {
    let mut raw = [0u8; 4];
    for slot in raw.iter_mut() {
        *slot = *input.get(*index).unwrap_or(&0);
        *index += 1;
    }
    Handle(u32::from_le_bytes(raw))
}

/// ELF parsing and load planning.
///
/// Invariants: parsing never panics; a successful plan is page-aligned, inside the user range,
/// sorted and non-overlapping; the plan does not depend on the parse accepting garbage.
///
/// The plan is lazy and yields one step per page, so a valid image with a huge `memsz` (the user
/// range allows terabytes) would take unbounded time here; the target inspects at most
/// [`MAX_STEPS`] steps, which is enough to check the invariants and keeps the sweep honest.
fn elf(input: &[u8]) {
    const MAX_STEPS: usize = 4096;
    let machine = Machine::X86_64;
    let range = USER_START..USER_END;
    let Ok(parsed) = Elf::parse(input, machine, range.clone()) else {
        return;
    };
    for page in [4096u64, 8192, 65536] {
        let Ok(steps) = plan_load(&parsed, page, LoadMode::Strict) else {
            continue;
        };
        let mut previous_end = 0u64;
        for step in steps.take(MAX_STEPS) {
            assert!(step.len > 0, "empty step");
            assert_eq!(step.vaddr % page, 0, "step is not page-aligned");
            assert_eq!(step.len % page, 0, "step length is not page-aligned");
            assert!(
                step.vaddr >= range.start,
                "step below the user range: {:#x}",
                step.vaddr
            );
            assert!(
                step.vaddr.saturating_add(step.len) <= range.end,
                "step above the user range"
            );
            assert!(previous_end <= step.vaddr, "overlapping steps");
            previous_end = step.vaddr + step.len;
            assert!(step.prot & !(prot::READ | prot::WRITE | prot::EXEC) as u8 == 0);
            let _ = map::pack(usize::from(step.prot), 0);
        }
    }
}

/// S11 scheme packet decoding.
///
/// Invariants: decoding arbitrary bytes never panics; a decoded packet validates or fails cleanly;
/// re-encoding a valid packet decodes back to the same opcode and path.
fn ipc(input: &[u8]) {
    let Ok(packet) = protocol::decode(input) else {
        return;
    };
    let validated = packet.validate(false);
    let mut bytes = vec![0u8; PACKET_SIZE];
    if protocol::encode(&packet, &mut bytes).is_err() {
        assert!(validated.is_err(), "an unencodable packet validated");
        return;
    }
    let Ok(round_trip) = protocol::decode(&bytes) else {
        panic!("an encoded packet does not decode");
    };
    assert_eq!(round_trip.opcode, packet.opcode);
    assert_eq!(round_trip.request_id, packet.request_id);
    assert_eq!(round_trip.path_bytes(), packet.path_bytes());
    assert_eq!(round_trip.validate(false), validated);
}

// --- deterministic PRNG ------------------------------------------------------------------------

/// xorshift64*: two lines, no dependencies, identical on every platform and Rust version.
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator seeded with `seed` (never zero).
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.state = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A value in `0..bound` (`bound` must be non-zero).
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }

    /// Fill `out`.
    pub fn fill(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let value = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&value[..chunk.len()]);
        }
    }
}

/// Run one target for `iterations` inputs derived from `seed`; returns the execution count.
///
/// Starting every run from [`seed`] and mutating with a seeded PRNG keeps a finding reproducible
/// from the printed seed and iteration number.
pub fn fuzz(target: Target, seed: u64, iterations: u64) -> u64 {
    let base = self::seed(target);
    let mut rng = Rng::new(seed);
    let mut executed = 0;
    for _ in 0..iterations {
        let input = mutate(&base, &mut rng);
        run(target, &input);
        executed += 1;
    }
    executed
}

/// Mutate `base` with up to four byte flips, truncations or duplications.
pub fn mutate(base: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut input = base.to_vec();
    if input.is_empty() {
        input.push(0);
        return input;
    }
    let edits = 1 + rng.below(4);
    for _ in 0..edits {
        match rng.below(4) {
            0 => {
                let index = rng.below(input.len() as u64) as usize;
                input[index] ^= 1 << rng.below(8);
            }
            1 => {
                let cut = rng.below(input.len() as u64) as usize;
                input.truncate(cut.max(1));
            }
            2 => {
                let index = rng.below(input.len() as u64) as usize;
                input.insert(index, rng.next_u64() as u8);
            }
            _ => {
                let index = rng.below(input.len() as u64) as usize;
                input[index] = rng.next_u64() as u8;
            }
        }
    }
    input
}

// --- corpus helpers ----------------------------------------------------------------------------

/// Why a hex corpus file could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HexError {
    /// An odd number of hex digits.
    OddLength,
    /// A character outside `0-9a-fA-F` and whitespace.
    BadDigit,
}

/// Decode a hex corpus file: hex digit pairs, with whitespace and `#` comments ignored.
pub fn from_hex(text: &str) -> Result<Vec<u8>, HexError> {
    let mut nibbles = Vec::new();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(index) => &line[..index],
            None => line,
        };
        for character in line.chars() {
            if character.is_whitespace() {
                continue;
            }
            let value = character.to_digit(16).ok_or(HexError::BadDigit)?;
            nibbles.push(value as u8);
        }
    }
    if !nibbles.len().is_multiple_of(2) {
        return Err(HexError::OddLength);
    }
    Ok(nibbles
        .chunks_exact(2)
        .map(|pair| pair[0] << 4 | pair[1])
        .collect())
}

/// Encode bytes as lower-case hex, one line per 32 bytes, for a corpus file.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 && index.is_multiple_of(32) {
            text.push('\n');
        }
        text.push_str(&format!("{byte:02x}"));
    }
    text.push('\n');
    text
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
