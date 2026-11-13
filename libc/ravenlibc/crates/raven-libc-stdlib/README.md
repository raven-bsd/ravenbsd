<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-stdlib

The logic behind `<stdlib.h>`, `<search.h>`, `<getopt.h>`, `<err.h>`, `<assert.h>` and
`<libgen.h>`: integer conversion, sorting and searching, the three random generators, the exit
machinery, the environment, option parsing, trees and hash tables, `realpath`, and the text of
`assert` and `err`. The area document is `architecture/raven-libc/donor-map/03-stdlib.md`; the
ticket is C41.

The crate is `no_std` + `alloc`, `#![forbid(unsafe_code)]` (class P, DR-39), never uses
`core::fmt`, and keeps every C pointer, `errno` store, lock, `atexit` function pointer and
`environ` array in the shell (`raven-libc`, C60, R-17-4).

```
cargo test -p raven-libc-stdlib                       # unit + integration tests
cargo clippy -p raven-libc-stdlib --all-targets -- -D warnings
cargo build -p raven-libc-stdlib --target x86_64-unknown-none
cargo xtask libc-oracle diff stdlib-openbsd           # and -netbsd, -freebsd, -musl
```

## Public API

Conversion (`conv`):

```rust
pub trait CodeUnit: Copy + Eq { fn to_u32(self) -> u32; }   // u8 and u32 only
pub struct Scan { pub value: u64, pub negative: bool, pub consumed: usize, pub status: IntStatus }
pub enum IntStatus { Ok, Range, InvalidBase, NoDigits }
pub fn scan_integer<C: CodeUnit>(s: &[C], base: i32, signed: bool) -> Scan;
pub fn strtol(s: &[u8], base: i32) -> (i64, usize, IntStatus);
pub fn strtoul(s: &[u8], base: i32) -> (u64, usize, IntStatus);
pub fn strtoll(s: &[u8], base: i32) -> (i64, usize, IntStatus);
pub fn strtoimax(s: &[u8], base: i32) -> (i64, usize, IntStatus);
pub fn strtoull(s: &[u8], base: i32) -> (u64, usize, IntStatus);
pub fn strtoumax(s: &[u8], base: i32) -> (u64, usize, IntStatus);
pub fn strtonum(s: &[u8], lo: i64, hi: i64) -> Result<i64, StrtonumError>;
pub enum StrtonumError { Invalid, TooSmall, TooLarge }      // as_bytes()/as_c_bytes()
pub fn atoi(s: &[u8]) -> i32;
pub fn atol(s: &[u8]) -> i64;
pub fn atoll(s: &[u8]) -> i64;
```

Arithmetic (`arith`): `abs`, `labs`, `llabs`, `imaxabs`; `DivT { quot, rem }`, `LDivT { quot,
rem }`, `div`, `ldiv`, `lldiv`, `imaxdiv`.

Sorting (`sort`):

```rust
pub const STABLE_QSORT: bool = false;
pub enum SortError { Inval, NoMem }
pub fn qsort<F: FnMut(&[u8], &[u8]) -> i32>(base: &mut [u8], es: usize, cmp: F);
pub fn heapsort<F: FnMut(&[u8], &[u8]) -> i32>(base: &mut [u8], es: usize, cmp: F) -> Result<(), SortError>;
pub fn mergesort<F: FnMut(&[u8], &[u8]) -> i32>(base: &mut [u8], es: usize, cmp: F) -> Result<(), SortError>;
pub fn bsearch<F: FnMut(&[u8]) -> i32>(base: &[u8], es: usize, cmp_key_elem: F) -> Option<usize>;
pub fn qsort_with<F: FnMut(&[u8], &[u8]) -> i32>(stable: bool, base: &mut [u8], es: usize, cmp: F);
```

Random (`rand`):

```rust
pub const RAND_MAX: u32 = 0x7fff_ffff;
pub struct RandomCtl { pub type: u32, pub deg: usize, pub sep: usize, pub fptr: usize, pub rptr: usize }
pub enum RandErr { Invalid }
pub fn srandom(ctl: &mut RandomCtl, st: &mut [u32], seed: u32);
pub fn random(ctl: &mut RandomCtl, st: &mut [u32]) -> u32;
pub fn initstate(ctl: &mut RandomCtl, st: &mut [u32], seed: u32, nbytes: usize) -> Result<(), RandErr>;
pub fn setstate(ctl: &mut RandomCtl, st: &mut [u32]) -> Result<(), RandErr>;
pub struct DefaultRandom { pub ctl: RandomCtl, pub state: [u32; 32] }  // const fn new(), rand(), srand()
pub fn rand(generator: &mut DefaultRandom) -> u32;
pub fn srand(generator: &mut DefaultRandom, seed: u32);
pub fn rand_r(seed: &mut u32) -> u32;
pub struct Rand48 { pub x: u64, pub a: u64, pub c: u16 }               // const fn new()
pub fn srand48/seed48/lcong48/lrand48/nrand48/mrand48/jrand48/drand48/erand48(...)
pub fn x_to_double(x: u64) -> f64;
```

Environment (`env`): `EnvTable::{new, adopt_initial, reload, getenv, getenv_r, setenv, putenv,
unsetenv, clearenv, as_slice, generation, len, is_empty}` with the ticket's fixed signatures;
`getenv`, `getenv_r`, `setenv`, `putenv`, `unsetenv`, `clearenv`, `secure_getenv(env, name,
at_secure)`.

`getopt`:

```rust
pub const GETOPT_PERMUTES: bool = false;
pub struct Option_ { pub name: &'static [u8], pub has_arg: i32, pub flag_is_null: bool, pub val: i32 }
pub struct LongOpt<'a> { pub name: &'a [u8], pub has_arg: i32, pub flag_is_null: bool, pub val: i32 }
pub enum OptArg { None, At { arg_index: usize, offset: usize } }
pub struct Step { pub code: i32, pub optarg: OptArg, pub long_index: Option<usize>, pub flag_to_set: Option<usize> }
pub trait ArgSource { type Arg: Copy; fn text(&self, a: Self::Arg) -> &[u8]; }
pub trait Diag { fn warnx(&mut self, parts: &[&[u8]]); }
impl GetoptState { const fn new(); getopt(..); getopt_long(.., only: bool, posixly_correct: bool, ..);
                    getopt_long_only(..); optind(); optopt(); set_optind(); set_opterr(); set_optreset(); }
```

`search`:

```rust
pub trait NodeStore { type Node: Copy + Eq; key/set_key/left/right/balance/set_*/alloc/free }
pub enum TDelete<N> { Parent(N), RootDeleted }
pub enum Visit { Preorder, Postorder, Endorder, Leaf }
pub fn tsearch/tfind/tdelete/twalk/tdestroy(...) -> ...;
pub fn hcreate<E: Entropy>(entropy: &mut E) -> Result<HTable, HSearchError>;
impl HTable { hsearch(key, data, Action, &KeyText) -> Result<SlotId, HSearchError>; entries_used(); capacity() }
pub enum Action { Find, Enter }
pub trait KeyText { fn text(&self, key: usize) -> &[u8]; }
pub trait Entropy { fn fill(&mut self, buf: &mut [u8]); }
pub trait ListStore { type Node: Copy + Eq; next/prev/set_next/set_prev }
pub fn insque/remque(...);
pub fn lfind(key, base, nel, width, cmp) -> Option<usize>;
pub fn lsearch(key, base, &mut nel, width, cmp) -> Result<usize, LSearchError>;
```

Diagnostics (`diag`): `assert_message(out: &mut [u8; 512], prog, file, line, func, expr) -> usize`,
`assert_fail`, `assert2`, `assert_bsd`, `DiagOut`, `verr/verrc/verrx/vwarn/vwarnc/vwarnx`,
`ProgName`, `basename`/`dirname` -> `GenName`.

Exit (`exit`):

```rust
pub struct Entry { pub func: usize, pub arg: usize, pub dso: usize, pub kind: Kind }
pub enum Kind { Atexit, Cxa }
impl ExitTable { const fn new(); register(Entry)->Result<(),Errno>; register_oldest(Entry)->...;
                 next_to_run()->Option<Entry>; finalize_dso(dso)->Option<Entry>;
                 finalize_range(&[(usize,usize)])->Option<Entry>; pending()->usize; inline_capacity() }
pub struct ThreadAtexit; pub trait DsoRefs { add_ref; drop_ref }
impl ThreadAtexit { const fn new(); push(Entry,&mut D)->Result<(),Errno>; pop(&mut D)->Option<Entry>; len(); is_empty() }
pub enum Step?  // the pure order is `exit_steps(&mut ExitTable, &mut ThreadAtexit, &mut D) -> Vec<Entry>`
pub trait AbortOps { unblock_sigabrt; raise_sigabrt; reset_sigabrt_default; terminate }
pub fn abort_sequence<A: AbortOps>(ops: &mut A);
pub struct QuickExitTable { register(func)->Result<(),Errno>; pop_next()->Option<Entry>; len(); is_empty() }
pub fn quick_exit_steps(&mut QuickExitTable) -> Vec<Entry>;
```

`path`/`proc`/`mb`/`misc`:

```rust
pub enum FileKind { Regular, Dir, Symlink, Other }
pub trait Walker { lstat_kind; readlink; getcwd }
pub fn realpath<W: Walker>(w: &mut W, path: &[u8], out: &mut [u8]) -> Result<usize, Errno>;
pub enum Sig { Chld, Int, Quit }
pub trait SystemBackend { block; spawn_sh; ignore; unblock_and_restore; wait }
pub fn system<B: SystemBackend>(b: &mut B, cmd: Option<&[u8]>) -> i32;
pub fn mblen(s: Option<&[u8]>, mode: MbMode) -> i32;
pub fn mbtowc(s: Option<&[u8]>, mode: MbMode) -> (i32, u32);
pub fn wctomb(wc: u32, mode: MbMode, out: &mut [u8; 4]) -> i32;
pub fn a64l(s: &[u8]) -> Result<i64, Errno>;
pub fn l64a(v: i64, out: &mut [u8; 8]);
pub struct Subopt { index, token, value, next }
pub fn getsubopt(pos: usize, options: &mut [u8], tokens: &[&[u8]]) -> Subopt;
```

## Deviations

D-1 `srandom(0)` behaves as seed 1; D-2 environment strings are never freed or overwritten; D-3
`getopt` does not permute while `getopt_long` does. The other adaptations are recorded in
`PROVENANCE.toml` (qsort indices/closure/alloc-free fallback, `mergesort` checked multiply,
`realpath1` fast-path removal, assert text/buffer, no mmap pages for `atexit`/`abort`, `hsearch`
entropy parameter).
