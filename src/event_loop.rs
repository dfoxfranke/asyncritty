// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Tokio event loop for driving an Alacritty terminal.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::future::{pending, poll_fn};
use std::io;

use alacritty_terminal::term;
use alacritty_terminal::vte::ansi;
use tokio::io::ReadBuf;
use tokio::sync::watch;
use tokio::time::{Instant, sleep_until};

use crate::WindowSize;
use crate::event::{Event, EventListener, SyncEventProxy, dispatch_event};
use crate::terminal::{self, SharedTerminal, TerminalReadGuard};
use crate::tty::{PtyControl, PtyOutput};

/// Maximum bytes read from the PTY in one parser batch.
const READ_BUFFER_SIZE: usize = 64 * 1024;

/// Lifecycle state published by the event loop and its handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopState {
    /// The event loop may continue accepting and performing work.
    Running,

    /// Shutdown has been requested and no further events will be dispatched to
    /// the listener.
    ShutdownRequested,

    /// The event loop has stopped.
    Stopped,
}

/// Lifecycle state shared by the event loop and its handles.
#[derive(Debug, Clone)]
struct LoopControl {
    /// Current lifecycle state and its transition notifications.
    state: watch::Sender<LoopState>,
}

impl LoopControl {
    /// Create running state and its first observer.
    fn new() -> (Self, watch::Receiver<LoopState>) {
        let (state, receiver) = watch::channel(LoopState::Running);
        let control = Self { state };
        (control, receiver)
    }

    /// Load the current lifecycle state.
    fn state(&self) -> LoopState {
        *self.state.borrow()
    }

    /// Return whether the loop is accepting work.
    fn is_running(&self) -> bool {
        self.state() == LoopState::Running
    }

    /// Transition a running loop to shutdown requested.
    ///
    /// If the state is already not running, the request is a no-op; lifecycle
    /// states never return to [`LoopState::Running`].
    fn request_shutdown(&self) {
        self.state.send_if_modified(|state| {
            if *state != LoopState::Running {
                return false;
            }

            *state = LoopState::ShutdownRequested;
            true
        });
    }

    /// Publish that the event loop has stopped.
    fn publish_stopped(&self) {
        self.state.send_if_modified(|state| {
            if *state == LoopState::Stopped {
                return false;
            }

            *state = LoopState::Stopped;
            true
        });
    }

    /// Observe the current loop state and future lifecycle transitions.
    fn subscribe(&self) -> watch::Receiver<LoopState> {
        self.state.subscribe()
    }
}

/// Marks a loop stopped when dropped.
///
/// This allows [`EventLoopHandle::wait_for_shutdown`] to observe the stop.
struct StopOnDrop(LoopControl);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.publish_stopped();
    }
}

/// Access to an event loop's terminal state, resize requests, and shutdown.
///
/// Cloning this handle retains access to the same terminal state. That state
/// remains available after the event loop stops.
#[derive(Clone)]
pub struct EventLoopHandle {
    /// Terminal state shared directly with the event loop.
    terminal: SharedTerminal,

    /// Latest requested terminal geometry.
    resizes: watch::Sender<WindowSize>,

    /// Lifecycle state shared with every handle clone.
    control: LoopControl,
}

impl fmt::Debug for EventLoopHandle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventLoopHandle")
            .finish_non_exhaustive()
    }
}

impl EventLoopHandle {
    /// Lock the terminal state for reading, for example to render its contents.
    ///
    /// While the guard is held, the event loop cannot apply PTY output or resize
    /// the terminal model. Drop it before waiting for either operation.
    ///
    /// Acquiring the guard marks the state as observed, enabling the next
    /// [`EventListener::wakeup`] notification. After the loop stops, the guard
    /// gives access to the final terminal state. Use
    /// [`TerminalReadGuard::changed`] to check whether this acquisition
    /// acknowledged a previously unobserved update.
    pub async fn terminal(&self) -> TerminalReadGuard<'_> {
        self.terminal.read().await
    }

    /// Request a new window size for both the PTY and terminal model.
    ///
    /// Returns immediately; the event loop reports the outcome through
    /// [`EventListener::resize_result`] after attempting the resize. If the PTY
    /// resize fails, the terminal model keeps its previous size and the loop
    /// continues processing requests.
    ///
    /// A new request replaces any request still waiting to be processed,
    /// including requests from other handle clones. Replaced requests produce
    /// no result event. A request made during a listener callback waits for that
    /// callback to finish.
    ///
    /// Stopping the loop can discard pending requests or their result events.
    /// Valid requests made after shutdown is requested or the loop stops are
    /// ignored.
    ///
    /// # Panics
    ///
    /// Panics if the requested geometry has no lines or fewer than two columns.
    pub fn resize(&self, window_size: WindowSize) {
        assert!(
            window_size.num_lines > 0,
            "terminal geometry must contain at least one line"
        );
        assert!(
            window_size.num_cols >= 2,
            "terminal geometry must contain at least two columns"
        );

        if self.control.is_running() {
            let _ = self.resizes.send(window_size);
        }
    }

    /// Request orderly shutdown of the event loop, returning immediately.
    ///
    /// The loop finishes any active listener callback, then discards queued
    /// events. An error from that callback is returned by [`EventLoop::run`].
    /// A callback that stays pending prevents shutdown from completing.
    ///
    /// Await [`EventLoop::run`] to recover the PTY output and control handles, or
    /// [`wait_for_shutdown`](Self::wait_for_shutdown) to observe that the loop
    /// has stopped.
    ///
    /// Repeated requests have no additional effect.
    pub fn shutdown(&self) {
        self.control.request_shutdown();
    }

    /// Wait until the event loop has stopped.
    ///
    /// Completes when [`EventLoop::run`] returns, or when the loop or its running
    /// future is dropped. At that point, PTY processing, resizing, and listener
    /// callbacks have ended. If the loop has already stopped, returns
    /// immediately.
    ///
    /// Call [`shutdown`](Self::shutdown) to request that the loop stop.
    ///
    /// Awaiting this method from the loop's own listener callback deadlocks:
    /// the loop must finish the callback before it can stop.
    pub async fn wait_for_shutdown(&self) {
        let mut lifecycle = self.control.subscribe();
        wait_until_stopped(&mut lifecycle).await;
    }
}

/// The I/O or listener error that stopped [`EventLoop::run`].
#[derive(Debug)]
pub enum EventLoopError<E: Error + Send + Sync + 'static> {
    /// Reading PTY output failed.
    Io(
        /// Underlying operating-system error.
        io::Error,
    ),

    /// An [`EventListener`] callback returned an error.
    Listener(
        /// Error returned by the configured listener.
        E,
    ),
}

impl<E: Error + Send + Sync + 'static> Display for EventLoopError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(_) => formatter.write_str("event-loop I/O operation failed"),
            Self::Listener(_) => formatter.write_str("terminal event listener failed"),
        }
    }
}

impl<E: Error + Send + Sync + 'static> Error for EventLoopError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Listener(error) => Some(error),
        }
    }
}

impl<E: Error + Send + Sync + 'static> From<io::Error> for EventLoopError<E> {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// An [`EventLoop::run`] error together with the PTY handles it owned.
///
/// The returned handles remain available for further PTY I/O.
#[derive(Debug)]
pub struct EventLoopFailure<E: Error + Send + Sync + 'static> {
    /// Failure that stopped the loop.
    pub error: EventLoopError<E>,

    /// PTY output handle released by the stopped loop.
    pub output: PtyOutput,

    /// PTY control handle released by the stopped loop.
    pub control: PtyControl,
}

impl<E: Error + Send + Sync + 'static> Display for EventLoopFailure<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("event loop failed")
    }
}

impl<E: Error + Send + Sync + 'static> Error for EventLoopFailure<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

/// Reads PTY output into an Alacritty [`Term`](alacritty_terminal::Term) and
/// delivers terminal events to an asynchronous [`EventListener`].
///
/// Call [`run`](Self::run) to drive the loop. Use the [`EventLoopHandle`]
/// returned by [`new`](Self::new) to inspect terminal state, resize the terminal,
/// or request shutdown. Input is written separately through
/// [`PtyInput`](crate::PtyInput).
pub struct EventLoop<L: EventListener> {
    /// Terminal model receiving parsed PTY output.
    terminal: SharedTerminal,

    /// Collector paired with the event proxy installed in the terminal.
    terminal_events: SyncEventProxy,

    /// Heap storage preserves `EventLoop<L>: Unpin` independently of `L`.
    listener: Box<L>,

    /// PTY output read by the event loop.
    output: PtyOutput,

    /// PTY settings applied by the event loop.
    pty_control: PtyControl,

    /// Latest resize not yet selected for processing.
    resizes: watch::Receiver<WindowSize>,

    /// Keeps the resize channel open if every external handle is dropped.
    _resize_guard: watch::Sender<WindowSize>,

    /// Current lifecycle state and transition notifications for the event loop.
    lifecycle: watch::Receiver<LoopState>,

    /// Publishes the stopped state if the loop or its running future is dropped.
    stop_on_drop: StopOnDrop,
}

impl<L: EventListener> EventLoop<L> {
    /// Create a terminal model and event loop from the output and control fields
    /// of a [`Pty`](crate::Pty). The caller retains its child and manages process
    /// waiting and signaling independently.
    ///
    /// `terminal_config` configures the Alacritty terminal model. Its initial
    /// size is the window size most recently applied through `pty_control`.
    ///
    /// `make_listener` receives a clone of the returned handle so that the
    /// listener can access terminal state and request resizes or shutdown.
    ///
    /// # Panics
    ///
    /// Panics if `pty_control`'s recorded window size has no lines or
    /// fewer than two columns.
    pub fn new<F>(
        output: PtyOutput,
        pty_control: PtyControl,
        terminal_config: term::Config,
        make_listener: F,
    ) -> (Self, EventLoopHandle)
    where
        F: FnOnce(EventLoopHandle) -> L,
    {
        let initial_window_size = pty_control.recorded_window_size();
        let (resizes_tx, resizes) = watch::channel(initial_window_size);
        let (control, lifecycle) = LoopControl::new();
        let (terminal, terminal_events) = terminal::new(terminal_config, initial_window_size);
        let handle = EventLoopHandle {
            terminal: terminal.clone(),
            resizes: resizes_tx.clone(),
            control: control.clone(),
        };
        let listener = Box::new(make_listener(handle.clone()));
        let event_loop = Self {
            terminal,
            terminal_events,
            listener,
            output,
            pty_control,
            resizes,
            _resize_guard: resizes_tx,
            lifecycle,
            stop_on_drop: StopOnDrop(control),
        };

        (event_loop, handle)
    }

    /// Process PTY output and listener events until shutdown, PTY EOF, or an
    /// error.
    ///
    /// On success, returns the PTY output and control handles. The loop never
    /// owns, waits for, or signals the independently managed child.
    ///
    /// PTY EOF completes the loop normally. The `EIO` error used for slave
    /// closure on Linux is also treated as EOF. At EOF, the loop applies any
    /// buffered synchronized update and delivers its events before returning.
    /// Child exit alone does not stop the loop: descendants may still hold the
    /// slave open.
    ///
    /// Events are delivered serially in emission order. Each callback completes
    /// before the loop reads more PTY output or processes a resize. The terminal
    /// lock is released before invoking callbacks, so they can read terminal
    /// state through [`EventLoopHandle::terminal`].
    ///
    /// [`EventLoopHandle::shutdown`] requests orderly completion, waiting for
    /// the active callback and discarding queued events.
    ///
    /// # Cancellation
    ///
    /// Dropping this future stops processing and cancels any active listener
    /// callback without waiting for it to finish. Queued events and output
    /// buffered by the parser are discarded. Already-applied model updates
    /// remain accessible through [`EventLoopHandle::terminal`].
    ///
    /// Cancellation drops the listener and the loop's PTY output and control
    /// handles. If no other handles keep the PTY master open, it closes, which
    /// can cause a terminal hangup.
    ///
    /// To let the active callback finish and recover the PTY handles, call
    /// [`EventLoopHandle::shutdown`] and continue awaiting this future. Shutdown
    /// cannot complete while that callback remains pending.
    ///
    /// # Errors
    ///
    /// PTY read errors stop the loop. The first
    /// listener callback error also stops the loop, discarding queued events.
    /// [`EventLoopFailure`] retains the PTY output and control handles
    /// along with the error.
    ///
    /// # Panics
    ///
    /// May panic when first polled outside a Tokio runtime. May also panic if
    /// parsed output starts a synchronized update while the runtime has no time
    /// driver.
    pub async fn run(mut self) -> Result<(PtyOutput, PtyControl), EventLoopFailure<L::Error>> {
        let mut parser: ansi::Processor = ansi::Processor::new();
        let mut read_buffer = vec![0; READ_BUFFER_SIZE];

        let result = loop {
            if !self.stop_on_drop.0.is_running() {
                break Ok(());
            }

            let sync_deadline = parser.sync_timeout().sync_timeout().map(Instant::from_std);
            let action = tokio::select! {
                _ = wait_until_not_running(&mut self.lifecycle) => LoopAction::Stop,
                resize = self.resizes.changed() => {
                    resize.expect("the event loop retains a resize sender");
                    LoopAction::Resize(*self.resizes.borrow_and_update())
                },
                _ = wait_for_deadline(sync_deadline) => LoopAction::SyncTimeout,
                result = read_once(&self.output, &mut read_buffer) => LoopAction::Read(result),
            };

            match action {
                LoopAction::Stop => break Ok(()),
                LoopAction::Resize(resize) => {
                    let events = self.apply_resize(resize).await;
                    if let Err(error) = self.deliver_events(events).await {
                        break Err(error);
                    }
                }
                LoopAction::SyncTimeout => {
                    let (_, events) = self
                        .terminal
                        .mutate(&self.terminal_events, |terminal| {
                            (parser.stop_sync(terminal), true)
                        })
                        .await;
                    if let Err(error) = self.deliver_events(events).await {
                        break Err(error);
                    }
                }
                LoopAction::Read(result) => {
                    let count = match result {
                        Ok(0) => None,
                        Ok(count) => Some(count),
                        Err(error) if is_linux_pty_eio(&error) => None,
                        Err(error) => break Err(error.into()),
                    };
                    let Some(count) = count else {
                        match self.finish_pty_eof(&mut parser).await {
                            Ok(()) => break Ok(()),
                            Err(error) => break Err(error),
                        }
                    };

                    let (_, events) = self
                        .terminal
                        .mutate(&self.terminal_events, |terminal| {
                            parser.advance(terminal, &read_buffer[..count]);
                            ((), parser.sync_bytes_count() < count)
                        })
                        .await;
                    if let Err(error) = self.deliver_events(events).await {
                        break Err(error);
                    }
                }
            }
        };

        self.complete(result)
    }

    /// Apply and deliver a synchronized update still buffered at PTY EOF.
    ///
    /// # Errors
    ///
    /// Returns a listener error if delivery of the applied update fails.
    async fn finish_pty_eof(
        &mut self,
        parser: &mut ansi::Processor,
    ) -> Result<(), EventLoopError<L::Error>> {
        if parser.sync_timeout().sync_timeout().is_none() {
            return Ok(());
        }

        let (_, events) = self
            .terminal
            .mutate(&self.terminal_events, |terminal| {
                (parser.stop_sync(terminal), true)
            })
            .await;
        self.deliver_events(events).await?;
        Ok(())
    }

    /// Apply one claimed resize and return its events in delivery order.
    async fn apply_resize(&mut self, window_size: WindowSize) -> VecDeque<Event> {
        if let Err(error) = self.pty_control.resize(window_size) {
            return VecDeque::from([Event::ResizeResult(Err(error))]);
        }

        let (_, mut events) = self
            .terminal
            .mutate(&self.terminal_events, |terminal| {
                (
                    terminal.resize(terminal::TerminalDimensions::from(window_size)),
                    true,
                )
            })
            .await;
        events.push_back(Event::ResizeResult(Ok(window_size)));
        events
    }

    /// Deliver one event batch in order while the loop remains running.
    ///
    /// Once a callback is admitted, it completes before shutdown is observed
    /// again. Undelivered events are dropped with the batch.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by an admitted listener callback.
    async fn deliver_events(
        &self,
        mut events: VecDeque<Event>,
    ) -> Result<(), EventLoopError<L::Error>> {
        while self.stop_on_drop.0.is_running() {
            let Some(event) = events.pop_front() else {
                return Ok(());
            };
            if let Err(error) = dispatch_event(self.listener.as_ref(), event).await {
                return Err(EventLoopError::Listener(error));
            }
        }

        Ok(())
    }

    /// Return the PTY capabilities alongside the loop outcome.
    ///
    /// # Errors
    ///
    /// Returns the loop error together with the PTY capabilities.
    #[allow(
        clippy::result_large_err,
        reason = "both completion paths return the same owned capabilities"
    )]
    fn complete(
        self,
        result: Result<(), EventLoopError<L::Error>>,
    ) -> Result<(PtyOutput, PtyControl), EventLoopFailure<L::Error>> {
        let output = self.output;
        let control = self.pty_control;
        match result {
            Ok(()) => Ok((output, control)),
            Err(error) => Err(EventLoopFailure {
                error,
                output,
                control,
            }),
        }
    }
}

/// The next operation selected by the event loop.
enum LoopAction {
    /// Lifecycle state no longer permits work.
    Stop,

    /// Latest resize claimed for processing.
    Resize(WindowSize),

    /// A synchronized-update deadline elapsed.
    SyncTimeout,

    /// Result of one PTY read.
    Read(io::Result<usize>),
}

/// Poll one PTY read without taking exclusive ownership of the output capability.
async fn read_once(output: &PtyOutput, buffer: &mut [u8]) -> io::Result<usize> {
    poll_fn(|context| {
        let mut read_buffer = ReadBuf::new(&mut *buffer);
        match output.poll_read_shared(context, &mut read_buffer) {
            std::task::Poll::Ready(Ok(())) => {
                std::task::Poll::Ready(Ok(read_buffer.filled().len()))
            }
            std::task::Poll::Ready(Err(error)) => std::task::Poll::Ready(Err(error)),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    })
    .await
}

/// Wait until the loop stops accepting requests.
async fn wait_until_not_running(lifecycle: &mut watch::Receiver<LoopState>) {
    if lifecycle
        .wait_for(|state| *state != LoopState::Running)
        .await
        .is_err()
    {
        pending::<()>().await;
    }
}

/// Wait until the event loop has stopped.
async fn wait_until_stopped(lifecycle: &mut watch::Receiver<LoopState>) {
    if lifecycle
        .wait_for(|state| *state == LoopState::Stopped)
        .await
        .is_err()
    {
        pending::<()>().await;
    }
}

/// Wait for a deadline, or forever when there is no deadline.
///
/// # Panics
///
/// Panics if `deadline` is present and the active Tokio runtime has no time
/// driver.
async fn wait_for_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => pending().await,
    }
}

/// Return whether Linux reported its PTY-slave-closure form of EOF.
fn is_linux_pty_eio(error: &io::Error) -> bool {
    cfg!(target_os = "linux") && error.raw_os_error() == Some(libc::EIO)
}

/// Unit coverage for resize validation and lifecycle reporting.
#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::task::{Context, Poll, Wake, Waker};
    use std::thread;

    use super::*;

    /// Handle and lifecycle controls exercised without a running actor.
    struct HandleFixture {
        /// Handle exercised by a unit test.
        handle: EventLoopHandle,

        /// Actor-side observer for resize publication.
        resize_requests: watch::Receiver<WindowSize>,

        /// Lifecycle state shared with the handle.
        control: LoopControl,
    }

    /// Build a handle and its independently controlled lifecycle state.
    fn handle_fixture() -> HandleFixture {
        let (control, _changed) = LoopControl::new();
        let window_size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        };
        let (resizes, resize_requests) = watch::channel(window_size);
        let (terminal, _event_proxy) = terminal::new(term::Config::default(), window_size);
        let handle = EventLoopHandle {
            terminal,
            resizes,
            control: control.clone(),
        };
        HandleFixture {
            handle,
            resize_requests,
            control,
        }
    }

    /// Poll one pinned future with the supplied wake target.
    fn poll_with_waker<F: Future>(mut future: Pin<&mut F>, waker: &Waker) -> Poll<F::Output> {
        future.as_mut().poll(&mut Context::from_waker(waker))
    }

    /// Records whether a registered future requested another poll.
    #[derive(Default)]
    struct WakeFlag(
        /// Set whenever the associated waker is invoked.
        AtomicBool,
    );

    impl Wake for WakeFlag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Invalid geometry panics before lifecycle handling and is never
    /// published to the actor.
    #[test]
    fn invalid_resize_panics_before_stopped_loop_discard() {
        let HandleFixture {
            handle,
            resize_requests,
            control,
        } = handle_fixture();
        control.publish_stopped();

        for invalid in [
            WindowSize {
                num_lines: 0,
                num_cols: 80,
                cell_width: 8,
                cell_height: 16,
            },
            WindowSize {
                num_lines: 24,
                num_cols: 1,
                cell_width: 8,
                cell_height: 16,
            },
        ] {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle.resize(invalid);
                }))
                .is_err(),
                "invalid terminal geometry should panic"
            );
            assert!(matches!(resize_requests.has_changed(), Ok(false)));
        }
    }

    /// A valid resize is not published in any non-running lifecycle state.
    #[test]
    fn non_running_loop_discards_valid_resize() {
        let HandleFixture {
            handle,
            resize_requests,
            control,
        } = handle_fixture();

        handle.shutdown();
        handle.resize(WindowSize {
            num_lines: 31,
            num_cols: 97,
            cell_width: 9,
            cell_height: 18,
        });
        assert!(matches!(resize_requests.has_changed(), Ok(false)));

        control.publish_stopped();
        handle.resize(WindowSize {
            num_lines: 32,
            num_cols: 98,
            cell_width: 9,
            cell_height: 18,
        });
        assert!(matches!(resize_requests.has_changed(), Ok(false)));
    }

    /// Concurrent shutdown requests make one monotonic transition and remain
    /// distinct from the stopped state.
    #[test]
    fn shutdown_request_is_idempotent() {
        let HandleFixture {
            handle, control, ..
        } = handle_fixture();
        let rendezvous = Arc::new(Barrier::new(3));
        thread::scope(|scope| {
            for handle in [handle.clone(), handle.clone()] {
                let rendezvous = Arc::clone(&rendezvous);
                scope.spawn(move || {
                    rendezvous.wait();
                    handle.shutdown();
                });
            }
            rendezvous.wait();
        });

        assert_eq!(control.state(), LoopState::ShutdownRequested);

        control.publish_stopped();
        handle.shutdown();
        assert_eq!(control.state(), LoopState::Stopped);
    }

    /// Shutdown waits do not request shutdown, all registered observers wake
    /// when the actor stops, and later waits are immediately ready.
    #[test]
    fn shutdown_wait_observes_stopped_state() {
        let HandleFixture {
            handle, control, ..
        } = handle_fixture();
        let first_flag = Arc::new(WakeFlag::default());
        let second_flag = Arc::new(WakeFlag::default());
        let first_waker = Waker::from(Arc::clone(&first_flag));
        let second_waker = Waker::from(Arc::clone(&second_flag));
        let other_handle = handle.clone();

        {
            let mut cancelled = std::pin::pin!(handle.wait_for_shutdown());
            assert_eq!(
                poll_with_waker(cancelled.as_mut(), Waker::noop()),
                Poll::Pending,
            );
        }
        assert_eq!(control.state(), LoopState::Running);

        let mut first = std::pin::pin!(handle.wait_for_shutdown());
        let mut second = std::pin::pin!(other_handle.wait_for_shutdown());
        assert_eq!(poll_with_waker(first.as_mut(), &first_waker), Poll::Pending);
        assert_eq!(
            poll_with_waker(second.as_mut(), &second_waker),
            Poll::Pending,
        );

        handle.shutdown();
        assert_eq!(poll_with_waker(first.as_mut(), &first_waker), Poll::Pending);
        assert_eq!(
            poll_with_waker(second.as_mut(), &second_waker),
            Poll::Pending,
        );
        first_flag.0.store(false, Ordering::SeqCst);
        second_flag.0.store(false, Ordering::SeqCst);

        control.publish_stopped();
        assert!(first_flag.0.load(Ordering::SeqCst));
        assert!(second_flag.0.load(Ordering::SeqCst));
        assert_eq!(
            poll_with_waker(first.as_mut(), &first_waker),
            Poll::Ready(()),
        );
        assert_eq!(
            poll_with_waker(second.as_mut(), &second_waker),
            Poll::Ready(()),
        );

        let mut late = std::pin::pin!(handle.wait_for_shutdown());
        assert_eq!(
            poll_with_waker(late.as_mut(), Waker::noop()),
            Poll::Ready(()),
        );
    }
}
