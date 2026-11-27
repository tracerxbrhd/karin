// SPDX-License-Identifier: GPL-3.0-or-later
//! Discord's documented local IPC transport and SET_ACTIVITY protocol.

use std::{
    io::{self, Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    time::{Duration, Instant},
};

use rustix::net::{self, AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
use serde::Deserialize;
use serde_json::json;

use super::{APPLICATION_ID, Activity, diagnostic, peer};

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;
const HEADER_LEN: usize = 8;
const MAX_PAYLOAD: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const EXCHANGE_LIMIT: Duration = Duration::from_millis(800);

pub(super) struct Connection {
    socket: UnixStream,
    nonce: u64,
    incoming: Vec<u8>,
    received: usize,
    frame_deadline: Option<Instant>,
}

#[derive(Deserialize)]
struct Response {
    cmd: Option<String>,
    evt: Option<String>,
    nonce: Option<String>,
    // READY account details and other Discord payload data are discarded.
}

#[derive(Deserialize)]
struct ErrorCode {
    code: i64,
}

impl ErrorCode {
    fn reported(self) -> Option<i64> {
        (1000..=9999).contains(&self.code).then_some(self.code)
    }
}

#[derive(Deserialize)]
struct ErrorResponse {
    data: ErrorCode,
}

impl Connection {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        Self::handshake(open_socket(path)?)
    }

    pub(super) fn handshake(descriptor: OwnedFd) -> io::Result<Self> {
        let started = Instant::now();
        let failure = |event: &'static str, error: &io::Error| {
            diagnostic(event, Some(error), None, Some(started));
        };
        let uid = peer::uid(&descriptor).inspect_err(|error| failure("socket_peer", error))?;
        if uid != rustix::process::getuid().as_raw() {
            let error = io::Error::from(io::ErrorKind::PermissionDenied);
            failure("socket_peer_uid", &error);
            return Err(error);
        }
        let socket = UnixStream::from(descriptor);
        socket
            .set_nonblocking(false)
            .inspect_err(|error| failure("socket_configure", error))?;
        let mut connection = Self {
            socket,
            nonce: 0,
            incoming: vec![0; HEADER_LEN],
            received: 0,
            frame_deadline: None,
        };
        let handshake_started = Instant::now();
        let deadline = handshake_started + EXCHANGE_LIMIT;
        connection
            .write_json(
                HANDSHAKE,
                &json!({"v":1,"client_id":APPLICATION_ID}),
                deadline,
            )
            .inspect_err(|error| {
                diagnostic(
                    "handshake_write",
                    Some(error),
                    None,
                    Some(handshake_started),
                )
            })?;
        connection
            .until(deadline, |response| {
                response.cmd.as_deref() == Some("DISPATCH")
                    && response.evt.as_deref() == Some("READY")
            })
            .inspect_err(|error| {
                diagnostic("handshake_wait", Some(error), None, Some(handshake_started))
            })?;
        diagnostic("ready", None, None, Some(handshake_started));
        Ok(connection)
    }

    pub(super) fn set_activity(&mut self, activity: Option<&Activity>) -> io::Result<()> {
        self.nonce = self.nonce.wrapping_add(1);
        let nonce = self.nonce.to_string();
        let started = Instant::now();
        let deadline = started + EXCHANGE_LIMIT;
        let (write_event, wait_event, ack_event) = if activity.is_some() {
            ("activity_write", "activity_wait", "activity_ack")
        } else {
            ("clear_write", "clear_wait", "clear_ack")
        };
        self.write_json(
            FRAME,
            &json!({
                "cmd": "SET_ACTIVITY",
                "args": {"pid": std::process::id(), "activity": activity},
                "nonce": nonce,
            }),
            deadline,
        )
        .inspect_err(|error| diagnostic(write_event, Some(error), None, Some(started)))?;
        self.until(deadline, |response| {
            response.cmd.as_deref() == Some("SET_ACTIVITY")
                && response.nonce.as_deref() == Some(nonce.as_str())
        })
        .inspect_err(|error| diagnostic(wait_event, Some(error), None, Some(started)))?;
        diagnostic(ack_event, None, None, Some(started));
        Ok(())
    }

    pub(super) fn poll(&mut self) -> io::Result<()> {
        let started = Instant::now();
        self.read_response(started + POLL_INTERVAL, Some(EXCHANGE_LIMIT))
            .map(|_| ())
            .inspect_err(|error| diagnostic("poll", Some(error), None, Some(started)))
    }

    fn until(&mut self, deadline: Instant, expected: impl Fn(&Response) -> bool) -> io::Result<()> {
        loop {
            match self.read_response(deadline, None)? {
                Some(response) if expected(&response) => return Ok(()),
                Some(_) => {}
                None => return Err(io::Error::from(io::ErrorKind::TimedOut)),
            }
        }
    }

    fn read_response(
        &mut self,
        deadline: Instant,
        idle_reply_budget: Option<Duration>,
    ) -> io::Result<Option<Response>> {
        while let Some(packet) = self.read_packet(deadline)? {
            let opcode = u32::from_le_bytes(packet[..4].try_into().unwrap());
            let bytes = &packet[HEADER_LEN..];
            match opcode {
                FRAME => {
                    let response: Response = serde_json::from_slice(bytes).map_err(invalid_data)?;
                    if response.evt.as_deref() == Some("ERROR") {
                        let error = io::Error::from(io::ErrorKind::PermissionDenied);
                        let code = serde_json::from_slice::<ErrorResponse>(bytes)
                            .ok()
                            .and_then(|response| response.data.reported());
                        diagnostic("rpc_error", Some(&error), code, None);
                        return Err(error);
                    }
                    return Ok(Some(response));
                }
                CLOSE => {
                    let error = io::Error::from(io::ErrorKind::UnexpectedEof);
                    let code = serde_json::from_slice::<ErrorCode>(bytes)
                        .ok()
                        .and_then(ErrorCode::reported);
                    diagnostic("rpc_close", Some(&error), code, None);
                    return Err(error);
                }
                PING => {
                    let reply_deadline =
                        idle_reply_budget.map_or(deadline, |budget| Instant::now() + budget);
                    self.write_packet(PONG, bytes, reply_deadline)?;
                }
                PONG => {}
                _ => return Err(io::Error::from(io::ErrorKind::InvalidData)),
            }
        }
        Ok(None)
    }

    fn read_packet(&mut self, deadline: Instant) -> io::Result<Option<Vec<u8>>> {
        loop {
            if self.received == self.incoming.len() {
                if self.incoming.len() == HEADER_LEN {
                    let length =
                        u32::from_le_bytes(self.incoming[4..HEADER_LEN].try_into().unwrap())
                            as usize;
                    if length > MAX_PAYLOAD {
                        return Err(io::Error::from(io::ErrorKind::InvalidData));
                    }
                    if length != 0 {
                        self.incoming.resize(HEADER_LEN + length, 0);
                        continue;
                    }
                }
                self.received = 0;
                self.frame_deadline = None;
                return Ok(Some(std::mem::replace(
                    &mut self.incoming,
                    vec![0; HEADER_LEN],
                )));
            }

            // A poll can finish in the middle of a frame. Keep its bytes for
            // the next poll or request, with a fixed total assembly deadline.
            let read_deadline = deadline.min(self.frame_deadline.unwrap_or(deadline));
            let Ok(time_left) = remaining(read_deadline) else {
                return self.incomplete_packet();
            };
            self.socket.set_read_timeout(Some(time_left))?;
            match self.socket.read(&mut self.incoming[self.received..]) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                Ok(read) => {
                    self.received += read;
                    self.frame_deadline
                        .get_or_insert_with(|| Instant::now() + EXCHANGE_LIMIT);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return self.incomplete_packet();
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn incomplete_packet(&self) -> io::Result<Option<Vec<u8>>> {
        if self
            .frame_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(io::Error::from(io::ErrorKind::TimedOut));
        }
        Ok(None)
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

pub(super) fn open_socket(path: &Path) -> io::Result<OwnedFd> {
    let started = Instant::now();
    let failure = |event: &'static str, error: &io::Error| {
        diagnostic(event, Some(error), None, Some(started));
    };
    let address = SocketAddrUnix::new(path)
        .map_err(io::Error::from)
        .inspect_err(|error| failure("socket_address", error))?;
    let descriptor = net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )
    .map_err(io::Error::from)
    .inspect_err(|error| failure("socket_create", error))?;
    net::connect(&descriptor, &address)
        .map_err(io::Error::from)
        .inspect_err(|error| failure("socket_connect", error))?;
    Ok(descriptor)
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
