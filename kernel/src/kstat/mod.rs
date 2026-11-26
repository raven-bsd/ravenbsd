// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed kernel-statistics record and bounded snapshot model.
//!
//! Illumos defines the kstat contract in `usr/src/uts/common/sys/kstat.h` (at
//! `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`): a `module:instance:name` namespace, a
//! type/unit/value record and a caller-buffer snapshot. This module is the allocation-free Rust
//! model. `crates/raven-kstat` includes this source so the same naming, type and overflow rules
//! run on the host and in the userland tool. The kernel can include the same source when a
//! provider ticket wires the scheduler/IPC/VM/console counters.
//!
//! The first consumer is the existing DTrace per-CPU ring statistics read through the bounded
//! `DTRACE_STATS` syscall (0x94); this ticket adds no kernel syscall.

use core::fmt;

/// Illumos `KSTAT_STRLEN`: 30 significant bytes plus the NUL terminator.
pub const KSTAT_STRLEN: usize = 30;

/// One fixed-size kstat string.
///
/// Names use the stable lowercase `[a-z0-9_.-]` alphabet; string values allow printable ASCII.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text {
    bytes: [u8; KSTAT_STRLEN],
    len: u8,
}

impl Text {
    /// Parse a name component: `[a-z0-9_.-]`, 1..=30 bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyComponent`] for an empty string, [`Error::NameTooLong`] above 30
    /// bytes and [`Error::InvalidCharacter`] for any other byte.
    pub fn parse_name(text: &str) -> Result<Self, Error> {
        Self::parse_with(text, is_name_byte)
    }

    /// Parse a text value: printable ASCII `0x20..=0x7e`, 1..=30 bytes.
    ///
    /// # Errors
    ///
    /// Returns the same bounds errors as [`Self::parse_name`].
    pub fn parse_value(text: &str) -> Result<Self, Error> {
        Self::parse_with(text, is_value_byte)
    }

    fn parse_with(text: &str, allowed: fn(u8) -> bool) -> Result<Self, Error> {
        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return Err(Error::EmptyComponent);
        }
        if bytes.len() > KSTAT_STRLEN {
            return Err(Error::NameTooLong);
        }
        let mut storage = [0u8; KSTAT_STRLEN];
        for (index, byte) in bytes.iter().enumerate() {
            if !allowed(*byte) {
                return Err(Error::InvalidCharacter);
            }
            if let Some(slot) = storage.get_mut(index) {
                *slot = *byte;
            }
        }
        Ok(Self {
            bytes: storage,
            len: bytes.len() as u8,
        })
    }

    /// The stored text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        let length = usize::from(self.len);
        let bytes = self.bytes.get(..length).unwrap_or(&[]);
        core::str::from_utf8(bytes).unwrap_or("")
    }

    /// Stored length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// True when no bytes are stored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Display for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
}

fn is_value_byte(byte: u8) -> bool {
    (0x20..=0x7e).contains(&byte)
}

/// The stable `module:instance:name` kstat name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KstatName {
    module: Text,
    instance: Text,
    name: Text,
}

impl KstatName {
    /// Build a name from validated components.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyComponent`] when any component is empty.
    pub fn new(module: Text, instance: Text, name: Text) -> Result<Self, Error> {
        if module.is_empty() || instance.is_empty() || name.is_empty() {
            return Err(Error::EmptyComponent);
        }
        Ok(Self {
            module,
            instance,
            name,
        })
    }

    /// Parse `module:instance:name`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidName`] when there are not exactly three colon-separated
    /// components, and the component errors from [`Text::parse_name`].
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut parts = text.split(':');
        let module = parts.next().ok_or(Error::InvalidName)?;
        let instance = parts.next().ok_or(Error::InvalidName)?;
        let name = parts.next().ok_or(Error::InvalidName)?;
        if parts.next().is_some() {
            return Err(Error::InvalidName);
        }
        Self::new(
            Text::parse_name(module)?,
            Text::parse_name(instance)?,
            Text::parse_name(name)?,
        )
    }

    /// Provider module.
    #[must_use]
    pub const fn module(&self) -> &Text {
        &self.module
    }

    /// Module instance.
    #[must_use]
    pub const fn instance(&self) -> &Text {
        &self.instance
    }

    /// Statistic name.
    #[must_use]
    pub const fn name(&self) -> &Text {
        &self.name
    }
}

impl fmt::Display for KstatName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.module, self.instance, self.name)
    }
}

/// The kstat data types from the Illumos `KSTAT_DATA_*` contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KstatType {
    /// `KSTAT_DATA_CHAR`.
    Char,
    /// `KSTAT_DATA_INT32`.
    Int32,
    /// `KSTAT_DATA_UINT32`.
    Uint32,
    /// `KSTAT_DATA_INT64`.
    Int64,
    /// `KSTAT_DATA_UINT64`.
    Uint64,
    /// `KSTAT_DATA_STRING`.
    Text,
}

impl KstatType {
    /// Stable wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Char => "char",
            Self::Int32 => "int32",
            Self::Uint32 => "uint32",
            Self::Int64 => "int64",
            Self::Uint64 => "uint64",
            Self::Text => "string",
        }
    }
}

/// The unit of a kstat value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KstatUnit {
    /// A dimensionless event count.
    Count,
    /// Bytes.
    Bytes,
    /// Nanoseconds.
    Nanoseconds,
    /// Clock ticks.
    Ticks,
}

impl KstatUnit {
    /// Stable wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Bytes => "bytes",
            Self::Nanoseconds => "ns",
            Self::Ticks => "ticks",
        }
    }
}

/// One typed kstat value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    /// A single byte.
    Char(u8),
    /// Signed 32-bit value.
    Int32(i32),
    /// Unsigned 32-bit value.
    Uint32(u32),
    /// Signed 64-bit value.
    Int64(i64),
    /// Unsigned 64-bit value.
    Uint64(u64),
    /// A short string.
    Text(Text),
}

impl Value {
    /// The kstat type of this value.
    #[must_use]
    pub const fn kind(self) -> KstatType {
        match self {
            Self::Char(_) => KstatType::Char,
            Self::Int32(_) => KstatType::Int32,
            Self::Uint32(_) => KstatType::Uint32,
            Self::Int64(_) => KstatType::Int64,
            Self::Uint64(_) => KstatType::Uint64,
            Self::Text(_) => KstatType::Text,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Char(value) => write!(formatter, "{value}"),
            Self::Int32(value) => write!(formatter, "{value}"),
            Self::Uint32(value) => write!(formatter, "{value}"),
            Self::Int64(value) => write!(formatter, "{value}"),
            Self::Uint64(value) => write!(formatter, "{value}"),
            Self::Text(value) => formatter.write_str(value.as_str()),
        }
    }
}

/// One typed kstat record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kstat {
    name: KstatName,
    value: Value,
    unit: KstatUnit,
}

impl Kstat {
    /// Build a record from a validated name and a typed value.
    #[must_use]
    pub const fn new(name: KstatName, value: Value, unit: KstatUnit) -> Self {
        Self { name, value, unit }
    }

    /// The stable name.
    #[must_use]
    pub const fn name(&self) -> &KstatName {
        &self.name
    }

    /// The typed value.
    #[must_use]
    pub const fn value(&self) -> Value {
        self.value
    }

    /// The value type.
    #[must_use]
    pub const fn kind(&self) -> KstatType {
        self.value.kind()
    }

    /// The value unit.
    #[must_use]
    pub const fn unit(&self) -> KstatUnit {
        self.unit
    }
}

/// A fixed-capacity snapshot. Reading a snapshot never allocates on the hot path.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot<const N: usize> {
    records: [Option<Kstat>; N],
    len: usize,
}

impl<const N: usize> Snapshot<N> {
    /// An empty snapshot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            records: [None; N],
            len: 0,
        }
    }

    /// Append one record.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Full`] when the caller's buffer is full.
    pub fn push(&mut self, record: Kstat) -> Result<(), Error> {
        if self.len >= N {
            return Err(Error::Full);
        }
        let slot = self.records.get_mut(self.len).ok_or(Error::Full)?;
        *slot = Some(record);
        self.len += 1;
        Ok(())
    }

    /// Number of stored records.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when no records are stored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// One record by position.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Kstat> {
        self.records.get(index).and_then(Option::as_ref)
    }

    /// Iterate in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Kstat> {
        self.records
            .get(..self.len)
            .unwrap_or(&[])
            .iter()
            .filter_map(Option::as_ref)
    }

    /// Drop every record.
    pub fn clear(&mut self) {
        for slot in &mut self.records {
            *slot = None;
        }
        self.len = 0;
    }
}

impl<const N: usize> Default for Snapshot<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// A kstat model or read error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A name component is empty.
    EmptyComponent,
    /// A component is longer than [`KSTAT_STRLEN`].
    NameTooLong,
    /// A component contains a character outside its stable alphabet.
    InvalidCharacter,
    /// The name is not exactly `module:instance:name`.
    InvalidName,
    /// The caller's snapshot buffer is full.
    Full,
    /// An aggregate exceeded `u64`.
    Overflow,
    /// The kernel statistics read failed.
    Kernel,
}

impl Error {
    /// A short, stable description for the CLI.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::EmptyComponent => "empty kstat name component",
            Self::NameTooLong => "kstat name component exceeds 30 bytes",
            Self::InvalidCharacter => "invalid character in kstat name",
            Self::InvalidName => "kstat name must be module:instance:name",
            Self::Full => "kstat snapshot is full",
            Self::Overflow => "kstat aggregate overflow",
            Self::Kernel => "kstat kernel read failed",
        }
    }
}

/// The read side of one kstat provider.
pub trait Provider<const N: usize> {
    /// Fill `out` with the provider's named set.
    ///
    /// # Errors
    ///
    /// Returns a model error or [`Error::Kernel`] when the backing read fails.
    fn snapshot(&self, out: &mut Snapshot<N>) -> Result<(), Error>;
}

/// Sum `values` with checked arithmetic.
///
/// # Errors
///
/// Returns [`Error::Overflow`] instead of wrapping.
pub fn checked_sum<I: IntoIterator<Item = u64>>(values: I) -> Result<u64, Error> {
    let mut total = 0u64;
    for value in values {
        total = total.checked_add(value).ok_or(Error::Overflow)?;
    }
    Ok(total)
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
