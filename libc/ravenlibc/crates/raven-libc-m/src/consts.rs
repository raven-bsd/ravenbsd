// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Floating-point constants: exception flags, rounding modes, classification values.
//!
//! The `FE_*` numbers are per architecture and come from the FreeBSD `fenv.h` headers
//! (`lib/msun/{x86,aarch64,riscv}/fenv.h`); the `FP_*` classes and `MATH_ERREXCEPT` come from
//! `lib/msun/src/math.h`. No shared code hard-codes one architecture's values (T-B13).

/// `FP_ILOGB0`: the `ilogb(0)` result (`math.h:45`).
pub const FP_ILOGB0: i32 = i32::MIN;
/// `FP_ILOGBNAN`: the `ilogb(NaN)` result (`math.h:46`).
pub const FP_ILOGBNAN: i32 = i32::MAX;

/// `MATH_ERRNO` (`math.h:60`).
pub const MATH_ERRNO: i32 = 1;
/// `MATH_ERREXCEPT` (`math.h:61`); Raven's `math_errhandling` is only this bit (4.9).
pub const MATH_ERREXCEPT: i32 = 2;

/// `FP_INFINITE` (`math.h:67`).
pub const FP_INFINITE: i32 = 0x01;
/// `FP_NAN` (`math.h:68`).
pub const FP_NAN: i32 = 0x02;
/// `FP_NORMAL` (`math.h:69`).
pub const FP_NORMAL: i32 = 0x04;
/// `FP_SUBNORMAL` (`math.h:70`).
pub const FP_SUBNORMAL: i32 = 0x08;
/// `FP_ZERO` (`math.h:71`).
pub const FP_ZERO: i32 = 0x10;

/// The value of `math_errhandling`: exceptions only, no `errno` (4.9).
pub const fn math_errhandling() -> i32 {
    MATH_ERREXCEPT
}

#[cfg(any(target_arch = "x86_64", target_arch = "x86", test))]
pub use x86_64::*;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

#[cfg(target_arch = "riscv64")]
pub use riscv64::*;

#[cfg(any(target_arch = "x86_64", target_arch = "x86", test))]
mod x86_64 {
    /// Invalid operation.
    pub const FE_INVALID: u32 = 0x01;
    /// Denormal operand (x86 extension).
    pub const FE_DENORMAL: u32 = 0x02;
    /// Division by zero.
    pub const FE_DIVBYZERO: u32 = 0x04;
    /// Overflow.
    pub const FE_OVERFLOW: u32 = 0x08;
    /// Underflow.
    pub const FE_UNDERFLOW: u32 = 0x10;
    /// Inexact result.
    pub const FE_INEXACT: u32 = 0x20;
    /// Every exception flag.
    pub const FE_ALL_EXCEPT: u32 =
        FE_DIVBYZERO | FE_DENORMAL | FE_INEXACT | FE_INVALID | FE_OVERFLOW | FE_UNDERFLOW;
    /// Round to nearest, ties to even.
    pub const FE_TONEAREST: u32 = 0x0000;
    /// Round toward negative infinity.
    pub const FE_DOWNWARD: u32 = 0x0400;
    /// Round toward positive infinity.
    pub const FE_UPWARD: u32 = 0x0800;
    /// Round toward zero.
    pub const FE_TOWARDZERO: u32 = 0x0c00;
}

#[cfg(target_arch = "aarch64")]
mod aarch64 {
    /// Invalid operation.
    pub const FE_INVALID: u32 = 0x0000_0001;
    /// Division by zero.
    pub const FE_DIVBYZERO: u32 = 0x0000_0002;
    /// Overflow.
    pub const FE_OVERFLOW: u32 = 0x0000_0004;
    /// Underflow.
    pub const FE_UNDERFLOW: u32 = 0x0000_0008;
    /// Inexact result.
    pub const FE_INEXACT: u32 = 0x0000_0010;
    /// Every exception flag.
    pub const FE_ALL_EXCEPT: u32 =
        FE_DIVBYZERO | FE_INEXACT | FE_INVALID | FE_OVERFLOW | FE_UNDERFLOW;
    /// Round to nearest, ties to even.
    pub const FE_TONEAREST: u32 = 0x0;
    /// Round toward positive infinity.
    pub const FE_UPWARD: u32 = 0x1;
    /// Round toward negative infinity.
    pub const FE_DOWNWARD: u32 = 0x2;
    /// Round toward zero.
    pub const FE_TOWARDZERO: u32 = 0x3;
}

#[cfg(target_arch = "riscv64")]
mod riscv64 {
    /// Invalid operation.
    pub const FE_INVALID: u32 = 0x0010;
    /// Division by zero.
    pub const FE_DIVBYZERO: u32 = 0x0008;
    /// Overflow.
    pub const FE_OVERFLOW: u32 = 0x0004;
    /// Underflow.
    pub const FE_UNDERFLOW: u32 = 0x0002;
    /// Inexact result.
    pub const FE_INEXACT: u32 = 0x0001;
    /// Every exception flag.
    pub const FE_ALL_EXCEPT: u32 =
        FE_DIVBYZERO | FE_INEXACT | FE_INVALID | FE_OVERFLOW | FE_UNDERFLOW;
    /// Round to nearest, ties to even.
    pub const FE_TONEAREST: u32 = 0x00 << 5;
    /// Round toward zero.
    pub const FE_TOWARDZERO: u32 = 0x01 << 5;
    /// Round toward negative infinity.
    pub const FE_DOWNWARD: u32 = 0x02 << 5;
    /// Round toward positive infinity.
    pub const FE_UPWARD: u32 = 0x03 << 5;
}
