// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 Daniel Fox Franke

//! Configure, spawn, and manage the lifetime of a PTY child process.

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::ExitStatus;

/// A program and its process options, ready to be spawned on a pseudoterminal.
///
/// Pass the command to [`Pty::spawn`](crate::Pty::spawn) to create its terminal
/// and start the process, or to [`Pty::from_fds`](crate::Pty::from_fds) to use an
/// existing terminal.
#[derive(Debug)]
pub struct Command {
    /// Underlying process builder consumed by PTY setup.
    inner: tokio::process::Command,
}

impl Command {
    /// Selects the executable to run, with no arguments and with the parent's
    /// environment and working directory inherited by default.
    ///
    /// A program name without a path is looked up in `PATH` when spawned.
    pub fn new<S>(program: S) -> Self
    where
        S: AsRef<OsStr>,
    {
        Self {
            inner: tokio::process::Command::new(program),
        }
    }

    /// Appends `argument` as one literal command-line argument.
    pub fn arg<S>(&mut self, argument: S) -> &mut Self
    where
        S: AsRef<OsStr>,
    {
        self.inner.arg(argument);
        self
    }

    /// Appends the arguments in iteration order, each as one literal
    /// command-line argument.
    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(arguments);
        self
    }

    /// Sets a child environment variable, overriding its inherited or
    /// previously configured value.
    pub fn env<K, V>(&mut self, key: K, value: V) -> &mut Self
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.env(key, value);
        self
    }

    /// Sets child environment variables as with repeated calls to [`Self::env`].
    pub fn envs<I, K, V>(&mut self, variables: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.inner.envs(variables);
        self
    }

    /// Prevents the child from receiving the named environment variable,
    /// whether inherited or configured earlier on this command.
    pub fn env_remove<K>(&mut self, key: K) -> &mut Self
    where
        K: AsRef<OsStr>,
    {
        self.inner.env_remove(key);
        self
    }

    /// Prevents the child from inheriting any environment variables and
    /// removes variables configured earlier on this command.
    ///
    /// Variables added after this call are still provided to the child.
    pub fn env_clear(&mut self) -> &mut Self {
        self.inner.env_clear();
        self
    }

    /// Sets the directory in which the child program starts.
    pub fn current_dir<P>(&mut self, directory: P) -> &mut Self
    where
        P: AsRef<Path>,
    {
        self.inner.current_dir(directory);
        self
    }

    /// Controls whether dropping the resulting [`Child`] requests termination
    /// with `SIGKILL`.
    ///
    /// Disabled by default. To ensure termination and reaping have completed,
    /// call [`Child::kill`] and await it before dropping the child.
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.inner.kill_on_drop(kill_on_drop);
        self
    }

    /// Sets the user ID under which the child program runs.
    ///
    /// Failure to change the user ID causes spawning to return an error.
    pub fn uid(&mut self, id: u32) -> &mut Self {
        self.inner.uid(id);
        self
    }

    /// Sets the group ID under which the child program runs.
    ///
    /// Failure to change the group ID causes spawning to return an error.
    pub fn gid(&mut self, id: u32) -> &mut Self {
        self.inner.gid(id);
        self
    }

    /// Overrides `argv[0]` without changing the executable selected by
    /// [`Command::new`].
    pub fn arg0<S>(&mut self, argument: S) -> &mut Self
    where
        S: AsRef<OsStr>,
    {
        self.inner.arg0(argument);
        self
    }

    /// Releases the process builder for crate-internal PTY setup.
    pub(crate) fn into_inner(self) -> tokio::process::Command {
        self.inner
    }
}

/// The process attached to the slave end of a [`Pty`](crate::Pty).
///
/// Use this handle to collect the exit status or terminate the process. Its
/// terminal I/O is available separately through [`PtyOutput`](crate::PtyOutput)
/// and [`PtyInput`](crate::PtyInput).
#[derive(Debug)]
pub struct Child {
    /// Underlying process handle used for lifecycle operations.
    inner: tokio::process::Child,
}

impl Child {
    /// Wraps a child created by the crate's PTY setup.
    pub(crate) fn from_inner(inner: tokio::process::Child) -> Self {
        Self { inner }
    }

    /// Returns the child's process ID, or `None` after its exit status has been
    /// collected by [`Self::wait`], [`Self::try_wait`], or [`Self::kill`].
    pub fn id(&self) -> Option<u32> {
        self.inner.id()
    }

    /// Reaps the child and returns its exit status if it has exited, or returns
    /// `None` if the status is not yet available.
    ///
    /// Once an exit status has been returned, later calls return the same
    /// status. This method does not wait for the child to exit.
    ///
    /// Returns an error if the operating system cannot query the child's
    /// status.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.inner.try_wait()
    }

    /// Waits for the child to exit, reaps it, and returns its exit status.
    ///
    /// Once an exit status has been returned, later calls return the same
    /// status. Cancelling this wait leaves the child available to wait on again.
    ///
    /// Returns an error if the operating system cannot wait for the child.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.inner.wait().await
    }

    /// Sends `SIGKILL` to the child without waiting for it to exit.
    ///
    /// Call [`Child::wait`] afterward to ensure the child has exited and been
    /// reaped. Succeeds without sending a signal if the exit status has already
    /// been collected.
    ///
    /// Returns an error if the termination request cannot be delivered.
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner.start_kill()
    }

    /// Sends `SIGKILL` to the child, then waits for its exit and reaps it.
    ///
    /// Succeeds without sending a signal if the exit status has already been
    /// collected.
    ///
    /// # Errors
    ///
    /// Returns an error if termination cannot be requested or the operating
    /// system cannot wait for the child.
    pub async fn kill(&mut self) -> io::Result<()> {
        self.inner.kill().await
    }
}
