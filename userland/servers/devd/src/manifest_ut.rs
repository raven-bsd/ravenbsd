// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the versioned driver manifest ABI.

use alloc::string::ToString;
use alloc::vec;

use raven_proto_dev::parse_drivers;

use super::*;

fn manifest_text(extra: &str) -> String {
    alloc::format!(
        "[[driver]]\nname = \"example\"\ncompatible = \"raven,example\"\nexec = \"/sbin/example\"\nargs = [\"bar=0\", \"resources=dma+irq\"]\n{extra}"
    )
}

#[test]
fn strips_abi_fields_and_keeps_the_driver_table() {
    let text = manifest_text(
        "abi = 1\nversion = 7\nops_version = 1\ncapabilities = [\"mmio\", \"irq\", \"dma\"]\n",
    );
    let manifest = parse(&text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    assert_eq!(drivers.len(), 1);
    assert_eq!(drivers[0].name, "example");
    assert_eq!(drivers[0].args, ["bar=0", "resources=dma+irq"]);
    assert_eq!(manifest.abi.len(), 1);
    let abi = &manifest.abi[0];
    assert_eq!(abi.name, "example");
    assert_eq!(abi.version, 7);
    assert_eq!(abi.ops_version, 1);
    assert_eq!(
        abi.capabilities,
        Some(vec![Capability::Mmio, Capability::Irq, Capability::Dma])
    );
    validate(&drivers, &manifest.abi).expect("validate");
}

#[test]
fn legacy_manifest_defaults_to_the_current_abi() {
    let text = "[[driver]]\nname = \"legacy\"\nvendor = 0x1234\ndevice = 0x5678\nexec = \"/sbin/legacy\"\n";
    let manifest = parse(text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    assert!(manifest.abi.is_empty());
    validate(&drivers, &manifest.abi).expect("legacy manifest validates");
}

#[test]
fn rejects_an_unsupported_manifest_abi() {
    let text = manifest_text(
        "abi = 2\nversion = 1\nops_version = 1\ncapabilities = [\"mmio\", \"dma\", \"irq\"]\n",
    );
    let manifest = parse(&text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    let error = validate(&drivers, &manifest.abi).expect_err("mismatch must fail");
    assert_eq!(
        error,
        ManifestError::UnsupportedAbi {
            name: "example".to_string(),
            found: 2,
            supported: MANIFEST_ABI_VERSION,
        }
    );
}

#[test]
fn rejects_an_unsupported_ops_version() {
    let text = manifest_text(
        "abi = 1\nversion = 1\nops_version = 2\ncapabilities = [\"mmio\", \"dma\", \"irq\"]\n",
    );
    let manifest = parse(&text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    let error = validate(&drivers, &manifest.abi).expect_err("mismatch must fail");
    assert_eq!(
        error,
        ManifestError::UnsupportedOps {
            name: "example".to_string(),
            found: 2,
            supported: OPS_ABI_VERSION,
        }
    );
}

#[test]
fn rejects_an_unknown_capability() {
    let text = manifest_text(
        "abi = 1\nversion = 1\nops_version = 1\ncapabilities = [\"mmio\", \"clock\"]\n",
    );
    let error = parse(&text).expect_err("unknown capability must fail");
    assert_eq!(
        error,
        ManifestError::UnknownCapability {
            capability: "clock".to_string(),
        }
    );
}

#[test]
fn rejects_a_requested_capability_that_is_not_declared() {
    let text = manifest_text(
        "abi = 1\nversion = 1\nops_version = 1\ncapabilities = [\"mmio\", \"irq\"]\n",
    );
    let manifest = parse(&text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    let error = validate(&drivers, &manifest.abi).expect_err("dma must be declared");
    assert_eq!(
        error,
        ManifestError::MissingCapability {
            name: "example".to_string(),
            capability: "dma",
        }
    );
}

#[test]
fn rejects_a_duplicate_metadata_key() {
    let text = manifest_text(
        "abi = 1\nabi = 1\nversion = 1\nops_version = 1\ncapabilities = [\"mmio\", \"dma\", \"irq\"]\n",
    );
    let error = parse(&text).expect_err("duplicate key must fail");
    assert_eq!(
        error,
        ManifestError::DuplicateKey {
            key: "abi",
            line: 7,
        }
    );
}

#[test]
fn rejects_metadata_without_a_driver_name() {
    let text = "[[driver]]\ncompatible = \"raven,nameless\"\nexec = \"/sbin/nameless\"\nabi = 1\n";
    let error = parse(text).expect_err("nameless metadata must fail");
    assert_eq!(error, ManifestError::MissingName { line: 4 });
}

#[test]
fn rejects_metadata_for_a_driver_that_does_not_exist() {
    let text = "[[driver]]\nname = \"ghost\"\ncompatible = \"raven,ghost\"\nexec = \"/sbin/ghost\"\nabi = 1\n";
    let manifest = parse(text).expect("parse");
    let drivers = parse_drivers(
        "[[driver]]\nname = \"real\"\ncompatible = \"raven,real\"\nexec = \"/sbin/real\"\n",
    )
    .expect("driver table");
    let error = validate(&drivers, &manifest.abi).expect_err("unknown driver must fail");
    assert_eq!(
        error,
        ManifestError::UnknownDriver {
            name: "ghost".to_string(),
        }
    );
}

#[test]
fn rejects_a_zero_driver_version() {
    let text = manifest_text(
        "abi = 1\nversion = 0\nops_version = 1\ncapabilities = [\"mmio\", \"dma\", \"irq\"]\n",
    );
    let manifest = parse(&text).expect("parse");
    let drivers = parse_drivers(&manifest.drivers).expect("driver table");
    let error = validate(&drivers, &manifest.abi).expect_err("zero version must fail");
    assert_eq!(
        error,
        ManifestError::InvalidVersion {
            name: "example".to_string(),
        }
    );
}

#[test]
fn parses_the_owned_platform_manifest() {
    let text = include_str!("../drivers.toml");
    let manifest = parse(text).expect("platform manifest");
    let drivers = parse_drivers(&manifest.drivers).expect("platform driver table");
    assert_eq!(drivers.len(), 4);
    validate(&drivers, &manifest.abi).expect("platform ABI validates");
    let genet = manifest
        .abi
        .iter()
        .find(|entry| entry.name == "genet")
        .expect("genet entry");
    assert_eq!(genet.version, 1);
    assert_eq!(
        genet.capabilities,
        Some(vec![Capability::Mmio, Capability::Dma, Capability::Irq])
    );
}
