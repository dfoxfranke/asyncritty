// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Run a process on a Unix pseudoterminal and parse its output into an
//! [`alacritty_terminal::Term`], using Tokio for asynchronous I/O.
//!
//! [`Pty::spawn`] starts a [`Command`] with its standard streams attached to the
//! terminal. Pass the resulting child, output, and control handles to
//! [`EventLoop::new`], then drive the terminal with [`EventLoop::run`]. The
//! application keeps [`PtyInput`] for writing keystrokes and terminal replies.
//!
//! [`EventLoopHandle`] provides access to the terminal state for rendering,
//! along with resize and shutdown requests. Implement [`EventListener`] to
//! handle events such as title changes, clipboard requests, and update
//! notifications. [`VoidListener`] ignores these events.
//!
//! # Example
//!
//! Capture the final screen of a command that writes colored text. The terminal
//! model interprets the escape sequences; extracting its text omits the color
//! attributes. Enable Tokio's `rt` and `macros` features to use the runtime entry
//! point shown here.
//!
//! ```no_run
//! use asyncritty::{Command, EventLoop, Pty, VoidListener, WindowSize};
//! use asyncritty::alacritty_terminal::grid::Dimensions;
//! use asyncritty::alacritty_terminal::index::Point;
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut command = Command::new("/bin/sh");
//!     command.args(["-c", "printf '\\033[31mHello, terminal!\\033[0m\\r\\n'"]);
//!     let size = WindowSize {
//!         num_lines: 24,
//!         num_cols: 80,
//!         cell_width: 8,
//!         cell_height: 16,
//!     };
//!     let Pty { child, output, control, input: _input, .. } = Pty::spawn(command, size)?;
//!     let (event_loop, handle) = EventLoop::new(
//!         child, output, control, Default::default(), |_| VoidListener,
//!     );
//!
//!     let (mut child, _output, _control) = event_loop.run().await?;
//!     let status = child.wait().await?;
//!     let terminal = handle.terminal().await;
//!     let screen = terminal.bounds_to_string(
//!         Point::default(),
//!         Point::new(terminal.bottommost_line(), terminal.last_column()),
//!     );
//!     println!("{screen}");
//!     println!("Exit status: {status}");
//!     Ok(())
//! }
//! ```

#[cfg(not(unix))]
compile_error!("asyncritty currently supports Unix platforms only");

mod event;
mod event_loop;
mod process;
mod terminal;
mod tty;

pub use alacritty_terminal;
#[doc(no_inline)]
pub use alacritty_terminal::{
    event::WindowSize,
    term::{Config, Term},
};
pub use event::{EventListener, SyncEventProxy, VoidListener};
pub use event_loop::{EventLoop, EventLoopError, EventLoopFailure, EventLoopHandle};
pub use process::{Child, Command};
pub use tty::{Pty, PtyControl, PtyInput, PtyOutput};
