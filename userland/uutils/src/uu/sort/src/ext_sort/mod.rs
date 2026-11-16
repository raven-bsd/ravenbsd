// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! External sort: sort large inputs that may not fit in memory.
//!
//! On most platforms this uses a multi-threaded chunked approach with
//! temporary files. On WASI (no threads) we fall back to an in-memory sort.

#[cfg(not(any(target_os = "wasi", target_os = "ravenbsd")))]
mod threaded;
#[cfg(not(any(target_os = "wasi", target_os = "ravenbsd")))]
pub use threaded::ext_sort;

#[cfg(any(target_os = "wasi", target_os = "ravenbsd"))]
mod wasi;
#[cfg(any(target_os = "wasi", target_os = "ravenbsd"))]
// `self::` needed to disambiguate from the `wasi` crate
pub use self::wasi::ext_sort;
