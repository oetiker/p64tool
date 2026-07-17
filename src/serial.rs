//! Minimal Linux serial port via libc termios.
//!
//! A serial port on Linux is just a character device file. We open it, put the
//! line discipline into raw 8N1 at the requested baud, and read/write bytes.
//! This keeps the dependency footprint tiny (just `libc`) so the binary can be
//! built fully static against musl and copied to any Linux box.

use anyhow::{Context, Result};
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

pub struct Serial {
    fd: RawFd,
    path: String,
}

impl Serial {
    /// Open `path` and configure it for raw 8N1 at 115200 baud, no flow control.
    pub fn open(path: &str) -> Result<Serial> {
        let cpath = std::ffi::CString::new(path).context("port path contains NUL")?;
        // Blocking open (no O_NONBLOCK): VMIN/VTIME below give us read timeouts.
        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| format!("opening {path}"));
        }

        let mut tio: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut tio) } != 0 {
            let e = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(e).with_context(|| format!("tcgetattr {path}"));
        }

        // Raw mode: clear input/output/local processing, set 8N1.
        tio.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON);
        tio.c_oflag &= !libc::OPOST;
        tio.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG | libc::IEXTEN);
        tio.c_cflag &= !(libc::CSIZE | libc::PARENB | libc::CSTOPB | libc::CRTSCTS);
        tio.c_cflag |= libc::CS8 | libc::CLOCAL | libc::CREAD;

        // Non-blocking-ish reads: return whatever is available; each read() call
        // waits up to VTIME*0.1s for the first byte.
        tio.c_cc[libc::VMIN] = 0;
        tio.c_cc[libc::VTIME] = 1; // 0.1s

        let speed = libc::B115200;
        unsafe {
            libc::cfsetispeed(&mut tio, speed);
            libc::cfsetospeed(&mut tio, speed);
        }

        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &tio) } != 0 {
            let e = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(e).with_context(|| format!("tcsetattr {path}"));
        }

        // Assert DTR and RTS. With both modem-control lines low the radio stays
        // silent (returns 0 bytes to CONNECT); it only answers once a line is
        // raised. The driver's power-on default for these lines varies by kernel
        // and across USB re-enumeration, so we must not rely on it — raise both
        // explicitly. Best-effort: a cable/driver that does not support the ioctl
        // is left as-is rather than failing the open.
        unsafe {
            let mut status: libc::c_int = 0;
            if libc::ioctl(fd, libc::TIOCMGET, &mut status) == 0 {
                status |= libc::TIOCM_DTR | libc::TIOCM_RTS;
                libc::ioctl(fd, libc::TIOCMSET, &status);
            }
        }

        let s = Serial {
            fd,
            path: path.to_string(),
        };
        s.flush_input()?;
        Ok(s)
    }

    /// Discard any pending input and output.
    pub fn flush_input(&self) -> Result<()> {
        if unsafe { libc::tcflush(self.fd, libc::TCIOFLUSH) } != 0 {
            return Err(std::io::Error::last_os_error()).context("tcflush");
        }
        Ok(())
    }

    pub fn write_all(&self, buf: &[u8]) -> Result<()> {
        let mut off = 0;
        while off < buf.len() {
            let n = unsafe {
                libc::write(
                    self.fd,
                    buf[off..].as_ptr() as *const libc::c_void,
                    buf.len() - off,
                )
            };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e).with_context(|| format!("writing to {}", self.path));
            }
            off += n as usize;
        }
        // Block until the bytes have actually been shifted out.
        unsafe { libc::tcdrain(self.fd) };
        Ok(())
    }

    /// Read one chunk of up to `buf.len()` bytes. Returns 0 on a read timeout.
    fn read_some(&self, buf: &mut [u8]) -> Result<usize> {
        loop {
            let n =
                unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e).with_context(|| format!("reading from {}", self.path));
            }
            return Ok(n as usize);
        }
    }

    /// Read up to `expected` bytes. Stops when `expected` is reached, when no new
    /// byte arrives for `gap`, or when the total `deadline` elapses.
    pub fn read_response(
        &self,
        expected: usize,
        gap: Duration,
        deadline: Duration,
    ) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(expected.max(64));
        let start = Instant::now();
        let mut last_progress = Instant::now();
        let mut tmp = [0u8; 4096];
        loop {
            if out.len() >= expected {
                break;
            }
            let want = (expected - out.len()).min(tmp.len());
            let n = self.read_some(&mut tmp[..want])?;
            if n > 0 {
                out.extend_from_slice(&tmp[..n]);
                last_progress = Instant::now();
            } else {
                // Timed out waiting for a byte.
                if !out.is_empty() && last_progress.elapsed() >= gap {
                    break;
                }
            }
            if start.elapsed() >= deadline {
                break;
            }
        }
        Ok(out)
    }
}

impl Drop for Serial {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}
