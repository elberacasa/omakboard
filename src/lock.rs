//! One conversation with the keyboard at a time.
//!
//! Replies are matched to requests by order, so two processes talking to the keyboard at once
//! would take each other's replies. The overlay's server and every command take this lock
//! around each exchange, and the server never holds it while idle.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct DeviceLock {
    _file: File,
}

impl DeviceLock {
    /// Waits up to `timeout` for the lock every omakboard shares.
    pub fn acquire(timeout: Duration) -> io::Result<Self> {
        Self::acquire_at(&default_path(), timeout)
    }

    pub fn acquire_at(path: &Path, timeout: Duration) -> io::Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        let deadline = Instant::now() + timeout;
        loop {
            // SAFETY: flock on a descriptor this function owns; it is released when the file closes.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Self { _file: file });
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(err);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "another omakboard is talking to the keyboard; try again in a moment",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub fn default_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join("omakboard/device.lock"),
        // SAFETY: getuid cannot fail.
        None => std::env::temp_dir().join(format!("omakboard-{}.lock", unsafe { libc::getuid() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_waits_then_gives_up() {
        let dir = std::env::temp_dir().join(format!("omakboard-lock-test-{}", std::process::id()));
        let path = dir.join("device.lock");
        let first = DeviceLock::acquire_at(&path, Duration::from_millis(50)).unwrap();
        let second = DeviceLock::acquire_at(&path, Duration::from_millis(30));
        assert_eq!(
            second.err().map(|e| e.kind()),
            Some(io::ErrorKind::WouldBlock)
        );
        drop(first);
        assert!(DeviceLock::acquire_at(&path, Duration::from_millis(50)).is_ok());
        let _ = fs::remove_dir_all(dir);
    }
}
