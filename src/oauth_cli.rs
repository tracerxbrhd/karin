// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicitly local, interactive OAuth experiment. The GTK application never calls it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use gtk::gio;
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
    #[error(
        "OAuth diagnostic requires a local controlling terminal; no credential input was read."
    )]
    NoTerminal,
    #[error(
        "Cannot use or restore the local terminal. If needed, run `stty sane` in that terminal."
    )]
    Terminal,
    #[error("OAuth diagnostic cancelled locally.")]
    Cancelled,
    #[error("Local input timed out; no authorization code was exchanged.")]
    InputTimeout,
    #[error(
        "Invalid local input; enter only the Client ID or one-time authorization code requested."
    )]
    InvalidInput,
    #[error("Cannot initialize the OAuth diagnostic. Check the local Karin Client ID.")]
    Configuration,
    #[error("Cannot open the system browser; no authorization code was exchanged.")]
    Browser,
    #[error("Cannot write the anonymized diagnostic report.")]
    Output,
}

/// Called only for the exact `--oauth-diagnostic` command, before GTK starts.
/// Credentials are accepted exclusively from `/dev/tty`, never stdin/arguments.
pub fn run() -> Result<bool, CliError> {
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| CliError::NoTerminal)?;
    tcgetattr(&tty).map_err(|_| CliError::NoTerminal)?;
    writeln!(
        tty,
        "Karin: local opt-in OAuth diagnostic.\n\
         Use only your own Karin user-authorization application's Client ID.\n\
         Never enter a client secret, password, OAuth token or browser URL.\n\
         Tokens remain in memory for this run. No music streams are requested.\n\
         The debug code page does not return observable state to this tool;\n\
         returned state will be reported as unverified. PKCE binds the code exchange.\n\
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
         Enter only the seven-digit one-time authorization code from Yandex's verification page."
    )
    .map_err(|_| CliError::Terminal)?;
    let code = prompt_hidden(&mut tty, "One-time authorization code (7 digits): ")?;
    // Restore the terminal before any network operation. No raw transport error
    // or native browser error is printed; only the backend's sanitized report.
    let report = attempt.complete(AuthorizationResponse::ManualCode(code));
    let passed = report.passed();
    writeln!(std::io::stdout().lock(), "{report}").map_err(|_| CliError::Output)?;
    Ok(passed)
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
        let mut descriptors = [PollFd::new(tty, PollFlags::IN)];
        match poll(&mut descriptors, Some(&wait)) {
            Ok(0) => return Err(CliError::InputTimeout),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return Err(CliError::Terminal),
        }
        let ready = descriptors[0].revents();
        if ready.intersects(PollFlags::ERR | PollFlags::HUP | PollFlags::NVAL) {
            return Err(CliError::Terminal);
        }
        if !ready.contains(PollFlags::IN) {
            return Err(CliError::Terminal);
        }
        match tty.read(byte.as_mut()) {
            Ok(1) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            _ => return Err(CliError::Terminal),
        }
        if input.push(byte[0])? {
            return input.finish();
        }
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
}
