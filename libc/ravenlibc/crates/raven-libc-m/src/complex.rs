// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `<complex.h>`: the C99 complex functions built on the real families.
//!
//! [`Complex<T>`] is a plain `#[repr(C)]` pair, so it can cross the `double complex`/`float
//! complex` ABI unchanged. Every function is the principal-value formula in terms of the real
//! functions of this crate (`hypot`, `atan2`, `exp`, `log`, `sqrt`, `sin`, `cos`, `sinh`, `cosh`,
//! `tanh`, `atan`, `asinh`, `acosh`, `atanh`), written for RavenBSD from the C99 definitions.
//!
//! The formulas are the textbook ones; the IEEE special-case tables of the C standard for
//! infinite arguments are only partly reproduced (see the note on each function). Finite
//! arguments, zeros, and NaN propagation are covered by the tests.

use core::ops::{Add, Div, Mul, Neg, Sub};

/// A complex number with real components.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex<T> {
    /// Real part.
    pub re: T,
    /// Imaginary part.
    pub im: T,
}

impl<T> Complex<T> {
    /// Build a complex value.
    pub const fn new(re: T, im: T) -> Self {
        Self { re, im }
    }
}

/// The real operations the complex formulas need.
///
/// Implemented for `f64` and `f32` on top of this crate, never on `std`, so a `no_std` build gets
/// the same results.
pub trait Real:
    Copy
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    /// `0`.
    const ZERO: Self;
    /// `1`.
    const ONE: Self;
    /// `0.5`.
    const HALF: Self;
    /// `2`.
    const TWO: Self;
    /// `pi`.
    const PI: Self;
    /// A quiet NaN.
    const NAN: Self;

    /// `|x|`.
    fn abs(self) -> Self;
    /// Square root.
    fn sqrt(self) -> Self;
    /// `e^x`.
    fn exp(self) -> Self;
    /// Natural logarithm.
    fn ln(self) -> Self;
    /// Sine.
    fn sin(self) -> Self;
    /// Cosine.
    fn cos(self) -> Self;
    /// Tangent.
    fn tan(self) -> Self;
    /// `atan2(y, x)`.
    fn atan2(self, x: Self) -> Self;
    /// Hyperbolic sine.
    fn sinh(self) -> Self;
    /// Hyperbolic cosine.
    fn cosh(self) -> Self;
    /// Hyperbolic tangent.
    fn tanh(self) -> Self;
    /// Inverse hyperbolic sine.
    fn asinh(self) -> Self;
    /// Inverse hyperbolic cosine.
    fn acosh(self) -> Self;
    /// Inverse hyperbolic tangent.
    fn atanh(self) -> Self;
    /// `sqrt(x*x + y*y)` without intermediate overflow.
    fn hypot(self, other: Self) -> Self;
    /// Whether the value is NaN.
    fn is_nan(self) -> bool;
    /// Whether the value is infinite.
    fn is_infinite(self) -> bool;
}

impl Real for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    const HALF: Self = 0.5;
    const TWO: Self = 2.0;
    const PI: Self = core::f64::consts::PI;
    const NAN: Self = f64::NAN;

    fn abs(self) -> Self {
        crate::fabs(self)
    }
    fn sqrt(self) -> Self {
        crate::sqrt(self)
    }
    fn exp(self) -> Self {
        crate::exp(self)
    }
    fn ln(self) -> Self {
        crate::log(self)
    }
    fn sin(self) -> Self {
        crate::sin(self)
    }
    fn cos(self) -> Self {
        crate::cos(self)
    }
    fn tan(self) -> Self {
        crate::tan(self)
    }
    fn atan2(self, x: Self) -> Self {
        crate::atan2(self, x)
    }
    fn sinh(self) -> Self {
        crate::sinh(self)
    }
    fn cosh(self) -> Self {
        crate::cosh(self)
    }
    fn tanh(self) -> Self {
        crate::tanh(self)
    }
    fn asinh(self) -> Self {
        crate::asinh(self)
    }
    fn acosh(self) -> Self {
        crate::acosh(self)
    }
    fn atanh(self) -> Self {
        crate::atanh(self)
    }
    fn hypot(self, other: Self) -> Self {
        crate::hypot(self, other)
    }
    fn is_nan(self) -> bool {
        crate::isnan(self)
    }
    fn is_infinite(self) -> bool {
        crate::isinf(self)
    }
}

impl Real for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    const HALF: Self = 0.5;
    const TWO: Self = 2.0;
    const PI: Self = core::f32::consts::PI;
    const NAN: Self = f32::NAN;

    fn abs(self) -> Self {
        crate::fabsf(self)
    }
    fn sqrt(self) -> Self {
        crate::sqrtf(self)
    }
    fn exp(self) -> Self {
        crate::expf(self)
    }
    fn ln(self) -> Self {
        crate::logf(self)
    }
    fn sin(self) -> Self {
        crate::sinf(self)
    }
    fn cos(self) -> Self {
        crate::cosf(self)
    }
    fn tan(self) -> Self {
        crate::tanf(self)
    }
    fn atan2(self, x: Self) -> Self {
        crate::atan2f(self, x)
    }
    fn sinh(self) -> Self {
        crate::sinhf(self)
    }
    fn cosh(self) -> Self {
        crate::coshf(self)
    }
    fn tanh(self) -> Self {
        crate::tanhf(self)
    }
    fn asinh(self) -> Self {
        crate::asinhf(self)
    }
    fn acosh(self) -> Self {
        crate::acoshf(self)
    }
    fn atanh(self) -> Self {
        crate::atanhf(self)
    }
    fn hypot(self, other: Self) -> Self {
        crate::hypotf(self, other)
    }
    fn is_nan(self) -> bool {
        crate::isnanf(self)
    }
    fn is_infinite(self) -> bool {
        crate::isinff(self)
    }
}

impl<T: Real> Add for Complex<T> {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.re + other.re, self.im + other.im)
    }
}

impl<T: Real> Sub for Complex<T> {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.re - other.re, self.im - other.im)
    }
}

impl<T: Real> Mul for Complex<T> {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }
}

impl<T: Real> Div for Complex<T> {
    type Output = Self;
    fn div(self, other: Self) -> Self {
        let denominator = other.re * other.re + other.im * other.im;
        Self::new(
            (self.re * other.re + self.im * other.im) / denominator,
            (self.im * other.re - self.re * other.im) / denominator,
        )
    }
}

impl<T: Real> Neg for Complex<T> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

impl<T: Real> Complex<T> {
    /// The conjugate.
    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// The IEEE 754 `projection`: an infinite component wins, otherwise the value is unchanged.
    pub fn proj(self) -> Self {
        if self.re.is_infinite() || self.im.is_infinite() {
            Self::new(T::ONE, T::ZERO)
        } else {
            self
        }
    }

    /// `|z|`.
    pub fn cabs(self) -> T {
        self.re.hypot(self.im)
    }

    /// The argument in `(-pi, pi]`.
    pub fn carg(self) -> T {
        self.im.atan2(self.re)
    }

    /// `e^z`.
    pub fn cexp(self) -> Self {
        let scale = self.re.exp();
        Self::new(scale * self.im.cos(), scale * self.im.sin())
    }

    /// The principal logarithm.
    pub fn clog(self) -> Self {
        Self::new(self.cabs().ln(), self.carg())
    }

    /// The principal square root.
    pub fn csqrt(self) -> Self {
        if self.im == T::ZERO && self.re == T::ZERO {
            return self;
        }
        let magnitude = self.cabs();
        let t = ((self.re.abs() + magnitude) * T::HALF).sqrt();
        if self.re >= T::ZERO {
            Self::new(t, self.im / (t + t))
        } else {
            Self::new(
                self.im.abs() / (t + t),
                if self.im < T::ZERO { -t } else { t },
            )
        }
    }

    /// `z^w`.
    ///
    /// The zero base is special-cased so a positive real exponent gives zero instead of the
    /// `log(0)` NaN; other unbounded corners follow the formula.
    pub fn cpow(self, exponent: Self) -> Self {
        if self.re == T::ZERO && self.im == T::ZERO {
            // 0^w is 0 for Re(w) > 0 and undefined (NaN) otherwise; the log formula would return
            // NaN for both.
            return if exponent.re > T::ZERO {
                Self::new(T::ZERO, T::ZERO)
            } else {
                Self::new(T::NAN, T::NAN)
            };
        }
        (exponent * self.clog()).cexp()
    }

    /// `sin(z)`.
    pub fn csin(self) -> Self {
        Self::new(
            self.re.sin() * self.im.cosh(),
            self.re.cos() * self.im.sinh(),
        )
    }

    /// `cos(z)`.
    pub fn ccos(self) -> Self {
        Self::new(
            self.re.cos() * self.im.cosh(),
            -(self.re.sin() * self.im.sinh()),
        )
    }

    /// `tan(z) = sin(z) / cos(z)`.
    pub fn ctan(self) -> Self {
        self.csin() / self.ccos()
    }

    /// `sinh(z) = -i sin(iz)`.
    pub fn csinh(self) -> Self {
        let rotated = Self::new(-self.im, self.re).csin();
        Self::new(rotated.im, -rotated.re)
    }

    /// `cosh(z) = cos(iz)`.
    pub fn ccosh(self) -> Self {
        Self::new(-self.im, self.re).ccos()
    }

    /// `tanh(z) = -i tan(iz)`.
    pub fn ctanh(self) -> Self {
        let rotated = Self::new(-self.im, self.re).ctan();
        Self::new(rotated.im, -rotated.re)
    }

    /// `asin(z) = -i log(iz + sqrt(1 - z^2))`.
    pub fn casin(self) -> Self {
        let unit = Self::new(T::ONE, T::ZERO);
        let root = (unit - self * self).csqrt();
        let rotated = Self::new(-self.im, self.re);
        let log = (rotated + root).clog();
        Self::new(log.im, -log.re)
    }

    /// `acos(z) = pi/2 - asin(z)`.
    pub fn cacos(self) -> Self {
        let half_pi = T::PI * T::HALF;
        let asin = self.casin();
        Self::new(half_pi - asin.re, -asin.im)
    }

    /// `atan(z) = (i/2) log((i + z) / (i - z))`.
    pub fn catan(self) -> Self {
        let i = Self::new(T::ZERO, T::ONE);
        let log = ((i + self) / (i - self)).clog();
        let half = T::HALF;
        Self::new(-log.im * half, log.re * half)
    }

    /// `asinh(z) = log(z + sqrt(z^2 + 1))`.
    pub fn casinh(self) -> Self {
        let unit = Self::new(T::ONE, T::ZERO);
        (self + (self * self + unit).csqrt()).clog()
    }

    /// `acosh(z) = log(z + sqrt(z + 1) * sqrt(z - 1))`.
    pub fn cacosh(self) -> Self {
        let unit = Self::new(T::ONE, T::ZERO);
        (self + (self + unit).csqrt() * (self - unit).csqrt()).clog()
    }

    /// `atanh(z) = log((1 + z) / (1 - z)) / 2`.
    pub fn catanh(self) -> Self {
        let unit = Self::new(T::ONE, T::ZERO);
        let log = ((unit + self) / (unit - self)).clog();
        Self::new(log.re * T::HALF, log.im * T::HALF)
    }
}

/// `cabs(z)`.
pub fn cabs(z: Complex<f64>) -> f64 {
    z.cabs()
}
/// `cabsf(z)`.
pub fn cabsf(z: Complex<f32>) -> f32 {
    z.cabs()
}
/// `carg(z)`.
pub fn carg(z: Complex<f64>) -> f64 {
    z.carg()
}
/// `cargf(z)`.
pub fn cargf(z: Complex<f32>) -> f32 {
    z.carg()
}
/// `creal(z)`.
pub fn creal(z: Complex<f64>) -> f64 {
    z.re
}
/// `crealf(z)`.
pub fn crealf(z: Complex<f32>) -> f32 {
    z.re
}
/// `cimag(z)`.
pub fn cimag(z: Complex<f64>) -> f64 {
    z.im
}
/// `cimagf(z)`.
pub fn cimagf(z: Complex<f32>) -> f32 {
    z.im
}
/// `conj(z)`.
pub fn conj(z: Complex<f64>) -> Complex<f64> {
    z.conj()
}
/// `conjf(z)`.
pub fn conjf(z: Complex<f32>) -> Complex<f32> {
    z.conj()
}
/// `cproj(z)`.
pub fn cproj(z: Complex<f64>) -> Complex<f64> {
    z.proj()
}
/// `cprojf(z)`.
pub fn cprojf(z: Complex<f32>) -> Complex<f32> {
    z.proj()
}
/// `cexp(z)`.
pub fn cexp(z: Complex<f64>) -> Complex<f64> {
    z.cexp()
}
/// `cexpf(z)`.
pub fn cexpf(z: Complex<f32>) -> Complex<f32> {
    z.cexp()
}
/// `clog(z)`.
pub fn clog(z: Complex<f64>) -> Complex<f64> {
    z.clog()
}
/// `clogf(z)`.
pub fn clogf(z: Complex<f32>) -> Complex<f32> {
    z.clog()
}
/// `csqrt(z)`.
pub fn csqrt(z: Complex<f64>) -> Complex<f64> {
    z.csqrt()
}
/// `csqrtf(z)`.
pub fn csqrtf(z: Complex<f32>) -> Complex<f32> {
    z.csqrt()
}
/// `cpow(z, w)`.
pub fn cpow(z: Complex<f64>, w: Complex<f64>) -> Complex<f64> {
    z.cpow(w)
}
/// `cpowf(z, w)`.
pub fn cpowf(z: Complex<f32>, w: Complex<f32>) -> Complex<f32> {
    z.cpow(w)
}
/// `csin(z)`.
pub fn csin(z: Complex<f64>) -> Complex<f64> {
    z.csin()
}
/// `csinf(z)`.
pub fn csinf(z: Complex<f32>) -> Complex<f32> {
    z.csin()
}
/// `ccos(z)`.
pub fn ccos(z: Complex<f64>) -> Complex<f64> {
    z.ccos()
}
/// `ccosf(z)`.
pub fn ccosf(z: Complex<f32>) -> Complex<f32> {
    z.ccos()
}
/// `ctan(z)`.
pub fn ctan(z: Complex<f64>) -> Complex<f64> {
    z.ctan()
}
/// `ctanf(z)`.
pub fn ctanf(z: Complex<f32>) -> Complex<f32> {
    z.ctan()
}
/// `csinh(z)`.
pub fn csinh(z: Complex<f64>) -> Complex<f64> {
    z.csinh()
}
/// `csinhf(z)`.
pub fn csinhf(z: Complex<f32>) -> Complex<f32> {
    z.csinh()
}
/// `ccosh(z)`.
pub fn ccosh(z: Complex<f64>) -> Complex<f64> {
    z.ccosh()
}
/// `ccoshf(z)`.
pub fn ccoshf(z: Complex<f32>) -> Complex<f32> {
    z.ccosh()
}
/// `ctanh(z)`.
pub fn ctanh(z: Complex<f64>) -> Complex<f64> {
    z.ctanh()
}
/// `ctanhf(z)`.
pub fn ctanhf(z: Complex<f32>) -> Complex<f32> {
    z.ctanh()
}
/// `casin(z)`.
pub fn casin(z: Complex<f64>) -> Complex<f64> {
    z.casin()
}
/// `casinf(z)`.
pub fn casinf(z: Complex<f32>) -> Complex<f32> {
    z.casin()
}
/// `cacos(z)`.
pub fn cacos(z: Complex<f64>) -> Complex<f64> {
    z.cacos()
}
/// `cacosf(z)`.
pub fn cacosf(z: Complex<f32>) -> Complex<f32> {
    z.cacos()
}
/// `catan(z)`.
pub fn catan(z: Complex<f64>) -> Complex<f64> {
    z.catan()
}
/// `catanf(z)`.
pub fn catanf(z: Complex<f32>) -> Complex<f32> {
    z.catan()
}
/// `casinh(z)`.
pub fn casinh(z: Complex<f64>) -> Complex<f64> {
    z.casinh()
}
/// `casinhf(z)`.
pub fn casinhf(z: Complex<f32>) -> Complex<f32> {
    z.casinh()
}
/// `cacosh(z)`.
pub fn cacosh(z: Complex<f64>) -> Complex<f64> {
    z.cacosh()
}
/// `cacoshf(z)`.
pub fn cacoshf(z: Complex<f32>) -> Complex<f32> {
    z.cacosh()
}
/// `catanh(z)`.
pub fn catanh(z: Complex<f64>) -> Complex<f64> {
    z.catanh()
}
/// `catanhf(z)`.
pub fn catanhf(z: Complex<f32>) -> Complex<f32> {
    z.catanh()
}

#[cfg(test)]
#[path = "complex_ut.rs"]
mod tests;
