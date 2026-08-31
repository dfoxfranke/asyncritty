// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Unix pseudoterminals with asynchronous input and output.
//!
//! Creation and child spawning are synchronous; I/O and waiting for child exit
//! use Tokio.

use std::ffi::OsString;
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStringExt as _;
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::task::{Context, Poll};

use rustix_openpty::rustix;
use rustix_openpty::rustix::termios::Winsize;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::WindowSize;
use crate::process::{Child, Command};

/// A stream of bytes read from the master end of a pseudoterminal.
///
/// This includes the child's standard output and standard error, as well as
/// any input echoed by the terminal. Use [`AsyncRead`] to read these bytes.
///
/// Closing the slave end can produce EOF or an `EIO` error, depending on the
/// platform.
#[derive(Debug)]
pub struct PtyOutput {
    /// Shared nonblocking descriptor and its Tokio readiness registration.
    inner: Arc<AsyncFd<OwnedFd>>,
}

impl PtyOutput {
    /// Poll a nonblocking read through a shared output borrow.
    ///
    /// # Errors
    ///
    /// Returns an error if readiness polling or reading from the PTY fails.
    /// Interrupted and would-block operations are retried.
    pub(crate) fn poll_read_shared(
        &self,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        loop {
            let mut ready = match self.inner.poll_read_ready(context) {
                Poll::Ready(Ok(ready)) => ready,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            };

            let unfilled = buffer.initialize_unfilled();
            match ready.try_io(|inner| read_fd(inner.get_ref(), unfilled)) {
                Ok(Ok(count)) => {
                    buffer.advance(count);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => continue,
                Ok(Err(error)) => return Poll::Ready(Err(error)),
                Err(_would_block) => continue,
            }
        }
    }

    /// Borrow the controller descriptor for output-side kernel probes.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.inner.get_ref().as_fd()
    }
}

impl AsRawFd for PtyOutput {
    /// Borrows the PTY master descriptor shared by the terminal capabilities.
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}

impl AsyncRead for PtyOutput {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_read_shared(context, buffer)
    }
}

/// A stream for writing terminal input to the child through the master end of
/// a pseudoterminal.
///
/// Use [`AsyncWrite`] to send input. Writes are unbuffered; flushing and writer
/// shutdown complete immediately, and shutdown leaves the PTY open.
#[derive(Debug)]
pub struct PtyInput {
    /// Shared nonblocking descriptor and its Tokio readiness registration.
    inner: Arc<AsyncFd<OwnedFd>>,
}

impl PtyInput {
    /// Poll a nonblocking write through the input descriptor.
    ///
    /// # Errors
    ///
    /// Returns an error if readiness polling or writing to the PTY fails.
    /// Interrupted and would-block operations are retried.
    fn poll_write_shared(
        &self,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }

        loop {
            let mut ready = match self.inner.poll_write_ready(context) {
                Poll::Ready(Ok(ready)) => ready,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            };

            match ready.try_io(|inner| write_fd(inner.get_ref(), buffer)) {
                Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => continue,
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }
}

impl AsRawFd for PtyInput {
    /// Borrows the PTY master descriptor shared by the terminal capabilities.
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}

impl AsyncWrite for PtyInput {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_write_shared(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Window-size control for a [`Pty`].
#[derive(Debug)]
pub struct PtyControl {
    /// Shared controller descriptor and its Tokio readiness registration.
    inner: Arc<AsyncFd<OwnedFd>>,

    /// Last window size successfully applied through this crate.
    window_size: WindowSize,
}

impl PtyControl {
    /// Changes the terminal size reported to programs using the PTY.
    ///
    /// After a successful resize, [`EventLoop::new`](crate::EventLoop::new)
    /// uses this size to initialize its terminal model. To resize a running
    /// event loop, use [`EventLoopHandle::resize`](crate::EventLoopHandle::resize).
    ///
    /// # Errors
    ///
    /// Returns an error if the terminal size cannot be updated. The retained
    /// size is unchanged when this method returns an error.
    pub fn resize(&mut self, window_size: WindowSize) -> io::Result<()> {
        // FIXME: On illumos and Solaris, TIOCSWINSZ can block indefinitely behind
        // flow-controlled master-to-slave input when the slave remains open but is not
        // being read. Move STREAMS ioctl handling into a Tokio-monitored asynchronous
        // task so it cannot stall the event loop.
        let applied: io::Result<()> =
            rustix::termios::tcsetwinsize(self.inner.get_ref(), to_winsize(window_size))
                .map_err(Into::into);
        update_retained_window_size(&mut self.window_size, window_size, applied)
    }

    /// Return the size last successfully applied through this crate.
    pub(crate) fn recorded_window_size(&self) -> WindowSize {
        self.window_size
    }

    /// Borrow the controller descriptor for control-side kernel probes.
    #[cfg(target_os = "dragonfly")]
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.inner.get_ref().as_fd()
    }
}

impl AsRawFd for PtyControl {
    /// Borrows the PTY master descriptor shared by the terminal capabilities.
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}

/// A child process and the master end of its pseudoterminal.
///
/// Read and write terminal bytes through [`AsyncRead`] and [`AsyncWrite`].
/// The inherent methods forward process operations to [`Self::child`] and
/// resizing to [`Self::control`]. Move the fields out to handle each direction,
/// terminal control, and the child independently.
/// [`EventLoop`](crate::EventLoop) takes `child`, `output`, and `control`,
/// leaving `input` available for application writes.
#[derive(Debug)]
pub struct Pty {
    /// Path to the slave terminal device, recorded during construction.
    pub name: OsString,

    /// Process whose standard streams are attached to the slave end.
    pub child: Child,

    /// Combined terminal output, including the child's stdout and stderr.
    pub output: PtyOutput,

    /// Control over the terminal size reported to the child.
    pub control: PtyControl,

    /// Terminal input delivered to the slave end for the child to read.
    pub input: PtyInput,
}

impl Pty {
    /// Creates a pseudoterminal and starts `command` in a new session with the
    /// slave as its controlling terminal.
    ///
    /// The child's stdin, stdout, and stderr are connected to the slave.
    /// `window_size` is applied before the program starts. The terminal uses
    /// the platform's default settings with UTF-8-aware canonical input enabled
    /// where supported.
    ///
    /// PTY creation and process spawning complete synchronously. Call this
    /// within a Tokio runtime with I/O enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the PTY cannot be opened, its slave device name
    /// cannot be obtained, its descriptors cannot be configured or registered
    /// with Tokio, or the child cannot be configured or started.
    ///
    /// # Panics
    ///
    /// Panics if there is no current Tokio runtime with an I/O driver.
    pub fn spawn(command: Command, window_size: WindowSize) -> io::Result<Self> {
        let pty = rustix_openpty::openpty(None, Some(&to_winsize(window_size)))?;
        enable_utf8_input(pty.user.as_fd())?;
        Self::from_fds(command, window_size, pty.controller, pty.user)
    }

    /// Starts `command` on an existing PTY master/slave pair.
    ///
    /// `master` and `slave` must be opposite ends of the same pseudoterminal.
    /// The child starts a new session with `slave` as its controlling terminal
    /// and with stdin, stdout, and stderr connected to it.
    ///
    /// Applies `window_size` before the program starts and preserves the slave's
    /// other terminal settings. The master is made nonblocking for asynchronous
    /// I/O.
    ///
    /// Process spawning completes synchronously. Call this within a Tokio
    /// runtime with I/O enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the slave device name cannot be obtained, the
    /// descriptors cannot be configured or registered with Tokio, or the child
    /// cannot be configured or started.
    ///
    /// # Panics
    ///
    /// Panics if there is no current Tokio runtime with an I/O driver.
    pub fn from_fds(
        command: Command,
        window_size: WindowSize,
        master: OwnedFd,
        slave: OwnedFd,
    ) -> io::Result<Self> {
        let mut command = command.into_inner();

        // `pre_exec` closes the original PTY descriptors after `Command` has
        // installed fd 0/1/2. Keep the originals above the stdio range so a
        // caller-provided descriptor cannot alias and accidentally close one
        // of the newly installed standard streams.
        let master = move_above_stdio(master)?;
        let slave = move_above_stdio(slave)?;

        #[cfg(any(target_os = "android", target_os = "linux"))]
        let name = rustix::pty::ptsname(&master, Vec::new())?;
        #[cfg(not(any(target_os = "android", target_os = "linux")))]
        let name = rustix::termios::ttyname(&slave, Vec::new())?;
        let name = OsString::from_vec(name.into_bytes());

        // Ensure the dimensions are applied for caller-provided descriptors as
        // well as descriptors created by `spawn`.
        rustix::termios::tcsetwinsize(&slave, to_winsize(window_size))?;

        let master_fd = master.as_raw_fd();
        let slave_fd = slave.as_raw_fd();

        // Register the controller before spawning. If registration fails, no
        // detached child is left behind. The capability types partition use of
        // the shared descriptor, so sharing the registration needs no lock.
        let controller = register_controller(master)?;
        let output = PtyOutput {
            inner: Arc::clone(&controller),
        };
        let control = PtyControl {
            inner: Arc::clone(&controller),
            window_size,
        };
        let input = PtyInput { inner: controller };

        command.stdin(Stdio::from(slave.try_clone()?));
        command.stdout(Stdio::from(slave.try_clone()?));
        command.stderr(Stdio::from(slave));

        let child_setup = move || {
            // SAFETY: `TIOCSCTTY` accepts zero as its request-specific argument
            // without dereferencing a userspace pointer. Every signal number is
            // a libc-defined constant and `SIG_DFL` is a valid disposition.
            // The remaining calls have no memory-validity preconditions;
            // invalid descriptors or process state are reported by the OS.
            unsafe {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }

                #[allow(clippy::unnecessary_cast)]
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }

                // The stdio descriptors have already been installed by
                // `Command`. Close the original controller and slave
                // descriptors inherited from the parent.
                let _ = libc::close(slave_fd);
                let _ = libc::close(master_fd);

                for signal in [
                    libc::SIGCHLD,
                    libc::SIGHUP,
                    libc::SIGINT,
                    libc::SIGQUIT,
                    libc::SIGTERM,
                    libc::SIGALRM,
                ] {
                    if libc::signal(signal, libc::SIG_DFL) == libc::SIG_ERR {
                        return Err(io::Error::last_os_error());
                    }
                }

                Ok(())
            }
        };

        // SAFETY: After `fork`, `child_setup` uses only stack-local state.
        // `setsid`, `close`, and `signal` are async-signal-safe;
        // `ioctl(TIOCSCTTY)` is a direct, nonallocating kernel operation, and
        // constructing an `io::Error` from `errno` neither allocates nor locks.
        // The closure accesses no shared Rust state. `move_above_stdio` made the
        // captured descriptors greater than `STDERR_FILENO`, so closing the
        // child's copies cannot close its installed standard streams or affect
        // the parent's descriptor table. The child then execs or exits without
        // dropping the inherited Rust owners of the controller registration or
        // slave descriptor.
        unsafe {
            command.pre_exec(child_setup);
        }

        let child = Child::from_inner(command.spawn()?);
        Ok(Self {
            name,
            child,
            output,
            control,
            input,
        })
    }

    /// Calls [`Child::id`] on [`Self::child`], returning its result.
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Calls [`Child::try_wait`] on [`Self::child`], returning its result.
    ///
    /// Returns an error if the operating system cannot query the child's
    /// status.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Awaits [`Child::wait`] on [`Self::child`], returning its result.
    ///
    /// Returns an error if the operating system cannot wait for the child.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Calls [`Child::start_kill`] on [`Self::child`], returning its result.
    ///
    /// Returns an error if the termination request cannot be delivered.
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.child.start_kill()
    }

    /// Awaits [`Child::kill`] on [`Self::child`], returning its result.
    ///
    /// # Errors
    ///
    /// Returns an error if termination cannot be requested or the operating
    /// system cannot wait for the child.
    pub async fn kill(&mut self) -> io::Result<()> {
        self.child.kill().await
    }

    /// Calls [`PtyControl::resize`] on [`Self::control`] with `window_size`,
    /// returning its result.
    ///
    /// Returns an error if the terminal size cannot be updated.
    pub fn resize(&mut self, window_size: WindowSize) -> io::Result<()> {
        self.control.resize(window_size)
    }
}

impl AsRawFd for Pty {
    /// Calls [`AsRawFd::as_raw_fd`] on [`Self::control`], returning its result.
    fn as_raw_fd(&self) -> RawFd {
        self.control.as_raw_fd()
    }
}

impl AsyncRead for Pty {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().output).poll_read(context, buffer)
    }
}

impl AsyncWrite for Pty {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().input).poll_write(context, buffer)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().input).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().input).poll_shutdown(context)
    }
}

cfg_if::cfg_if! {
    // This first condition matches Rustix’s `InputModes::IUTF8` availability
    // condition with its private cfg aliases expanded.
    if #[cfg(not(any(
        target_os = "aix",
        target_os = "dragonfly",
        target_os = "emscripten",
        target_os = "freebsd",
        target_os = "haiku",
        target_os = "hurd",
        target_os = "illumos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "redox",
        target_os = "solaris",
    )))] {
        use rustix_openpty::rustix::termios::{InputModes, OptionalActions};

        /// Enable UTF-8-aware canonical input handling on a newly opened slave.
        ///
        /// # Errors
        ///
        /// Returns an error if the slave attributes cannot be read or updated.
        fn enable_utf8_input(fd: BorrowedFd<'_>) -> io::Result<()> {
            let mut attributes = rustix::termios::tcgetattr(fd)?;
            attributes.input_modes.insert(InputModes::IUTF8);
            rustix::termios::tcsetattr(fd, OptionalActions::Now, &attributes)?;
            Ok(())
        }
    } else if #[cfg(target_os = "freebsd")] {
        /// Enable UTF-8-aware canonical input handling through native terminal
        /// attributes.
        ///
        /// # Errors
        ///
        /// Returns an error if the slave attributes cannot be read or updated.
        fn enable_utf8_input(fd: BorrowedFd<'_>) -> io::Result<()> {
            // WORKAROUND: FreeBSD exposes IUTF8 in its terminal ABI, but libc and
            // rustix do not yet bind the flag. Remove the raw value when either
            // dependency exposes a named constant.
            // https://cgit.freebsd.org/src/tree/sys/sys/_termios.h
            const IUTF8: libc::tcflag_t = 0x0000_4000;

            let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
            // SAFETY: `fd` remains open for the call, and `attributes` points to
            // aligned, writable storage large enough for one native `termios` value.
            let result = unsafe {
                libc::ioctl(fd.as_raw_fd(), libc::TIOCGETA, attributes.as_mut_ptr())
            };
            if result == -1 {
                return Err(io::Error::last_os_error());
            }

            // SAFETY: A successful TIOCGETA call initialized the complete native
            // `termios` value.
            let mut attributes = unsafe { attributes.assume_init() };
            attributes.c_iflag |= IUTF8;

            // SAFETY: `fd` remains open for the call, and the pointer refers to a
            // fully initialized native `termios` value that remains live throughout
            // the synchronous ioctl.
            let result = unsafe {
                libc::ioctl(
                    fd.as_raw_fd(),
                    libc::TIOCSETA,
                    std::ptr::from_ref(&attributes),
                )
            };
            if result == -1 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        }
    } else if #[cfg(any(target_os = "illumos", target_os = "solaris"))] {
        // WORKAROUND: On illumos and Solaris, libc exposes the outer I_STR ioctl but
        // neither libc nor rustix binds its strioctl argument, the CSDATA_SET command,
        // or its ldterm payload. These definitions mirror the native ABI and can be
        // removed once a dependency supplies typed bindings.
        // https://github.com/illumos/illumos-gate/blob/master/usr/src/uts/common/sys/stropts.h
        // https://github.com/illumos/illumos-gate/blob/master/usr/src/uts/common/sys/ldterm.h
        // https://github.com/illumos/illumos-gate/blob/master/usr/src/uts/common/sys/csiioctl.h
        // https://github.com/illumos/illumos-gate/blob/master/usr/src/uts/common/sys/param.h

        /// Native STREAMS command for installing line-discipline codeset data.
        const CSDATA_SET: libc::c_int = 0xc301;

        /// Native line-discipline codeset description used with `CSDATA_SET`.
        #[repr(C)]
        struct LdtermCodeset {
            /// Version of the line-discipline codeset ABI.
            version: u8,
            /// Line-discipline codeset discriminator.
            codeset_type: u8,
            /// Native `csinfo_num` value; UTF-8 uses four.
            codeset_info_count: u8,
            /// Native ABI padding byte.
            padding: u8,
            /// NUL-terminated locale name.
            locale_name: [libc::c_char; 256],
            /// Codeset width records unused by the UTF-8 discriminator.
            widths: [[u8; 4]; 10],
        }

        const _: () = assert!(std::mem::size_of::<LdtermCodeset>() == 300);

        /// Native argument passed to the STREAMS `I_STR` ioctl.
        #[repr(C)]
        struct StrIoctl {
            /// Command sent through the stream.
            command: libc::c_int,
            /// Driver timeout in seconds.
            timeout: libc::c_int,
            /// Size of the pointed-to command payload.
            length: libc::c_int,
            /// Mutable command payload.
            data: *mut libc::c_char,
        }

        /// Enable UTF-8-aware canonical input handling in the STREAMS line
        /// discipline.
        ///
        /// # Errors
        ///
        /// Returns an error if the line discipline rejects the codeset update.
        fn enable_utf8_input(fd: BorrowedFd<'_>) -> io::Result<()> {
            let mut locale_name = [0; 256];
            for (destination, source) in locale_name.iter_mut().zip(b"UTF-8") {
                *destination = *source as libc::c_char;
            }

            let mut codeset = LdtermCodeset {
                version: 1,
                codeset_type: 3,
                codeset_info_count: 4,
                padding: 0,
                locale_name,
                widths: [[0; 4]; 10],
            };
            let mut request = StrIoctl {
                command: CSDATA_SET,
                timeout: 0,
                length: std::mem::size_of::<LdtermCodeset>() as libc::c_int,
                data: std::ptr::from_mut(&mut codeset).cast(),
            };

            // SAFETY: `fd` remains open for the call. Both `repr(C)` values exactly
            // match the native ABI, are fully initialized, and remain live throughout
            // the synchronous ioctl. `request.length` covers the complete `codeset`
            // value, and its pointer is aligned and writable.
            let result = unsafe {
                libc::ioctl(
                    fd.as_raw_fd(),
                    libc::I_STR,
                    std::ptr::from_mut(&mut request),
                )
            };
            if result == -1 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        }
    } else {
        /// Leave UTF-8 handling unchanged on targets without a supported selector.
        ///
        /// This implementation is infallible.
        fn enable_utf8_input(_fd: BorrowedFd<'_>) -> io::Result<()> {
            Ok(())
        }
    }
}

/// Attempt one read directly on a nonblocking descriptor.
///
/// # Errors
///
/// Returns an error if the descriptor read fails.
fn read_fd(fd: &OwnedFd, buffer: &mut [u8]) -> io::Result<usize> {
    rustix::io::read(fd, buffer).map_err(Into::into)
}

/// Attempt one write directly on a nonblocking descriptor.
///
/// # Errors
///
/// Returns an error if the descriptor write fails.
fn write_fd(fd: &OwnedFd, buffer: &[u8]) -> io::Result<usize> {
    rustix::io::write(fd, buffer).map_err(Into::into)
}

/// Add nonblocking mode without changing the descriptor’s other status flags.
///
/// # Errors
///
/// Returns an error if the descriptor flags cannot be read or updated.
fn set_nonblocking(fd: &OwnedFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(fd)?;
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).map_err(Into::into)
}

/// Make a controller descriptor nonblocking and register it with Tokio.
///
/// # Errors
///
/// Returns an error if descriptor configuration or runtime registration fails.
///
/// # Panics
///
/// Panics if there is no current Tokio runtime with an I/O driver.
fn register_controller(fd: OwnedFd) -> io::Result<Arc<AsyncFd<OwnedFd>>> {
    set_nonblocking(&fd)?;
    AsyncFd::new(fd).map(Arc::new)
}

/// Ensure a descriptor does not occupy the standard-input, output, or error
/// slot.
///
/// A low-numbered descriptor is duplicated with close-on-exec and the original
/// is closed when its owner is dropped.
///
/// # Errors
///
/// Returns an error if duplicating a low-numbered descriptor fails.
fn move_above_stdio(fd: OwnedFd) -> io::Result<OwnedFd> {
    if fd.as_raw_fd() > libc::STDERR_FILENO {
        return Ok(fd);
    }

    rustix::io::fcntl_dupfd_cloexec(&fd, libc::STDERR_FILENO + 1).map_err(Into::into)
}

/// Convert terminal geometry into the kernel representation.
///
/// The horizontal pixel extent is `num_cols` × `cell_width`, and the vertical
/// pixel extent is `num_lines` × `cell_height`; each product saturates at
/// `u16::MAX`.
fn to_winsize(window_size: WindowSize) -> Winsize {
    let ws_row = window_size.num_lines;
    let ws_col = window_size.num_cols;
    let ws_xpixel = ws_col.saturating_mul(window_size.cell_width);
    let ws_ypixel = ws_row.saturating_mul(window_size.cell_height);

    Winsize {
        ws_row,
        ws_col,
        ws_xpixel,
        ws_ypixel,
    }
}

/// Replace the retained size only after the kernel update succeeds.
///
/// # Errors
///
/// Returns `applied`'s error without changing `retained`.
fn update_retained_window_size(
    retained: &mut WindowSize,
    requested: WindowSize,
    applied: io::Result<()>,
) -> io::Result<()> {
    applied?;
    *retained = requested;
    Ok(())
}

/// Unit tests for PTY capability traits and terminal geometry conversion.
#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;

    /// Directional capabilities and their aggregate expose the intended Tokio
    /// byte-stream traits.
    #[test]
    fn pty_capabilities_implement_async_io() {
        fn assert_async_read<T: AsyncRead + Unpin>() {}
        fn assert_async_write<T: AsyncWrite + Unpin>() {}
        fn assert_async_io<T: AsyncRead + AsyncWrite + Unpin>() {}

        assert_async_read::<PtyOutput>();
        assert_async_write::<PtyInput>();
        assert_async_io::<Pty>();
    }

    /// Empty directional I/O, flushing, and writer shutdown complete
    /// immediately, and writer shutdown retains the input descriptor.
    #[tokio::test]
    #[ntest::timeout(15_000)]
    async fn empty_io_and_shutdown_retain_input() {
        let window_size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        };
        let pty = rustix_openpty::openpty(None, Some(&to_winsize(window_size))).unwrap();
        let _slave = pty.user;
        let controller = register_controller(pty.controller).unwrap();
        let mut output = PtyOutput {
            inner: Arc::clone(&controller),
        };
        let _control = PtyControl {
            inner: Arc::clone(&controller),
            window_size,
        };
        let mut input = PtyInput { inner: controller };

        let mut empty = [];
        assert_eq!(output.read(&mut empty).await.unwrap(), 0);
        assert_eq!(input.write(&[]).await.unwrap(), 0);
        input.flush().await.unwrap();
        input.shutdown().await.unwrap();

        rustix::fs::fcntl_getfl(input.inner.get_ref()).unwrap();
    }

    /// The three capabilities share one readiness registration and descriptor.
    #[tokio::test]
    async fn capabilities_share_controller_registration() {
        let pty = rustix_openpty::openpty(None, None).unwrap();
        let controller = register_controller(pty.controller).unwrap();
        let output = PtyOutput {
            inner: Arc::clone(&controller),
        };
        let control = PtyControl {
            inner: Arc::clone(&controller),
            window_size: WindowSize {
                num_lines: 24,
                num_cols: 80,
                cell_width: 8,
                cell_height: 16,
            },
        };
        let input = PtyInput { inner: controller };

        assert!(Arc::ptr_eq(&output.inner, &input.inner));
        assert!(Arc::ptr_eq(&output.inner, &control.inner));
        assert_eq!(
            output.inner.get_ref().as_raw_fd(),
            input.inner.get_ref().as_raw_fd()
        );
        assert_eq!(
            output.inner.get_ref().as_raw_fd(),
            control.inner.get_ref().as_raw_fd()
        );
    }

    /// A successful application replaces the retained size, while an error
    /// preserves the previously retained size and error kind.
    #[test]
    fn retained_window_size_follows_application_result() {
        let initial = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        };
        let replacement = WindowSize {
            num_lines: 30,
            num_cols: 100,
            cell_width: 9,
            cell_height: 18,
        };
        let mut retained = initial;

        let error = update_retained_window_size(
            &mut retained,
            replacement,
            Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            (
                retained.num_lines,
                retained.num_cols,
                retained.cell_width,
                retained.cell_height,
            ),
            (
                initial.num_lines,
                initial.num_cols,
                initial.cell_width,
                initial.cell_height,
            )
        );

        update_retained_window_size(&mut retained, replacement, Ok(())).unwrap();
        assert_eq!(
            (
                retained.num_lines,
                retained.num_cols,
                retained.cell_width,
                retained.cell_height,
            ),
            (
                replacement.num_lines,
                replacement.num_cols,
                replacement.cell_width,
                replacement.cell_height,
            )
        );
    }

    /// Pixel extents saturate while row and column counts remain unchanged.
    #[test]
    fn pixel_dimension_saturation() {
        let winsize = to_winsize(WindowSize {
            num_lines: 100,
            num_cols: u16::MAX,
            cell_width: 2,
            cell_height: u16::MAX,
        });

        assert_eq!(winsize.ws_row, 100);
        assert_eq!(winsize.ws_col, u16::MAX);
        assert_eq!(winsize.ws_xpixel, u16::MAX);
        assert_eq!(winsize.ws_ypixel, u16::MAX);
    }

    /// Without overflow, row and column counts are copied unchanged and pixel
    /// extents equal their exact cell products.
    #[test]
    fn pixel_dimension_products() {
        let winsize = to_winsize(WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        });

        assert_eq!(winsize.ws_row, 24);
        assert_eq!(winsize.ws_col, 80);
        assert_eq!(winsize.ws_xpixel, 640);
        assert_eq!(winsize.ws_ypixel, 384);
    }
}
