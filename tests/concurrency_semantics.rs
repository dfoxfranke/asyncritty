// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Concurrency contracts that require a live PTY event loop.

mod common;

use std::convert::Infallible;
use std::future::{Future, poll_fn};
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use asyncritty::alacritty_terminal::grid::Dimensions as _;
use asyncritty::alacritty_terminal::term::Config as TermConfig;
use asyncritty::{
    Child, Command, EventListener, EventLoop, EventLoopError, EventLoopHandle, Pty, PtyInput,
    WindowSize,
};
use tokio::io::{AsyncWrite as _, AsyncWriteExt as _};
use tokio::sync::{Mutex, Notify, Semaphore, mpsc};
use tokio::time::sleep;

use common::ChildProcessGuard;
use tempfile::TempDir;

/// Polls `future` once without arranging another poll.
fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    let mut context = Context::from_waker(Waker::noop());
    future.poll(&mut context)
}

/// Returns the cell and pixel geometry installed on each test PTY.
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

/// Stops and reaps a test child that may still be running.
async fn stop_child(child: &mut Child) {
    if child.try_wait().unwrap().is_none() {
        child.start_kill().unwrap();
    }
    child.wait().await.unwrap();
}

/// Waits until `path` exists without imposing its own timeout.
async fn wait_for_path(path: &Path) {
    while !path.exists() {
        sleep(Duration::from_millis(10)).await;
    }
}

/// Blocks delivery of the first title until its test-controlled permit arrives.
#[derive(Clone)]
struct GateListener {
    /// Channel exposing title callbacks in delivery order.
    titles: mpsc::Sender<String>,

    /// Permit gate held by the first callback.
    release_first: Arc<Semaphore>,
}

impl EventListener for GateListener {
    type Error = Infallible;

    async fn title(&mut self, title: String) -> Result<(), Self::Error> {
        let is_first = title == "first";
        let _ = self.titles.send(title).await;
        if is_first {
            self.release_first
                .acquire()
                .await
                .expect("no test code closes the listener gate")
                .forget();
        }
        Ok(())
    }
}

/// A pending listener applies output backpressure without preventing direct
/// input from reaching the PTY.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn listener_backpressure_does_not_block_direct_input() {
    let artifacts = TempDir::new().unwrap();
    let second_written = artifacts.path().join("second-written");
    let mut command = shell(
        "trap '' HUP; \
         printf '\\033]2;first\\007'; \
         IFS= read -r line; \
         printf '\\033]2;second\\007'; \
         printf written > \"$ASYNCRITTY_SECOND_WRITTEN\"; \
         exec sleep 30",
    );
    command.env("ASYNCRITTY_SECOND_WRITTEN", &second_written);
    let pty = Pty::spawn(command, window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        mut input,
        ..
    } = pty;
    let (titles_tx, mut titles_rx) = mpsc::channel(2);
    let release_first = Arc::new(Semaphore::new(0));
    let mut listener = GateListener {
        titles: titles_tx,
        release_first: Arc::clone(&release_first),
    };
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    let task = tokio::spawn(async move {
        event_loop
            .run(&mut control, &mut output, &mut listener)
            .await
            .map(|()| (output, control))
    });

    let first = titles_rx
        .recv()
        .await
        .expect("the running event loop owns the title sender");
    assert_eq!(first, "first");

    input.write_all(b"continue\n").await.unwrap();
    wait_for_path(&second_written).await;
    assert!(
        matches!(titles_rx.try_recv(), Err(mpsc::error::TryRecvError::Empty)),
        "the event loop should defer its next PTY read until the listener completes",
    );

    release_first.add_permits(1);
    let second = titles_rx
        .recv()
        .await
        .expect("the running event loop owns the title sender");
    assert_eq!(second, "second");

    handle.shutdown();
    let (_output, _control) = task.await.unwrap().unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// One successfully applied resize observed by its completion callback.
#[derive(Debug)]
struct ResizeCompletion {
    /// Geometry reported by the event loop after application.
    window_size: WindowSize,

    /// Logical terminal dimensions visible from the completion callback.
    terminal_dimensions: (usize, usize),
}

/// Requests coalesced resizes and records their asynchronous completions.
#[derive(Clone)]
struct CoalescingResizeListener {
    /// Handle belonging to this listener's event loop.
    handle: EventLoopHandle,

    /// Requests made synchronously from the readiness-title callback.
    initial_requests: [WindowSize; 3],

    /// Request made after the coalesced completion has been delivered.
    follow_up: WindowSize,

    /// Channel retaining completion order and callback-visible state.
    completions: mpsc::UnboundedSender<ResizeCompletion>,

    /// Signals that the title callback is pending after publishing its resizes.
    title_pending: mpsc::UnboundedSender<()>,

    /// Allows the test to release the deliberately pending title callback.
    release_title: Arc<Notify>,
}

impl EventListener for CoalescingResizeListener {
    type Error = io::Error;

    async fn title(&mut self, title: String) -> Result<(), Self::Error> {
        if title == "ready" {
            // Retaining this guard makes premature resize processing wait on
            // state owned by the pending callback. The test's later successful
            // completion therefore detects resize processing while the callback
            // remains pending, without using elapsed time as an absence oracle.
            let terminal = self.handle.terminal().await;
            for window_size in self.initial_requests {
                let (): () = self.handle.resize(window_size);
            }
            let _ = self.title_pending.send(());
            self.release_title.notified().await;
            drop(terminal);
        }
        Ok(())
    }

    async fn resize_result(&mut self, result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        let window_size = result?;
        let terminal_dimensions = {
            let terminal = self.handle.terminal().await;
            (terminal.screen_lines(), terminal.columns())
        };
        let _ = self.completions.send(ResizeCompletion {
            window_size,
            terminal_dimensions,
        });

        if same_window_size(window_size, self.initial_requests[2]) {
            self.handle.resize(self.follow_up);
        } else if same_window_size(window_size, self.follow_up) {
            self.handle.shutdown();
        }

        Ok(())
    }
}

/// Listener-originated requests coalesce before processing, and a request made
/// from their completion callback is applied separately without deadlock.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn listener_resizes_are_coalesced_and_reported_as_events() {
    let pty = Pty::spawn(
        shell("printf '\\033]2;ready\\007'; trap '' HUP; exec sleep 30"),
        window_size(),
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        input: _input,
        ..
    } = pty;
    let initial_requests = [
        WindowSize {
            num_lines: 30,
            num_cols: 90,
            cell_width: 7,
            cell_height: 14,
        },
        WindowSize {
            num_lines: 31,
            num_cols: 91,
            cell_width: 8,
            cell_height: 16,
        },
        WindowSize {
            num_lines: 32,
            num_cols: 92,
            cell_width: 9,
            cell_height: 18,
        },
    ];
    let follow_up = WindowSize {
        num_lines: 41,
        num_cols: 121,
        cell_width: 10,
        cell_height: 20,
    };
    let (completions_tx, mut completions_rx) = mpsc::unbounded_channel();
    let (title_pending_tx, mut title_pending_rx) = mpsc::unbounded_channel();
    let release_title = Arc::new(Notify::new());
    let listener_release_title = Arc::clone(&release_title);
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    let mut listener = CoalescingResizeListener {
        handle: handle.clone(),
        initial_requests,
        follow_up,
        completions: completions_tx,
        title_pending: title_pending_tx,
        release_title: listener_release_title,
    };
    let run = event_loop.run(&mut control, &mut output, &mut listener);
    tokio::pin!(run);
    tokio::select! {
        pending = title_pending_rx.recv() => {
            pending.expect("the pending event loop owns the title sender");
        }
        result = run.as_mut() => {
            panic!("the event loop completed before its title callback was pending: {result:?}")
        }
    }
    poll_fn(|context| {
        assert!(matches!(run.as_mut().poll(context), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    release_title.notify_one();

    run.as_mut().await.unwrap();
    handle.wait_for_shutdown().await;

    let mut completions = Vec::new();
    while let Ok(completion) = completions_rx.try_recv() {
        completions.push(completion);
    }
    let observed_completions = completions
        .iter()
        .map(|completion| {
            (
                window_size_components(completion.window_size),
                completion.terminal_dimensions,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed_completions,
        [
            (
                window_size_components(initial_requests[2]),
                (
                    usize::from(initial_requests[2].num_lines),
                    usize::from(initial_requests[2].num_cols),
                ),
            ),
            (
                window_size_components(follow_up),
                (
                    usize::from(follow_up.num_lines),
                    usize::from(follow_up.num_cols),
                ),
            ),
        ],
        "only the latest initial request and the callback's follow-up should complete",
    );

    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Returns a comparable representation of all logical and pixel dimensions.
fn window_size_components(window_size: WindowSize) -> (u16, u16, u16, u16) {
    (
        window_size.num_lines,
        window_size.num_cols,
        window_size.cell_width,
        window_size.cell_height,
    )
}

/// Returns whether two window sizes contain the same logical and pixel values.
fn same_window_size(left: WindowSize, right: WindowSize) -> bool {
    window_size_components(left) == window_size_components(right)
}

/// Outcome of one best-effort terminal-generated reply.
#[derive(Debug)]
enum ReplyDisposition {
    /// Another writer owned the explicit input lock, so the reply was dropped.
    LockContended,

    /// The single write poll accepted this prefix length.
    Accepted(usize),

    /// The single write poll encountered PTY backpressure.
    Pending,
}

/// Result of handling one terminal-generated reply.
#[derive(Debug)]
struct DsrReport {
    /// Outcome of the best-effort reply.
    disposition: ReplyDisposition,
}

/// Implements a drop-on-backpressure reply policy using explicit sharing.
#[derive(Clone)]
struct DsrListener {
    /// Explicitly shared input capability.
    input: Arc<Mutex<PtyInput>>,

    /// Channel exposing completed callback operations.
    reports: mpsc::UnboundedSender<DsrReport>,
}

impl EventListener for DsrListener {
    type Error = io::Error;

    async fn pty_write(&mut self, reply: String) -> Result<(), Self::Error> {
        let disposition = match self.input.try_lock() {
            Err(_) => ReplyDisposition::LockContended,
            Ok(mut input) => {
                let mut context = Context::from_waker(Waker::noop());
                match Pin::new(&mut *input).poll_write(&mut context, reply.as_bytes()) {
                    Poll::Ready(Ok(count)) => ReplyDisposition::Accepted(count),
                    Poll::Ready(Err(error)) => return Err(error),
                    Poll::Pending => ReplyDisposition::Pending,
                }
            }
        };

        let _ = self.reports.send(DsrReport { disposition });
        Ok(())
    }
}

/// An external mutex and one direct write poll are sufficient to implement a
/// terminal-reply policy that discards input rather than waiting under
/// contention or PTY backpressure.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn listener_can_drop_best_effort_replies() {
    const INPUT_SIZE: usize = 4 * 1024 * 1024;

    let artifacts = TempDir::new().unwrap();
    let consume = artifacts.path().join("consume");
    let mut command = shell(
        "stty raw -echo; \
         printf '\\033[6n'; \
         while [ ! -e \"$ASYNCRITTY_CONSUME_MARKER\" ]; do sleep 0.01; done; \
         dd bs=1 count=1 of=/dev/null 2>/dev/null; \
         printf '\\033[6n'; \
         trap '' HUP; \
         exec sleep 30",
    );
    command.env("ASYNCRITTY_CONSUME_MARKER", &consume);
    let pty = Pty::spawn(command, window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        input,
        ..
    } = pty;
    let input = Arc::new(Mutex::new(input));
    let (reports_tx, mut reports_rx) = mpsc::unbounded_channel();
    let listener_input = Arc::clone(&input);
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    let mut listener = DsrListener {
        input: listener_input,
        reports: reports_tx,
    };
    let task = tokio::spawn(async move {
        event_loop
            .run(&mut control, &mut output, &mut listener)
            .await
            .map(|()| (output, control))
    });

    let first = reports_rx
        .recv()
        .await
        .expect("the running event loop owns the DSR report sender");
    assert!(
        matches!(
            first.disposition,
            ReplyDisposition::Accepted(count) if count > 0
        ),
        "an uncontended input capability should accept a reply prefix",
    );

    let writer_input = Arc::clone(&input);
    let writer = tokio::spawn(async move {
        writer_input
            .lock()
            .await
            .write_all(&vec![b'x'; INPUT_SIZE])
            .await
    });
    while input.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    // This payload is substantially larger than supported Unix PTY input
    // queues, so the write cannot finish while the child abstains from reads.
    assert!(
        !writer.is_finished(),
        "the full write should remain pending while the child is not consuming input",
    );

    std::fs::write(&consume, "consume").unwrap();

    let second = reports_rx
        .recv()
        .await
        .expect("the running event loop owns the DSR report sender");
    assert!(
        matches!(second.disposition, ReplyDisposition::LockContended),
        "the second reply should be dropped while the writer owns the input lock",
    );

    writer.abort();
    let _ = writer.await;

    handle.shutdown();
    let (_output, _control) = task.await.unwrap().unwrap();
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Independently reaping the child leaves PTY processing active until its descendant exits.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn child_exit_does_not_stop_event_loop() {
    let pty = Pty::spawn(
        shell("trap '' HUP; { IFS= read -r line; printf 'descendant:%s' \"$line\"; } <&0 & exit 7"),
        window_size(),
    )
    .unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        mut input,
        ..
    } = pty;
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    let mut listener = asyncritty::VoidListener;
    let task = tokio::spawn(async move {
        event_loop
            .run(&mut control, &mut output, &mut listener)
            .await
            .map(|()| (output, control))
    });

    let status = child.wait().await.unwrap();
    // The group leader has been reaped, so cleanup must not address its reusable ID.
    // Dropping the PTY on failure closes the descendant's blocked read instead.
    child_guard.disarm();
    assert_eq!(status.code(), Some(7));
    assert!(
        !task.is_finished(),
        "the descendant still holds the terminal open"
    );
    let shutdown_wait = handle.wait_for_shutdown();
    tokio::pin!(shutdown_wait);
    assert!(matches!(poll_once(shutdown_wait.as_mut()), Poll::Pending));

    input.write_all(b"done\n").await.unwrap();
    let (_output, _control) = task.await.unwrap().unwrap();
    shutdown_wait.await;
    let terminal = handle.terminal().await;
    let screen = terminal.bounds_to_string(
        asyncritty::alacritty_terminal::index::Point::default(),
        asyncritty::alacritty_terminal::index::Point::new(
            terminal.bottommost_line(),
            terminal.last_column(),
        ),
    );
    assert!(screen.contains("descendant:done"));
    assert_eq!(child.wait().await.unwrap().code(), Some(7));
}

/// Dropping an event loop before it runs makes pending and later shutdown waits
/// ready.
#[tokio::test(flavor = "current_thread")]
async fn dropping_unrun_event_loop_completes_shutdown_wait() {
    let mut command = shell("trap '' HUP; exec sleep 30");
    command.kill_on_drop(true);
    let pty = Pty::spawn(command, window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        output: _output,
        control,
        input: _input,
        ..
    } = pty;
    let (event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    let shutdown_wait = handle.wait_for_shutdown();
    tokio::pin!(shutdown_wait);

    assert!(matches!(poll_once(shutdown_wait.as_mut()), Poll::Pending));
    drop(event_loop);
    assert!(matches!(poll_once(shutdown_wait.as_mut()), Poll::Ready(())));

    let late_wait = handle.wait_for_shutdown();
    tokio::pin!(late_wait);
    assert!(matches!(poll_once(late_wait.as_mut()), Poll::Ready(())));
    assert!(
        child.try_wait().unwrap().is_none(),
        "dropping an unrun loop must retain the independently owned child"
    );
    stop_child(&mut child).await;
    child_guard.disarm();
}

/// Result produced when a test releases a gated wakeup callback.
#[derive(Clone, Copy)]
enum WakeupOutcome {
    /// The callback completes successfully.
    Success,

    /// The callback reports the fixture's deliberate listener error.
    Failure,
}

/// Holds wakeup delivery pending until the test permits its completion.
#[derive(Clone)]
struct GatedWakeupListener {
    /// Notification sent after the wakeup callback starts.
    entered: Arc<Notify>,

    /// Permit gate controlling when the callback may complete.
    release: Arc<Semaphore>,

    /// Result returned after the gate opens.
    outcome: WakeupOutcome,

    /// Whether the resize result queued behind the wakeup was delivered.
    resize_result_delivered: Arc<AtomicBool>,
}

impl EventListener for GatedWakeupListener {
    type Error = io::Error;

    async fn wakeup(&mut self) -> Result<(), Self::Error> {
        self.entered.notify_one();
        self.release
            .acquire()
            .await
            .expect("no test code closes the wakeup gate")
            .forget();
        match self.outcome {
            WakeupOutcome::Success => Ok(()),
            WakeupOutcome::Failure => Err(io::Error::other("deliberate wakeup failure")),
        }
    }

    async fn resize_result(&mut self, _result: io::Result<WindowSize>) -> Result<(), Self::Error> {
        self.resize_result_delivered.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Explicit shutdown waits for the active callback to succeed, then stops
/// delivery of events queued behind it.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn shutdown_waits_for_pending_listener_to_succeed() {
    let pty = Pty::spawn(shell("trap '' HUP; exec sleep 30"), window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        input: _input,
        ..
    } = pty;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Semaphore::new(0));
    let resize_result_delivered = Arc::new(AtomicBool::new(false));
    let mut listener = GatedWakeupListener {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        outcome: WakeupOutcome::Success,
        resize_result_delivered: Arc::clone(&resize_result_delivered),
    };
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    {
        let _terminal = handle.terminal().await;
    }
    handle.resize(WindowSize {
        num_lines: 40,
        num_cols: 120,
        cell_width: 9,
        cell_height: 18,
    });
    let run = event_loop.run(&mut control, &mut output, &mut listener);
    tokio::pin!(run);

    tokio::select! {
        _ = entered.notified() => {}
        _ = run.as_mut() => {
            panic!("the event loop should remain pending at the wakeup gate")
        }
    }
    handle.shutdown();
    let shutdown_wait = handle.wait_for_shutdown();
    tokio::pin!(shutdown_wait);
    assert!(
        matches!(poll_once(run.as_mut()), Poll::Pending),
        "shutdown should not cancel the active wakeup callback",
    );
    assert!(
        matches!(poll_once(shutdown_wait.as_mut()), Poll::Pending),
        "shutdown completion should wait for the active callback",
    );

    release.add_permits(1);
    run.as_mut().await.unwrap();
    shutdown_wait.as_mut().await;
    assert!(
        !resize_result_delivered.load(Ordering::SeqCst),
        "shutdown should defer the resize result queued behind the wakeup",
    );

    stop_child(&mut child).await;
    child_guard.disarm();
}

/// A listener error produced after shutdown is requested remains the event-loop
/// result, and events already queued behind it are not delivered in that run.
#[tokio::test(flavor = "current_thread")]
#[ntest::timeout(15_000)]
async fn shutdown_reports_pending_listener_failure() {
    let pty = Pty::spawn(shell("trap '' HUP; exec sleep 30"), window_size()).unwrap();
    let mut child_guard = ChildProcessGuard::for_pty(&pty);
    let Pty {
        mut child,
        mut output,
        mut control,
        input: _input,
        ..
    } = pty;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Semaphore::new(0));
    let resize_result_delivered = Arc::new(AtomicBool::new(false));
    let mut listener = GatedWakeupListener {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        outcome: WakeupOutcome::Failure,
        resize_result_delivered: Arc::clone(&resize_result_delivered),
    };
    let (mut event_loop, handle) = EventLoop::new(TermConfig::default(), &control);
    {
        let _terminal = handle.terminal().await;
    }
    handle.resize(WindowSize {
        num_lines: 40,
        num_cols: 120,
        cell_width: 9,
        cell_height: 18,
    });
    let run = event_loop.run(&mut control, &mut output, &mut listener);
    tokio::pin!(run);

    tokio::select! {
        _ = entered.notified() => {}
        _ = run.as_mut() => {
            panic!("the event loop should remain pending at the wakeup gate")
        }
    }
    handle.shutdown();
    assert!(
        matches!(poll_once(run.as_mut()), Poll::Pending),
        "shutdown should not cancel the active wakeup callback",
    );

    release.add_permits(1);
    let failure = run.as_mut().await.unwrap_err();
    assert!(matches!(
        &failure,
        EventLoopError::Listener(error) if error.kind() == io::ErrorKind::Other
    ));
    assert!(
        !resize_result_delivered.load(Ordering::SeqCst),
        "shutdown should defer the resize result queued behind the wakeup",
    );

    stop_child(&mut child).await;
    child_guard.disarm();
}
