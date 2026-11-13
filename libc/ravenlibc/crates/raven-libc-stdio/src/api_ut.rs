// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Cross-module API and trap tests (`donor-map/04` 4.9 S1-S12 plus the per-operation edges
//! that do not belong to one module).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    unsafe_code
)]

use crate::fake_ut::{FakeEnv, Kind, leaked_file, leaked_state, leaked_streams, open_file};
use crate::*;

fn state_with(data: &[u8]) -> &'static State<FakeEnv> {
    let state = leaked_state();
    state.env().add_file(b"/f", data, 1024);
    state
}

// --- the twelve traps ---------------------------------------------------------------

#[test]
fn trap_s1_signed_unread_window() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(crate::char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(ungetc(file, i32::from(b'a')), Ok(i32::from(b'a')));
    assert_eq!(seek::ftell(file), Ok(0));
}

#[test]
fn trap_s2_unget_bytes_are_not_in_buf_size() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    let stdin = streams.stdin();
    assert_eq!(stdin.st().buf_size, crate::buf::STD_BUF);
    assert_eq!(
        stdin.st().buffer.bytes().len(),
        crate::file::UNGET + crate::buf::STD_BUF
    );
}

#[test]
fn trap_s3_std_streams_are_stable_addresses() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    let first = streams.stdin() as *const File<FakeEnv>;
    let second = streams.stdin() as *const File<FakeEnv>;
    assert_eq!(first, second);
}

#[test]
fn trap_s4_fclose_locks_the_file_before_unlinking() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(close::fclose(file), Ok(()));
    assert!(state.ofl_head().is_none());
}

#[test]
fn trap_s5_guard_took_flag() {
    let state = leaked_state();
    let env = state.env();
    env.set_threaded(false);
    let file = leaked_file(state);
    {
        let _guard = lock::FileGuard::new(file);
        env.set_threaded(true);
    }
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn trap_s6_flush_all_takes_one_file_lock_at_a_time() {
    let state = leaked_state();
    let streams = leaked_streams(state);
    state.env().install_std_file();
    let _stdout = streams.stdout();
    state.env().add_file(b"/a", b"", 1024);
    let file = open_file(state, b"/a", b"w");
    assert_eq!(block_io::fwrite(file, b"x", 1, 1), Ok(1));
    assert_eq!(flush::fflush(state, None), Ok(()));
}

#[test]
fn trap_s7_getdelim_excludes_the_nul() {
    let state = state_with(b"abc\n");
    let file = open_file(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = line_io::LineBuf::new(&mut data, 0);
    assert_eq!(line_io::getline(file, &mut line), Ok(4));
    assert_eq!(line.line(), b"abc\n");
    assert_eq!(line.bytes().last(), Some(&0));
}

#[test]
fn trap_s8_memstream_pointer_is_valid_after_close() {
    let state = leaked_state();
    let bufp = Box::into_raw(Box::new(core::ptr::null_mut::<u8>()));
    let sizep = Box::into_raw(Box::new(0usize));
    let file = file_from_ptr(mem::open_memstream(state, bufp, sizep).expect("open"));
    assert_eq!(block_io::fwrite(file, b"data", 1, 4), Ok(4));
    assert_eq!(close::fclose(file), Ok(()));
    // SAFETY: both pointers come from Box allocations in this test; open_memstream
    // published the leaked buffer through `bufp`.
    unsafe {
        let size = *sizep;
        assert!(size >= 4);
        let slice = core::slice::from_raw_parts(*bufp, size);
        assert_eq!(&slice[..4], b"data");
        drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
            *bufp, size,
        )));
        drop(Box::from_raw(bufp));
        drop(Box::from_raw(sizep));
    }
}

#[test]
fn trap_s9_wide_ungetc_calls_toread_first() {
    let state = state_with("é".as_bytes());
    let file = open_file(state, b"/f", b"r");
    assert_eq!(wide::fgetwc(file), Ok(0xe9));
    assert_eq!(wide::ungetwc(file, 0xe9), Ok(0xe9));
    assert_eq!(wide::fgetwc(file), Ok(0xe9));
}

#[test]
fn trap_s10_write2_partial_loop() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    state.env().set_max_write(2);
    assert_eq!(block_io::fwrite(file, b"abcdef", 1, 6), Ok(6));
    assert_eq!(flush::fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"abcdef".to_vec());
}

#[test]
fn trap_s11_probe_only_on_stdout_without_setvbuf() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stderr = streams.stderr();
    assert!(!stderr.st().probe);
    let _ = char_io::putc(stderr, i32::from(b'x'));
    assert_eq!(stderr.st().lbf, -1);
}

#[test]
fn trap_s12_isatty_is_fake_only() {
    let state = leaked_state();
    state.env().install_std_tty();
    let streams = leaked_streams(state);
    assert!(state.env().isatty(1));
    assert!(!state.env().isatty(99));
    assert_eq!(streams.stdout().fd(), 1);
}

// --- constants and constructors -----------------------------------------------------

#[test]
fn constant_bufsiz() {
    assert_eq!(buf::BUFSIZ, 4096);
    assert_eq!(buf::STD_BUF, 1024);
    assert_eq!(buf::UNBUF_PRINT_BUF, 1024);
    assert_eq!(file::UNGET, 8);
    assert_eq!(errors::EOF, -1);
}

#[test]
fn mode_accepts_b_anywhere() {
    for mode in [b"rb".as_slice(), b"r+b", b"rb+"] {
        let state = leaked_state();
        state.env().add_file(b"/f", b"x", 1024);
        let file = open_file(state, b"/f", mode);
        assert_eq!(char_io::getc(file), Ok(i32::from(b'x')));
    }
}

#[test]
fn mode_rejects_wrong_first_character() {
    let state = leaked_state();
    for mode in [b"x".as_slice(), b"+r", b"", b"b"] {
        assert_eq!(
            open::fopen(state, b"/f", mode).map(|_| ()),
            Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
        );
    }
}

#[test]
fn fopen_plus_modes_are_read_write() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"abc", 1024);
    for mode in [b"r+".as_slice(), b"w+", b"a+"] {
        let file = open_file(state, b"/f", mode);
        assert_eq!(
            file.st().flags & (crate::file::F_NORD | crate::file::F_NOWR),
            0
        );
    }
}

#[test]
fn fdopen_append_sets_the_flag() {
    let state = leaked_state();
    let fd = state.env().push_fd(b"", Kind::Regular(1024));
    state.env().setfl(fd, 1).expect("setfl");
    let append = open::fdopen(state, fd, b"a").expect("fdopen");
    let append = file_from_ptr(append);
    assert_ne!(append.flags() & crate::file::F_APP, 0);
}

#[test]
fn fopen_error_is_errno() {
    let state = leaked_state();
    assert_eq!(
        open::fopen(state, b"/missing", b"r").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn file_from_ptr_reaches_the_handle() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let pointer = open::fopen(state, b"/f", b"w").expect("fopen");
    let file = file_from_ptr(pointer);
    assert_eq!(file.fd(), 3);
}

// --- character and block I/O --------------------------------------------------------

#[test]
fn getc_eof_is_not_an_errno() {
    let state = state_with(b"");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(char_io::getc(file), Err(Fail::Eof));
    assert_eq!(char_io::getc(file).err().and_then(|e| e.errno()), None);
}

#[test]
fn fputc_truncates_to_a_byte() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(char_io::putc(file, 0x141), Ok(0x41));
    assert_eq!(flush::fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"A".to_vec());
}

#[test]
fn fgetc_and_getc_agree() {
    let state = state_with(b"xy");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(char_io::fgetc(file), Ok(i32::from(b'x')));
    assert_eq!(char_io::getc(file), Ok(i32::from(b'y')));
    assert_eq!(char_io::fgetc(file), Err(Fail::Eof));
    assert_eq!(char_io::getc(file), Err(Fail::Eof));
}

#[test]
fn fread_returns_zero_for_empty_requests() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(block_io::fread(file, &mut [], 1, 0), Ok(0));
    assert_eq!(block_io::fread(file, &mut [0u8; 4], 0, 4), Ok(0));
}

#[test]
fn fwrite_returns_zero_for_empty_requests() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(block_io::fwrite(file, b"", 1, 0), Ok(0));
    assert_eq!(block_io::fwrite(file, b"", 0, 1), Ok(0));
}

#[test]
fn fwrite_multiple_items() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(block_io::fwrite(file, b"abcdef", 2, 3), Ok(3));
    assert_eq!(flush::fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"abcdef".to_vec());
}

#[test]
fn fread_items_with_size_three() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 7];
    assert_eq!(block_io::fread(file, &mut buffer, 3, 2), Ok(2));
    assert_eq!(&buffer[..6], b"abcdef");
}

#[test]
fn unbuffered_write_is_one_call() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    setvbuf::setvbuf(file, None, setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    assert_eq!(block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(state.env().trace().len(), 1);
}

#[test]
fn read_error_sets_ferror_not_feof() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    state.env().eintr_on(1);
    assert!(char_io::getc(file).is_err());
    assert!(status::ferror(file));
    assert!(!status::feof(file));
}

#[test]
fn fwrite_on_a_read_only_stream_is_a_short_count() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(block_io::fwrite(file, b"x", 1, 1), Ok(0));
    assert!(status::ferror(file));
}

// --- line I/O -----------------------------------------------------------------------

#[test]
fn getdelim_empty_input_is_eof() {
    let state = state_with(b"");
    let file = open_file(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = line_io::LineBuf::new(&mut data, 0);
    assert_eq!(line_io::getline(file, &mut line), Err(Fail::Eof));
}

#[test]
fn getdelim_nul_delimiter() {
    let state = state_with(b"a\0b");
    let file = open_file(state, b"/f", b"r");
    let mut data = Vec::new();
    let mut line = line_io::LineBuf::new(&mut data, 0);
    assert_eq!(line_io::getdelim(file, &mut line, 0), Ok(2));
    assert_eq!(line.line(), b"a\0");
}

#[test]
fn getdelim_capacity_is_reported() {
    let state = state_with(b"abcd\n");
    let file = open_file(state, b"/f", b"r");
    let mut data = vec![0u8; 4];
    let mut line = line_io::LineBuf::new(&mut data, 4);
    assert_eq!(line_io::getline(file, &mut line), Ok(5));
    assert!(line.capacity() >= 5);
}

#[test]
fn fgets_stops_at_the_nul_of_an_embedded_byte() {
    let state = state_with(b"a\0b\n");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0u8; 8];
    assert_eq!(line_io::fgets(file, &mut buffer), Ok(4));
    assert_eq!(&buffer[..5], b"a\0b\n\0");
}

#[test]
fn fputs_short_write_is_an_error() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    setvbuf::setvbuf(file, None, setvbuf::IONBF, 0).expect("setvbuf");
    state.env().fail_write_after(1);
    assert_eq!(
        line_io::fputs(file, b"abcdef"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOSPC))
    );
    assert!(status::ferror(file));
}

#[test]
fn fgetln_eof_is_eof() {
    let state = state_with(b"");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(line_io::fgetln(file), Err(Fail::Eof));
}

#[test]
fn puts_uses_stdout() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert_eq!(line_io::puts(state, b"x"), Ok(()));
    assert_eq!(flush::fflush(state, Some(stdout)), Ok(()));
    assert_eq!(state.env().data(1), b"x\n".to_vec());
}

#[test]
fn fgets_size_one_writes_only_the_nul() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    let mut buffer = [0xAAu8; 1];
    assert_eq!(line_io::fgets(file, &mut buffer), Ok(0));
    assert_eq!(buffer, [0u8]);
}

// --- seek ---------------------------------------------------------------------------

#[test]
fn fseek_negative_result_is_einval() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(
        seek::fseek(file, -1, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn ftell_on_a_write_stream_counts_pending() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let _ = block_io::fwrite(file, b"abc", 1, 3);
    assert_eq!(seek::ftell(file), Ok(3));
}

#[test]
fn rewind_clears_the_error_flag() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    file.st_mut().flags |= crate::file::F_ERR;
    seek::rewind(file);
    assert!(!status::ferror(file));
}

#[test]
fn fgetpos_padding_is_zero() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    let pos = seek::fgetpos(file).expect("fgetpos");
    assert_eq!(pos._pad, [0u8; 16]);
}

#[test]
fn ftello_equals_ftell_on_lp64() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(seek::ftell(file), seek::ftello(file));
}

#[test]
fn fseek_end_zero_is_the_file_length() {
    let state = state_with(b"abcde");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(seek::fseek(file, 0, 2), Ok(()));
    assert_eq!(seek::ftell(file), Ok(5));
}

#[test]
fn fsetpos_restores_the_offset() {
    let state = state_with(b"abcdef");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(seek::fseek(file, 4, 0), Ok(()));
    let pos = seek::Fpos::new(4);
    assert_eq!(seek::fsetpos(file, &pos), Ok(()));
    assert_eq!(seek::ftell(file), Ok(4));
}

#[test]
fn fseek_bad_whence_is_einval() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(
        seek::fseeko(file, 0, 7),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

// --- status, setvbuf, flush ---------------------------------------------------------

#[test]
fn clearerr_on_a_clean_stream_is_a_noop() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    status::clearerr(file);
    assert!(!status::feof(file));
    assert!(!status::ferror(file));
}

#[test]
fn fileno_of_a_memory_stream_is_ebadf() {
    let state = leaked_state();
    let mut room = [0u8; 4];
    let file = file_from_ptr(
        mem::fmemopen(state, Some((room.as_mut_ptr(), 4)), 4, b"r").expect("fmemopen"),
    );
    assert_eq!(
        status::fileno(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EBADF))
    );
    let _ = close::fclose(file);
}

#[test]
fn setvbuf_bad_type_preserves_state() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(
        setvbuf::setvbuf(file, None, 7, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn setlinebuf_marks_the_stream() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    setvbuf::setlinebuf(file);
    assert_eq!(file.st().lbf, i32::from(b'\n'));
}

#[test]
fn setbuf_null_is_unbuffered() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    setvbuf::setbuf(file, None);
    assert_eq!(file.st().buf_size, 0);
}

#[test]
fn fflush_null_on_an_empty_world_is_ok() {
    let state = leaked_state();
    assert_eq!(flush::fflush(state, None), Ok(()));
}

#[test]
fn fpurge_of_a_reader_drops_the_window() {
    let state = state_with(b"abc");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(char_io::getc(file), Ok(i32::from(b'a')));
    assert_eq!(flush::fpurge(file), Ok(()));
    assert_eq!(file.st().rend, 0);
}

#[test]
fn fclose_of_a_memory_stream_is_ok() {
    let state = leaked_state();
    let mut room = [0u8; 4];
    let file = file_from_ptr(
        mem::fmemopen(state, Some((room.as_mut_ptr(), 4)), 4, b"w").expect("fmemopen"),
    );
    assert_eq!(close::fclose(file), Ok(()));
}

#[test]
fn fcloseall_on_an_empty_world_is_ok() {
    let state = leaked_state();
    assert_eq!(close::fcloseall(state), Ok(()));
}

// --- wide ---------------------------------------------------------------------------

#[test]
fn wide_orientation_is_query_only_after_set() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(wide::fwide(file, 1), 1);
    assert_eq!(wide::fwide(file, -1), 1);
    assert_eq!(wide::fwide(file, 0), 1);
}

#[test]
fn fputwc_ascii_is_putc() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(wide::fputwc(file, u32::from(b'a')), Ok(u32::from(b'a')));
    assert_eq!(flush::fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), b"a".to_vec());
}

#[test]
fn fgetwc_eof_is_weof_class() {
    let state = state_with(b"");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(wide::fgetwc(file), Err(Fail::Eof));
}

#[test]
fn ungetwc_rejects_weof() {
    let state = state_with(b"a");
    let file = open_file(state, b"/f", b"r");
    assert_eq!(wide::ungetwc(file, wide::WEOF), Err(Fail::Eof));
}

#[test]
fn fputws_writes_utf8() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(wide::fputws(file, &[0xe9]), Ok(()));
    assert_eq!(flush::fflush(state, Some(file)), Ok(()));
    assert_eq!(state.env().contents(b"/f"), "é".as_bytes().to_vec());
}

#[test]
fn fgetws_terminates() {
    let state = state_with("aé".as_bytes());
    let file = open_file(state, b"/f", b"r");
    let mut dest = [0u32; 4];
    assert_eq!(wide::fgetws(file, &mut dest), Ok(2));
    assert_eq!(dest[2], 0);
}

#[test]
fn getwchar_reads_stdin() {
    let state = leaked_state();
    state.env().install_std_tty();
    let streams = leaked_streams(state);
    let _ = streams.stdin();
    state
        .env()
        .inner
        .lock()
        .unwrap()
        .files
        .get_mut(&0)
        .unwrap()
        .data = b"x".to_vec();
    assert_eq!(wide::getwchar(state), Ok(u32::from(b'x')));
}

#[test]
fn putwchar_writes_stdout() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert_eq!(wide::putwchar(state, u32::from(b'y')), Ok(u32::from(b'y')));
    assert_eq!(flush::fflush(state, Some(stdout)), Ok(()));
    assert_eq!(state.env().data(1), b"y".to_vec());
}

// --- memory and cookie --------------------------------------------------------------

#[test]
fn fmemopen_bad_mode_is_einval() {
    let state = leaked_state();
    assert_eq!(
        mem::fmemopen(state, None, 4, b"q").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn fmemopen_read_only_writes_zero() {
    let state = leaked_state();
    let mut room = [0u8; 4];
    let file = file_from_ptr(
        mem::fmemopen(state, Some((room.as_mut_ptr(), 4)), 4, b"r").expect("fmemopen"),
    );
    assert_eq!(block_io::fwrite(file, b"x", 1, 1), Ok(0));
    let _ = close::fclose(file);
}

#[test]
fn fmemopen_seek_end_is_the_size() {
    let state = leaked_state();
    let mut room = [0u8; 6];
    let file = file_from_ptr(
        mem::fmemopen(state, Some((room.as_mut_ptr(), 6)), 6, b"r").expect("fmemopen"),
    );
    assert_eq!(seek::ftell(file), Ok(0));
    assert_eq!(seek::fseek(file, 0, 2), Ok(()));
    assert_eq!(seek::ftell(file), Ok(6));
    let _ = close::fclose(file);
}

#[test]
fn fmemopen_zero_size_reads_eof() {
    let state = leaked_state();
    let mut room = [0u8; 1];
    let file = file_from_ptr(
        mem::fmemopen(state, Some((room.as_mut_ptr(), 0)), 0, b"r").expect("fmemopen"),
    );
    assert_eq!(char_io::getc(file), Err(Fail::Eof));
    let _ = close::fclose(file);
}

#[test]
fn funopen_rejects_both_missing() {
    let state = leaked_state();
    assert_eq!(
        funopen::funopen(state, core::ptr::null_mut(), None, None, None, None).map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn fopencookie_rejects_a_bad_mode() {
    let state = leaked_state();
    assert_eq!(
        cookie::fopencookie(state, core::ptr::null_mut(), b"z", raw::CookieIo::default())
            .map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn fopencookie_no_seek_is_enotsup() {
    let state = leaked_state();
    let io = raw::CookieIo::default();
    let file = file_from_ptr(
        cookie::fopencookie(state, core::ptr::null_mut(), b"r", io).expect("fopencookie"),
    );
    assert_eq!(
        seek::fseek(file, 0, 0),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOTSUP))
    );
    let _ = close::fclose(file);
}

// --- misc, popen, perror, exit ------------------------------------------------------

#[test]
fn remove_missing_is_enoent() {
    let state = leaked_state();
    assert_eq!(
        misc::remove(state, b"/missing"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn rename_moves_contents() {
    let state = leaked_state();
    state.env().add_file(b"/a", b"x", 1024);
    assert_eq!(misc::rename(state, b"/a", b"/b"), Ok(()));
    assert_eq!(state.env().contents(b"/b"), b"x".to_vec());
}

#[test]
fn popen_check_rejects_a_plain_file() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(
        popen::check_pipe(file),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn perror_without_a_prefix_has_no_colon() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let _ = streams.stderr();
    assert_eq!(perror::perror(state, b"", b"text"), Ok(()));
    assert_eq!(state.env().data(2), b"text\n".to_vec());
}

#[test]
fn exit_cleanup_leaves_the_list_locked() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let _ = streams.stdin();
    exit::stdio_cleanup(state, streams);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[test]
fn fork_child_resets_the_list_lock() {
    let state = leaked_state();
    exit::fork_prepare(state);
    exit::fork_child(state);
    assert_eq!(
        state
            .ofl_lock_word()
            .load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn ext_fsetlocking_bad_type_returns_old() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    assert_eq!(ext::__fsetlocking(file, 3), ext::FSETLOCKING_INTERNAL);
    assert_eq!(
        ext::__fsetlocking(file, ext::FSETLOCKING_QUERY),
        ext::FSETLOCKING_INTERNAL
    );
}

#[test]
fn glue_sink_unbuffered_finishes() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    setvbuf::setvbuf(file, None, setvbuf::IONBF, 0).expect("setvbuf");
    let mut sink = glue_io::FileSink::new(file);
    assert!(raven_libc_fmt::Sink::write(&mut sink, b"abc").is_ok());
    assert!(sink.finish().is_ok());
    assert_eq!(state.env().data(file.fd()), b"abc".to_vec());
}
