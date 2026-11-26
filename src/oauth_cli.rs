// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-local authorization commands; neither starts the GTK application.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use gtk::gio;
use karin::credentials::Cancellation;
use karin::independent_auth::{Error as IndependentError, IndependentAuth, Outcome};
use karin::oauth::{AuthorizationAttempt, AuthorizationResponse};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::termios::{
    InputModes, LocalModes, OptionalActions, SpecialCodeIndex, Termios, tcgetattr, tcsetattr,
};
use zeroize::Zeroizing;

const INPUT_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_INPUT_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CliError {
    #[error("This command requires a local controlling terminal; no credential input was read.")]
    NoTerminal,
    #[error(
        "Cannot use or restore the local terminal. If needed, run `stty sane` in that terminal."
    )]
    Terminal,
    #[error("OAuth operation cancelled locally.")]
    Cancelled,
    #[error("Local input timed out; no authorization code was exchanged.")]
    InputTimeout,
    #[error("Invalid local input; enter only the Client ID or complete redirect URL requested.")]
    InvalidInput,
    #[error("Cannot initialize the OAuth diagnostic. Check the local Karin Client ID.")]
    Configuration,
    #[error("Cannot open the system browser; no authorization code was exchanged.")]
    Browser,
    #[error("Cannot write the sanitized command result.")]
    Output,
    #[error("Enter a registered loopback port between 1024 and 65535.")]
    InvalidPort,
    #[error(transparent)]
    Independent(#[from] IndependentError),
}

fn open_terminal() -> Result<File, CliError> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| CliError::NoTerminal)?;
    tcgetattr(&tty).map_err(|_| CliError::NoTerminal)?;
    Ok(tty)
}

/// Called only for the exact `--oauth-diagnostic` command, before GTK starts.
/// Credentials are accepted exclusively from `/dev/tty`, never stdin/arguments.
pub fn run() -> Result<bool, CliError> {
    let mut tty = open_terminal()?;
    writeln!(
        tty,
        "Karin: local opt-in OAuth diagnostic.\n\
         Use only your own Karin user-authorization application's Client ID.\n\
         Never enter a client secret, password or OAuth token.\n\
         Tokens remain in memory for this run. No music streams are requested.\n\
         Paste the complete returned verification URL only at the redirect prompt;\n\
         its state must match this attempt before the PKCE code exchange.\n\
         Input is hidden. Empty input, Ctrl-C, Ctrl-D or Escape cancels."
    )
    .map_err(|_| CliError::Terminal)?;

    let client_id = prompt_hidden(&mut tty, "Karin Client ID: ")?;
    let attempt = AuthorizationAttempt::new(&client_id).map_err(|_| CliError::Configuration)?;
    drop(client_id);
    {
        let authorization_url = attempt.authorization_url();
        gio::AppInfo::launch_default_for_uri(
            authorization_url.as_str(),
            None::<&gio::AppLaunchContext>,
        )
        .map_err(|_| CliError::Browser)?;
    }
    writeln!(
        tty,
        "Continue in the system browser. If you decline consent, cancel here.\n\
         Copy the complete returned URL from the browser's address bar.\n\
         It must be the verification_code redirect with state and code (or an OAuth error).\n\
         Do not paste just the displayed code or an implicit-flow token URL."
    )
    .map_err(|_| CliError::Terminal)?;
    let redirect = prompt_hidden(&mut tty, "Full redirect URL: ")?;
    // Restore the terminal before any network operation. No raw transport error
    // or native browser error is printed; only the backend's sanitized report.
    let report = attempt.complete(AuthorizationResponse::ReturnedRedirect(redirect));
    let passed = report.passed();
    writeln!(std::io::stdout().lock(), "{report}").map_err(|_| CliError::Output)?;
    Ok(passed)
}

pub enum IndependentCommand {
    Login { remember: bool },
    Restore,
    Logout,
}

impl IndependentCommand {
    pub fn parse(args: &[String]) -> Option<Self> {
        let [flag, command, options @ ..] = args else {
            return None;
        };
        if flag != "--independent-auth" {
            return None;
        }
        match (command.as_str(), options) {
            ("login", []) => Some(Self::Login { remember: false }),
            ("login", [option]) if option == "--remember" => Some(Self::Login { remember: true }),
            ("restore", []) => Some(Self::Restore),
            ("logout", []) => Some(Self::Logout),
            _ => None,
        }
    }
}

/// Uses only the independent credential namespace, including for remembered login.
pub fn run_independent(command: IndependentCommand) -> Result<(), CliError> {
    let mut tty = open_terminal()?;
    writeln!(tty, "Karin: isolated local sign-in.").map_err(|_| CliError::Terminal)?;
    let outcome = match command {
        IndependentCommand::Login { remember } => {
            writeln!(
                tty,
                "Use your own Karin registration. Never enter a client secret, password or token.\n\
                 Register exactly http://127.0.0.1:<port>/oauth/callback and enter its port below.\n\
                 Sign in and grant access in the system browser."
            )
            .map_err(|_| CliError::Terminal)?;
            let client_id = prompt_hidden(&mut tty, "Karin Client ID: ")?;
            let port = parse_port(&prompt_hidden(&mut tty, "Registered loopback port: ")?)?;
            let mut auth = IndependentAuth::new()?;
            with_cancellation(&mut tty, |cancel| {
                auth.login(&client_id, port, remember, cancel, |uri| {
                    gio::AppInfo::launch_default_for_uri(uri, None::<&gio::AppLaunchContext>)
                        .map_err(|_| IndependentError::Browser)
                })
            })?
        }
        IndependentCommand::Restore => {
            let client_id = prompt_hidden(&mut tty, "Same Karin Client ID used at login: ")?;
            let mut auth = IndependentAuth::new()?;
            with_cancellation(&mut tty, |cancel| auth.restore(&client_id, cancel))?
        }
        IndependentCommand::Logout => with_cancellation(&mut tty, IndependentAuth::logout)?,
    };
    writeln!(std::io::stdout().lock(), "{outcome}").map_err(|_| CliError::Output)
}

fn parse_port(input: &str) -> Result<u16, CliError> {
    if input.len() > 5 || !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(CliError::InvalidPort);
    }
    input
        .parse::<u16>()
        .ok()
        .filter(|port| *port >= 1024)
        .ok_or(CliError::InvalidPort)
}

fn with_cancellation(
    tty: &mut File,
    operation: impl FnOnce(&Cancellation) -> Result<Outcome, IndependentError>,
) -> Result<Outcome, CliError> {
    let guard = TerminalGuard::hide(tty)?;
    writeln!(tty, "Press Enter, Ctrl-C, Ctrl-D or Escape here to cancel.")
        .map_err(|_| CliError::Terminal)?;
    let cancel = Cancellation::new();
    let result = thread::scope(|scope| {
        let (done, pending) = mpsc::channel();
        let watcher_cancel = cancel.clone();
        let watcher = thread::Builder::new()
            .spawn_scoped(scope, move || {
                if wait_for_cancellation(tty, pending).is_err() {
                    watcher_cancel.cancel();
                }
            })
            .map_err(|_| CliError::Terminal)?;
        let result = operation(&cancel);
        // Disconnect the watcher even on unwind, so the scope can always join it.
        drop(done);
        watcher.join().map_err(|_| CliError::Terminal)?;
        result.map_err(CliError::Independent)
    });
    guard.restore()?;
    result
}

fn wait_for_cancellation(tty: &mut File, pending: mpsc::Receiver<()>) -> Result<(), CliError> {
    let wait = Timespec {
        tv_sec: 0,
        tv_nsec: 50_000_000,
    };
    let mut byte = Zeroizing::new([0_u8; 1]);
    while matches!(pending.try_recv(), Err(mpsc::TryRecvError::Empty)) {
        if read_byte(tty, &wait, byte.as_mut())? && matches!(byte[0], b'\r' | b'\n' | 3 | 4 | 27) {
            return Err(CliError::Cancelled);
        }
    }
    Ok(())
}

struct TerminalGuard {
    terminal: File,
    original: Termios,
    restored: bool,
}

impl TerminalGuard {
    fn hide(terminal: &File) -> Result<Self, CliError> {
        let terminal = terminal.try_clone().map_err(|_| CliError::Terminal)?;
        let original = tcgetattr(&terminal).map_err(|_| CliError::Terminal)?;
        let mut hidden = original.clone();
        // Handle keyboard cancellation ourselves so a terminal-generated SIGINT
        // cannot skip Drop while echo is disabled. Keep output newline handling.
        hidden.local_modes.remove(
            LocalModes::ECHO
                | LocalModes::ECHONL
                | LocalModes::ICANON
                | LocalModes::ISIG
                | LocalModes::IEXTEN,
        );
        hidden
            .input_modes
            .remove(InputModes::IXON | InputModes::IXOFF);
        hidden.special_codes[SpecialCodeIndex::VMIN] = 1;
        hidden.special_codes[SpecialCodeIndex::VTIME] = 0;
        let guard = Self {
            terminal,
            original,
            restored: false,
        };
        // Flush queued input before displaying a prompt; on every handled exit,
        // also flush unread paste bytes so they cannot become shell commands.
        tcsetattr(&guard.terminal, OptionalActions::Flush, &hidden)
            .map_err(|_| CliError::Terminal)?;
        Ok(guard)
    }

    fn restore(mut self) -> Result<(), CliError> {
        tcsetattr(&self.terminal, OptionalActions::Flush, &self.original)
            .map_err(|_| CliError::Terminal)?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.restored {
            let _ = tcsetattr(&self.terminal, OptionalActions::Flush, &self.original);
        }
    }
}

fn prompt_hidden(tty: &mut File, prompt: &str) -> Result<Zeroizing<String>, CliError> {
    let guard = TerminalGuard::hide(tty)?;
    write!(tty, "{prompt}").map_err(|_| CliError::Terminal)?;
    tty.flush().map_err(|_| CliError::Terminal)?;
    let result = read_hidden(tty, INPUT_TIMEOUT);
    let restoration = guard.restore();
    let newline = writeln!(tty).map_err(|_| CliError::Terminal);
    restoration?;
    newline?;
    result
}

fn read_hidden(tty: &mut File, timeout: Duration) -> Result<Zeroizing<String>, CliError> {
    let deadline = Instant::now() + timeout;
    let mut input = HiddenInput::new();
    let mut byte = Zeroizing::new([0_u8; 1]);
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(CliError::InputTimeout)?;
        let wait = Timespec::try_from(remaining).map_err(|_| CliError::Terminal)?;
        if read_byte(tty, &wait, byte.as_mut())? && input.push(byte[0])? {
            return input.finish();
        }
    }
}

fn read_byte(tty: &mut File, wait: &Timespec, byte: &mut [u8]) -> Result<bool, CliError> {
    let mut descriptors = [PollFd::new(tty, PollFlags::IN)];
    match poll(&mut descriptors, Some(wait)) {
        Ok(0) | Err(rustix::io::Errno::INTR) => return Ok(false),
        Ok(_) => {}
        Err(_) => return Err(CliError::Terminal),
    }
    let ready = descriptors[0].revents();
    if ready.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL)
        || !ready.contains(PollFlags::IN)
    {
        return Err(CliError::Terminal);
    }
    match tty.read(byte) {
        Ok(1) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(false),
        _ => Err(CliError::Terminal),
    }
}

/// Fixed-capacity secret input avoids reallocation copies; deleted bytes are
/// overwritten, and retained bytes are zeroed on all handled exits.
struct HiddenInput {
    bytes: Zeroizing<Vec<u8>>,
}

impl HiddenInput {
    fn new() -> Self {
        Self {
            bytes: Zeroizing::new(Vec::with_capacity(MAX_INPUT_BYTES)),
        }
    }

    fn push(&mut self, byte: u8) -> Result<bool, CliError> {
        match byte {
            b'\n' | b'\r' => return Ok(true),
            0x03 | 0x04 | 0x1b => return Err(CliError::Cancelled),
            0x08 | 0x7f => {
                if let Some(index) = self.bytes.len().checked_sub(1) {
                    self.bytes[index] = 0;
                    self.bytes.truncate(index);
                }
            }
            0x20..=0x7e if self.bytes.len() < MAX_INPUT_BYTES => self.bytes.push(byte),
            _ => return Err(CliError::InvalidInput),
        }
        Ok(false)
    }

    fn finish(self) -> Result<Zeroizing<String>, CliError> {
        if self.bytes.is_empty() {
            return Err(CliError::Cancelled);
        }
        // Every byte was checked above; do not create an error carrying input.
        let input = std::str::from_utf8(&self.bytes).map_err(|_| CliError::InvalidInput)?;
        Ok(Zeroizing::new(input.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_port_requires_a_non_privileged_decimal_port() {
        assert_eq!(parse_port("1024"), Ok(1024));
        assert_eq!(parse_port("65535"), Ok(65535));
        for port in ["", "0", "1023", "65536", "+1024", "127.0.0.1:1024"] {
            assert_eq!(parse_port(port), Err(CliError::InvalidPort));
        }
    }

    #[test]
    fn hidden_input_accepts_ascii_and_edits_without_echo_or_retained_deleted_byte() {
        let mut input = HiddenInput::new();
        for byte in b"120" {
            assert!(!input.push(*byte).unwrap());
        }
        assert!(!input.push(0x7f).unwrap());
        assert!(!input.push(b'3').unwrap());
        assert!(input.push(b'\n').unwrap());
        assert_eq!(input.finish().unwrap().as_str(), "123");
    }

    #[test]
    fn hidden_input_cancel_and_invalid_data_do_not_include_input_in_errors() {
        for byte in [0x03, 0x04, 0x1b] {
            let mut input = HiddenInput::new();
            input.push(b'7').unwrap();
            assert_eq!(input.push(byte), Err(CliError::Cancelled));
        }
        for byte in [0x00, 0x16, 0x80, 0xff] {
            assert_eq!(HiddenInput::new().push(byte), Err(CliError::InvalidInput));
        }
        assert_eq!(HiddenInput::new().finish(), Err(CliError::Cancelled));
        assert!(!CliError::InvalidInput.to_string().contains('7'));
    }

    #[test]
    fn hidden_input_is_bounded_without_reallocation() {
        let mut input = HiddenInput::new();
        let original_capacity = input.bytes.capacity();
        for _ in 0..MAX_INPUT_BYTES {
            input.push(b'1').unwrap();
        }
        assert_eq!(input.bytes.capacity(), original_capacity);
        assert_eq!(input.push(b'2'), Err(CliError::InvalidInput));
    }

    #[test]
    fn hidden_input_preserves_a_complete_opaque_redirect_without_normalizing_it() {
        // A synthetic URL; no real browser, identity or authorization grant.
        let redirect = "https://oauth.yandex.ru/verification_code?state=fixture-state&code=OpaqueAlpha9%2B%2F%3D%26&cid=fixture-cid";
        let mut input = HiddenInput::new();
        for byte in redirect.bytes() {
            assert!(!input.push(byte).unwrap());
        }
        assert!(input.push(b'\r').unwrap());
        assert_eq!(input.finish().unwrap().as_str(), redirect);
    }
}
