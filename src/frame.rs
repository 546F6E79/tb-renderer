//! Frame buffer: memfd + seals + SCM_RIGHTS registration + RW mapping for drawing.

use std::io;
use std::os::fd::RawFd;

use crate::conn::Conn;
use crate::wire;

use libc::{F_ADD_SEALS, F_SEAL_GROW, F_SEAL_SEAL, F_SEAL_SHRINK};

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    fd: RawFd,
    map: *mut u8,
    map_len: usize,
    pub buffer_id: u32,
}

// The mapping is exclusively ours while alive (the service maps its own RO copy of the memfd).
unsafe impl Send for Frame {}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            if !self.map.is_null() {
                libc::munmap(self.map as *mut libc::c_void, self.map_len);
            }
            libc::close(self.fd);
        }
    }
}

impl Frame {
    pub fn create(width: u32, height: u32, buffer_id: u32) -> io::Result<Frame> {
        let stride = width.checked_mul(4).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "stride"))?;
        let len = (stride as u64)
            .checked_mul(height as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "len"))? as usize;
        let name = c"tb-renderer";
        let fd = unsafe {
            libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING)
        };
        if fd < 0 {
            let e = io::Error::last_os_error();
            return Err(io::Error::new(e.kind(), format!("memfd_create: {e}")));
        }
        if unsafe { libc::ftruncate(fd, len as libc::off_t) } != 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(io::Error::new(e.kind(), format!("ftruncate: {e}")));
        }
        // Seal the SIZE only (F_SEAL_WRITE must NOT be set: we keep drawing through our mapping).
        if unsafe { libc::fcntl(fd, F_ADD_SEALS, (F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_SEAL) as libc::c_ulong) } != 0 {
            let e = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(io::Error::new(e.kind(), format!("fcntl seals: {e}")));
        }
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if map == libc::MAP_FAILED {
            let e = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(io::Error::new(e.kind(), format!("mmap: {e}")));
        }
        Ok(Frame { width, height, stride, fd, map: map as *mut u8, map_len: len, buffer_id })
    }

    pub fn pixels(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.map, self.map_len) }
    }

    /// RegisterBuffer + the exactly-one SCM_RIGHTS fd.
    pub fn register(&self, conn: &Conn, req_id: u32) -> io::Result<()> {
        let msg = wire::register_buffer(self.buffer_id, self.stride, self.map_len as u64, req_id);
        send_with_fd(conn, &msg, self.fd)
    }
}

pub fn send_with_fd(conn: &Conn, msg: &[u8], fd: RawFd) -> io::Result<()> {
    let mut iov = libc::iovec {
        iov_base: msg.as_ptr() as *mut libc::c_void,
        iov_len: msg.len(),
    };
    let mut cmsg_space = [0u8; unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize }];
    let mut hdr: libc::msghdr = unsafe { std::mem::zeroed() };
    hdr.msg_iov = &mut iov;
    hdr.msg_iovlen = 1;
    hdr.msg_control = cmsg_space.as_mut_ptr() as *mut libc::c_void;
    hdr.msg_controllen = cmsg_space.len();
    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&hdr) };
    debug_assert!(!cmsg.is_null());
    unsafe {
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;
        let data = libc::CMSG_DATA(cmsg) as *mut RawFd;
        *data = fd;
    }
    for _ in 0..64 {
        let rc = unsafe { libc::sendmsg(conn.fd, &hdr, libc::MSG_NOSIGNAL) };
        if rc >= 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::WouldBlock {
            std::thread::sleep(std::time::Duration::from_millis(2));
            continue;
        }
        if err.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return Err(err);
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "sendmsg would-block loop exhausted"))
}
