//! Terminal-only processing leaves child reaping independent of listener progress.

use asyncritty::{Command, EventListener, EventLoop, Pty, WindowSize};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::Notify;

/// A listener whose title callback waits indefinitely after announcing entry.
struct BlockedTitle {
    /// Signals that output was parsed and the callback admitted.
    entered: Arc<Notify>,
}
impl EventListener for BlockedTitle {
    type Error = io::Error;
    async fn title(&self, _: String) -> io::Result<()> {
        self.entered.notify_one();
        std::future::pending().await
    }
}

/// Child status can be reaped while the terminal is blocked in an unrelated callback.
#[tokio::test]
#[ntest::timeout(15_000)]
async fn child_reaping_is_independent_of_listener() {
    let mut command = Command::new("/bin/sh");
    command.kill_on_drop(true);
    command.args(["-c", r"printf '\033]2;blocked\007'"]);
    let Pty {
        mut child,
        output,
        control,
        input,
        ..
    } = Pty::spawn(
        command,
        WindowSize {
            num_lines: 2,
            num_cols: 3,
            cell_width: 0,
            cell_height: 0,
        },
    )
    .unwrap();
    let entered = Arc::new(Notify::new());
    let (event_loop, handle) =
        EventLoop::new(output, control, Default::default(), |_| BlockedTitle {
            entered: entered.clone(),
        });
    let task = tokio::spawn(event_loop.run());
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    task.abort();
    let _ = task.await;
    handle.wait_for_shutdown().await;
    drop(input);
}
