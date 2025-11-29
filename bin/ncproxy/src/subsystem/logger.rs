use std::{fs::OpenOptions, io::Write, os::unix::net::UnixDatagram};

/// Spawn a lightweight syslog receiver on a UNIX datagram socket and
/// append received HAProxy log lines to `http.log` under the state dir.
pub(crate) fn spawn(state: &crate::models::SystemStateRef) {
  let socket_path = format!("{}/log/haproxy.sock", state.store.dir);
  let http_log_path = format!("{}/log/http.log", state.store.dir);

  // Ensure log dir exists
  if let Err(e) = std::fs::create_dir_all(format!("{}/log", state.store.dir)) {
    log::warn!("logger: failed to create log dir: {e}");
    return;
  }

  // Clean up any pre-existing socket
  let _ = std::fs::remove_file(&socket_path);

  // Bind datagram socket for HAProxy syslog
  let sock = match UnixDatagram::bind(&socket_path) {
    Ok(s) => s,
    Err(e) => {
      log::warn!("logger: failed to bind {}: {e}", socket_path);
      return;
    }
  };

  // Make sure only our process can access the socket
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(&socket_path) {
      let mut perms = metadata.permissions();
      // world-writable to avoid user/group mismatch inside container
      perms.set_mode(0o666);
      let _ = std::fs::set_permissions(&socket_path, perms);
    }
  }

  std::thread::spawn(move || {
    let mut http_file = match OpenOptions::new()
      .create(true)
      .append(true)
      .open(&http_log_path)
    {
      Ok(f) => f,
      Err(e) => {
        log::warn!("logger: failed to open {}: {e}", http_log_path);
        return;
      }
    };
    log::info!(
      "logger: receiving haproxy logs on {} -> {}",
      socket_path,
      http_log_path
    );
    let mut buf = vec![0u8; 65535];
    loop {
      match sock.recv(&mut buf) {
        Ok(sz) => {
          if sz == 0 {
            continue;
          }
          let line = match std::str::from_utf8(&buf[..sz]) {
            Ok(s) => s,
            Err(_) => continue,
          };
          // Strip leading syslog priority if present (e.g. "<134>")
          let cleaned = if let Some(end) = line
            .strip_prefix('<')
            .and_then(|s| s.find('>'))
            .map(|i| i + 1)
          {
            &line[end..]
          } else {
            line
          };
          // Ensure exactly one newline per entry
          if let Err(e) = writeln!(http_file, "{}", cleaned.trim_end()) {
            log::warn!("logger: failed to write http.log: {e}");
          }
        }
        Err(e) => {
          log::warn!("logger: recv error: {e}");
          // prevent busy loop
          std::thread::sleep(std::time::Duration::from_millis(50));
        }
      }
    }
  });
}
