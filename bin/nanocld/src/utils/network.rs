use std::io::Error;
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr};
use libc::{ioctl, c_char, sockaddr, sockaddr_in};

pub(crate) fn get_default_ip() -> std::io::Result<IpAddr> {
  let socket = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
  if socket < 0 {
    return Err(Error::last_os_error());
  }

  let ifname = "eth0\0";
  let mut ifr = MaybeUninit::<libc::ifreq>::uninit();
  unsafe {
    let ifr_ptr = ifr.as_mut_ptr();
    let name_ptr = &mut (*ifr_ptr).ifr_name[0] as *mut i8;
    let c_ifname = CStr::from_bytes_with_nul(ifname.as_bytes()).unwrap();
    c_ifname
      .as_ptr()
      .copy_to_nonoverlapping(name_ptr, c_ifname.to_bytes().len() + 1);
    ioctl(socket, libc::SIOCGIFADDR, ifr_ptr as *mut c_char);
  }

  let ifr = unsafe { ifr.assume_init() };
  let sa = unsafe { &ifr.ifr_ifru.ifru_addr as *const sockaddr };
  let sin = sa as *const sockaddr_in;
  let addr = unsafe { (*sin).sin_addr.s_addr };
  let ip = Ipv4Addr::from(u32::from_be(addr));

  unsafe { libc::close(socket) };
  log::info!("Default ip detected: {ip}");
  log::info!("You can override this with the --host-gateway option");
  Ok(IpAddr::V4(ip))
}
