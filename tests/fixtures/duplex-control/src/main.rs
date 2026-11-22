// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K210 duplex control fixture: SCM_RIGHTS handle passing, bytes, datagram boundaries,
//! peer credentials and negative control cases through the raw kernel ABI.

#![no_std]
#![no_main]
#![allow(unsafe_code)]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_rt::entry;
use raven_rt::pipe::{O_NONBLOCK, pipe};
use raven_rt::println;
use raven_rt::sys::{handle_close, handle_dup, read, syscall6, write};

entry!(main);

/// Raw duplex pair number.
const DUPLEX_PAIR: usize = 0x110;
/// Raw duplex sendmsg number.
const DUPLEX_SENDMSG: usize = 0x112;
/// Raw duplex recvmsg number.
const DUPLEX_RECVMSG: usize = 0x113;
/// Raw duplex peer-credential number.
const DUPLEX_PEERCRED: usize = 0x114;
/// Datagram mode flag.
const DUPLEX_DATAGRAM: u32 = 1;
/// `SOL_SOCKET` control level.
const SOL_SOCKET: u32 = 1;
/// `SCM_RIGHTS` control type.
const SCM_RIGHTS: u32 = 1;
/// Maximum handles in one control message.
const MAX_HANDLES: usize = 16;
/// Raw `PROC_SELF_INFO` number.
const PROC_SELF_INFO: usize = 0x35;

#[repr(C)]
#[derive(PartialEq)]
struct Message {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

#[repr(C)]
#[derive(PartialEq)]
struct Cred {
    pid: u64,
    uid: u32,
    gid: u32,
}

fn pair(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut handles = [0u32; 2];
    // SAFETY: `handles` is a live two-word array and the kernel writes exactly eight bytes.
    let result = unsafe {
        syscall6(
            DUPLEX_PAIR,
            handles.as_mut_ptr() as usize,
            flags as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result)?;
    Ok((Handle(handles[0]), Handle(handles[1])))
}

fn sendmsg(
    handle: Handle,
    data: &[u8],
    control: &[u8],
    flags: u32,
) -> Result<usize, Errno> {
    let mut message = Message {
        data: data.as_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_ptr() as u64,
        control_len: control.len() as u64,
        flags,
        handle_count: 0,
    };
    // SAFETY: the message fields point at live caller slices; the kernel validates every range.
    let result = unsafe {
        syscall6(
            DUPLEX_SENDMSG,
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result).map(|count| count as usize)
}

fn recvmsg(
    handle: Handle,
    data: &mut [u8],
    control: &mut [u8],
) -> Result<(usize, Message), Errno> {
    let mut message = Message {
        data: data.as_mut_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_mut_ptr() as u64,
        control_len: control.len() as u64,
        flags: 0,
        handle_count: 0,
    };
    // SAFETY: the message fields point at live caller slices; the kernel validates every range.
    let result = unsafe {
        syscall6(
            DUPLEX_RECVMSG,
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        )
    };
    let count = Errno::from_ret(result)? as usize;
    Ok((count, message))
}

fn peercred(handle: Handle) -> Result<Cred, Errno> {
    let mut cred = Cred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    // SAFETY: `cred` is a live 16-byte record and the kernel writes exactly that size.
    let result = unsafe {
        syscall6(
            DUPLEX_PEERCRED,
            handle.0 as usize,
            (&mut cred as *mut Cred) as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result)?;
    Ok(cred)
}

fn self_info() -> Result<[u64; 8], Errno> {
    let mut info = [0u64; 8];
    // SAFETY: `info` is a live eight-word array and the kernel writes exactly that size.
    let result = unsafe {
        syscall6(
            PROC_SELF_INFO,
            info.as_mut_ptr() as usize,
            info.len(),
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result)?;
    Ok(info)
}

fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = buffer.get_mut(offset..offset.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn read_u32(buffer: &[u8], offset: usize) -> Result<u32, Errno> {
    let slot = buffer
        .get(offset..offset.saturating_add(4))
        .ok_or(Errno::EINVAL)?;
    let bytes: [u8; 4] = slot.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u32::from_le_bytes(bytes))
}

fn make_control(handles: &[Handle], kind: u32, buffer: &mut [u8]) -> usize {
    let length = 16usize.saturating_add(handles.len().saturating_mul(4));
    write_u32(buffer, 0, length as u32);
    write_u32(buffer, 4, SOL_SOCKET);
    write_u32(buffer, 8, kind);
    write_u32(buffer, 12, handles.len() as u32);
    for (index, handle) in handles.iter().enumerate() {
        write_u32(buffer, 16 + index * 4, handle.0);
    }
    length
}

fn first_handle(control: &[u8], length: usize) -> Result<u32, Errno> {
    if length < 16 {
        return Err(Errno::EINVAL);
    }
    let record_len = read_u32(control, 0)? as usize;
    let level = read_u32(control, 4)?;
    let kind = read_u32(control, 8)?;
    let count = read_u32(control, 12)?;
    if record_len < 16
        || record_len > length
        || level != SOL_SOCKET
        || kind != SCM_RIGHTS
        || count != 1
    {
        return Err(Errno::EINVAL);
    }
    read_u32(control, 16)
}

fn transfer_once(sender: Handle, receiver: Handle) -> Result<bool, Errno> {
    let (read_end, write_end) = pipe()?;
    let mut control = [0u8; 64];
    let control_len = make_control(&[write_end], SCM_RIGHTS, &mut control);
    let sent = sendmsg(sender, b"fd", &control[..control_len], 0) == Ok(2);

    let mut data = [0u8; 16];
    let mut received_control = [0u8; 64];
    let (count, message) = recvmsg(receiver, &mut data, &mut received_control)?;
    let received = if count == 2 && &data[..2] == b"fd" && message.handle_count == 1 {
        first_handle(&received_control, message.control_len as usize).ok()
    } else {
        None
    };
    let Some(handle) = received.map(Handle) else {
        let _ = handle_close(write_end);
        let _ = handle_close(read_end);
        return Ok(false);
    };

    handle_close(write_end)?;
    let wrote = write(handle, b"ok") == Ok(2);
    let mut reply = [0u8; 2];
    let read_ok = read(read_end, &mut reply) == Ok(2) && &reply == b"ok";
    handle_close(handle)?;
    let eof = read(read_end, &mut reply) == Ok(0);
    handle_close(read_end)?;
    Ok(sent && wrote && read_ok && eof)
}

fn stream() -> Result<bool, Errno> {
    let (left, right) = pair(0)?;
    let forward = transfer_once(left, right)?;
    let reverse = transfer_once(right, left)?;
    handle_close(left)?;
    handle_close(right)?;
    Ok(forward && reverse)
}

fn datagram() -> Result<bool, Errno> {
    let (left, right) = pair(DUPLEX_DATAGRAM | O_NONBLOCK)?;
    let (read_end, write_end) = pipe()?;
    let mut control = [0u8; 64];
    let control_len = make_control(&[write_end], SCM_RIGHTS, &mut control);
    let first = sendmsg(left, b"one", &control[..control_len], 0) == Ok(3);
    let second = sendmsg(left, b"two", &[], 0) == Ok(3);

    let mut data = [0u8; 16];
    let mut received_control = [0u8; 64];
    let (count, message) = recvmsg(right, &mut data, &mut received_control)?;
    let one = count == 3
        && &data[..3] == b"one"
        && message.handle_count == 1
        && first_handle(&received_control, message.control_len as usize).is_ok();
    let handle = Handle(first_handle(&received_control, message.control_len as usize)?);

    let (second_count, second_message) = recvmsg(right, &mut data, &mut received_control)?;
    let two = second_count == 3
        && &data[..3] == b"two"
        && second_message.handle_count == 0
        && second_message.control_len == 0;

    handle_close(write_end)?;
    let wrote = write(handle, b"ok") == Ok(2);
    let mut reply = [0u8; 2];
    let read_ok = read(read_end, &mut reply) == Ok(2) && &reply == b"ok";
    handle_close(handle)?;
    let eof = read(read_end, &mut reply) == Ok(0);
    handle_close(read_end)?;
    handle_close(left)?;
    handle_close(right)?;
    Ok(first && second && one && two && wrote && read_ok && eof)
}

fn negative() -> Result<bool, Errno> {
    let (left, right) = pair(0)?;
    let (read_end, write_end) = pipe()?;

    let malformed = sendmsg(left, b"x", &[0u8; 8], 0) == Err(Errno::EINVAL);

    let mut unknown = [0u8; 16];
    let unknown_len = make_control(&[], SCM_RIGHTS + 1, &mut unknown);
    let unknown = sendmsg(left, b"x", &unknown[..unknown_len], 0) == Err(Errno::EINVAL);

    let mut bad_flags = Message {
        data: b"x".as_ptr() as u64,
        data_len: 1,
        control: 0,
        control_len: 0,
        flags: 1,
        handle_count: 0,
    };
    // SAFETY: the message has one live data byte and no control buffer.
    let flags_result = unsafe {
        syscall6(
            DUPLEX_SENDMSG,
            left.0 as usize,
            (&mut bad_flags as *mut Message) as usize,
            0,
            0,
            0,
            0,
        )
    };
    let bad_flags = Errno::from_ret(flags_result) == Err(Errno::EINVAL);

    let mut duplicates = [Handle(0); MAX_HANDLES + 1];
    let mut too_many = true;
    for slot in &mut duplicates {
        match handle_dup(write_end, Rights::ALL) {
            Ok(handle) => *slot = handle,
            Err(_) => {
                too_many = false;
                break;
            }
        }
    }
    let mut many_control = [0u8; 256];
    let many_len = make_control(&duplicates, SCM_RIGHTS, &mut many_control);
    let too_many = too_many
        && sendmsg(left, b"x", &many_control[..many_len], 0) == Err(Errno::EMSGSIZE);
    for handle in duplicates {
        let _ = handle_close(handle);
    }

    let mut control = [0u8; 64];
    let control_len = make_control(&[write_end], SCM_RIGHTS, &mut control);
    let sent = sendmsg(left, b"z", &control[..control_len], 0) == Ok(1);
    let mut data = [0u8; 8];
    let mut small = [0u8; 8];
    let small_rejected = recvmsg(right, &mut data, &mut small) == Err(Errno::EMSGSIZE);
    let mut enough = [0u8; 64];
    let (count, message) = recvmsg(right, &mut data, &mut enough)?;
    let retry = count == 1 && &data[..1] == b"z" && message.handle_count == 1;
    let handle = Handle(first_handle(&enough, message.control_len as usize)?);

    handle_close(handle)?;
    handle_close(write_end)?;
    handle_close(read_end)?;
    handle_close(left)?;
    handle_close(right)?;
    Ok(malformed && unknown && bad_flags && too_many && sent && small_rejected && retry)
}

fn credentials() -> Result<bool, Errno> {
    let (left, right) = pair(0)?;
    let info = self_info()?;
    let left_cred = peercred(left)?;
    let right_cred = peercred(right)?;
    let expected = left_cred.pid == info[0]
        && left_cred.uid == info[4] as u32
        && left_cred.gid == info[6] as u32
        && left_cred == right_cred;
    handle_close(left)?;
    handle_close(right)?;
    Ok(expected)
}

fn run() -> Result<(), Errno> {
    let stream_ok = stream()?;
    let datagram_ok = datagram()?;
    let credentials_ok = credentials()?;
    let negative_ok = negative()?;
    println!(
        "duplex-control: stream={} datagram={}",
        u8::from(stream_ok),
        u8::from(datagram_ok)
    );
    println!(
        "duplex-control: creds={} negative={}",
        u8::from(credentials_ok),
        u8::from(negative_ok)
    );
    if !(stream_ok && datagram_ok && credentials_ok && negative_ok) {
        println!("duplex-control: FAIL checks");
        return Err(Errno::EINVAL);
    }
    println!("duplex-control: ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("duplex-control: error {error:?}");
            1
        }
    }
}
