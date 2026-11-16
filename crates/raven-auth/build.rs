// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate the default root hash from the release password at crate build time.
//!
//! The image builder consumes [`raven_auth::DEFAULT_ROOT_HASH`] or
//! [`raven_auth::default_root_shadow`]; no plaintext password file is installed in the image.

use std::env;
use std::fs;
use std::path::PathBuf;

use sha_crypt::{PasswordHasher, ShaCrypt};

/// The fixed salt of the release root hash. Changing it changes the default hash.
const ROOT_SALT: &[u8] = b"RavenBSD-root1!";
/// The release root password.
const ROOT_PASSWORD: &[u8] = b"raven";

fn main() {
    let hash = ShaCrypt::default()
        .hash_password_with_salt(ROOT_PASSWORD, ROOT_SALT)
        .expect("the fixed root salt is valid")
        .to_string();
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("root_hash.rs");
    fs::write(
        output,
        format!("/// The build-time hash of the release `root` password.\npub const DEFAULT_ROOT_HASH: &str = {hash:?};\n"),
    )
    .expect("write the generated root hash");
    println!("cargo:rerun-if-changed=build.rs");
}
