use std::fs::File;
use std::net::Ipv4Addr;
use std::io::{Error, ErrorKind, BufRead};

/// Get host default gateway
pub fn get_default_gateway() -> std::io::Result<Ipv4Addr> {
  let file = File::open("/proc/net/route")?;
  let lines = std::io::BufReader::new(file).lines();
  let mut ip_addr = None;
  for line in lines {
    let line = line.map_err(|e| Error::new(ErrorKind::Other, e))?;
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() < 3 {
      continue;
    }
    if fields[1] == "00000000" {
      let ip = u32::from_str_radix(fields[2], 16)
        .map_err(|e| Error::new(ErrorKind::Other, e))?;
      // ip is in network byte order, so we need to convert it to host byte order
      let ip = Ipv4Addr::from(ip.to_le_bytes());
      ip_addr = Some(ip);
    }
  }
  ip_addr
    .ok_or_else(|| Error::new(ErrorKind::Other, "No default gateway found"))
}
