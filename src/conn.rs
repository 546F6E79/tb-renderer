//! SOCK_SEQPACKET connection to /run/t1bridge/touchbar.sock + poll helper.
//! std's UnixStream is SOCK_STREAM, so raw libc is used here.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

pub const SOCKET_PATH: &str = "/run/t1bridge/touchbar.sock";

pub struct Conn {
    pub fd: i32,
}

impl Drop for Conn {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

impl Conn {
    pub fn connect(path: &str) -> io::Result<Conn> {
        std::ffi::CString::new(path).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path"))?;
        let fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                0,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
        let bytes = Path::new(path).as_os_str().as_bytes();
        if bytes.len() >= addr.sun_path.len() {
            unsafe { libc::close(fd) };
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "path too long"));
        }
        for (dst, src) in addr.sun_path[..bytes.len()].iter_mut().zip(bytes.iter()) {
                *dst = *src as libc::c_char;
            }
        let len = (addr.sun_path.as_ptr() as usize - &addr as *const _ as usize)
            + bytes.len()
            + 1;
        let rc = unsafe {
            libc::connect(
                fd,
                &addr as *const _ as *const libc::sockaddr,
                len as libc::socklen_t,
            )
        };
        if rc != 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }
        Ok(Conn { fd })
    }

    /// Send one datagram (EINTR/EAGAIN retried a few times, mirroring the reference client).
    pub fn send(&self, buf: &[u8]) -> io::Result<()> {
        for attempt in 0..64u32 {
            let rc = unsafe { libc::send(self.fd, buf.as_ptr() as *const libc::c_void, buf.len(), libc::MSG_NOSIGNAL) };
            if rc >= 0 {
                return Ok(());
            }
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                std::thread::sleep(std::time::Duration::from_millis(2));
                let _ = attempt;
                continue;
            }
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "send would-block loop exhausted"))
    }

    /// Receive one datagram into `buf`; None on EAGAIN (nothing yet), Err on real errors.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<Option<usize>> {
        let rc = unsafe { libc::recv(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), libc::MSG_CMSG_CLOEXEC) };
        if rc >= 0 {
            return Ok(Some(rc as usize));
        }
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock {
            return Ok(None);
        }
        if err.kind() == io::ErrorKind::Interrupted {
            return Ok(None);
        }
        Err(err)
    }
}

/// Wait until `fd` is readable or timeout passes. true = readable.
pub fn wait_readable(fd: i32, timeout_ms: i32) -> bool {
    let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    rc > 0 && (pfd.revents & libc::POLLIN) != 0
}
