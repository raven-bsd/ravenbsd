// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The firmware image chunk plan.
//!
//! The real maximum load-window write size is UNVERIFIED; [`MAX_CHUNK`] is a model bound, not a
//! hardware fact.

use core::fmt;

/// Largest chunk the model accepts.
pub const MAX_CHUNK: u32 = 4096;
/// Largest chunk count (the progress/log field is 16 bits).
pub const MAX_CHUNKS: u32 = 0xFFFF;

/// Why a plan is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The image has no bytes.
    EmptyBlob,
    /// The chunk size is zero.
    EmptyChunk,
    /// The chunk size exceeds [`MAX_CHUNK`].
    ChunkTooLarge {
        /// Requested chunk size.
        length: u32,
    },
    /// More chunks than the model tracks.
    TooManyChunks {
        /// Required count.
        chunks: u32,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBlob => f.write_str("firmware image is empty"),
            Self::EmptyChunk => f.write_str("chunk size is zero"),
            Self::ChunkTooLarge { length } => {
                write!(f, "chunk size {length} exceeds {MAX_CHUNK}")
            }
            Self::TooManyChunks { chunks } => {
                write!(f, "{chunks} chunks exceed the model limit")
            }
        }
    }
}

impl core::error::Error for PlanError {}

/// One load window chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Zero-based chunk index.
    pub index: u32,
    /// Byte offset in the image.
    pub offset: u32,
    /// Byte count (the last chunk may be short).
    pub length: u32,
}

/// A validated chunk plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    blob_len: u32,
    chunk_len: u32,
    chunks: u32,
}

impl Plan {
    /// Build a plan.
    ///
    /// # Errors
    ///
    /// Returns [`PlanError`] for an empty image, a zero/oversized chunk or too many chunks.
    pub const fn new(blob_len: u32, chunk_len: u32) -> Result<Self, PlanError> {
        if blob_len == 0 {
            return Err(PlanError::EmptyBlob);
        }
        if chunk_len == 0 {
            return Err(PlanError::EmptyChunk);
        }
        if chunk_len > MAX_CHUNK {
            return Err(PlanError::ChunkTooLarge { length: chunk_len });
        }
        let chunks = (blob_len as u64).div_ceil(chunk_len as u64);
        if chunks > MAX_CHUNKS as u64 {
            return Err(PlanError::TooManyChunks {
                chunks: chunks as u32,
            });
        }
        Ok(Self {
            blob_len,
            chunk_len,
            chunks: chunks as u32,
        })
    }

    /// Image length in bytes.
    #[must_use]
    pub const fn blob_len(&self) -> u32 {
        self.blob_len
    }

    /// Maximum chunk size.
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
        let remaining = self.blob_len - offset;
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

#[cfg(test)]
#[path = "plan_ut.rs"]
mod tests;
