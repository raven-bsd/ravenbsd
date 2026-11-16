// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SHA-512-crypt hashing and verification.
//!
//! The output is the standard Modular Crypt Format `$6$<salt>$<digest>` used by `/etc/shadow`.
//! The salt argument is raw bytes; the crate encodes it into the salt field. Verification uses
//! the crate's constant-time comparison.

use alloc::string::{String, ToString};

use sha_crypt::{PasswordHash, PasswordHasher, PasswordVerifier, ShaCrypt};

use crate::errors::AuthError;

include!(concat!(env!("OUT_DIR"), "/root_hash.rs"));

/// The fixed raw salt of the release root hash.
pub const DEFAULT_ROOT_SALT: &[u8] = b"RavenBSD-root1!";

/// Hash a password with SHA-512-crypt and the supplied raw salt.
///
/// # Errors
/// [`AuthError::Hash`] when the salt cannot be represented in the Modular Crypt Format.
pub fn hash_password(password: &str, salt: &[u8]) -> Result<String, AuthError> {
    let hash = ShaCrypt::default()
        .hash_password_with_salt(password.as_bytes(), salt)
        .map_err(|_| AuthError::Hash)?;
    Ok(hash.to_string())
}

/// The build-time default hash of the release `root` password.
#[must_use]
pub fn default_root_hash() -> &'static str {
    DEFAULT_ROOT_HASH
}

/// Verify a password against a `$6$` shadow hash in constant time.
#[must_use]
pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    ShaCrypt::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
#[path = "hash_ut.rs"]
mod tests;
