// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Process fixture that exercises PTY setup with descriptors one and two.

use std::fs::File;
use std::io;
use std::os::fd::AsRawFd as _;
use std::process::ExitCode;

use asyncritty::{Child, Command, Pty, WindowSize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Input sent to the shell through the PTY controller.
const ROUNDTRIP_INPUT: &[u8] = b"hello-pty\n";

/// Output proving that the shell's standard output reaches the controller.
const STDOUT_TOKEN: &[u8] = b"stdout:hello-pty";

/// Output proving that the shell's standard error reaches the controller.
const STDERR_TOKEN: &[u8] = b"stderr:hello-pty";

/// Maximum output retained while waiting for both response tokens.
const MAX_OUTPUT: usize = 8 * 1024;

/// Script that accepts the exact input line and identifies both output streams.
const ROUNDTRIP_SCRIPT: &str = "IFS= read -r line || exit 1; \
    [ \"$line\" = hello-pty ] || exit 1; \
    printf 'stdout:hello-pty\\n'; \
    printf 'stderr:hello-pty\\n' >&2";

/// Runs the low-descriptor scenario and exposes only success or failure as a
/// process status.
fn main() -> ExitCode {
    if std::env::args_os().nth(1).is_some() {
        return ExitCode::FAILURE;
    }

    match exercise_low_descriptors() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Builds the runtime before releasing the standard descriptor numbers, then
/// completes the PTY interaction on that runtime.
///
/// # Errors
///
/// Returns an error if the runtime cannot be built or any part of descriptor
/// setup, child execution, I/O, status collection, or cleanup fails.
fn exercise_low_descriptors() -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(exercise_low_descriptors_async())
}

/// Makes descriptors one and two the newly opened PTY pair and completes a
/// bidirectional exchange with a shell attached through `Pty::from_fds`.
///
/// # Errors
///
/// Returns an error if descriptor reuse does not match the required layout, PTY
/// setup or I/O fails, the response is incomplete, or the child cannot be
/// reaped after success or failure.
async fn exercise_low_descriptors_async() -> io::Result<()> {
    close_standard_descriptors()?;

    let stdin_reservation = File::open("/dev/null")?;
    require_descriptor(
        stdin_reservation.as_raw_fd(),
        libc::STDIN_FILENO,
        "standard-input reservation",
    )?;

    let descriptors = rustix_openpty::openpty(None, None)?;
    require_descriptor(
        descriptors.controller.as_raw_fd(),
        libc::STDOUT_FILENO,
        "PTY controller",
    )?;
    require_descriptor(
        descriptors.user.as_raw_fd(),
        libc::STDERR_FILENO,
        "PTY slave",
    )?;

    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(ROUNDTRIP_SCRIPT).kill_on_drop(true);
    let mut pty = Pty::from_fds(
        command,
        window_size(),
        descriptors.controller,
        descriptors.user,
    )?;

    if let Err(exchange_error) = exchange(&mut pty).await {
        terminate_child(&mut pty.child).await?;
        return Err(exchange_error);
    }

    Ok(())
}

/// Closes inherited standard descriptors exactly once so their numbers become
/// available for deterministic reuse.
///
/// Retrying an interrupted close is intentionally avoided because the original
/// descriptor may already have been released.
///
/// # Errors
///
/// Returns the first close error reported by the operating system.
fn close_standard_descriptors() -> io::Result<()> {
    for descriptor in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY: The helper is single-threaded at this point, no Rust owner
        // controls the inherited standard descriptors, and every descriptor is
        // passed to `close` exactly once. `close` has no pointer-validity or
        // memory-validity requirements.
        if unsafe { libc::close(descriptor) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Requires an allocation to use the descriptor number that establishes the
/// scenario under test.
///
/// # Errors
///
/// Returns an error when `actual` differs from `expected`.
fn require_descriptor(actual: i32, expected: i32, role: &str) -> io::Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "expected {role} fd {expected}, received fd {actual}"
        )))
    }
}

/// Exchanges one input line with the shell and observes both response streams.
///
/// # Errors
///
/// Returns an error if PTY I/O fails, output ends or exceeds the retention bound
/// before both tokens arrive, or the child exits unsuccessfully.
async fn exchange(pty: &mut Pty) -> io::Result<()> {
    pty.write_all(ROUNDTRIP_INPUT).await?;

    let mut received = Vec::new();
    let mut buffer = [0_u8; 256];
    while !contains(&received, STDOUT_TOKEN) || !contains(&received, STDERR_TOKEN) {
        let count = pty.read(&mut buffer).await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PTY closed before both response tokens arrived",
            ));
        }

        received.extend_from_slice(&buffer[..count]);
        if received.len() > MAX_OUTPUT {
            return Err(io::Error::other(format!(
                "shell output exceeded {MAX_OUTPUT} bytes"
            )));
        }
    }

    let status = pty.child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("shell exited with {status}")))
    }
}

/// Returns whether `stream` contains `token` as a contiguous byte sequence.
fn contains(stream: &[u8], token: &[u8]) -> bool {
    token.is_empty() || stream.windows(token.len()).any(|window| window == token)
}

/// Terminates and reaps a child after an unsuccessful exchange.
///
/// # Errors
///
/// Returns an error if the child's status cannot be queried or a running child
/// cannot be terminated and reaped.
async fn terminate_child(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_none() {
        child.kill().await?;
    }
    Ok(())
}

/// Returns the geometry to install on the low-numbered PTY.
fn window_size() -> WindowSize {
    WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 8,
        cell_height: 16,
    }
}
