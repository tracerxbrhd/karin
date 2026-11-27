// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    io,
    os::fd::{AsFd, AsRawFd},
    ptr,
};

pub(super) fn uid(descriptor: impl AsFd) -> io::Result<libc::uid_t> {
    // A peer outside our PID namespace has PID zero. Use the kernel's raw
    // credentials layout so that case still yields a valid UID.
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let expected_length = size_of::<libc::ucred>() as libc::socklen_t;
    let mut length = expected_length;
    // SAFETY: The borrowed descriptor stays open for the call, and both
    // pointers refer to initialized storage of the sizes expected by Linux.
    let result = unsafe {
        libc::getsockopt(
            descriptor.as_fd().as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            ptr::from_mut(&mut credentials).cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    if length != expected_length {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected Unix peer credentials size",
        ));
    }
    Ok(credentials.uid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::File, os::unix::net::UnixStream};

    #[test]
    fn reads_uid_from_connected_unix_socket() {
        let (client, _peer) = UnixStream::pair().unwrap();
        assert_eq!(uid(&client).unwrap(), rustix::process::getuid().as_raw());
    }

    #[test]
    fn rejects_non_socket_descriptors() {
        let file = File::open("/dev/null").unwrap();
        assert_eq!(uid(&file).unwrap_err().raw_os_error(), Some(libc::ENOTSOCK));
    }
}
