use std::io::Error;
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::net::{IpAddr, Ipv4Addr};
use libc::sockaddr_in;

/// Get the default IP address of the system.
/// This is used to determine the IP address of the host.
/// We detect it by reading the default route to know the default interface name.
/// Then we get the IP address of the interface.
pub(crate) fn get_default_ip() -> std::io::Result<IpAddr> {
  // Detect default interface name by reading the default route.
  let routes = std::fs::read_to_string("/proc/net/route")?;
  let mut default_interface = None;
  for line in routes.lines() {
    let mut parts = line.split_whitespace();
    let interface = parts.next().unwrap();
    let destination = parts.next().unwrap();
    if destination == "00000000" {
      default_interface = Some(interface);
      break;
    }
  }
  let default_interface = default_interface.ok_or(Error::new(
    std::io::ErrorKind::NotFound,
    "No default route found",
  ))?;

  println!("Default interface: {default_interface}");
  // Get the IP address of the default interface.
  // Using getifaddrs call from libc
  let mut ip = None;
  let mut ifaddrs = MaybeUninit::uninit();
  let ret = unsafe { libc::getifaddrs(ifaddrs.as_mut_ptr()) };
  if ret != 0 {
    return Err(Error::last_os_error());
  }
  let ifaddrs = unsafe { ifaddrs.assume_init() };
  let mut ifa = ifaddrs;
  while !ifa.is_null() {
    let ifa_name = unsafe { CStr::from_ptr((*ifa).ifa_name) };
    if ifa_name
      .to_str()
      .map_err(|err| Error::new(std::io::ErrorKind::Other, err))?
      == default_interface
    {
      let addr = unsafe { (*ifa).ifa_addr };
      if !addr.is_null() && unsafe { (*addr).sa_family } == 2 {
        let addr = unsafe { &*(addr as *const sockaddr_in) };
        ip = Some(IpAddr::V4(Ipv4Addr::from(addr.sin_addr.s_addr.to_be())));
        break;
      }
    }
    ifa = unsafe { (*ifa).ifa_next };
  }
  unsafe { libc::freeifaddrs(ifaddrs) };
  let ip = ip.ok_or(Error::new(
    std::io::ErrorKind::NotFound,
    "No IP address found for the default interface",
  ))?;

  log::debug!("Default IP address: {ip}");
  log::debug!("You can override it with the --host-gateway option.");
  Ok(ip)
}
