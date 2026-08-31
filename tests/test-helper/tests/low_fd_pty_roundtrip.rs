// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! End-to-end coverage for PTY setup with low-numbered descriptors.

use std::process::{Command, Stdio};

/// PTY setup remains functional when the lowest descriptors are available for
/// reuse.
#[test]
#[ntest::timeout(15_000)]
fn low_fd_pty_roundtrip() {
    let status = Command::new(env!("CARGO_BIN_EXE_asyncritty-low-fd-test-helper"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert!(
        status.success(),
        "the low-descriptor PTY helper exited with {status}"
    );
}
