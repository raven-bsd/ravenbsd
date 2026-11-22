// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Management-tree resolver for generated POSIX data.
//!
//! `RAVEN_MGMT_ROOT` when set, otherwise `../ravenbsd-prjmgmt` relative to the product
//! checkout. The product `docs/posix` tree is frozen; a missing management tree is a hard
//! error naming `RAVEN_MGMT_ROOT`.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// Resolve the management root.
pub fn root(product_root: &Path) -> Result<PathBuf> {
    let raw = std::env::var("RAVEN_MGMT_ROOT")
        .ok()
        .filter(|value| !value.is_empty());
    let path = raw.map(PathBuf::from).unwrap_or_else(|| {
        product_root
            .parent()
            .unwrap_or(product_root)
            .join("ravenbsd-prjmgmt")
    });
    if !path.is_dir() {
        bail!(
            "RAVEN_MGMT_ROOT: management root {} is not a directory; set RAVEN_MGMT_ROOT to the \
             ravenbsd-prjmgmt checkout (no product docs fallback)",
            path.display()
        );
    }
    Ok(path)
}

/// Resolve the management `architecture/product-data/posix` directory.
pub fn posix_dir(product_root: &Path) -> Result<PathBuf> {
    Ok(root(product_root)?.join("architecture/product-data/posix"))
}
