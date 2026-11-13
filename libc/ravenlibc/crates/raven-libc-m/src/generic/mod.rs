// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer-only implementations of the exact-class operations, generic over [`Float`].

pub use self::ceil::ceil_status;
pub use self::copysign::copysign;
pub use self::fabs::fabs;
pub use self::fdim::fdim;
pub use self::floor::floor_status;
pub use self::fma::fma_round;
pub use self::fmax::fmax;
pub use self::fmaximum::fmaximum;
pub use self::fmaximum_num::fmaximum_num;
pub use self::fmin::fmin;
pub use self::fminimum::fminimum;
pub use self::fminimum_num::fminimum_num;
pub use self::fmod::fmod;
pub use self::frexp::frexp;
pub use self::ilogb::ilogb;
pub use self::rint::rint_status;
pub use self::round::round;
pub use self::scalbn::scalbn;
pub use self::sqrt::sqrt_round;
pub use self::trunc::trunc_status;

mod ceil;
mod copysign;
mod fabs;
mod fdim;
mod floor;
mod fma;
mod fma_wide;
mod fmax;
mod fmaximum;
mod fmaximum_num;
mod fmin;
mod fminimum;
mod fminimum_num;
mod fmod;
mod frexp;
mod ilogb;
mod rint;
mod round;
mod scalbn;
mod sqrt;
mod trunc;

pub use crate::support::Round;
