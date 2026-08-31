// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Shared ownership of Alacritty's terminal state machine.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::Term;
use alacritty_terminal::event::Event as TerminalEvent;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::WindowSize;
use crate::event::{Event, SyncEventProxy};

/// Stable terminal-model dimensions derived from a PTY window size.
pub(crate) struct TerminalDimensions {
    /// Number of visible terminal lines.
    screen_lines: usize,

    /// Number of cells in each terminal line.
    columns: usize,
}

impl From<WindowSize> for TerminalDimensions {
    fn from(window_size: WindowSize) -> Self {
        Self {
            screen_lines: usize::from(window_size.num_lines),
            columns: usize::from(window_size.num_cols),
        }
    }
}

impl Dimensions for TerminalDimensions {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// Terminal state and observation status shared by the event loop and handles.
#[derive(Clone)]
pub(crate) struct SharedTerminal {
    /// State retained by every clone.
    inner: Arc<SharedTerminalInner>,
}

/// Terminal model and the status used to coalesce update notifications.
struct SharedTerminalInner {
    /// Alacritty terminal model protected from concurrent mutation.
    terminal: RwLock<Term<SyncEventProxy>>,

    /// Whether a handle has observed the current terminal state.
    observed: AtomicBool,
}

impl SharedTerminal {
    /// Acquire the terminal state and acknowledge everything visible in it.
    ///
    /// Observation is recorded only after the read lock is acquired, so a
    /// pending or cancelled acquisition does not acknowledge an intervening
    /// update.
    pub(crate) async fn read(&self) -> RwLockReadGuard<'_, Term<SyncEventProxy>> {
        let terminal = self.inner.terminal.read().await;
        self.inner.observed.store(true, Ordering::Relaxed);
        terminal
    }

    /// Mutate the terminal and return the events emitted by that mutation.
    ///
    /// The operation's boolean result reports whether it applied an update. An
    /// update resets the observation status and appends a wakeup after any
    /// terminal-emitted events only when the preceding state was observed. The
    /// returned queue preserves emission order, and no event listener is
    /// invoked while the terminal state is locked.
    pub(crate) async fn mutate<F, R>(
        &self,
        event_proxy: &SyncEventProxy,
        operation: F,
    ) -> (R, VecDeque<Event>)
    where
        F: FnOnce(&mut Term<SyncEventProxy>) -> (R, bool) + Send,
        R: Send,
    {
        let mut terminal = self.inner.terminal.write().await;
        let preceding_events = event_proxy.drain();
        debug_assert!(preceding_events.is_empty());
        let (output, updated) = operation(&mut terminal);
        let mut events = event_proxy.drain();

        if updated && self.inner.observed.swap(false, Ordering::Relaxed) {
            events.push_back(TerminalEvent::Wakeup.into());
        }

        (output, events)
    }
}

/// Create terminal state and its paired synchronous event collector.
///
/// # Panics
///
/// Panics if the viewport has no screen lines or fewer than two columns.
pub(crate) fn new(config: Config, window_size: WindowSize) -> (SharedTerminal, SyncEventProxy) {
    let dimensions = TerminalDimensions::from(window_size);
    assert!(
        dimensions.screen_lines > 0,
        "terminal dimensions must include at least one screen line"
    );
    assert!(
        dimensions.columns >= 2,
        "terminal dimensions must include at least two columns"
    );

    let events = SyncEventProxy::new();
    let term = Term::new(config, &dimensions, events.clone());
    let construction_events = events.drain();
    debug_assert!(construction_events.is_empty());

    let terminal = SharedTerminal {
        inner: Arc::new(SharedTerminalInner {
            terminal: RwLock::new(term),
            observed: AtomicBool::new(false),
        }),
    };

    (terminal, events)
}

#[cfg(test)]
mod tests {
    use std::thread;

    use alacritty_terminal::event::Event as TerminalEvent;

    use super::*;

    /// Construct a window size with irrelevant cell dimensions zeroed.
    fn window_size(num_lines: u16, num_cols: u16) -> WindowSize {
        WindowSize {
            num_lines,
            num_cols,
            cell_width: 0,
            cell_height: 0,
        }
    }

    /// A one-line, two-column viewport is the smallest accepted terminal model.
    #[tokio::test]
    async fn minimum_dimensions() {
        let (terminal, _event_proxy) = new(Config::default(), window_size(1, 2));

        let terminal = terminal.read().await;
        let dimensions = (terminal.screen_lines(), terminal.columns());
        assert_eq!(dimensions, (1, 2));
    }

    /// Terminal construction panics when the viewport has no screen lines.
    #[test]
    #[should_panic]
    fn zero_screen_lines() {
        let _terminal = new(Config::default(), window_size(0, 2));
    }

    /// Terminal construction panics when the viewport has only one column.
    #[test]
    #[should_panic]
    fn one_column() {
        let _terminal = new(Config::default(), window_size(1, 1));
    }

    /// A mutation returns its events in order and leaves no event for the next
    /// mutation.
    #[tokio::test]
    async fn mutation_events_are_scoped() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));

        drop(terminal.read().await);
        let ((), mut events) = terminal
            .mutate(&event_proxy, |term| {
                term.toggle_vi_mode();
                ((), true)
            })
            .await;
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::CursorBlinkingChange))
        ));
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());

        let ((), events) = terminal.mutate(&event_proxy, |_term| ((), false)).await;
        assert!(events.is_empty());
    }

    /// Events emitted on a scoped OS thread are collected without relying on
    /// task-local state.
    #[tokio::test]
    async fn mutation_can_emit_from_scoped_thread() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));

        let ((), mut events) = terminal
            .mutate(&event_proxy, |term| {
                thread::scope(|scope| {
                    scope.spawn(|| term.toggle_vi_mode()).join().unwrap();
                });
                ((), true)
            })
            .await;

        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::CursorBlinkingChange))
        ));
        assert!(events.is_empty());
    }

    /// A newly constructed terminal does not wake for its first update.
    #[tokio::test]
    async fn terminal_starts_unobserved() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));

        let ((), events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;

        assert!(events.is_empty());
    }

    /// Only the first update after observation appends a wakeup.
    #[tokio::test]
    async fn updates_coalesce_until_reobserved() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));
        drop(terminal.read().await);

        let ((), mut events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());

        let ((), events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;
        assert!(events.is_empty());
    }

    /// Observation through any clone enables the next shared update wakeup.
    #[tokio::test]
    async fn clone_observation_is_shared() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));
        let observer = terminal.clone();
        drop(observer.read().await);

        let ((), mut events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;

        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());
    }

    /// Reobserving an updated terminal enables one more wakeup.
    #[tokio::test]
    async fn reobservation_enables_another_wakeup() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));
        drop(terminal.read().await);
        let ((), _events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;
        drop(terminal.read().await);

        let ((), mut events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;

        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());
    }

    /// A mutation that applies no update preserves the observed status.
    #[tokio::test]
    async fn unapplied_mutation_preserves_observation() {
        let (terminal, event_proxy) = new(Config::default(), window_size(1, 2));
        drop(terminal.read().await);

        let ((), events) = terminal.mutate(&event_proxy, |_term| ((), false)).await;
        assert!(events.is_empty());

        let ((), mut events) = terminal.mutate(&event_proxy, |_term| ((), true)).await;
        assert!(matches!(
            events.pop_front(),
            Some(Event::Terminal(TerminalEvent::Wakeup))
        ));
        assert!(events.is_empty());
    }

    /// A pending read marks the terminal observed only after acquiring its
    /// guard.
    #[tokio::test]
    async fn observation_follows_lock_acquisition() {
        let (terminal, _event_proxy) = new(Config::default(), window_size(1, 2));
        let write_guard = terminal.inner.terminal.write().await;
        let read = terminal.read();
        tokio::pin!(read);

        tokio::select! {
            biased;
            _guard = &mut read => panic!("read unexpectedly acquired the write-locked terminal"),
            () = async {} => {},
        }
        assert!(!terminal.inner.observed.load(Ordering::Relaxed));

        drop(write_guard);
        let read_guard = read.await;
        assert!(terminal.inner.observed.load(Ordering::Relaxed));
        drop(read_guard);
    }
}
