//! POSIX system call bindings and safe wrappers

use std::ffi::CStr;
use std::io;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 1;
pub const POLLERR: i16 = 8;
pub const POLLHUP: i16 = 16;

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGUSR1: i32 = 10;
pub const SIGTERM: i32 = 15;
pub const SIGCHLD: i32 = 17;

pub const IN_CLOEXEC: i32 = 0x00080000;
pub const IN_NONBLOCK: i32 = 0x00000800;
pub const IN_CREATE: u32 = 0x00000100;
pub const IN_DELETE: u32 = 0x00000200;

#[repr(C)]
struct Ifaddrs {
    ifa_next: *mut Ifaddrs,
    ifa_name: *mut std::ffi::c_char,
    ifa_flags: u32,
    ifa_addr: *mut SockAddr,
    ifa_netmask: *mut SockAddr,
    ifa_ifu: *mut std::ffi::c_void,
    ifa_data: *mut std::ffi::c_void,
}

#[repr(C)]
struct SockAddr {
    sa_family: u16,
    sa_data: [std::ffi::c_char; 14],
}

#[repr(C)]
struct SockAddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: InAddr,
    sin_zero: [u8; 8],
}

#[repr(C)]
struct InAddr {
    s_addr: u32,
}

const AF_INET: u16 = 2;

unsafe extern "C" {
    fn getuid() -> u32;
    fn getpid() -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    fn pipe(pipefd: *mut i32) -> i32;
    fn close(fd: i32) -> i32;
    fn read(fd: i32, buf: *mut std::ffi::c_void, count: usize) -> isize;
    fn write(fd: i32, buf: *const std::ffi::c_void, count: usize) -> isize;
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    fn unlink(path: *const std::ffi::c_char) -> i32;
    fn rename(oldpath: *const std::ffi::c_char, newpath: *const std::ffi::c_char) -> i32;
    fn inotify_init1(flags: i32) -> i32;
    fn inotify_add_watch(fd: i32, pathname: *const std::ffi::c_char, mask: u32) -> i32;
    fn getifaddrs(ifap: *mut *mut Ifaddrs) -> i32;
    fn freeifaddrs(ifa: *mut Ifaddrs);
}

pub fn posix_getuid() -> u32 {
    unsafe { getuid() }
}

pub fn posix_getpid() -> i32 {
    unsafe { getpid() }
}

pub fn posix_kill(pid: i32, sig: i32) -> io::Result<()> {
    let ret = unsafe { kill(pid, sig) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_signal(sig: i32, handler: extern "C" fn(i32)) -> usize {
    unsafe { signal(sig, handler) }
}

pub fn posix_signal_ign(sig: i32) -> usize {
    unsafe { signal(sig, std::mem::transmute::<usize, extern "C" fn(i32)>(1usize)) }
}

pub fn posix_pipe() -> io::Result<(i32, i32)> {
    let mut fds = [0i32; 2];
    let ret = unsafe { pipe(fds.as_mut_ptr()) };
    if ret == 0 {
        Ok((fds[0], fds[1]))
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_close(fd: i32) -> io::Result<()> {
    let ret = unsafe { close(fd) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_read(fd: i32, buf: &mut [u8]) -> io::Result<usize> {
    let ret = unsafe { read(fd, buf.as_mut_ptr() as *mut std::ffi::c_void, buf.len()) };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_write(fd: i32, buf: &[u8]) -> io::Result<usize> {
    let ret = unsafe { write(fd, buf.as_ptr() as *const std::ffi::c_void, buf.len()) };
    if ret >= 0 {
        Ok(ret as usize)
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_poll(fds: &mut [PollFd], timeout_ms: i32) -> io::Result<i32> {
    let ret = unsafe { poll(fds.as_mut_ptr(), fds.len(), timeout_ms) };
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_unlink(path: &CStr) -> io::Result<()> {
    let ret = unsafe { unlink(path.as_ptr()) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_rename(oldpath: &CStr, newpath: &CStr) -> io::Result<()> {
    let ret = unsafe { rename(oldpath.as_ptr(), newpath.as_ptr()) };
    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_inotify_init1(flags: i32) -> io::Result<i32> {
    let ret = unsafe { inotify_init1(flags) };
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn posix_inotify_add_watch(fd: i32, path: &CStr, mask: u32) -> io::Result<i32> {
    let ret = unsafe { inotify_add_watch(fd, path.as_ptr(), mask) };
    if ret >= 0 {
        Ok(ret)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Retrieve IPv4 address with CIDR netmask for a given interface (e.g., "192.168.1.50/24")
/// using the native POSIX `getifaddrs` system call.
pub fn get_interface_ipv4(target_iface: &str) -> Option<String> {
    let mut ifap: *mut Ifaddrs = std::ptr::null_mut();
    if unsafe { getifaddrs(&mut ifap) } != 0 {
        return None;
    }
    if ifap.is_null() {
        return None;
    }

    let mut result = None;
    let mut curr = ifap;

    while !curr.is_null() {
        let entry = unsafe { &*curr };
        if !entry.ifa_name.is_null() {
            let name = unsafe { CStr::from_ptr(entry.ifa_name) }.to_string_lossy();
            if name == target_iface && !entry.ifa_addr.is_null() {
                let sa = unsafe { &*entry.ifa_addr };
                if sa.sa_family == AF_INET {
                    let sin = unsafe { &*(entry.ifa_addr as *const SockAddrIn) };
                    let ip = u32::from_be(sin.sin_addr.s_addr);
                    let ip_str = format!(
                        "{}.{}.{}.{}",
                        (ip >> 24) & 0xff,
                        (ip >> 16) & 0xff,
                        (ip >> 8) & 0xff,
                        ip & 0xff
                    );

                    let cidr_suffix = if !entry.ifa_netmask.is_null() {
                        let mask_sa = unsafe { &*entry.ifa_netmask };
                        if mask_sa.sa_family == AF_INET {
                            let mask_sin = unsafe { &*(entry.ifa_netmask as *const SockAddrIn) };
                            let mask_bits = mask_sin.sin_addr.s_addr.count_ones();
                            format!("/{}", mask_bits)
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };

                    result = Some(format!("{}{}", ip_str, cidr_suffix));
                    break;
                }
            }
        }
        curr = entry.ifa_next;
    }

    unsafe {
        freeifaddrs(ifap);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posix_getuid_getpid() {
        assert!(posix_getuid() < 65535 || posix_getuid() >= 1000);
        assert!(posix_getpid() > 0);
    }

    #[test]
    fn test_posix_pipe() {
        let (r, w) = posix_pipe().expect("pipe should succeed");
        assert!(r >= 0 && w >= 0);
        let msg = b"inerium";
        let nw = posix_write(w, msg).expect("write should succeed");
        assert_eq!(nw, msg.len());

        let mut buf = [0u8; 16];
        let nr = posix_read(r, &mut buf).expect("read should succeed");
        assert_eq!(nr, msg.len());
        assert_eq!(&buf[..nr], msg);

        let _ = posix_close(r);
        let _ = posix_close(w);
    }

    #[test]
    fn test_get_interface_ipv4_lo() {
        // lo should always have 127.0.0.1/8
        let lo_ip = get_interface_ipv4("lo");
        assert!(lo_ip.is_some());
        let val = lo_ip.unwrap();
        assert!(val.starts_with("127.0.0.1"), "lo ip: {}", val);
    }
}
