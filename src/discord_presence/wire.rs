// SPDX-License-Identifier: GPL-3.0-or-later
//! Discord's documented local IPC transport and SET_ACTIVITY protocol.

use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

use rustix::net::{self, AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
use serde::Deserialize;
use serde_json::json;

use super::{APPLICATION_ID, Activity};

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;
const MAX_PAYLOAD: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const EXCHANGE_LIMIT: Duration = Duration::from_millis(800);

pub(super) struct Connection {
    socket: UnixStream,
    nonce: u64,
}

#[derive(Deserialize)]
struct Response {
    cmd: Option<String>,
    evt: Option<String>,
    nonce: Option<String>,
    // READY account details and other Discord payload data are discarded.
}

impl Connection {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let address = SocketAddrUnix::new(path)?;
        let descriptor = net::socket_with(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            None,
        )?;
        net::connect(&descriptor, &address)?;
        if net::sockopt::socket_peercred(&descriptor)?.uid != rustix::process::getuid() {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        let socket = UnixStream::from(descriptor);
        socket.set_nonblocking(false)?;
        let mut connection = Self { socket, nonce: 0 };
        let deadline = Instant::now() + EXCHANGE_LIMIT;
        connection.write_json(
            HANDSHAKE,
            &json!({"v":1,"client_id":APPLICATION_ID}),
            deadline,
        )?;
        connection.until(deadline, |response| {
            response.cmd.as_deref() == Some("DISPATCH") && response.evt.as_deref() == Some("READY")
        })?;
        Ok(connection)
    }

    pub(super) fn set_activity(&mut self, activity: Option<&Activity>) -> io::Result<()> {
        self.nonce = self.nonce.wrapping_add(1);
        let nonce = self.nonce.to_string();
        let deadline = Instant::now() + EXCHANGE_LIMIT;
        self.write_json(
            FRAME,
            &json!({
                "cmd": "SET_ACTIVITY",
                "args": {"pid": std::process::id(), "activity": activity},
                "nonce": nonce,
            }),
            deadline,
        )?;
        self.until(deadline, |response| {
            response.cmd.as_deref() == Some("SET_ACTIVITY")
                && response.nonce.as_deref() == Some(nonce.as_str())
        })
    }

    pub(super) fn poll(&mut self) -> io::Result<()> {
        self.read_response(Instant::now() + POLL_INTERVAL)
            .map(|_| ())
    }

    fn until(&mut self, deadline: Instant, expected: impl Fn(&Response) -> bool) -> io::Result<()> {
        loop {
            match self.read_response(deadline)? {
                Some(response) if expected(&response) => return Ok(()),
                Some(_) => {}
                None => return Err(io::Error::from(io::ErrorKind::TimedOut)),
            }
        }
    }

    fn read_response(&mut self, deadline: Instant) -> io::Result<Option<Response>> {
        while let Some((opcode, bytes)) = self.read_packet(deadline)? {
            match opcode {
                FRAME => {
                    let response: Response =
                        serde_json::from_slice(&bytes).map_err(invalid_data)?;
                    if response.evt.as_deref() == Some("ERROR") {
                        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                    }
                    return Ok(Some(response));
                }
                CLOSE => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                PING => self.write_packet(PONG, &bytes, deadline)?,
                PONG => {}
                _ => return Err(io::Error::from(io::ErrorKind::InvalidData)),
            }
        }
        Ok(None)
    }

    fn read_packet(&mut self, deadline: Instant) -> io::Result<Option<(u32, Vec<u8>)>> {
        let mut header = [0; 8];
        let time_left = remaining(deadline)?;
        self.socket.set_read_timeout(Some(time_left))?;
        let initial = match self.socket.read(&mut header) {
            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            Ok(read) => read,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        self.read_rest(&mut header[initial..], deadline)?;
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        if length > MAX_PAYLOAD {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        let mut bytes = vec![0; length];
        self.read_rest(&mut bytes, deadline)?;
        Ok(Some((opcode, bytes)))
    }

    fn read_rest(&mut self, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            self.socket.set_read_timeout(Some(remaining(deadline)?))?;
            let read = self.socket.read(bytes)?;
            if read == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            bytes = &mut bytes[read..];
        }
        Ok(())
    }

    fn write_json(
        &mut self,
        opcode: u32,
        value: &serde_json::Value,
        deadline: Instant,
    ) -> io::Result<()> {
        let payload = serde_json::to_vec(value).map_err(invalid_data)?;
        self.write_packet(opcode, &payload, deadline)
    }

    fn write_packet(&mut self, opcode: u32, payload: &[u8], deadline: Instant) -> io::Result<()> {
        if payload.len() > MAX_PAYLOAD {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        let mut packet = Vec::with_capacity(8 + payload.len());
        packet.extend(opcode.to_le_bytes());
        packet.extend((payload.len() as u32).to_le_bytes());
        packet.extend(payload);
        let mut remaining_bytes = packet.as_slice();
        while !remaining_bytes.is_empty() {
            self.socket.set_write_timeout(Some(remaining(deadline)?))?;
            let written = self.socket.write(remaining_bytes)?;
            if written == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            remaining_bytes = &remaining_bytes[written..];
        }
        Ok(())
    }
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let time = deadline.saturating_duration_since(Instant::now());
    if time.is_zero() {
        return Err(io::Error::from(io::ErrorKind::TimedOut));
    }
    Ok(time)
}

fn invalid_data(_: serde_json::Error) -> io::Error {
    io::Error::from(io::ErrorKind::InvalidData)
}
