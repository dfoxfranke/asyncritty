// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Application callbacks and the bridge from Alacritty's synchronous events.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use alacritty_terminal::event::Event as TerminalEvent;
use alacritty_terminal::event::EventListener as AlacrittyEventListener;
use alacritty_terminal::event::WindowSize;
use alacritty_terminal::term::ClipboardType;
use alacritty_terminal::vte::ansi::Rgb;

/// Application callbacks for terminal requests and state changes.
///
/// Every callback defaults to a successful no-op. Override the callbacks your
/// application needs, such as changing its window title, accessing the
/// clipboard, or sending terminal replies through [`PtyInput`](crate::PtyInput).
///
/// [`EventLoop::run`](crate::EventLoop::run) describes callback ordering,
/// failure handling, and cancellation.
pub trait EventListener: Send + Sync + 'static {
    /// Callback failure reported as [`EventLoopError::Listener`](crate::EventLoopError::Listener).
    type Error: Error + Send + Sync + 'static;

    /// Reconsider the mouse cursor shape after a terminal-state change.
    fn mouse_cursor_dirty(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Set the window title requested by the terminal application.
    fn title(&self, _title: String) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Restore the window title to the application's default.
    fn reset_title(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Store text in the selected clipboard at the terminal application's request.
    fn clipboard_store(
        &self,
        _clipboard: ClipboardType,
        _text: String,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Reply to the terminal application's request for clipboard contents.
    ///
    /// Pass the selected clipboard's contents to the supplied formatter, then
    /// write the resulting escape sequence through [`PtyInput`](crate::PtyInput).
    fn clipboard_load(
        &self,
        _clipboard: ClipboardType,
        _formatter: Arc<dyn Fn(&str) -> String + Send + Sync + 'static>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Reply to the terminal application's query for a color's RGB value.
    ///
    /// The index uses Alacritty's [`Colors`](alacritty_terminal::term::color::Colors)
    /// numbering. Pass the color's value to the supplied formatter, then write
    /// the resulting escape sequence through [`PtyInput`](crate::PtyInput).
    fn color_request(
        &self,
        _index: usize,
        _formatter: Arc<dyn Fn(Rgb) -> String + Send + Sync + 'static>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Send terminal-generated text through [`PtyInput`](crate::PtyInput).
    fn pty_write(&self, _text: String) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Reply to the terminal application's query for its text area size.
    ///
    /// Pass the current geometry, including cell dimensions in pixels, to the
    /// supplied formatter, then write the resulting escape sequence through
    /// [`PtyInput`](crate::PtyInput).
    fn text_area_size_request(
        &self,
        _formatter: Arc<dyn Fn(WindowSize) -> String + Send + Sync + 'static>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Update cursor blinking after the terminal's blinking state changes.
    ///
    /// Read the current setting from [`Term::cursor_style`](crate::Term::cursor_style)
    /// through [`EventLoopHandle::terminal`](crate::EventLoopHandle::terminal).
    fn cursor_blinking_change(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Notify the application of an update to terminal state it has observed.
    ///
    /// A newly created terminal starts unobserved. Acquiring the guard returned
    /// by [`EventLoopHandle::terminal`](crate::EventLoopHandle::terminal)
    /// marks its current state as observed. The first subsequent application of
    /// PTY output or successful terminal-model resize invokes this callback;
    /// further updates are coalesced until the terminal is acquired again
    /// through any handle clone. PTY output held by synchronized-update
    /// buffering does not invoke this callback until it is applied.
    fn wakeup(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Handle a request to ring the terminal bell.
    fn bell(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Handle a shutdown request from the terminal model.
    fn exit(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    /// Handle the outcome of an [`EventLoopHandle::resize`](crate::EventLoopHandle::resize)
    /// request.
    ///
    /// On success, receives the requested [`WindowSize`] after the PTY size and
    /// terminal grid dimensions have been updated. An error reports that the PTY
    /// resize failed; the terminal model keeps its previous dimensions.
    fn resize_result(
        &self,
        _result: io::Result<WindowSize>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }
}

/// An event listener that discards every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct VoidListener;

impl EventListener for VoidListener {
    type Error = Infallible;
}

/// One event awaiting asynchronous listener delivery.
pub(crate) enum Event {
    /// Terminal-state event.
    Terminal(TerminalEvent),

    /// Result of applying one resize request selected by the event loop.
    ResizeResult(io::Result<WindowSize>),
}

impl From<TerminalEvent> for Event {
    fn from(event: TerminalEvent) -> Self {
        Self::Terminal(event)
    }
}

/// Invoke the listener callback corresponding to one internal event.
///
/// Returns the error produced by the selected callback unchanged.
pub(crate) async fn dispatch_event<L: EventListener + ?Sized>(
    listener: &L,
    event: Event,
) -> Result<(), L::Error> {
    match event {
        Event::Terminal(event) => match event {
            TerminalEvent::MouseCursorDirty => listener.mouse_cursor_dirty().await,
            TerminalEvent::Title(title) => listener.title(title).await,
            TerminalEvent::ResetTitle => listener.reset_title().await,
            TerminalEvent::ClipboardStore(clipboard, text) => {
                listener.clipboard_store(clipboard, text).await
            }
            TerminalEvent::ClipboardLoad(clipboard, formatter) => {
                listener.clipboard_load(clipboard, formatter).await
            }
            TerminalEvent::ColorRequest(index, formatter) => {
                listener.color_request(index, formatter).await
            }
            TerminalEvent::PtyWrite(text) => listener.pty_write(text).await,
            TerminalEvent::TextAreaSizeRequest(formatter) => {
                listener.text_area_size_request(formatter).await
            }
            TerminalEvent::CursorBlinkingChange => listener.cursor_blinking_change().await,
            TerminalEvent::Wakeup => listener.wakeup().await,
            TerminalEvent::Bell => listener.bell().await,
            TerminalEvent::Exit => listener.exit().await,
            // Child supervision belongs to the application. Alacritty's enum
            // includes this variant, but this loop does not produce it.
            TerminalEvent::ChildExit(_) => Ok(()),
        },
        Event::ResizeResult(result) => listener.resize_result(result).await,
    }
}

/// Synchronous event collector installed in [`Term`](crate::Term).
///
/// This type is public only because it appears in the target of the read guard
/// returned by
/// [`EventLoopHandle::terminal`](crate::EventLoopHandle::terminal).
#[doc(hidden)]
#[derive(Clone)]
pub struct SyncEventProxy {
    /// Events shared by the proxy installed in the terminal and its owner.
    events: Arc<Mutex<VecDeque<Event>>>,
}

impl SyncEventProxy {
    /// Create an empty collector.
    pub(crate) fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Remove the events collected since the preceding drain.
    pub(crate) fn drain(&self) -> VecDeque<Event> {
        std::mem::take(&mut *self.events())
    }

    /// Lock the event queue shared by every proxy clone.
    fn events(&self) -> MutexGuard<'_, VecDeque<Event>> {
        self.events.lock().unwrap()
    }
}

impl AlacrittyEventListener for SyncEventProxy {
    fn send_event(&self, event: TerminalEvent) {
        self.events().push_back(Event::Terminal(event));
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    use super::*;

    /// Callback observations retained by the exhaustive routing listener.
    #[derive(Debug, PartialEq, Eq)]
    enum Observation {
        /// Mouse cursor state must be reconsidered.
        MouseCursorDirty,

        /// New window title.
        Title(String),

        /// Default window title restoration.
        ResetTitle,

        /// Clipboard destination and stored text.
        ClipboardStore(ClipboardType, String),

        /// Clipboard source and formatted sample contents.
        ClipboardLoad(ClipboardType, String),

        /// Color index and formatted sample color.
        ColorRequest(usize, String),

        /// Text sent to the PTY.
        PtyWrite(String),

        /// Formatted sample text area size.
        TextAreaSizeRequest(String),

        /// Cursor blinking state changed.
        CursorBlinkingChange,

        /// New terminal content is available.
        Wakeup,

        /// Terminal bell rang.
        Bell,

        /// Terminal application requested shutdown.
        Exit,

        /// Applied geometry or resize failure kind.
        ResizeResult(Result<(u16, u16, u16, u16), io::ErrorKind>),
    }

    /// Listener that records every callback and exercises formatter payloads.
    #[derive(Default)]
    struct RecordingListener {
        /// Callback observations in dispatch order.
        observations: Mutex<Vec<Observation>>,
    }

    impl RecordingListener {
        /// Append one callback observation.
        fn record(&self, observation: Observation) {
            self.observations.lock().unwrap().push(observation);
        }

        /// Remove every callback observation in dispatch order.
        fn take_observations(&self) -> Vec<Observation> {
            std::mem::take(&mut *self.observations.lock().unwrap())
        }
    }

    impl EventListener for RecordingListener {
        type Error = Infallible;

        fn mouse_cursor_dirty(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::MouseCursorDirty);
            ready(Ok(()))
        }

        fn title(&self, title: String) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::Title(title));
            ready(Ok(()))
        }

        fn reset_title(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::ResetTitle);
            ready(Ok(()))
        }

        fn clipboard_store(
            &self,
            clipboard: ClipboardType,
            text: String,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::ClipboardStore(clipboard, text));
            ready(Ok(()))
        }

        fn clipboard_load(
            &self,
            clipboard: ClipboardType,
            formatter: Arc<dyn Fn(&str) -> String + Send + Sync + 'static>,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::ClipboardLoad(
                clipboard,
                formatter("sample clipboard"),
            ));
            ready(Ok(()))
        }

        fn color_request(
            &self,
            index: usize,
            formatter: Arc<dyn Fn(Rgb) -> String + Send + Sync + 'static>,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::ColorRequest(
                index,
                formatter(Rgb { r: 1, g: 2, b: 3 }),
            ));
            ready(Ok(()))
        }

        fn pty_write(&self, text: String) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::PtyWrite(text));
            ready(Ok(()))
        }

        fn text_area_size_request(
            &self,
            formatter: Arc<dyn Fn(WindowSize) -> String + Send + Sync + 'static>,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::TextAreaSizeRequest(formatter(WindowSize {
                num_lines: 5,
                num_cols: 10,
                cell_width: 3,
                cell_height: 7,
            })));
            ready(Ok(()))
        }

        fn cursor_blinking_change(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::CursorBlinkingChange);
            ready(Ok(()))
        }

        fn wakeup(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::Wakeup);
            ready(Ok(()))
        }

        fn bell(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::Bell);
            ready(Ok(()))
        }

        fn exit(&self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            self.record(Observation::Exit);
            ready(Ok(()))
        }

        fn resize_result(
            &self,
            result: io::Result<WindowSize>,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send {
            let result = result
                .map(|size| {
                    (
                        size.num_lines,
                        size.num_cols,
                        size.cell_width,
                        size.cell_height,
                    )
                })
                .map_err(|error| error.kind());
            self.record(Observation::ResizeResult(result));
            ready(Ok(()))
        }
    }

    /// Cloned proxies append to and drain one shared FIFO queue.
    #[test]
    fn clones_share_ordered_events() {
        let proxy = SyncEventProxy::new();
        let clone = proxy.clone();

        AlacrittyEventListener::send_event(&proxy, TerminalEvent::Bell);
        AlacrittyEventListener::send_event(&clone, TerminalEvent::Wakeup);

        let mut events = proxy.drain();
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Bell))
        ));
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());
        assert!(clone.drain().is_empty());
    }

    /// Terminal events transfer their payloads to the corresponding callbacks;
    /// child-exit events are ignored.
    #[tokio::test]
    async fn dispatches_terminal_events_and_ignores_child_exit() {
        let listener = RecordingListener::default();
        let events = [
            TerminalEvent::MouseCursorDirty.into(),
            TerminalEvent::Title("new title".into()).into(),
            TerminalEvent::ResetTitle.into(),
            TerminalEvent::ClipboardStore(ClipboardType::Clipboard, "stored text".into()).into(),
            TerminalEvent::ClipboardLoad(
                ClipboardType::Selection,
                Arc::new(|text| format!("clipboard: {text}")),
            )
            .into(),
            TerminalEvent::ColorRequest(
                42,
                Arc::new(|color| format!("rgb: {}, {}, {}", color.r, color.g, color.b)),
            )
            .into(),
            TerminalEvent::PtyWrite("terminal reply".into()).into(),
            TerminalEvent::TextAreaSizeRequest(Arc::new(|size| {
                format!(
                    "{}x{} cells at {}x{} pixels",
                    size.num_cols, size.num_lines, size.cell_width, size.cell_height
                )
            }))
            .into(),
            TerminalEvent::CursorBlinkingChange.into(),
            TerminalEvent::Wakeup.into(),
            TerminalEvent::Bell.into(),
            TerminalEvent::Exit.into(),
            TerminalEvent::ChildExit(ExitStatus::from_raw(7 << 8)).into(),
            Event::ResizeResult(Ok(WindowSize {
                num_lines: 31,
                num_cols: 97,
                cell_width: 9,
                cell_height: 18,
            })),
            Event::ResizeResult(Err(io::Error::from(io::ErrorKind::PermissionDenied))),
        ];

        for event in events {
            dispatch_event(&listener, event).await.unwrap();
        }

        assert_eq!(
            listener.take_observations(),
            [
                Observation::MouseCursorDirty,
                Observation::Title("new title".into()),
                Observation::ResetTitle,
                Observation::ClipboardStore(ClipboardType::Clipboard, "stored text".into()),
                Observation::ClipboardLoad(
                    ClipboardType::Selection,
                    "clipboard: sample clipboard".into(),
                ),
                Observation::ColorRequest(42, "rgb: 1, 2, 3".into()),
                Observation::PtyWrite("terminal reply".into()),
                Observation::TextAreaSizeRequest("10x5 cells at 3x7 pixels".into()),
                Observation::CursorBlinkingChange,
                Observation::Wakeup,
                Observation::Bell,
                Observation::Exit,
                Observation::ResizeResult(Ok((31, 97, 9, 18))),
                Observation::ResizeResult(Err(io::ErrorKind::PermissionDenied)),
            ]
        );
    }

    /// A listener that overrides no callback accepts every internal event.
    #[tokio::test]
    async fn provided_callbacks_accept_every_event() {
        let events = [
            TerminalEvent::MouseCursorDirty.into(),
            TerminalEvent::Title(String::new()).into(),
            TerminalEvent::ResetTitle.into(),
            TerminalEvent::ClipboardStore(ClipboardType::Clipboard, String::new()).into(),
            TerminalEvent::ClipboardLoad(ClipboardType::Selection, Arc::new(str::to_owned)).into(),
            TerminalEvent::ColorRequest(0, Arc::new(|_| String::new())).into(),
            TerminalEvent::PtyWrite(String::new()).into(),
            TerminalEvent::TextAreaSizeRequest(Arc::new(|_| String::new())).into(),
            TerminalEvent::CursorBlinkingChange.into(),
            TerminalEvent::Wakeup.into(),
            TerminalEvent::Bell.into(),
            TerminalEvent::Exit.into(),
            TerminalEvent::ChildExit(ExitStatus::from_raw(0)).into(),
            Event::ResizeResult(Ok(WindowSize {
                num_lines: 1,
                num_cols: 2,
                cell_width: 0,
                cell_height: 0,
            })),
            Event::ResizeResult(Err(io::Error::from(io::ErrorKind::InvalidInput))),
        ];

        for event in events {
            dispatch_event(&VoidListener, event).await.unwrap();
        }
    }
}
