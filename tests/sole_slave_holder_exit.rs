// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Unix regression coverage for child-exit suppression after the final PTY
//! slave descriptor closes.
//!
//! This scenario must not share a process with tests that spawn other children.
//! Libtest normally runs the tests belonging to one integration-test executable
//! on concurrent threads. If one thread owns this test's slave descriptor while
//! another thread forks, the second child inherits that descriptor along with
//! the rest of the process descriptor table. `CLOEXEC` does not close the
//! inherited copy until that child successfully replaces its process image,
//! so it remains a real slave holder throughout the fork-to-exec interval.
//!
//! If this test's child exits during that interval, its exit status becomes
//! waitable while the inherited descriptor still keeps the PTY alive. Reporting
//! `child_exit` is correct in that state: the direct child exited, but the
//! terminal genuinely outlived it. When the unrelated child later successfully
//! executes its program, `CLOEXEC` releases its copy and the master finally
//! observes HUP. A test that assumes the direct child was the sole holder would
//! therefore fail, even though delivering the child-exit notification in such a
//! circumstance is correct production behavior.
//!
//! On systems where `openpty` cannot set `CLOEXEC` atomically, isolation also
//! prevents an unrelated fork between descriptor creation and the later
//! `fcntl`. A descriptor inherited during that interval would remain open even
//! after a successful `exec`.
//!
//! Cargo builds each top-level file under `tests/` as a separate executable.
//! Keeping this scenario alone in this file gives it a separate descriptor
//! table, so children created by unrelated test executables cannot inherit its
//! slave.

mod common;

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::process::ExitStatus;
use std::task::{Context, Poll, Waker};

use asyncritty::alacritty_terminal::term::Config as TermConfig;
use asyncritty::{Child, Command, EventListener, EventLoop, Pty, WindowSize};
use tokio::sync::mpsc;

use common::ChildProcessGuard;

/// Records any child status delivered before PTY EOF completes the event loop.
#[derive(Clone)]
struct ChildExitListener {
    /// Channel observed without advancing Tokio's I/O driver.
    statuses: mpsc::Sender<ExitStatus>,
}

impl EventListener for ChildExitListener {
    type Error = Infallible;

    async fn child_exit(&self, status: ExitStatus) -> Result<(), Self::Error> {
        let _ = self.statuses.send(status).await;
        Ok(())
    }
}

/// Returns the cell and pixel geometry installed on the test PTY.
fn window_size() -> WindowSize {
    WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 8,
        cell_height: 16,
    }
}

/// Builds a command that evaluates `script` with the system's Bourne shell.
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

/// Reaps `child` without yielding control to the current-thread Tokio runtime.
///
/// This permits the OS to schedule the child while leaving reactor readiness
/// unpublished until the event loop receives its deliberate first poll.
///
/// # Panics
///
/// Panics if querying the child status fails.
fn reap_without_runtime_progress(child: &mut Child) -> ExitStatus {
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }

        std::thread::yield_now();
    }
}

/// Polls `future` once without waiting for or arranging a wakeup.
fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

/// Kernel-visible slave closure suppresses child-exit delivery before Tokio
/// publishes PTY read readiness.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn sole_slave_closure_suppresses_child_exit_before_read_readiness() {
    let mut pty = Pty::spawn(shell("printf x; exit 7"), window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);

    // There is deliberately no `.await` between spawning and the event loop's
    // first manual poll. The child writes one unread byte and exits while this
    // thread only calls `try_wait`; Tokio therefore has not had an opportunity
    // to publish the PTY's read readiness.
    let status = reap_without_runtime_progress(&mut pty.child);
    // The child and its process group no longer exist, so do not let a later
    // assertion failure risk signalling a recycled process-group identifier.
    child_guard.disarm();
    assert_eq!(status.code(), Some(7));

    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (statuses_tx, mut statuses_rx) = mpsc::channel(1);
    let (event_loop, _) =
        EventLoop::new(child, output, control, TermConfig::default(), move |_| {
            ChildExitListener {
                statuses: statuses_tx,
            }
        });
    let run = event_loop.run();
    tokio::pin!(run);

    // The cached child status is the only asynchronously ready branch. The
    // event loop must synchronously detect final-slave closure and suppress
    // `child_exit`, then remain pending on reactor-mediated PTY reading. Removing
    // that closure probe makes the listener send immediately during this same
    // manual poll.
    assert!(
        matches!(poll_once(run.as_mut()), Poll::Pending),
        "the first event-loop poll should suppress child exit and await PTY read readiness",
    );
    assert!(
        matches!(
            statuses_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ),
        "final-slave closure should suppress the child-exit callback"
    );

    let (mut child, _output, _control) = run.as_mut().await.unwrap();
    assert!(
        matches!(
            statuses_rx.try_recv(),
            Err(mpsc::error::TryRecvError::Disconnected)
        ),
        "the listener should close without a child-exit callback"
    );
    assert_eq!(
        child
            .try_wait()
            .unwrap()
            .expect("the child was reaped before the event loop started")
            .code(),
        Some(7),
    );
}
