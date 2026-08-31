// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! End-to-end contracts for PTY I/O, event-loop lifecycle, and terminal event
//! delivery.

mod common;

use std::convert::Infallible;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::future::pending;
use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use asyncritty::alacritty_terminal::grid::Dimensions as _;
use asyncritty::alacritty_terminal::term::Config as TermConfig;
use asyncritty::{
    Child, Command, EventListener, EventLoop, EventLoopError, EventLoopHandle, Pty, WindowSize,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::{Notify, mpsc};
use tokio::time::sleep;

use common::ChildProcessGuard;

/// Return the cell and pixel geometry installed on each test PTY.
fn window_size() -> WindowSize {
    WindowSize {
        num_lines: 24,
        num_cols: 80,
        cell_width: 8,
        cell_height: 16,
    }
}

/// Build a command that evaluates `script` with the system's Bourne shell.
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

/// Stop and reap a test child that may still be running.
///
/// # Panics
///
/// Panics if child status, signaling, or waiting fails.
async fn stop_child(child: &mut Child) {
    if child.try_wait().unwrap().is_none() {
        child.start_kill().unwrap();
    }
    child.wait().await.unwrap();
}

/// Wait until `path` exists.
async fn wait_for_path(path: &Path) {
    while !path.exists() {
        sleep(Duration::from_millis(10)).await;
    }
}

/// The aggregate writes PTY input and reads output from both child output
/// streams.
async fn assert_pty_roundtrip(mut pty: Pty) {
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    pty.write_all(b"hello-pty\n").await.unwrap();

    let mut received = Vec::new();
    let mut buffer = [0_u8; 128];
    while ![
        b"stdout:hello-pty".as_slice(),
        b"stderr:hello-pty".as_slice(),
    ]
    .into_iter()
    .all(|expected| {
        received
            .windows(expected.len())
            .any(|window| window == expected)
    }) {
        let count = pty.read(&mut buffer).await.unwrap();
        assert_ne!(
            count, 0,
            "the PTY should remain open until the response arrives"
        );
        received.extend_from_slice(&buffer[..count]);
    }

    let status = pty.child.wait().await.unwrap();
    assert!(status.success(), "the PTY child should exit successfully");
    child_guard.disarm();
}

/// `Pty` forwards Tokio reads and writes to its directional capabilities.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn async_pty_roundtrip() {
    let pty = Pty::spawn(
        shell(
            "IFS= read -r line; \
             printf 'stdout:%s\\n' \"$line\"; \
             printf 'stderr:%s\\n' \"$line\" >&2",
        ),
        window_size(),
    )
    .unwrap();
    assert_pty_roundtrip(pty).await;
}

/// The name returned by `Pty::spawn` opens the slave device whose output is
/// read through the aggregate.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn spawned_pty_name_identifies_slave() {
    let mut pty = Pty::spawn(shell("exec sleep 30"), window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let mut slave = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(&pty.name)
        .unwrap();

    let message = b"named-slave-output";
    std::io::Write::write_all(&mut slave, message).unwrap();
    let mut received = [0; 18];
    pty.read_exact(&mut received).await.unwrap();
    assert_eq!(&received, message);

    pty.kill().await.unwrap();
    child_guard.disarm();
}

/// Killing a PTY child waits for termination and retains its exit status.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn child_kill_terminates_and_reaps() {
    let mut pty = Pty::spawn(shell("trap '' HUP; exec sleep 30"), window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);

    pty.child.kill().await.unwrap();
    assert!(
        pty.child.try_wait().unwrap().is_some(),
        "the killed child should retain its exit status",
    );
    child_guard.disarm();
}

/// `Pty::from_fds` records the supplied slave's device name and applies and
/// records its geometry for kernel and event-loop initialization.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn from_fds_initial_geometry() {
    let descriptors = rustix_openpty::openpty(None, None).unwrap();
    rustix_openpty::rustix::termios::tcsetwinsize(
        &descriptors.user,
        rustix_openpty::rustix::termios::Winsize {
            ws_row: 11,
            ws_col: 19,
            ws_xpixel: 23,
            ws_ypixel: 29,
        },
    )
    .unwrap();
    let observed_slave = descriptors.user.try_clone().unwrap();
    let expected = WindowSize {
        num_lines: 31,
        num_cols: 97,
        cell_width: 7,
        cell_height: 13,
    };
    let pty = Pty::from_fds(
        shell("trap '' HUP; exec sleep 30"),
        expected,
        descriptors.controller,
        descriptors.user,
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);

    let named_device = std::fs::metadata(&pty.name).unwrap();
    let supplied_device = std::fs::File::from(observed_slave.try_clone().unwrap())
        .metadata()
        .unwrap();
    assert_eq!(
        (named_device.dev(), named_device.ino(), named_device.rdev()),
        (
            supplied_device.dev(),
            supplied_device.ino(),
            supplied_device.rdev(),
        )
    );
    assert_kernel_window_size(&observed_slave, expected);
    drop(observed_slave);

    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            asyncritty::VoidListener
        });
    let terminal_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(
        terminal_size,
        (
            usize::from(expected.num_lines),
            usize::from(expected.num_cols)
        )
    );

    handle.shutdown();
    let (mut child, _output, _control) = event_loop.run().await.unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Event-loop construction derives terminal dimensions from a resize applied
/// through its `PtyControl` before ownership is transferred.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn event_loop_uses_resized_control_geometry() {
    let creation_size = WindowSize {
        num_lines: 17,
        num_cols: 53,
        cell_width: 7,
        cell_height: 11,
    };
    let resized = WindowSize {
        num_lines: 31,
        num_cols: 97,
        cell_width: 9,
        cell_height: 18,
    };
    let descriptors = rustix_openpty::openpty(None, None).unwrap();
    let observed_slave = descriptors.user.try_clone().unwrap();
    let mut pty = Pty::from_fds(
        shell("trap '' HUP; exec sleep 30"),
        creation_size,
        descriptors.controller,
        descriptors.user,
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    pty.control.resize(resized).unwrap();
    assert_kernel_window_size(&observed_slave, resized);

    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            asyncritty::VoidListener
        });
    let terminal_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(terminal_size, (31, 97));

    handle.shutdown();
    let (mut child, _output, _control) = event_loop.run().await.unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Stable test-only error used by fallible listener fixtures.
#[derive(Debug)]
struct ListenerFailure(&'static str);

impl Display for ListenerFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for ListenerFailure {}

/// Forwards parsed title changes to the observing test task.
#[derive(Clone)]
struct TitleListener {
    /// Channel carrying title text in listener-delivery order.
    titles: mpsc::Sender<String>,
}

impl EventListener for TitleListener {
    type Error = ListenerFailure;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        self.titles
            .send(title)
            .await
            .map_err(|_| ListenerFailure("title receiver closed"))
    }
}

/// Forwards completed resize operations to the observing test task.
#[derive(Clone)]
struct ResizeResultListener {
    /// Channel carrying resize outcomes in listener-delivery order.
    results: mpsc::UnboundedSender<io::Result<WindowSize>>,

    /// Channel carrying wakeups emitted before resize outcomes.
    wakeups: mpsc::UnboundedSender<()>,
}

impl EventListener for ResizeResultListener {
    type Error = Infallible;

    async fn wakeup(&self) -> Result<(), Self::Error> {
        let _ = self.wakeups.send(());
        Ok(())
    }

    async fn resize_result(&self, result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        let _ = self.results.send(result);
        Ok(())
    }
}

/// Assert equality of every cell and pixel dimension in two window sizes.
fn assert_window_size_eq(actual: WindowSize, expected: WindowSize) {
    assert_eq!(actual.num_lines, expected.num_lines);
    assert_eq!(actual.num_cols, expected.num_cols);
    assert_eq!(actual.cell_width, expected.cell_width);
    assert_eq!(actual.cell_height, expected.cell_height);
}

/// Assert the kernel geometry visible through an independently retained slave.
fn assert_kernel_window_size(slave: &OwnedFd, expected: WindowSize) {
    let actual = rustix_openpty::rustix::termios::tcgetwinsize(slave).unwrap();
    assert_eq!(actual.ws_row, expected.num_lines);
    assert_eq!(actual.ws_col, expected.num_cols);
    assert_eq!(
        actual.ws_xpixel,
        expected.num_cols.saturating_mul(expected.cell_width)
    );
    assert_eq!(
        actual.ws_ypixel,
        expected.num_lines.saturating_mul(expected.cell_height)
    );
}

/// Input is usable before event-loop construction and after event-loop
/// shutdown because its ownership is independent of the loop.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn input_is_independent_of_event_loop_lifetime() {
    let artifacts = tempfile::Builder::new()
        .prefix("asyncritty-independent-input-")
        .tempdir()
        .unwrap();
    let after_marker = artifacts.path().join("after");
    let mut command = shell(
        "IFS= read -r first; \
         printf '\\033]2;%s\\007' \"$first\"; \
         IFS= read -r second; \
         printf '%s' \"$second\" > \"$ASYNCRITTY_AFTER_MARKER\"; \
         trap '' HUP; \
         exec sleep 30",
    );
    command.env("ASYNCRITTY_AFTER_MARKER", &after_marker);
    let mut pty = Pty::spawn(command, window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);

    pty.input.write_all(b"before-loop\n").await.unwrap();
    let Pty {
        child,
        output,
        control,
        mut input,
        ..
    } = pty;
    let (titles_tx, mut titles_rx) = mpsc::channel(1);
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), move |_| {
            TitleListener { titles: titles_tx }
        });
    let task = tokio::spawn(event_loop.run());

    let title = titles_rx
        .recv()
        .await
        .expect("the running event loop retains the title sender");
    assert_eq!(title, "before-loop");
    handle.shutdown();
    let (mut child, _output, _control) = task.await.unwrap().unwrap();

    input.write_all(b"after-loop\n").await.unwrap();
    wait_for_path(&after_marker).await;
    assert_eq!(
        std::fs::read_to_string(&after_marker).unwrap(),
        "after-loop",
    );

    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Listener callbacks relevant to terminal-update ordering.
#[derive(Debug, PartialEq, Eq)]
enum ListenerObservation {
    /// Title text delivered by the terminal parser.
    Title(String),
    /// Notification that the terminal state changed.
    Wakeup,
}

/// Records title and wakeup callbacks in listener-delivery order.
#[derive(Clone)]
struct RecordingListener {
    /// Channel retaining observations after the event loop releases its listener.
    observations: mpsc::UnboundedSender<ListenerObservation>,
}

impl EventListener for RecordingListener {
    type Error = Infallible;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        let _ = self.observations.send(ListenerObservation::Title(title));
        Ok(())
    }

    async fn wakeup(&self) -> Result<(), Self::Error> {
        let _ = self.observations.send(ListenerObservation::Wakeup);
        Ok(())
    }
}

/// Records callbacks and observes terminal state after each wakeup.
#[derive(Clone)]
struct ReobservingRecordingListener {
    /// Channel retaining observations after the event loop releases its listener.
    observations: mpsc::UnboundedSender<ListenerObservation>,

    /// Handle used to acknowledge a wakeup before event delivery continues.
    handle: EventLoopHandle,
}

impl EventListener for ReobservingRecordingListener {
    type Error = Infallible;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        let _ = self.observations.send(ListenerObservation::Title(title));
        Ok(())
    }

    async fn wakeup(&self) -> Result<(), Self::Error> {
        let _ = self.observations.send(ListenerObservation::Wakeup);
        let _terminal = self.handle.terminal().await;
        Ok(())
    }
}

/// Receive the next title or wakeup callback.
///
/// # Panics
///
/// Panics if the listener is released first.
async fn next_listener_observation(
    observations: &mut mpsc::UnboundedReceiver<ListenerObservation>,
) -> ListenerObservation {
    observations
        .recv()
        .await
        .expect("the running event loop retains its listener")
}

/// PTY updates remain silent until observation and coalesce to one wakeup
/// between later observations.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn terminal_observation_coalesces_pty_wakeups() {
    let pty = Pty::spawn(
        shell(
            "stty -echo; \
             printf '\\033]2;ready\\007'; \
             while IFS= read -r action; do \
                 if [ \"$action\" = data ]; then \
                     printf x; \
                 else \
                     printf '\\033]2;%s\\007' \"$action\"; \
                 fi; \
             done",
        ),
        window_size(),
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        mut input,
        ..
    } = pty;
    let (observations_tx, mut observations_rx) = mpsc::unbounded_channel();
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), move |_| {
            RecordingListener {
                observations: observations_tx,
            }
        });
    let task = tokio::spawn(event_loop.run());

    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Title("ready".into()),
    );
    input.write_all(b"still-unobserved\n").await.unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Title("still-unobserved".into()),
    );

    {
        let _terminal = handle.terminal().await;
    }
    input.write_all(b"data\n").await.unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Wakeup,
    );

    input.write_all(b"first-wakeup-delivered\n").await.unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Title("first-wakeup-delivered".into()),
    );

    input
        .write_all(b"data\ncoalesced-update-applied\n")
        .await
        .unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Title("coalesced-update-applied".into()),
    );

    {
        let _terminal = handle.terminal().await;
    }
    input.write_all(b"data\n").await.unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Wakeup,
    );
    input.write_all(b"second-wakeup-delivered\n").await.unwrap();
    assert_eq!(
        next_listener_observation(&mut observations_rx).await,
        ListenerObservation::Title("second-wakeup-delivered".into()),
    );

    handle.shutdown();
    let (mut child, _output, _control) = task.await.unwrap().unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Closing every slave descriptor completes the event loop without waiting for
/// the child and returns all event-loop-owned capabilities.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn slave_eof_returns_capabilities_and_stops_handle() {
    let pty = Pty::spawn(shell("exec sleep 30 0<&- 1>&- 2>&-"), window_size()).unwrap();
    let original_child = pty.child.id();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        name,
        child,
        output,
        control,
        input,
    } = pty;
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            asyncritty::VoidListener
        });
    let task = tokio::spawn(event_loop.run());

    let (child, output, control) = task
        .await
        .unwrap()
        .expect("slave EOF is a normal event-loop completion condition");
    let mut pty = Pty {
        name,
        child,
        output,
        control,
        input,
    };
    assert_eq!(pty.child.id(), original_child);
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "the child should still be running when EOF completes the loop",
    );
    handle.wait_for_shutdown().await;
    handle.resize(window_size());

    stop_child(&mut pty.child).await;
    child_guard.disarm();
}

/// Slave EOF flushes an unterminated synchronized update and delivers its
/// events before returning.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn slave_eof_flushes_synchronized_update() {
    let pty = Pty::spawn(
        shell(
            "printf '\\033[?2026h\\033]2;eof-flushed\\007'; \
             exec sleep 30 0<&- 1>&- 2>&-",
        ),
        window_size(),
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (observations_tx, mut observations_rx) = mpsc::unbounded_channel();
    let (event_loop, handle) = EventLoop::new(
        child,
        output,
        control,
        TermConfig::default(),
        move |handle| ReobservingRecordingListener {
            observations: observations_tx,
            handle,
        },
    );
    {
        let _terminal = handle.terminal().await;
    }

    let (mut child, _output, _control) = event_loop.run().await.unwrap();
    let mut observations = Vec::new();
    while let Some(observation) = observations_rx.recv().await {
        observations.push(observation);
    }
    let title_index = observations
        .iter()
        .position(|observation| {
            matches!(
                observation,
                ListenerObservation::Title(title) if title == "eof-flushed"
            )
        })
        .expect("slave EOF flushes the buffered title before closing the listener");
    assert!(
        observations[title_index + 1..]
            .iter()
            .any(|observation| matches!(observation, ListenerObservation::Wakeup)),
        "EOF should deliver a final wakeup after the flushed update",
    );

    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Successful resizes update both terminal representations, persist across
/// event loops, and wake only once after terminal observation.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn resize_updates_pty_and_terminal() {
    let descriptors = rustix_openpty::openpty(None, None).unwrap();
    let observed_slave = descriptors.user.try_clone().unwrap();
    let pty = Pty::from_fds(
        shell("trap '' HUP; exec sleep 30"),
        window_size(),
        descriptors.controller,
        descriptors.user,
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (results_tx, mut results_rx) = mpsc::unbounded_channel();
    let (wakeups_tx, mut wakeups_rx) = mpsc::unbounded_channel();
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), move |_| {
            ResizeResultListener {
                results: results_tx,
                wakeups: wakeups_tx,
            }
        });
    let task = tokio::spawn(event_loop.run());
    let initially_unobserved = WindowSize {
        num_lines: 40,
        num_cols: 120,
        cell_width: 9,
        cell_height: 18,
    };

    handle.resize(initially_unobserved);
    let applied = results_rx
        .recv()
        .await
        .expect("the running event loop retains the resize-result sender")
        .unwrap();
    assert_window_size_eq(applied, initially_unobserved);
    assert_kernel_window_size(&observed_slave, initially_unobserved);
    assert!(
        matches!(wakeups_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "an initially unobserved terminal should not emit a resize wakeup",
    );
    let logical_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(logical_size, (40, 120));

    let observed = WindowSize {
        num_lines: 41,
        num_cols: 121,
        cell_width: 10,
        cell_height: 20,
    };
    handle.resize(observed);
    let applied = results_rx
        .recv()
        .await
        .expect("the running event loop retains the resize-result sender")
        .unwrap();
    assert_window_size_eq(applied, observed);
    assert_kernel_window_size(&observed_slave, observed);
    wakeups_rx
        .recv()
        .await
        .expect("the running event loop retains the resize-wakeup sender");
    assert!(
        matches!(wakeups_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "one observed resize should emit exactly one wakeup",
    );

    let coalesced = WindowSize {
        num_lines: 42,
        num_cols: 122,
        cell_width: 11,
        cell_height: 22,
    };
    handle.resize(coalesced);
    let applied = results_rx
        .recv()
        .await
        .expect("the running event loop retains the resize-result sender")
        .unwrap();
    assert_window_size_eq(applied, coalesced);
    assert_kernel_window_size(&observed_slave, coalesced);
    assert!(
        matches!(wakeups_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "a further resize while unobserved should not emit a wakeup",
    );

    handle.shutdown();
    let (child, output, control) = task.await.unwrap().unwrap();
    let final_logical_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(final_logical_size, (42, 122));
    let (next_event_loop, next_handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            asyncritty::VoidListener
        });
    let next_terminal_size = {
        let terminal = next_handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(next_terminal_size, (42, 122));

    next_handle.shutdown();
    let (mut child, _output, _control) = next_event_loop.run().await.unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Zero rows and fewer than two columns panic before submitting a resize, while
/// the minimum valid geometry remains usable afterward.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn invalid_resize_panics_before_submission() {
    let initial = window_size();
    let pty = Pty::spawn(shell("trap '' HUP; exec sleep 30"), initial).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        input: _input,
        ..
    } = pty;
    let (results_tx, mut results_rx) = mpsc::unbounded_channel();
    let (wakeups_tx, _wakeups_rx) = mpsc::unbounded_channel();
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), move |_| {
            ResizeResultListener {
                results: results_tx,
                wakeups: wakeups_tx,
            }
        });
    let task = tokio::spawn(event_loop.run());

    for invalid in [
        WindowSize {
            num_lines: 0,
            num_cols: initial.num_cols,
            cell_width: 8,
            cell_height: 16,
        },
        WindowSize {
            num_lines: initial.num_lines,
            num_cols: 1,
            cell_width: 8,
            cell_height: 16,
        },
    ] {
        assert!(
            catch_unwind(AssertUnwindSafe(|| handle.resize(invalid))).is_err(),
            "invalid terminal geometry should panic synchronously",
        );
    }

    let logical_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(logical_size, (24, 80));

    let minimum = WindowSize {
        num_lines: 1,
        num_cols: 2,
        cell_width: 8,
        cell_height: 16,
    };
    handle.resize(minimum);
    let applied = results_rx
        .recv()
        .await
        .expect("the running event loop retains the resize-result sender")
        .expect("the minimum geometry satisfies the resize bounds");
    assert_window_size_eq(applied, minimum);

    handle.shutdown();
    let (mut child, _output, _control) = task.await.unwrap().unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Announces when the child's readiness title has been delivered.
#[derive(Clone)]
struct ReadyListener {
    /// Notification observed by the task coordinating shutdown or abort.
    ready: Arc<Notify>,
}

impl EventListener for ReadyListener {
    type Error = Infallible;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        if title == "ready" {
            self.ready.notify_one();
        }
        Ok(())
    }
}

/// Signals when a pending listener callback future is dropped.
struct CallbackFutureDropProbe {
    /// Notification emitted as the callback future is dropped.
    dropped: Arc<Notify>,
}

impl Drop for CallbackFutureDropProbe {
    fn drop(&mut self) {
        self.dropped.notify_one();
    }
}

/// Keeps the readiness-title callback pending until its run future is dropped.
#[derive(Clone)]
struct PendingReadyListener {
    /// Notification sent after the readiness callback starts.
    entered: Arc<Notify>,

    /// Notification sent when the callback future is dropped.
    dropped: Arc<Notify>,
}

impl EventListener for PendingReadyListener {
    type Error = Infallible;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        if title == "ready" {
            let _drop_probe = CallbackFutureDropProbe {
                dropped: Arc::clone(&self.dropped),
            };
            self.entered.notify_one();
            pending::<()>().await;
        }
        Ok(())
    }
}

/// Event-loop shutdown releases its child, output, and control while the
/// independently owned input continues to keep the master side open. After all
/// master-side capabilities are dropped, the spawned program retains no
/// inherited controller descriptor that would suppress hangup.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn final_master_owner_controls_hangup() {
    let artifacts = tempfile::Builder::new()
        .prefix("asyncritty-final-master-owner-")
        .tempdir()
        .unwrap();
    let closed_marker = artifacts.path().join("closed");
    let input_marker = artifacts.path().join("input");
    // Use a foreground reader whose lifetime is tied to the PTY: shells defer
    // HUP traps until a foreground command exits, while `cat` stops blocking
    // when the final master closes.
    let mut command = shell(
        "trap 'printf closed > \"$ASYNCRITTY_CLOSED_MARKER\"; exit 0' HUP; \
         printf '\\033]2;ready\\007'; \
         IFS= read -r line; \
         printf '%s' \"$line\" > \"$ASYNCRITTY_INPUT_MARKER\"; \
         cat >/dev/null",
    );
    command
        .env("ASYNCRITTY_CLOSED_MARKER", &closed_marker)
        .env("ASYNCRITTY_INPUT_MARKER", &input_marker);
    let pty = Pty::spawn(command, window_size()).unwrap();
    let original_child = pty.child.id();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        mut input,
        ..
    } = pty;
    let ready = Arc::new(Notify::new());
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            ReadyListener {
                ready: Arc::clone(&ready),
            }
        });
    let task = tokio::spawn(event_loop.run());

    ready.notified().await;
    handle.shutdown();
    handle.shutdown();
    let (mut child, output, control) = task.await.unwrap().unwrap();

    handle.wait_for_shutdown().await;
    handle.shutdown();
    assert_eq!(child.id(), original_child);
    assert!(
        child.try_wait().unwrap().is_none(),
        "the child should still be running after event-loop shutdown",
    );
    assert!(
        !closed_marker.exists(),
        "event-loop shutdown should leave both master-side capabilities open",
    );

    drop(output);
    drop(control);
    input.write_all(b"input-open\n").await.unwrap();
    wait_for_path(&input_marker).await;
    assert_eq!(
        std::fs::read_to_string(&input_marker).unwrap(),
        "input-open",
    );
    assert!(
        !closed_marker.exists(),
        "the input capability should keep the master side open",
    );
    drop(input);
    wait_for_path(&closed_marker).await;
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Output and control can each keep the shared master descriptor open after
/// the other PTY capabilities are dropped.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn output_and_control_each_keep_master_open() {
    for keep_output in [true, false] {
        let label = if keep_output {
            "output-only-master-owner"
        } else {
            "control-only-master-owner"
        };
        let artifacts = tempfile::Builder::new()
            .prefix(&format!("asyncritty-{label}-"))
            .tempdir()
            .unwrap();
        let ready_marker = artifacts.path().join("ready");
        let closed_marker = artifacts.path().join("closed");
        // Use a foreground reader whose lifetime is tied to the PTY: shells defer
        // HUP traps until a foreground command exits, while `cat` stops blocking
        // when the final master closes.
        let mut command = shell(
            "trap 'printf closed > \"$ASYNCRITTY_CLOSED_MARKER\"; exit 0' HUP; \
             printf ready > \"$ASYNCRITTY_READY_MARKER\"; \
             cat >/dev/null",
        );
        command
            .env("ASYNCRITTY_READY_MARKER", &ready_marker)
            .env("ASYNCRITTY_CLOSED_MARKER", &closed_marker);
        let pty = Pty::spawn(command, window_size()).unwrap();
        let mut child_guard = ChildProcessGuard::for_pty(&pty);
        wait_for_path(&ready_marker).await;

        let Pty {
            mut child,
            output,
            mut control,
            input,
            ..
        } = pty;
        drop(input);
        let retained: Box<dyn std::any::Any> = if keep_output {
            drop(control);
            Box::new(output)
        } else {
            drop(output);
            control.resize(window_size()).unwrap();
            Box::new(control)
        };
        assert!(
            !closed_marker.exists(),
            "a retained PTY capability should keep the master side open",
        );

        drop(retained);
        wait_for_path(&closed_marker).await;
        stop_child(&mut child).await;
        child_guard.disarm();
    }
}

/// Rejects the fixture's deliberate failure title.
#[derive(Clone, Copy)]
struct FailingTitleListener;

impl EventListener for FailingTitleListener {
    type Error = ListenerFailure;

    async fn title(&self, title: String) -> Result<(), Self::Error> {
        if title == "fail" {
            Err(ListenerFailure("deliberate listener failure"))
        } else {
            Ok(())
        }
    }
}

/// A listener failure remains in the source chain and returns every
/// event-loop-owned capability for `Pty` reconstruction.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn listener_failure_retains_capabilities() {
    let pty = Pty::spawn(
        shell("trap '' HUP; printf '\\033]2;fail\\007'; exec sleep 30"),
        window_size(),
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        name,
        child,
        output,
        control,
        input,
    } = pty;
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            FailingTitleListener
        });

    let failure = event_loop.run().await.unwrap_err();
    handle.wait_for_shutdown().await;
    let final_logical_size = {
        let terminal = handle.terminal().await;
        (terminal.screen_lines(), terminal.columns())
    };
    assert_eq!(final_logical_size, (24, 80));
    assert!(matches!(
        &failure.error,
        EventLoopError::Listener(error) if error.0 == "deliberate listener failure"
    ));
    let loop_source = failure
        .source()
        .and_then(|source| source.downcast_ref::<EventLoopError<ListenerFailure>>())
        .expect("EventLoopFailure exposes EventLoopError as its source");
    let listener_source = loop_source
        .source()
        .and_then(|source| source.downcast_ref::<ListenerFailure>())
        .expect("EventLoopError::Listener exposes its listener error as its source");
    assert_eq!(listener_source.0, "deliberate listener failure");

    let mut pty = Pty {
        name,
        child: failure.child,
        output: failure.output,
        control: failure.control,
        input,
    };
    pty.control.resize(window_size()).unwrap();
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "the child should still be running after listener failure",
    );
    stop_child(&mut pty.child).await;
    child_guard.disarm();
}

/// Aborting the event-loop task drops its active callback, output, and control,
/// while independently owned input retains the master side until it is dropped.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn abort_retains_independent_input() {
    let artifacts = tempfile::Builder::new()
        .prefix("asyncritty-aborted-output-")
        .tempdir()
        .unwrap();
    let closed_marker = artifacts.path().join("closed");
    let input_marker = artifacts.path().join("input");
    // Use a foreground reader whose lifetime is tied to the PTY: shells defer
    // HUP traps until a foreground command exits, while `cat` stops blocking
    // when the final master closes.
    let mut command = shell(
        "trap 'printf closed > \"$ASYNCRITTY_CLOSED_MARKER\"; exit 0' HUP; \
         printf '\\033]2;ready\\007'; \
         IFS= read -r line; \
         printf '%s' \"$line\" > \"$ASYNCRITTY_INPUT_MARKER\"; \
         cat >/dev/null",
    );
    command
        .env("ASYNCRITTY_CLOSED_MARKER", &closed_marker)
        .env("ASYNCRITTY_INPUT_MARKER", &input_marker);
    let pty = Pty::spawn(command, window_size()).unwrap();
    let _child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        child,
        output,
        control,
        mut input,
        ..
    } = pty;
    let callback_entered = Arc::new(Notify::new());
    let callback_dropped = Arc::new(Notify::new());
    let (event_loop, handle) =
        EventLoop::new(child, output, control, TermConfig::default(), |_| {
            PendingReadyListener {
                entered: Arc::clone(&callback_entered),
                dropped: Arc::clone(&callback_dropped),
            }
        });
    let task = tokio::spawn(event_loop.run());

    callback_entered.notified().await;
    task.abort();
    let join_error = task.await.unwrap_err();
    assert!(join_error.is_cancelled());
    callback_dropped.notified().await;
    handle.wait_for_shutdown().await;
    input.write_all(b"input-open\n").await.unwrap();
    wait_for_path(&input_marker).await;
    assert!(
        !closed_marker.exists(),
        "the independently owned input should keep the master side open",
    );

    drop(input);
    wait_for_path(&closed_marker).await;
}
