// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded FMA ereport schema and a no-allocation kernel-to-userland ring.
//!
//! The schema is first-party and fixed-width. The producer writes into the caller's buffer; the
//! ring never allocates. Illumos FMA was read for the contract only (CDDL, no code copied).
//!
//! This file carries no crate-level attributes so the kernel's `fm` module can include it with
//! `#[path]` and share the exact schema without a dependency edge.

/// Wire magic (`FM`).
pub const MAGIC: u16 = 0x464d;
/// Wire version.
pub const VERSION: u8 = 1;
/// Maximum class string length.
pub const CLASS_LEN: usize = 32;
/// Maximum resource string length.
pub const RESOURCE_LEN: usize = 32;
/// Maximum payload length.
pub const PAYLOAD_LEN: usize = 64;
/// Encoded ereport length, including reserved bytes.
pub const EREPORT_LEN: usize = 144;

/// One ereport severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Recoverable/minor.
    Minor,
    /// Major fault.
    Major,
    /// Critical fault.
    Critical,
}

impl Severity {
    fn as_u8(self) -> u8 {
        match self {
            Self::Minor => 0,
            Self::Major => 1,
            Self::Critical => 2,
        }
    }

    fn from_u8(value: u8) -> Result<Self, FmError> {
        match value {
            0 => Ok(Self::Minor),
            1 => Ok(Self::Major),
            2 => Ok(Self::Critical),
            _ => Err(FmError::BadSeverity),
        }
    }
}

/// A bounded ASCII string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Str32 {
    bytes: [u8; CLASS_LEN],
    len: u8,
}

impl Str32 {
    /// Build a string of at most 31 printable ASCII bytes.
    ///
    /// The wire field is 32 bytes and keeps a terminating NUL, so a full 32-byte string cannot be
    /// represented.
    ///
    /// # Errors
    ///
    /// Returns [`FmError::BadString`] for empty, over-long or non-printable input.
    pub fn new(text: &str) -> Result<Self, FmError> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() >= CLASS_LEN {
            return Err(FmError::BadString);
        }
        if !bytes.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
            return Err(FmError::BadString);
        }
        let mut value = Self::default();
        value.bytes[..bytes.len()].copy_from_slice(bytes);
        value.len = u8::try_from(bytes.len()).map_err(|_| FmError::BadString)?;
        Ok(value)
    }

    /// The string bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// The string as text, or `None` when it is not valid ASCII.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }
}

/// A decoded ereport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ereport {
    /// Event class, for example `ereport.io.mmio.bad`.
    pub class: Str32,
    /// Severity.
    pub severity: Severity,
    /// Affected resource.
    pub resource: Str32,
    /// Bounded payload bytes.
    pub payload: [u8; PAYLOAD_LEN],
    /// Number of valid payload bytes.
    pub payload_len: u8,
    /// Producer timestamp in milliseconds.
    pub timestamp: u64,
}

impl Ereport {
    /// Build an ereport with a bounded payload.
    ///
    /// # Errors
    ///
    /// Returns [`FmError::BadPayload`] when the payload is larger than [`PAYLOAD_LEN`].
    pub fn new(
        class: &str,
        severity: Severity,
        resource: &str,
        payload: &[u8],
        timestamp: u64,
    ) -> Result<Self, FmError> {
        if payload.len() > PAYLOAD_LEN {
            return Err(FmError::BadPayload);
        }
        let mut event = Self {
            class: Str32::new(class)?,
            severity,
            resource: Str32::new(resource)?,
            payload: [0; PAYLOAD_LEN],
            payload_len: u8::try_from(payload.len()).map_err(|_| FmError::BadPayload)?,
            timestamp,
        };
        event.payload[..payload.len()].copy_from_slice(payload);
        Ok(event)
    }

    /// Encode into exactly [`EREPORT_LEN`] bytes.
    ///
    /// # Errors
    ///
    /// Returns [`FmError::BadPayload`] when the payload length is out of range.
    pub fn encode(&self, out: &mut [u8; EREPORT_LEN]) -> Result<(), FmError> {
        if usize::from(self.payload_len) > PAYLOAD_LEN {
            return Err(FmError::BadPayload);
        }
        out.fill(0);
        out[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        out[2] = VERSION;
        out[3] = self.severity.as_u8();
        out[4..4 + CLASS_LEN].copy_from_slice(&self.class.bytes);
        out[36..36 + RESOURCE_LEN].copy_from_slice(&self.resource.bytes);
        out[68] = self.payload_len;
        out[69..69 + PAYLOAD_LEN].copy_from_slice(&self.payload);
        out[133..141].copy_from_slice(&self.timestamp.to_le_bytes());
        Ok(())
    }

    /// Decode and validate exactly [`EREPORT_LEN`] bytes.
    ///
    /// # Errors
    ///
    /// Returns [`FmError`] for a short buffer, bad magic/version/severity, a malformed string,
    /// a payload length above [`PAYLOAD_LEN`], or non-zero reserved bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, FmError> {
        if bytes.len() != EREPORT_LEN {
            return Err(FmError::BadLength);
        }
        if u16::from_le_bytes([bytes[0], bytes[1]]) != MAGIC {
            return Err(FmError::BadMagic);
        }
        if bytes[2] != VERSION {
            return Err(FmError::BadVersion);
        }
        let severity = Severity::from_u8(bytes[3])?;
        if bytes[141..144].iter().any(|byte| *byte != 0) {
            return Err(FmError::BadReserved);
        }
        let class = decode_str(&bytes[4..4 + CLASS_LEN])?;
        let resource = decode_str(&bytes[36..36 + RESOURCE_LEN])?;
        let payload_len = bytes[68];
        if usize::from(payload_len) > PAYLOAD_LEN {
            return Err(FmError::BadPayload);
        }
        let mut payload = [0; PAYLOAD_LEN];
        payload.copy_from_slice(&bytes[69..69 + PAYLOAD_LEN]);
        let timestamp =
            u64::from_le_bytes(bytes[133..141].try_into().map_err(|_| FmError::BadLength)?);
        Ok(Self {
            class,
            severity,
            resource,
            payload,
            payload_len,
            timestamp,
        })
    }

    /// The valid payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.payload_len)]
    }
}

fn decode_str(bytes: &[u8]) -> Result<Str32, FmError> {
    let Some(first_zero) = bytes.iter().position(|byte| *byte == 0) else {
        return Err(FmError::BadString);
    };
    if first_zero == 0 || bytes[first_zero..].iter().any(|byte| *byte != 0) {
        return Err(FmError::BadString);
    }
    let text = core::str::from_utf8(&bytes[..first_zero]).map_err(|_| FmError::BadString)?;
    Str32::new(text)
}

/// A bounded ereport queue error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FmError {
    /// Buffer is not exactly [`EREPORT_LEN`].
    BadLength,
    /// Magic does not match.
    BadMagic,
    /// Version is not [`VERSION`].
    BadVersion,
    /// Severity is unknown.
    BadSeverity,
    /// Class or resource string is malformed.
    BadString,
    /// Payload length is invalid.
    BadPayload,
    /// Reserved bytes are non-zero.
    BadReserved,
    /// The ring is full.
    Full,
    /// The ring is empty.
    Empty,
}

/// A fixed-capacity producer/consumer ring for encoded ereports.
#[derive(Clone, Debug)]
pub struct EreportRing<const N: usize> {
    slots: [[u8; EREPORT_LEN]; N],
    head: usize,
    tail: usize,
    count: usize,
}

impl<const N: usize> Default for EreportRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> EreportRing<N> {
    /// Create an empty ring.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [[0; EREPORT_LEN]; N],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    /// Number of queued ereports.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Whether the ring is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Encode and enqueue one ereport without allocating.
    ///
    /// # Errors
    ///
    /// Returns [`FmError::Full`] when the ring is full or the encoded event is invalid.
    pub fn push(&mut self, event: &Ereport) -> Result<(), FmError> {
        if N == 0 || self.count == N {
            return Err(FmError::Full);
        }
        let mut encoded = [0u8; EREPORT_LEN];
        event.encode(&mut encoded)?;
        let slot = self.slots.get_mut(self.head).ok_or(FmError::Full)?;
        slot.copy_from_slice(&encoded);
        self.head = (self.head + 1) % N;
        self.count += 1;
        Ok(())
    }

    /// Dequeue and validate one ereport.
    ///
    /// # Errors
    ///
    /// Returns [`FmError::Empty`] when the ring is empty, or the decode error for a corrupt slot.
    pub fn pop(&mut self) -> Result<Ereport, FmError> {
        if self.count == 0 {
            return Err(FmError::Empty);
        }
        let slot = *self.slots.get(self.tail).ok_or(FmError::Empty)?;
        self.tail = (self.tail + 1) % N;
        self.count -= 1;
        Ereport::decode(&slot)
    }
}
