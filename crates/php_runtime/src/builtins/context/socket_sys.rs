//! Audited Unix socket-option adapter for options without safe `std` wrappers.

use std::io;
use std::net::UdpSocket;
use std::os::fd::{AsRawFd, RawFd};
use std::time::Duration;

pub(super) fn set_udp_int_option(
    socket: &UdpSocket,
    level: i32,
    option: i32,
    value: i32,
) -> io::Result<()> {
    set_int_option_fd(socket.as_raw_fd(), level, option, value)
}

pub(super) fn set_int_option(
    socket: &impl AsRawFd,
    level: i32,
    option: i32,
    value: i32,
) -> io::Result<()> {
    set_int_option_fd(socket.as_raw_fd(), level, option, value)
}

// SAFETY: this function is the audited Unix FFI boundary for integer socket options.
#[allow(unsafe_code)]
fn set_int_option_fd(fd: RawFd, level: i32, option: i32, value: i32) -> io::Result<()> {
    // SAFETY: `socket` owns a live descriptor for the duration of the call;
    // `value` is passed with its exact initialized size and the kernel only
    // reads that memory during `setsockopt`.
    let result = unsafe {
        libc::setsockopt(
            fd,
            level,
            option,
            (&value as *const i32).cast(),
            std::mem::size_of::<i32>() as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

// SAFETY: this function is the audited Unix FFI boundary for integer socket options.
#[allow(unsafe_code)]
pub(super) fn get_int_option(socket: &impl AsRawFd, level: i32, option: i32) -> io::Result<i32> {
    let mut value = 0_i32;
    let mut length = std::mem::size_of::<i32>() as libc::socklen_t;
    // SAFETY: `socket` owns a live descriptor, and both output pointers refer
    // to initialized writable storage with the exact advertised sizes.
    let result = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            level,
            option,
            (&mut value as *mut i32).cast(),
            &mut length,
        )
    };
    if result == 0 {
        Ok(value)
    } else {
        Err(io::Error::last_os_error())
    }
}

// SAFETY: this function is the audited Unix FFI boundary for poll descriptors.
#[allow(unsafe_code)]
pub(super) fn poll_readable(sockets: &[(i64, RawFd)], timeout: Duration) -> io::Result<Vec<i64>> {
    let mut descriptors = sockets
        .iter()
        .map(|(_, fd)| libc::pollfd {
            fd: *fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect::<Vec<_>>();
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: `descriptors` is a contiguous initialized `pollfd` buffer which
    // remains alive and exclusively borrowed for the duration of `poll`.
    let result = unsafe {
        libc::poll(
            descriptors.as_mut_ptr(),
            descriptors.len() as libc::nfds_t,
            timeout_ms,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sockets
        .iter()
        .zip(descriptors)
        .filter_map(|((id, _), descriptor)| {
            (descriptor.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0)
                .then_some(*id)
        })
        .collect())
}
