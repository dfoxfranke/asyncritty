// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Child-side fixture for the UTF-8 canonical-erase integration test.

use std::io::{self, Read as _, Write};
use std::process::ExitCode;

use rustix_openpty::rustix::termios::{InputModes, LocalModes, OptionalActions, SpecialCodeIndex};

/// Token emitted after the terminal is ready to receive the test sequence.
const READY_TOKEN: &[u8] = b"READY";

/// Token emitted when canonical input contains only the expected newline.
const OK_TOKEN: &[u8] = b"OK";

/// Runs the canonical-input protocol and exposes only success or failure as a
/// process status.
fn main() -> ExitCode {
    if std::env::args_os().nth(1).is_some() {
        return ExitCode::FAILURE;
    }

    if run_utf8_canonical_erase().is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Configures canonical erase, signals readiness, and requires the first
/// delivered input byte to be a newline.
///
/// # Errors
///
/// Returns an error if terminal configuration or standard-stream I/O fails, or
/// if canonical input retains any byte from the erased character.
fn run_utf8_canonical_erase() -> io::Result<()> {
    let stdin = io::stdin();
    let mut attributes = rustix_openpty::rustix::termios::tcgetattr(&stdin)?;
    attributes
        .input_modes
        .remove(InputModes::ISTRIP | InputModes::INLCR);
    attributes.local_modes.insert(LocalModes::ICANON);
    attributes.special_codes[SpecialCodeIndex::VERASE] = 0x7f;
    rustix_openpty::rustix::termios::tcsetattr(&stdin, OptionalActions::Now, &attributes)?;

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    write_token(&mut stdout, READY_TOKEN)?;

    let mut byte = [0_u8];
    stdin.lock().read_exact(&mut byte)?;
    if byte != *b"\n" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "canonical erase retained a byte from the UTF-8 character",
        ));
    }

    write_token(&mut stdout, OK_TOKEN)
}

/// Writes a protocol token immediately, without waiting for a line-buffering
/// boundary.
///
/// # Errors
///
/// Returns an error if the complete token cannot be written and flushed.
fn write_token(output: &mut impl Write, token: &[u8]) -> io::Result<()> {
    output.write_all(token)?;
    output.flush()
}
