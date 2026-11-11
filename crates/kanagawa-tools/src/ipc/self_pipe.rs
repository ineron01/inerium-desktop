//! Self-Pipe Trick for atomic and race-free signal handling (e.g., SIGUSR1).

use std::sync::atomic::{AtomicI32, Ordering};
use crate::sys::posix::*;

static WAKE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn sigusr1_handler(_: i32) {
    let fd = WAKE_FD.load(Ordering::Relaxed);
    if fd >= 0 {
        let b = [1u8];
        let _ = posix_write(fd, &b);
    }
}

pub struct SelfPipe {
    read_fd: i32,
    write_fd: i32,
}

impl SelfPipe {
    /// Create a new SelfPipe and install SIGUSR1 handler.
    pub fn new() -> io::Result<Self> {
        let (r, w) = posix_pipe()?;
        WAKE_FD.store(w, Ordering::SeqCst);
        posix_signal(SIGUSR1, sigusr1_handler);
        Ok(Self {
            read_fd: r,
            write_fd: w,
        })
    }

    #[inline]
    pub fn read_fd(&self) -> i32 {
        self.read_fd
    }

    /// Wait up to `timeout_ms` for a signal or timeout.
    /// Returns `true` if woken by a signal, `false` if timed out.
    pub fn wait_event(&self, timeout_ms: i32) -> bool {
        let mut fds = [PollFd {
            fd: self.read_fd,
            events: POLLIN,
            revents: 0,
        }];

        if let Ok(n) = posix_poll(&mut fds, timeout_ms)
            && n > 0 && (fds[0].revents & POLLIN != 0) {
                self.drain();
                return true;
            }
        false
    }

    /// Drain all pending wake bytes from the pipe.
    pub fn drain(&self) {
        let mut buf = [0u8; 64];
        let _ = posix_read(self.read_fd, &mut buf);
    }
}

impl Drop for SelfPipe {
    fn drop(&mut self) {
        WAKE_FD.store(-1, Ordering::SeqCst);
        let _ = posix_close(self.read_fd);
        let _ = posix_close(self.write_fd);
    }
}

use std::io;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_pipe_timeout() {
        let sp = SelfPipe::new().expect("Failed to create SelfPipe");
        // Wait 10ms with no signal
        assert!(!sp.wait_event(10));
    }
}
