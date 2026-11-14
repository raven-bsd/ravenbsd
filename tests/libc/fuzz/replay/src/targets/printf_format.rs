// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 1: `printf`/`wprintf` (C43).
//!
//! The body runs the narrow and wide `snprintf` cores, the counting and dry-run paths, and the
//! three `%n` policies over a hostile `FmtEnv`.

use raven_libc_fmt::glue::{vsnprintf_core, vswprintf_core};
use raven_libc_fmt::{CountingSink, FmtOptions, NoFloat, PctN, vformat, vformat_dry};

use crate::env::FuzzEnv;
use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn policy(bits: u8) -> PctN {
    match bits & 3 {
        0 => PctN::Store,
        1 => PctN::Abort,
        _ => PctN::Reject,
    }
}

/// Run target 1.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let flags = cursor.u8();
    let wide = flags & 4 != 0;
    let cap = match (flags >> 3) & 3 {
        0 => 0usize,
        1 => 1,
        2 => 7,
        _ => 64,
    };
    let cap = if flags & 0x20 != 0 { 4096 } else { cap };
    let env = FuzzEnv::from_cursor(&mut cursor);
    let format = cursor.take(512);
    let options = FmtOptions {
        pct_n: policy(flags),
        ..FmtOptions::default()
    };

    let mut counting = CountingSink::<u8>::new();
    let mut counting_env = env.clone();
    let counted = vformat(&mut counting, format, &mut counting_env, &options, &NoFloat);
    let total = counting.count();

    if wide {
        let wide_format: Vec<u32> = format.iter().map(|byte| u32::from(*byte)).collect();
        let mut out = vec![0xaa_u32; cap.max(1)];
        let mut core_env = env.clone();
        if let Ok(length) = vswprintf_core(
            &mut out,
            cap,
            &wide_format,
            &mut core_env,
            &options,
            &NoFloat,
        ) && cap > 0
        {
            assert!(length >= 0, "P-printf-2: a negative wide length");
            let at = (length as usize).min(cap.saturating_sub(1));
            assert_eq!(
                out.get(at).copied(),
                Some(0),
                "P-printf-2: the wide buffer is not NUL-terminated"
            );
        }
        let mut dry_env = env.clone();
        let _ = vformat_dry(&wide_format, &mut dry_env);
    } else {
        let mut out = vec![0xaa_u8; cap.max(1)];
        let mut core_env = env.clone();
        if let Ok(length) = vsnprintf_core(&mut out, cap, format, &mut core_env, &options, &NoFloat)
        {
            if cap > 0 {
                assert!(length >= 0, "P-printf-2: a negative length");
                let at = (length as usize).min(cap.saturating_sub(1));
                assert_eq!(
                    out.get(at).copied(),
                    Some(0),
                    "P-printf-2: the narrow buffer is not NUL-terminated"
                );
                assert!(
                    at < cap,
                    "P-printf-2: the terminator lies outside the buffer"
                );
            }
            assert!(
                u64::try_from(length).ok() == Some(total),
                "P-printf-3: the returned length differs from the counting sink"
            );
        }
        let mut dry_env = env.clone();
        let _ = vformat_dry(format, &mut dry_env);
    }

    let mut store_env = FuzzEnv::from_cursor(&mut Cursor::new(&[0u8; 512]));
    let store_result = vformat(
        &mut CountingSink::<u8>::new(),
        b"%n",
        &mut store_env,
        &FmtOptions {
            pct_n: PctN::Store,
            ..FmtOptions::default()
        },
        &NoFloat,
    );
    assert!(
        store_result.is_ok() && store_env.stores().len() == 1,
        "P-printf-4: PctN::Store did not store exactly once"
    );
    let mut abort_env = FuzzEnv::from_cursor(&mut Cursor::new(&[0u8; 512]));
    let abort_result = vformat(
        &mut CountingSink::<u8>::new(),
        b"%n",
        &mut abort_env,
        &FmtOptions {
            pct_n: PctN::Abort,
            ..FmtOptions::default()
        },
        &NoFloat,
    );
    assert!(
        abort_result == Err(raven_libc_fmt::FmtErr::PctNAbort),
        "P-printf-4: PctN::Abort did not return PctNAbort"
    );
    let mut reject_env = FuzzEnv::from_cursor(&mut Cursor::new(&[0u8; 512]));
    let reject_result = vformat(
        &mut CountingSink::<u8>::new(),
        b"%n",
        &mut reject_env,
        &FmtOptions {
            pct_n: PctN::Reject,
            ..FmtOptions::default()
        },
        &NoFloat,
    );
    assert!(
        reject_result == Err(raven_libc_fmt::FmtErr::Invalid) && reject_env.stores().is_empty(),
        "P-printf-4: PctN::Reject stored a count"
    );

    assert!(
        env.out_of_arena() == 0,
        "P-printf-5: the environment recorded an out-of-arena request"
    );
    assert!(
        env.stores().len() <= 64 && env.dropped_stores() == 0,
        "P-printf-5: the store log overflowed"
    );
    alloc_probe::note(hash(&[u64::from(flags), total, counted.is_ok() as u64]));
}

#[cfg(test)]
#[path = "printf_format_ut.rs"]
mod tests;
