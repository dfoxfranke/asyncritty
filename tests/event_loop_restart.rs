// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Repeated runs retain terminal state and reconcile externally applied sizes.

mod common;

use std::convert::Infallible;
use std::fs::File;
use std::future::pending;
use std::io::{self, Write as _};
use std::sync::Arc;

use asyncritty::alacritty_terminal::grid::Dimensions as _;
use asyncritty::alacritty_terminal::index::Point;
use asyncritty::{
    Command, EventListener, EventLoop, EventLoopError, EventLoopHandle, Pty, WindowSize,
};
use tokio::sync::Notify;

use common::ChildProcessGuard;

/// Create a live PTY with output controlled directly by the test.
///
/// # Panics
///
/// Panics if PTY allocation, descriptor duplication, or child creation fails.
fn fixture() -> (Pty, File, ChildProcessGuard) {
    let descriptors = rustix_openpty::openpty(None, None).unwrap();
    let slave = File::from(descriptors.user.try_clone().unwrap());
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exec sleep 30"]);
    let pty = Pty::from_fds(
        command,
        WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        },
        descriptors.controller,
        descriptors.user,
    )
    .unwrap();
    let guard = ChildProcessGuard::for_pty(&pty);
    (pty, slave, guard)
}

/// Retains callback arguments in ordinary mutable fields, then stops each run.
#[derive(Default)]
struct FailingListener {
    /// Titles delivered across all runs using this listener.
    titles: Vec<String>,
    /// Successful sizes reported across all runs using this listener.
    sizes: Vec<WindowSize>,
}

impl EventListener for FailingListener {
    type Error = io::Error;

    async fn title(&mut self, title: String) -> Result<(), Self::Error> {
        self.titles.push(title);
        Err(io::Error::other("stop after title"))
    }

    async fn resize_result(&mut self, result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        self.sizes.push(result?);
        Err(io::Error::other("stop after resize"))
    }
}

/// Records a title and requests successful completion of the current run.
struct StoppingListener {
    /// Handle used to stop processing from inside the callback.
    handle: EventLoopHandle,
    /// Title received by this listener.
    title: Option<String>,
    /// Resize results delivered before the stopping title.
    sizes: Vec<WindowSize>,
}

impl EventListener for StoppingListener {
    type Error = Infallible;

    async fn title(&mut self, title: String) -> Result<(), Self::Error> {
        self.title = Some(title);
        self.handle.shutdown();
        Ok(())
    }

    async fn resize_result(&mut self, result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        if let Ok(size) = result {
            self.sizes.push(size);
        }
        Ok(())
    }
}

/// Every run reconciles changed dimensions before reading output, including
/// changes before the first run and changes only to cell pixel dimensions.
/// Unchanged dimensions do not produce duplicate resize results.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn each_run_reconciles_control_size_before_reading() {
    let (mut pty, mut slave, mut child_guard) = fixture();
    let (mut event_loop, handle) = EventLoop::new(Default::default(), &pty.control);
    let mut listener = FailingListener::default();
    let mut expected = WindowSize {
        num_lines: 31,
        num_cols: 97,
        cell_width: 9,
        cell_height: 18,
    };

    for change in 0..5 {
        match change {
            1 => expected.num_lines += 1,
            2 => expected.num_cols += 1,
            3 => expected.cell_width += 1,
            4 => expected.cell_height += 1,
            _ => {}
        }
        pty.control.resize(expected).unwrap();
        slave.write_all(b"\x1b]2;after-resize\x07").unwrap();
        let failure = event_loop
            .run(&mut pty.control, &mut pty.output, &mut listener)
            .await
            .unwrap_err();
        assert!(matches!(failure, EventLoopError::Listener(_)));
        handle.wait_for_shutdown().await;
        assert_eq!(listener.titles.len(), change);
        assert_eq!(listener.sizes.len(), change + 1);
        let actual = listener.sizes[change];
        assert_eq!(actual.num_lines, expected.num_lines);
        assert_eq!(actual.num_cols, expected.num_cols);
        assert_eq!(actual.cell_width, expected.cell_width);
        assert_eq!(actual.cell_height, expected.cell_height);
        {
            let terminal = handle.terminal().await;
            assert_eq!(terminal.screen_lines(), usize::from(expected.num_lines));
            assert_eq!(terminal.columns(), usize::from(expected.num_cols));
        }

        let failure = event_loop
            .run(&mut pty.control, &mut pty.output, &mut listener)
            .await
            .unwrap_err();
        assert!(matches!(failure, EventLoopError::Listener(_)));
        assert_eq!(listener.titles.len(), change + 1);
        assert_eq!(listener.titles[change], "after-resize");
        assert_eq!(listener.sizes.len(), change + 1);
    }

    pty.child.kill().await.unwrap();
    child_guard.disarm();
}

/// Callback ordering and the model dimensions visible during each callback.
#[derive(Debug, PartialEq, Eq)]
enum ResizeObservation {
    /// A wakeup and the model dimensions it observes.
    Wakeup(usize, usize),
    /// A resize result's dimensions followed by the model dimensions it observes.
    Resize(u16, u16, usize, usize),
}

/// Records resize callbacks and stops at test-selected points in delivery.
struct OrderedResizeListener {
    /// Terminal access and orderly shutdown for the current run.
    handle: EventLoopHandle,
    /// Callback order and dimensions observed across runs.
    observations: Vec<ResizeObservation>,
    /// Whether wakeups stop the current run with an error.
    fail_wakeup: bool,
    /// Whether wakeups request orderly shutdown instead of returning an error.
    shutdown_wakeup: bool,
    /// Whether resize results stop with an error instead of orderly shutdown.
    fail_resize: bool,
    /// Row count whose successful resize result requests shutdown.
    stop_at_lines: u16,
}

impl EventListener for OrderedResizeListener {
    type Error = io::Error;

    async fn wakeup(&mut self) -> Result<(), Self::Error> {
        let terminal = self.handle.terminal().await;
        self.observations.push(ResizeObservation::Wakeup(
            terminal.screen_lines(),
            terminal.columns(),
        ));
        if self.shutdown_wakeup {
            self.handle.shutdown();
            Ok(())
        } else if self.fail_wakeup {
            Err(io::Error::other("stop before the resize result"))
        } else {
            Ok(())
        }
    }

    async fn resize_result(&mut self, result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        let size = result?;
        let terminal = self.handle.terminal().await;
        self.observations.push(ResizeObservation::Resize(
            size.num_lines,
            size.num_cols,
            terminal.screen_lines(),
            terminal.columns(),
        ));
        if self.fail_resize {
            Err(io::Error::other(
                "stop while draining the queued resize result",
            ))
        } else {
            if size.num_lines == self.stop_at_lines {
                self.handle.shutdown();
            }
            Ok(())
        }
    }
}

/// A queued resize result observes the old model before a startup resize.
/// Failure while draining defers resizing until a later run, which rechecks
/// the latest control dimensions rather than applying an earlier snapshot.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn queued_events_precede_resize_even_after_another_listener_error() {
    for shutdown in [false, true] {
        let (mut pty, _slave, mut child_guard) = fixture();
        let (mut event_loop, handle) = EventLoop::new(Default::default(), &pty.control);
        let mut listener = OrderedResizeListener {
            handle: handle.clone(),
            observations: Vec::new(),
            fail_wakeup: true,
            shutdown_wakeup: shutdown,
            fail_resize: true,
            stop_at_lines: 42,
        };
        drop(handle.terminal().await);
        let mut size = WindowSize {
            num_lines: 31,
            num_cols: 97,
            cell_width: 8,
            cell_height: 16,
        };
        handle.resize(size);
        let result = event_loop
            .run(&mut pty.control, &mut pty.output, &mut listener)
            .await;
        if shutdown {
            result.unwrap();
        } else {
            assert!(matches!(result, Err(EventLoopError::Listener(_))));
        }
        assert_eq!(listener.observations, [ResizeObservation::Wakeup(31, 97)]);

        size.num_lines = 40;
        pty.control.resize(size).unwrap();
        assert!(matches!(
            event_loop
                .run(&mut pty.control, &mut pty.output, &mut listener)
                .await,
            Err(EventLoopError::Listener(_))
        ));
        assert_eq!(
            listener.observations,
            [
                ResizeObservation::Wakeup(31, 97),
                ResizeObservation::Resize(31, 97, 31, 97),
            ]
        );
        {
            let terminal = handle.terminal().await;
            assert_eq!(terminal.screen_lines(), 31);
        }

        size.num_lines = 42;
        pty.control.resize(size).unwrap();
        listener.fail_wakeup = false;
        listener.shutdown_wakeup = false;
        listener.fail_resize = false;
        event_loop
            .run(&mut pty.control, &mut pty.output, &mut listener)
            .await
            .unwrap();
        assert_eq!(
            listener.observations,
            [
                ResizeObservation::Wakeup(31, 97),
                ResizeObservation::Resize(31, 97, 31, 97),
                ResizeObservation::Wakeup(42, 97),
                ResizeObservation::Resize(42, 97, 42, 97),
            ]
        );

        pty.child.kill().await.unwrap();
        child_guard.disarm();
    }
}

/// Successfully draining an old resize result is followed by the startup
/// resize in the same run, with each callback seeing its corresponding model.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn startup_resize_follows_successfully_drained_events() {
    let (mut pty, _slave, mut child_guard) = fixture();
    let (mut event_loop, handle) = EventLoop::new(Default::default(), &pty.control);
    let mut listener = OrderedResizeListener {
        handle: handle.clone(),
        observations: Vec::new(),
        fail_wakeup: true,
        shutdown_wakeup: false,
        fail_resize: false,
        stop_at_lines: 40,
    };
    drop(handle.terminal().await);
    let mut size = WindowSize {
        num_lines: 31,
        num_cols: 97,
        cell_width: 8,
        cell_height: 16,
    };
    handle.resize(size);
    assert!(matches!(
        event_loop
            .run(&mut pty.control, &mut pty.output, &mut listener)
            .await,
        Err(EventLoopError::Listener(_))
    ));

    size.num_lines = 40;
    pty.control.resize(size).unwrap();
    listener.fail_wakeup = false;
    event_loop
        .run(&mut pty.control, &mut pty.output, &mut listener)
        .await
        .unwrap();
    assert_eq!(
        listener.observations,
        [
            ResizeObservation::Wakeup(31, 97),
            ResizeObservation::Resize(31, 97, 31, 97),
            ResizeObservation::Wakeup(40, 97),
            ResizeObservation::Resize(40, 97, 40, 97),
        ]
    );

    pty.child.kill().await.unwrap();
    child_guard.disarm();
}

/// Listener errors preserve screen contents and incomplete parser input, and
/// the next run can use a listener with a different error type.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn listener_failure_preserves_state_for_another_listener() {
    let (mut pty, mut slave, mut child_guard) = fixture();
    let (mut event_loop, handle) = EventLoop::new(Default::default(), &pty.control);
    let mut listener = FailingListener::default();
    slave
        .write_all(b"retained screen\x1b]2;fail\x07\x1b]2;resumed")
        .unwrap();
    let failure = event_loop
        .run(&mut pty.control, &mut pty.output, &mut listener)
        .await
        .unwrap_err();
    assert!(matches!(failure, EventLoopError::Listener(_)));
    assert_eq!(listener.titles, ["fail"]);
    assert!(listener.sizes.is_empty());

    slave.write_all(b"\x07").unwrap();
    let mut replacement = StoppingListener {
        handle: handle.clone(),
        title: None,
        sizes: Vec::new(),
    };
    event_loop
        .run(&mut pty.control, &mut pty.output, &mut replacement)
        .await
        .unwrap();
    assert_eq!(replacement.title.as_deref(), Some("resumed"));
    handle.wait_for_shutdown().await;
    {
        let terminal = handle.terminal().await;
        let screen = terminal.bounds_to_string(
            Point::default(),
            Point::new(terminal.bottommost_line(), terminal.last_column()),
        );
        assert!(screen.contains("retained screen"));
    }

    pty.child.kill().await.unwrap();
    child_guard.disarm();
}

/// Records callback entry before waiting for cancellation.
struct PendingListener {
    /// Notification awaited by the test before cancelling the run.
    entered: Arc<Notify>,
    /// Number of admitted wakeup callbacks.
    calls: usize,
}

impl EventListener for PendingListener {
    type Error = Infallible;

    async fn wakeup(&mut self) -> Result<(), Self::Error> {
        self.calls += 1;
        self.entered.notify_one();
        pending().await
    }
}

/// Cancelling a borrowed run completes shutdown waits and retains its listener,
/// PTY capabilities, and queued events for another run.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn cancelling_run_retains_borrowed_capabilities() {
    let (mut pty, mut slave, mut child_guard) = fixture();
    let (mut event_loop, handle) = EventLoop::new(Default::default(), &pty.control);
    let entered = Arc::new(Notify::new());
    let mut listener = PendingListener {
        entered: Arc::clone(&entered),
        calls: 0,
    };
    drop(handle.terminal().await);
    handle.resize(WindowSize {
        num_lines: 31,
        num_cols: 97,
        cell_width: 8,
        cell_height: 16,
    });
    tokio::select! {
        _ = entered.notified() => {}
        result = event_loop.run(&mut pty.control, &mut pty.output, &mut listener) => {
            panic!("the pending callback should prevent completion: {result:?}");
        }
    }
    handle.wait_for_shutdown().await;
    assert_eq!(listener.calls, 1);

    slave.write_all(b"\x1b]2;after-cancellation\x07").unwrap();
    let mut replacement = StoppingListener {
        handle: handle.clone(),
        title: None,
        sizes: Vec::new(),
    };
    event_loop
        .run(&mut pty.control, &mut pty.output, &mut replacement)
        .await
        .unwrap();
    assert_eq!(replacement.title.as_deref(), Some("after-cancellation"));
    assert_eq!(replacement.sizes.len(), 1);
    assert_eq!(replacement.sizes[0].num_lines, 31);
    assert_eq!(replacement.sizes[0].num_cols, 97);

    pty.child.kill().await.unwrap();
    child_guard.disarm();
}
