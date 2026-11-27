// SPDX-License-Identifier: GPL-3.0-or-later
//! Resolve a native Discord socket in a fresh instance of the same Flatpak.

use std::{
    io::{self, IoSlice, IoSliceMut},
    mem::MaybeUninit,
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    process::{Command, Stdio},
    time::Duration,
};

use rustix::net::{
    self, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};

use super::{is_flatpak, runtime_directory, socket_unavailable, wire};

const HELPER_FLAG: &str = "--discord-socket-helper";
const HELPER_TIMEOUT: Duration = Duration::from_secs(2);
const OPEN: u8 = b'+';
const UNAVAILABLE: u8 = b'-';
const FAILED: u8 = b'!';

pub(super) fn refresh_native_socket() -> io::Result<OwnedFd> {
    let (control, inherited) = UnixStream::pair()?;
    control.set_read_timeout(Some(HELPER_TIMEOUT))?;
    let mut helper = Command::new("/usr/bin/flatpak-spawn")
        .args(["--watch-bus", "--no-network", "/app/bin/karin", HELPER_FLAG])
        .stdin(Stdio::from(OwnedFd::from(inherited)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let result = receive_socket(&control);
    // The socket belongs to this worker once received. Closing the spawn
    // client's bus connection also terminates a helper that did not reply.
    let _ = helper.kill();
    let _ = helper.wait();
    result
}

/// Dispatch the private helper before initializing GTK or accessing app data.
pub fn run_discord_socket_helper(args: &[String]) -> Option<io::Result<()>> {
    if args != [HELPER_FLAG] {
        return None;
    }
    if !is_flatpak() {
        return Some(Err(io::Error::from(io::ErrorKind::Unsupported)));
    }
    let socket = runtime_directory()
        .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
        .and_then(|runtime| wire::open_socket(&runtime.join("discord-ipc-0")));
    Some(send_socket(io::stdin(), socket))
}

fn send_socket(control: impl AsFd, socket: io::Result<OwnedFd>) -> io::Result<()> {
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut ancillary = SendAncillaryBuffer::new(&mut space);
    let descriptors;
    let reply = match &socket {
        Ok(socket) => {
            descriptors = [socket.as_fd()];
            assert!(ancillary.push(SendAncillaryMessage::ScmRights(&descriptors)));
            OPEN
        }
        Err(error) if socket_unavailable(error) => UNAVAILABLE,
        Err(_) => FAILED,
    };
    net::sendmsg(
        control,
        &[IoSlice::new(&[reply])],
        &mut ancillary,
        SendFlags::NOSIGNAL | SendFlags::DONTWAIT,
    )?;
    Ok(())
}

fn receive_socket(control: &UnixStream) -> io::Result<OwnedFd> {
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut ancillary = RecvAncillaryBuffer::new(&mut space);
    let mut reply = [0];
    let result = net::recvmsg(
        control,
        &mut [IoSliceMut::new(&mut reply)],
        &mut ancillary,
        RecvFlags::CMSG_CLOEXEC,
    )?;
    if result.bytes != 1 || result.flags.contains(ReturnFlags::CTRUNC) {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    match (reply[0], ancillary.drain().next()) {
        (OPEN, Some(RecvAncillaryMessage::ScmRights(mut descriptors))) => {
            let socket = descriptors.next().filter(|_| descriptors.next().is_none());
            socket.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))
        }
        (UNAVAILABLE, None) => Err(io::Error::from(io::ErrorKind::NotFound)),
        (FAILED, None) => Err(io::Error::other("Discord socket helper could not connect")),
        _ => Err(io::Error::from(io::ErrorKind::InvalidData)),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    #[test]
    fn returns_connected_socket_after_helper_closes() {
        let (control, helper) = UnixStream::pair().unwrap();
        let (socket, mut discord) = UnixStream::pair().unwrap();
        send_socket(helper, Ok(socket.into())).unwrap();

        let mut socket = UnixStream::from(receive_socket(&control).unwrap());
        discord.write_all(b"ready").unwrap();
        let mut response = [0; 5];
        socket.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"ready");
    }

    #[test]
    fn reports_unavailable_socket_without_a_descriptor() {
        let (control, helper) = UnixStream::pair().unwrap();
        send_socket(helper, Err(io::ErrorKind::ConnectionRefused.into())).unwrap();
        assert_eq!(
            receive_socket(&control).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
