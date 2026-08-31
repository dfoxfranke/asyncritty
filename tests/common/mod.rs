// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Process cleanup shared by the Unix integration-test crates.

use std::io;

use asyncritty::Pty;

/// Kills a PTY child’s process group unless normal cleanup disarms it.
///
/// `Pty` makes its child a session leader, so the child’s process ID also
/// identifies the process group containing any shell descendants created by a
/// test script. Dropping the guard panics if an existing group cannot be
/// signaled, except when the thread is already unwinding.
#[must_use = "dropping the guard immediately kills the test child"]
pub struct ChildProcessGuard {
    /// Session process group to kill while the guard remains armed.
    process_group: Option<libc::pid_t>,
}

impl ChildProcessGuard {
    /// Arms cleanup for the child currently owned by `pty`.
    ///
    /// # Panics
    ///
    /// Panics if the child no longer has a process ID or if its process ID
    /// cannot be represented as a positive [`libc::pid_t`].
    pub fn for_pty(pty: &Pty) -> Self {
        let process_id = pty
            .child
            .id()
            .expect("the guard is constructed before the PTY child is reaped");
        let process_group = libc::pid_t::try_from(process_id)
            .expect("the child ID originated as a pid_t process identifier");
        assert!(
            process_group > 0,
            "test child process ID should be positive"
        );

        Self {
            process_group: Some(process_group),
        }
    }

    /// Disarms emergency cleanup after the child group has exited and its child
    /// handle has been reaped.
    ///
    /// Calling this before normal cleanup finishes forfeits the guard’s process
    /// cleanup.
    pub fn disarm(&mut self) {
        self.process_group = None;
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        let Some(process_group) = self.process_group else {
            return;
        };

        // SAFETY: `process_group` is a positive `pid_t` returned for the PTY
        // child, negation therefore remains representable, `SIGKILL` is a valid
        // signal number, and `kill` receives no pointers.
        let result = unsafe { libc::kill(-process_group, libc::SIGKILL) };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) && !std::thread::panicking() {
                panic!("the test child process group should accept `SIGKILL`: {error}");
            }
        }
    }
}
