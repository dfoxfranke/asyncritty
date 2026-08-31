// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! End-to-end coverage for UTF-8 canonical input on spawned PTYs.

use std::io;

use asyncritty::{Child, Command, Pty, WindowSize};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

/// Return the geometry installed on the test PTY.
fn window_size() -> WindowSize {
    WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 8,
        cell_height: 16,
    }
}

/// Wait until PTY output contains `expected`.
///
/// # Errors
///
/// Returns an error if reading fails or the child closes its output before
/// emitting the token.
async fn read_until_token(
    pty: &mut Pty,
    received: &mut Vec<u8>,
    expected: &[u8],
) -> io::Result<()> {
    if expected.is_empty() {
        return Ok(());
    }

    let mut buffer = [0_u8; 128];
    while !received
        .windows(expected.len())
        .any(|window| window == expected)
    {
        let count = pty.read(&mut buffer).await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "PTY output closed before the expected token",
            ));
        }
        received.extend_from_slice(&buffer[..count]);
    }
    Ok(())
}

/// Stop and reap a child that may still be running.
///
/// # Panics
///
/// Panics if querying, signaling, or waiting for the child fails.
async fn stop_child(child: &mut Child) {
    if child.try_wait().unwrap().is_none() {
        child.start_kill().unwrap();
    }
    child.wait().await.unwrap();
}

/// `Pty::spawn` produces a terminal whose canonical erase removes a complete
/// UTF-8 character.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn spawn_uses_utf8_aware_canonical_input() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_asyncritty-utf8-test-helper"));
    command.kill_on_drop(true);

    let mut pty = Pty::spawn(command, window_size()).unwrap();

    let outcome = async {
        let mut received = Vec::new();
        if let Err(error) = read_until_token(&mut pty, &mut received, b"READY").await {
            return Err((error, received));
        }
        if let Err(error) = pty.write_all(b"\xc3\xa9\x7f\n").await {
            return Err((error, received));
        }
        if let Err(error) = read_until_token(&mut pty, &mut received, b"OK").await {
            return Err((error, received));
        }
        match pty.child.wait().await {
            Ok(status) => Ok((status, received)),
            Err(error) => Err((error, received)),
        }
    }
    .await;

    let (status, received) = match outcome {
        Ok(result) => result,
        Err((error, received)) => {
            stop_child(&mut pty.child).await;
            panic!(
                "the UTF-8 canonical-input interaction should succeed: {error}; output: {:?}",
                String::from_utf8_lossy(&received),
            );
        }
    };

    assert!(
        status.success(),
        "the UTF-8 canonical-input child should exit successfully; output: {:?}",
        String::from_utf8_lossy(&received),
    );
}
