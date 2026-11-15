// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic chunking of the firmware image.
//!
//! The plan is pure arithmetic: an image length, a chunk length and the resulting chunk list. The
//! chunk length is a model bound, not a hardware claim; the bench determines the real maximum
//! message size.

use core::fmt;

/// Largest chunk the model accepts (64 KiB).
pub const MAX_CHUNK_LEN: u32 = 0x1_0000;
/// Largest chunk count whose index still fits the 16-bit sequence field.
pub const MAX_CHUNKS: u32 = 0xFFFF;

/// Why a transfer plan is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The image has no bytes.
    EmptyImage,
    /// The chunk length is zero.
    EmptyChunk,
    /// The chunk length exceeds [`MAX_CHUNK_LEN`].
    ChunkTooLarge {
        /// Requested chunk length.
        length: u32,
    },
    /// The image needs more chunks than a 16-bit sequence can address.
    TooManyChunks {
        /// Required chunk count.
        chunks: u32,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => f.write_str("firmware image is empty"),
            Self::EmptyChunk => f.write_str("chunk length is zero"),
            Self::ChunkTooLarge { length } => {
                write!(f, "chunk length {length} exceeds {MAX_CHUNK_LEN}")
            }
            Self::TooManyChunks { chunks } => {
                write!(f, "{chunks} chunks exceed the 16-bit sequence space")
            }
        }
    }
}

impl core::error::Error for PlanError {}

/// One message in the transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Zero-based chunk index, also used as the sequence number.
    pub index: u32,
    /// Byte offset in the image.
    pub offset: u32,
    /// Byte count; the last chunk may be shorter than the plan's chunk length.
    pub length: u32,
}

/// The validated chunk list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferPlan {
    image_len: u32,
    chunk_len: u32,
    chunks: u32,
}

impl TransferPlan {
    /// Build a plan.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError`] for an empty image, a zero or oversized chunk, or more chunks than
    /// the 16-bit sequence field can address.
    pub const fn new(image_len: u32, chunk_len: u32) -> Result<Self, PlanError> {
        if image_len == 0 {
            return Err(PlanError::EmptyImage);
        }
        if chunk_len == 0 {
            return Err(PlanError::EmptyChunk);
        }
        if chunk_len > MAX_CHUNK_LEN {
            return Err(PlanError::ChunkTooLarge { length: chunk_len });
        }
        let chunks = (image_len as u64).div_ceil(chunk_len as u64) as u32;
        if chunks > MAX_CHUNKS {
            return Err(PlanError::TooManyChunks { chunks });
        }
        Ok(Self {
            image_len,
            chunk_len,
            chunks,
        })
    }

    /// Image length in bytes.
    #[must_use]
    pub const fn image_len(&self) -> u32 {
        self.image_len
    }

    /// Maximum chunk length.
    #[must_use]
    pub const fn chunk_len(&self) -> u32 {
        self.chunk_len
    }

    /// Number of chunks.
    #[must_use]
    pub const fn chunks(&self) -> u32 {
        self.chunks
    }

    /// The chunk at `index`, or `None` past the end.
    #[must_use]
    pub const fn chunk(&self, index: u32) -> Option<Chunk> {
        if index >= self.chunks {
            return None;
        }
        let offset = index * self.chunk_len;
        let remaining = self.image_len - offset;
        let length = if remaining < self.chunk_len {
            remaining
        } else {
            self.chunk_len
        };
        Some(Chunk {
            index,
            offset,
            length,
        })
    }
}

/// Why a transfer transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// The response names a chunk other than the one in flight.
    OutOfOrder {
        /// Chunk the host expected.
        expected: u32,
        /// Chunk the response named.
        got: u32,
    },
    /// The transfer already completed.
    Complete,
    /// The chunk used up its retry budget.
    RetriesExhausted {
        /// Chunk index.
        index: u32,
        /// Attempts made, including the failed one.
        attempts: u16,
    },
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfOrder { expected, got } => {
                write!(f, "chunk {got} answered while {expected} was in flight")
            }
            Self::Complete => f.write_str("the transfer is already complete"),
            Self::RetriesExhausted { index, attempts } => {
                write!(f, "chunk {index} failed {attempts} time(s)")
            }
        }
    }
}

impl core::error::Error for TransferError {}

/// The in-flight transfer state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    plan: TransferPlan,
    next: u32,
    attempts: u16,
}

impl Transfer {
    /// Start at chunk zero.
    #[must_use]
    pub const fn new(plan: TransferPlan) -> Self {
        Self {
            plan,
            next: 0,
            attempts: 0,
        }
    }

    /// The plan being executed.
    #[must_use]
    pub const fn plan(&self) -> TransferPlan {
        self.plan
    }

    /// Index of the chunk in flight, or `chunks` when complete.
    #[must_use]
    pub const fn next_index(&self) -> u32 {
        self.next
    }

    /// Failed attempts at the current chunk.
    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }

    /// The chunk in flight, or `None` when complete.
    #[must_use]
    pub const fn current(&self) -> Option<Chunk> {
        self.plan.chunk(self.next)
    }

    /// True when every chunk was acknowledged.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.next >= self.plan.chunks()
    }

    /// Accept an acknowledgement for `index` and return the next chunk.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::OutOfOrder`] for a different chunk and
    /// [`TransferError::Complete`] after the last chunk.
    pub fn acked(&mut self, index: u32) -> Result<Option<Chunk>, TransferError> {
        if index != self.next {
            return Err(TransferError::OutOfOrder {
                expected: self.next,
                got: index,
            });
        }
        if self.is_complete() {
            return Err(TransferError::Complete);
        }
        self.next = self.next.saturating_add(1);
        self.attempts = 0;
        Ok(self.plan.chunk(self.next))
    }

    /// Record a failed attempt at `index` and return the chunk to send again.
    ///
    /// # Errors
    ///
    /// Returns [`TransferError::OutOfOrder`] for a different chunk,
    /// [`TransferError::Complete`] when the transfer is complete, and
    /// [`TransferError::RetriesExhausted`] when `limit` attempts were already made.
    pub fn retry(&mut self, index: u32, limit: u16) -> Result<Chunk, TransferError> {
        if index != self.next {
            return Err(TransferError::OutOfOrder {
                expected: self.next,
                got: index,
            });
        }
        if self.is_complete() {
            return Err(TransferError::Complete);
        }
        self.attempts = self.attempts.saturating_add(1);
        if self.attempts > limit {
            return Err(TransferError::RetriesExhausted {
                index,
                attempts: self.attempts,
            });
        }
        self.plan.chunk(self.next).ok_or(TransferError::Complete)
    }
}

#[cfg(test)]
#[path = "transfer_ut.rs"]
mod tests;
