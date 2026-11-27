// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FMA baseline fixture: inject bad-MMIO/ECC ereports, diagnose, retire/degrade and prove the
//! malformed path is rejected without a panic.

#![no_std]
#![no_main]

use raven_fmd::{Action, Fmd};
use raven_proto_fm::{EREPORT_LEN, Ereport, EreportRing, FmError, Severity};
use raven_rt::entry;

const POLICY: &str = include_str!("../../policy.conf");

entry!(main);

fn run() -> Result<(), ()> {
    let mmio = Ereport::new(
        "ereport.io.mmio.bad",
        Severity::Major,
        "pci0:1:0",
        &[0xde, 0xad, 0xbe, 0xef],
        100,
    )
    .map_err(|_| ())?;
    let ecc =
        Ereport::new("ereport.io.ecc", Severity::Critical, "dimm0", &[7], 101).map_err(|_| ())?;

    let mut bytes = [0u8; EREPORT_LEN];
    mmio.encode(&mut bytes).map_err(|_| ())?;
    let decoded = Ereport::decode(&bytes).map_err(|_| ())?;
    raven_rt::println!(
        "fma: ereport class={} resource={} severity=major",
        decoded.class.as_str().unwrap_or("?"),
        decoded.resource.as_str().unwrap_or("?")
    );

    let mut fmd = Fmd::from_policy(POLICY).map_err(|_| ())?;
    raven_rt::println!(
        "fma: policy entries={} source=policy.conf",
        fmd.policy_count()
    );
    let diagnosis = fmd.diagnose(&decoded, 100);
    if diagnosis.action != Action::Retire || !diagnosis.accepted {
        return Err(());
    }
    raven_rt::println!(
        "fma: diagnosis case={} action=retire resource={}",
        diagnosis.case,
        diagnosis.resource.as_str().unwrap_or("?")
    );

    let ecc_bytes = {
        let mut buffer = [0u8; EREPORT_LEN];
        ecc.encode(&mut buffer).map_err(|_| ())?;
        buffer
    };
    let ecc_decoded = Ereport::decode(&ecc_bytes).map_err(|_| ())?;
    let diagnosis = fmd.diagnose(&ecc_decoded, 101);
    if diagnosis.action != Action::Degrade {
        return Err(());
    }
    raven_rt::println!(
        "fma: diagnosis case={} action=degrade resource={}",
        diagnosis.case,
        diagnosis.resource.as_str().unwrap_or("?")
    );

    let mut ring = EreportRing::<4>::new();
    ring.push(&decoded).map_err(|_| ())?;
    ring.push(&ecc_decoded).map_err(|_| ())?;
    ring.push(&decoded).map_err(|_| ())?;
    ring.push(&ecc_decoded).map_err(|_| ())?;
    if ring.push(&decoded) != Err(FmError::Full) {
        return Err(());
    }
    for _ in 0..2 {
        ring.pop().map_err(|_| ())?;
    }
    raven_rt::println!("fma: ring full=rejected depth={} restarted=yes", ring.len());

    let mut rejects = 0;
    let mutations: [fn(&mut [u8; EREPORT_LEN]); 5] = [
        |b| b[0] ^= 0xff,
        |b| b[2] = 9,
        |b| b[3] = 9,
        |b| b[68] = 65,
        |b| b[143] = 1,
    ];
    for mutation in mutations {
        let mut malformed = bytes;
        mutation(&mut malformed);
        if Ereport::decode(&malformed).is_err() {
            rejects += 1;
        }
    }
    if Ereport::decode(&bytes[..EREPORT_LEN - 1]).is_err() {
        rejects += 1;
    }
    if rejects != 6 {
        return Err(());
    }
    let _ = fmd.reject(200);
    raven_rt::println!("fma: malformed rejected count={rejects}");

    let mut restarted = Fmd::from_policy(POLICY).map_err(|_| ())?;
    let replay = restarted.diagnose(&decoded, 300);
    if replay.action != Action::Retire || replay.case != 1 {
        return Err(());
    }
    raven_rt::println!(
        "fma: restart action=retire case={} audits={}",
        replay.case,
        restarted.audit_count()
    );
    raven_rt::println!("fma: done");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(()) => {
            raven_rt::println!("fma: fail");
            1
        }
    }
}
